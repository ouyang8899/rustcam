use crate::api::FullState;
use crate::application::ws_broadcaster::WsBroadcaster;
use crate::domain::device::Protocol;

use crate::error::AppError;
use crate::media::{
    AudioCodec, BasicMediaNode, LinearPcmToG711Middleware, MediaNode, MediaNodeId,
    MediaNodeType, SendToReceiversMiddleware, WebSocketMediaNodeTransport,
};
use crate::protocol::adapter_manager::{ self };

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use bytes::{ BytesMut };
use futures_util::{SinkExt, StreamExt};

use std::sync::Arc;
use tokio::sync::{ broadcast, mpsc };

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<FullState>>,
) -> Result<axum::response::Response, AppError> {
    let broadcaster = state.app.registry.ws_broadcaster.clone();

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, broadcaster)))
}

async fn handle_socket(socket: WebSocket, broadcaster: Arc<WsBroadcaster>) {
    let (mut sender, mut receiver) = socket.split();
    let mut broadcast_rx = broadcaster.subscribe();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(_text))) => {
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("[WebSocket] Error: {}", e);
                        break;
                    }
                    None => {
                        break;
                    }
                    _ => {}
                }
            }

            broadcast_msg = broadcast_rx.recv() => {
                match broadcast_msg {
                    Ok(msg) => {
                        let text = serde_json::to_string(&msg).unwrap_or_default();
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            tracing::warn!("[WebSocket] Failed to send message, closing");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("[WebSocket] Lagged {} messages", n);
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }

}

pub async fn audio_talk_handler(
    ws: WebSocketUpgrade,
    Path((device_tag, channel_id)): Path<(String, String)>,
    State(state): State<Arc<FullState>>,
) -> Result<axum::response::Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_audio_talk(socket, device_tag, channel_id, state)))
}

