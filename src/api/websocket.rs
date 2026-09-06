use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade, Path};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use crate::api::FullState;
use crate::application::ws_broadcaster::WsBroadcaster;
use crate::error::AppError;
use crate::protocol::adapter_manager;

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
    Path(device_tag): Path<String>,
    State(state): State<Arc<FullState>>,
) -> Result<axum::response::Response, AppError> {
    tracing::info!("[AudioTalk] WebSocket connection request for device {}", device_tag);
    Ok(ws.on_upgrade(move |socket| handle_audio_talk(socket, device_tag, state)))
}

async fn handle_audio_talk(socket: WebSocket, device_tag: String, state: Arc<FullState>) {
    let (mut sender, mut receiver) = socket.split();

    tracing::info!("[AudioTalk] WebSocket connected for device {}", device_tag);

    let device_info = if let Some(device) = state.app.registry.device_service.get_device_by_device_tag(&device_tag).ok().flatten() {
        let ip = device.host.clone();
        let port = if device.port > 0 { device.port } else { 5060 };
        Some((ip, port))
    } else {
        tracing::warn!("[AudioTalk] Device not found: {}", device_tag);
        None
    };

    let mut event_rx = state.app.registry.infra.event_bus.subscribe();
    let mut talk_invited = false;
    let mut talk_ready = false;

    if let Some(ref adapter_key) = state.app.registry.device_service.get_device_by_device_tag(&device_tag).ok().flatten().and_then(|d| d.device_tag.clone()) {
        if let Some(adapter) = adapter_manager::get_adapter(adapter_key) {
            let guard = adapter.lock().await;
            if guard.is_audio_talk_active(&device_tag).await {
                tracing::info!("[AudioTalk] Audio talk already active for device {}, restoring session", device_tag);
                talk_invited = true;
                talk_ready = true;
                let resp = serde_json::json!({
                    "status": "ready",
                    "message": "你可以说话了"
                });
                let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
            }
        }
    }

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

                        if let Some(ref adapter_key) = state.app.registry.device_service.get_device_by_device_tag(&device_tag).ok().flatten().and_then(|d| d.device_tag.clone()) {
                            if let Some(adapter) = adapter_manager::get_adapter(adapter_key) {
                                let mut guard = adapter.lock().await;

                                let pcm_data: Vec<i16> = data.chunks_exact(2)
                                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                                    .collect();

                                if let Err(e) = guard.send_audio_to_device_v2(adapter_key, &pcm_data).await {
                                    tracing::error!("[AudioTalk] Failed to send audio: {}", e);
                                }
                            }
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
                                } else if let Some((ip, port)) = device_info.clone() {
                                    if let Some(ref adapter_key) = state.app.registry.device_service.get_device_by_device_tag(&device_tag).ok().flatten().and_then(|d| d.device_tag.clone()) {
                                        if let Some(adapter) = adapter_manager::get_adapter(adapter_key) {
                                            let mut guard = adapter.lock().await;
                                            match guard.start_audio_talk_v2(adapter_key, &format!("audio-talk-{}", device_tag), &ip, port, port, crate::protocol::event::TransportType::Udp).await {
                                                Ok(()) => {
                                                    tracing::info!("[AudioTalk] INVITE sent for device {}, waiting for 200 OK", device_tag);
                                                    talk_invited = true;
                                                    let resp = serde_json::json!({
                                                        "status": "inviting",
                                                        "message": "正在呼叫..."
                                                    });
                                                    let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
                                                }
                                                Err(e) => {
                                                    tracing::error!("[AudioTalk] Failed to start: {}", e);
                                                    let resp = serde_json::json!({
                                                        "status": "error",
                                                        "message": format!("启动失败: {}", e)
                                                    });
                                                    let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    let resp = serde_json::json!({
                                        "status": "error",
                                        "message": "设备不存在"
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
                    Ok(crate::protocol::event::SignalEvent::AudioTalkReady { device_id: _, device_tag: ref tag }) => {
                        if tag.as_ref() == Some(&device_tag) {
                            tracing::info!("[AudioTalk] Received AudioTalkReady for device {}", device_tag);
                            talk_ready = true;
                            let resp = serde_json::json!({
                                "status": "ready",
                                "message": "你可以说话了"
                            });
                            let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
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

    if let Some(ref adapter_key) = state.app.registry.device_service.get_device_by_device_tag(&device_tag).ok().flatten().and_then(|d| d.device_tag.clone()) {
        if let Some(adapter) = adapter_manager::get_adapter(adapter_key) {
            let mut guard = adapter.lock().await;
            if let Err(e) = guard.stop_audio_talk_v2(adapter_key).await {
                tracing::error!("[AudioTalk] Failed to stop audio talk: {}", e);
            } else {
                tracing::info!("[AudioTalk] Audio talk stopped for device {}", device_tag);
            }
        }
    }

    tracing::info!("[AudioTalk] Connection closed for device {}", device_tag);
}
