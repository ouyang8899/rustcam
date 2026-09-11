use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade, Path};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use bytes::BytesMut;
use crate::api::FullState;
use crate::application::ws_broadcaster::WsBroadcaster;
use crate::error::AppError;
use crate::protocol::adapter_manager;
use crate::media::{MediaNodeType, MediaNodeId, BasicMediaNode, MediaNode, BasicMiddleware, SendToReceiversMiddleware, NullTransport, DataType};
use std::collections::HashMap;

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
async fn handle_socket(
    socket: WebSocket,
    broadcaster: Arc<WsBroadcaster>,
) {
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
    tracing::info!("[AudioTalk] WebSocket connection request for device {} channel {}", device_tag, channel_id);
    Ok(ws.on_upgrade(move |socket| handle_audio_talk(socket, device_tag, channel_id, state)))
}

async fn handle_audio_talk(socket: WebSocket, device_tag: String, channel_id: String, state: Arc<FullState>) {
    let (mut sender, mut receiver) = socket.split();

    tracing::info!("[AudioTalk] WebSocket connected for device {} channel {}", device_tag, channel_id);

    let session_key = format!("{}_{}", device_tag, channel_id);
    let node_id = MediaNodeId::new(MediaNodeType::WebSocket, &session_key);
    let node_id_for_delete = node_id.clone();
    let ws_node: Arc<dyn MediaNode> = Arc::new(BasicMediaNode::new(node_id, Arc::new(NullTransport)));

    ws_node.add_send_chain(MediaNodeType::GB28181_BROADCAST_VOICE, Box::new(BasicMiddleware::new("passthrough", |_| true)));
    ws_node.add_send_chain(MediaNodeType::GB28181_BROADCAST_VOICE, Box::new(SendToReceiversMiddleware::new(Arc::downgrade(&ws_node), MediaNodeType::GB28181_BROADCAST_VOICE)));

    let manager = match crate::media::get_media_session_manager() {
        Some(m) => m,
        None => {
            tracing::error!("[AudioTalk] No MediaSessionManager");
            return;
        }
    };
    let session = manager.get_or_create(&session_key);

    if let Err(e) = session.add_node(ws_node.clone()) {
        tracing::error!("[AudioTalk] Failed to add ws_node to session: {}", e);
        return;
    }
    tracing::info!("[AudioTalk] ws_node added to session for device {}", device_tag);

    let mut event_rx = state.app.registry.infra.event_bus.subscribe();
    let mut talk_invited = false;
    let mut talk_ready = false;
    let connection_id = node_id_for_delete.id;

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        if !talk_ready {
                            tracing::debug!("[AudioTalk] Ignoring audio data before ready for device {}", device_tag);
                            continue;
                        }

                        tracing::debug!("[AudioTalk] Received {} bytes from browser for device {}", data.len(), device_tag);

                        let mut data_map = HashMap::new();
                        data_map.insert(DataType::OrgData, BytesMut::from(&data[..]));
                        if let Err(e) = ws_node.sendto_receivers(data_map).await {
                            tracing::error!("[AudioTalk] Failed to send audio: {}", e);
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        tracing::debug!("[AudioTalk] Received text: {}", text);

                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            if json.get("action").and_then(|v| v.as_str()) == Some("start_talk") {
                                if talk_invited {
                                    let resp = serde_json::json!({
                                        "status": "already_started",
                                        "message": "语音对讲已启动"
                                    });
                                    let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
                                } else {
                                    // 发布 StartAudioTalk 事件
                                    tracing::info!("[AudioTalk] Publishing StartAudioTalk event for device {}", device_tag);
                                    let event = crate::protocol::event::SignalEvent::StartAudioTalk {
                                        device_tag: device_tag.clone(),
                                        connection_id,
                                    };
                                    let _ = state.app.registry.infra.event_bus.publish(event);
                                    talk_invited = true;
                                    let resp = serde_json::json!({
                                        "status": "inviting",
                                        "message": "正在呼叫..."
                                    });
                                    let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("[AudioTalk] Client closed connection for device {}", device_tag);
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("[AudioTalk] Error: {}", e);
                        break;
                    }
                    None => {
                        tracing::info!("[AudioTalk] Client disconnected for device {}", device_tag);
                        break;
                    }
                    _ => {}
                }
            }
            event = event_rx.recv() => {
                match event {
                    Ok(crate::protocol::event::SignalEvent::AudioTalkResponse { device_tag: ref tag, connection_id: cid, status, message }) => {
                        if tag == &device_tag && cid == connection_id {
                            tracing::info!("[AudioTalk] Received AudioTalkResponse for device {}: {:?}", device_tag, status);
                            match status {
                                crate::protocol::event::TalkResponseStatus::Ready => {
                                    talk_ready = true;
                                    let resp = serde_json::json!({
                                        "status": "ready",
                                        "message": message.as_deref().unwrap_or("你可以说话了")
                                    });
                                    let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
                                }
                                crate::protocol::event::TalkResponseStatus::Error => {
                                    talk_invited = false;
                                    let resp = serde_json::json!({
                                        "status": "error",
                                        "message": message.as_deref().unwrap_or("启动失败")
                                    });
                                    let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
                                }
                                crate::protocol::event::TalkResponseStatus::Inviting => {
                                    // 继续等待，不需要处理
                                }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("[AudioTalk] Event receive error: {:?}", e);
                    }
                }
            }
        }
    }

    // 发布 StopAudioTalk 事件
    tracing::info!("[AudioTalk] Publishing StopAudioTalk event for device {}", device_tag);
    let event = crate::protocol::event::SignalEvent::StopAudioTalk {
        device_tag: device_tag.clone(),
        connection_id,
    };
    let _ = state.app.registry.infra.event_bus.publish(event);

    if let Some(mgr) = crate::media::get_media_session_manager() {
        if let Some(session) = mgr.get(&device_tag) {
            session.remove_node(&node_id_for_delete);
            tracing::info!("[AudioTalk] MediaNode removed for device {}", device_tag);
        }
    }

    tracing::info!("[AudioTalk] Connection closed for device {}", device_tag);
}