async fn handle_audio_talk(
    socket: WebSocket,
    device_tag: String,
    channel_id: String,
    state: Arc<FullState>,
) {
    let (tx, _rx) = broadcast::channel::<BytesMut>(100);

    let transport = Arc::new(WebSocketMediaNodeTransport::new(tx));

    let device_protocol = {
        match state
            .app
            .registry
            .device_service
            .get_device_by_device_tag(&device_tag)
        {
            Ok(Some(device)) => {
                Some(device.protocol)
            }
            Ok(None) => {
                tracing::warn!("[AudioTalk] Device {} not found", device_tag);
                None
            }
            Err(e) => {
                tracing::error!("[AudioTalk] Failed to get device {}: {}", device_tag, e);
                None
            }
        }
    };

    let device_protocol = match device_protocol {
        Some(p) => p,
        None => {
            tracing::error!(
                "[AudioTalk] Device {} not found or error, closing connection",
                device_tag
            );
            let resp =
                serde_json::json!({"status": "error", "message": "设备不存在或无法获取设备信息"});
            let _ = transport
                .send_text(&serde_json::to_string(&resp).unwrap())
                .await;
            return;
        }
    };

    let node_id = MediaNodeId::new(
        MediaNodeType::WebSocket,
        &device_tag,
        Some(channel_id.clone()),
    );
    let node_id_for_delete = node_id.clone();
    let ws_node: Arc<dyn MediaNode> =
        Arc::new(BasicMediaNode::new(node_id.clone(), transport.clone()));

    match device_protocol {
        Protocol::Gb28181 => {
            ws_node.add_send_chain(
                MediaNodeType::GB28181_BROADCAST_VOICE,
                Box::new(LinearPcmToG711Middleware::new(AudioCodec::Pcma)),
            );
            ws_node.add_send_chain(
                MediaNodeType::GB28181_BROADCAST_VOICE,
                Box::new(SendToReceiversMiddleware::new(
                    Arc::downgrade(&ws_node),
                    MediaNodeType::GB28181_BROADCAST_VOICE,
                )),
            );
        }
        Protocol::Onvif => {

            if adapter_manager::get_or_create_adapter(&Protocol::Onvif, &device_tag).is_none() {
                tracing::error!("[AudioTalk] ONVIF adapter factory not registered");
                let resp = serde_json::json!({"status": "error", "message": "ONVIF 适配器未注册"});
                let _ = transport
                    .send_text(&serde_json::to_string(&resp).unwrap())
                    .await;
                return;
            }

            ws_node.add_send_chain(
                MediaNodeType::ONVIF_BROADCAST_VOICE,
                Box::new(LinearPcmToG711Middleware::new(AudioCodec::Pcma)),
            );
            ws_node.add_send_chain(
                MediaNodeType::ONVIF_BROADCAST_VOICE,
                Box::new(SendToReceiversMiddleware::new(
                    Arc::downgrade(&ws_node),
                    MediaNodeType::ONVIF_BROADCAST_VOICE,
                )),
            );
        }
        other => {
            tracing::error!(
                "[AudioTalk] Unsupported protocol {:?} for device {}, closing connection",
                other,
                device_tag
            );
            let resp = serde_json::json!({"status": "error", "message": format!("设备协议 {:?} 不支持语音广播", other)});
            let _ = transport
                .send_text(&serde_json::to_string(&resp).unwrap())
                .await;
            return;
        }
    }

    let manager = match crate::media::get_media_session_manager() {
        Some(m) => m,
        None => {
            tracing::error!("[AudioTalk] No MediaSessionManager");
            return;
        }
    };
    let session = manager.get_or_create(&device_tag);

    if let Err(e) = session.add_node(ws_node.clone()) {
        tracing::error!("[AudioTalk] Failed to add ws_node to session: {}", e);
        return;
    }

    let node_id_clone = node_id_for_delete.clone();
    let state_clone = state.clone();
    let ws_node_clone = ws_node.clone();

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let transport_clone = transport.clone();
    transport.run(
        socket,
        move |data| {
            let ws_node = ws_node_clone.clone();
            async move {
                ws_node.on_media_data(data).await;
            }
        },
        move |text| {
            let node_id = node_id_clone.clone();
            let state = state_clone.clone();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                let action = json.get("action").and_then(|v| v.as_str());
                if action == Some("start_talk") {
                    let event = crate::protocol::event::SignalEvent::StartAudioTalk { node_id };
                    let event_bus = state.app.registry.infra.event_bus.clone();
                    tokio::spawn(async move {
                        if let Err(e) = event_bus.publish(event).await {
                            tracing::error!(
                                "[AudioTalk] Failed to publish StartAudioTalk event: {}",
                                e
                            );
                        } else {
                        }
                    });
                } else if action == Some("stop_talk") {
                    let event = crate::protocol::event::SignalEvent::StopAudioTalk { node_id };
                    let event_bus = state.app.registry.infra.event_bus.clone();
                    tokio::spawn(async move {
                        if let Err(e) = event_bus.publish(event).await {
                            tracing::error!(
                                "[AudioTalk] Failed to publish StopAudioTalk event: {}",
                                e
                            );
                        } else {
                        }
                    });
                }
            }
        },
        Some(shutdown_tx.clone()),
    );

    let mut event_rx = state.app.registry.infra.event_bus.subscribe();
    let node_id_for_compare = node_id_for_delete.clone();
    let device_tag_clone = device_tag.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    match event {
                        Ok(crate::protocol::event::SignalEvent::AudioTalkResponse { node_id: ref resp_node_id, status, message }) => {
                            if *resp_node_id == node_id_for_compare {
                                match status {
                                    crate::protocol::event::TalkResponseStatus::Ready => {
                                        let resp = serde_json::json!({"status": "ready", "message": message.as_deref().unwrap_or("你可以说话了")});
                                        let _ = transport_clone.send_text(&serde_json::to_string(&resp).unwrap()).await;
                                    }
                                    crate::protocol::event::TalkResponseStatus::Error => {
                                        let resp = serde_json::json!({"status": "error", "message": message.as_deref().unwrap_or("启动失败")});
                                        let _ = transport_clone.send_text(&serde_json::to_string(&resp).unwrap()).await;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!("[AudioTalk] Event error: {:?}", e),
                    }
                }
                _ = shutdown_rx.recv() => {
                    let event = crate::protocol::event::SignalEvent::StopAudioTalk {
                        node_id: node_id_for_delete.clone(),
                    };
                    let _ = state.app.registry.infra.event_bus.publish(event).await;
                    if let Some(mgr) = crate::media::get_media_session_manager() {
                        if let Some(session) = mgr.get(&device_tag_clone) {
                            session.remove_node(&node_id_for_delete);
                        }
                    }
                    break;
                }
            }
        }
    });
}
