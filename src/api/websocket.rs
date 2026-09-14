use crate::api::FullState;
use crate::application::ws_broadcaster::WsBroadcaster;
use crate::error::AppError;
use crate::media::{
    AudioCodec, BasicMediaNode, DataType, MediaNode, MediaNodeId, MediaNodeType,
    NullTransport, LinearPcmToG711Middleware, SendToReceiversMiddleware,
    WebSocketMediaNodeTransport,
};
use crate::protocol::adapter_manager;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

/// WebSocket 连接处理器
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<FullState>>,
) -> Result<axum::response::Response, AppError> {
    let broadcaster = state.app.registry.ws_broadcaster.clone();

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, broadcaster)))
}

/// 处理 WebSocket 连接
///
/// 同时处理：
/// - 接收客户端消息（Ping/Pong/Close）
/// - 发送服务端广播消息
async fn handle_socket(socket: WebSocket, broadcaster: Arc<WsBroadcaster>) {
    let (mut sender, mut receiver) = socket.split();
    let mut broadcast_rx = broadcaster.subscribe();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        tracing::debug!("[WebSocket] Received: {}", text);
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("[WebSocket] Client closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("[WebSocket] Error: {}", e);
                        break;
                    }
                    None => {
                        tracing::info!("[WebSocket] Client disconnected");
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
                        tracing::info!("[WebSocket] Broadcast channel closed");
                        break;
                    }
                }
            }
        }
    }

    tracing::info!("[WebSocket] Connection closed");
}

/// WebSocket audio talk handler (Browser -> Device)
pub async fn audio_talk_handler(
    ws: WebSocketUpgrade,
    Path((device_tag, channel_id)): Path<(String, String)>,
    State(state): State<Arc<FullState>>,
) -> Result<axum::response::Response, AppError> {
    tracing::info!(
        "[AudioTalk] WebSocket connection request for device {} channel {}",
        device_tag,
        channel_id
    );
    Ok(ws.on_upgrade(move |socket| handle_audio_talk(socket, device_tag, channel_id, state)))
}

async fn handle_audio_talk(
    socket: WebSocket,
    device_tag: String,
    channel_id: String,
    state: Arc<FullState>,
) {
    let (tx, _rx) = broadcast::channel::<BytesMut>(100);

    tracing::info!(
        "[AudioTalk] WebSocket connected for device {} channel {}",
        device_tag,
        channel_id
    );

    let node_id = MediaNodeId::new(MediaNodeType::WebSocket, &device_tag, Some(channel_id.clone()));
    let node_id_for_delete = node_id.clone();
    let transport = Arc::new(WebSocketMediaNodeTransport::new(tx));
    let ws_node: Arc<dyn MediaNode> =
        Arc::new(BasicMediaNode::new(node_id.clone(), transport.clone()));

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
    transport.run(socket,
        move |data| {
            let ws_node = ws_node_clone.clone();
            async move {
                ws_node.on_media_data(data).await;
            }
        },
        move |text| {
            let node_id = node_id_clone.clone();
            let state = state_clone.clone();
            tracing::debug!("[AudioTalk] Received text message: {}", text);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if json.get("action").and_then(|v| v.as_str()) == Some("start_talk") {
                    tracing::info!("[AudioTalk] start_talk action received, publishing StartAudioTalk event");
                    let event = crate::protocol::event::SignalEvent::StartAudioTalk { node_id };
                    let event_bus = state.app.registry.infra.event_bus.clone();
                    tokio::spawn(async move {
                        if let Err(e) = event_bus.publish(event).await {
                            tracing::error!("[AudioTalk] Failed to publish StartAudioTalk event: {}", e);
                        } else {
                            tracing::info!("[AudioTalk] StartAudioTalk event published successfully");
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
                    tracing::info!("[AudioTalk] Shutdown signal received, cleaning up");
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
    tracing::info!("[AudioTalk] Connection closed for device {}", device_tag);
}
