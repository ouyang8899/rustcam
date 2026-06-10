use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use crate::api::FullState;
use crate::application::ws_broadcaster::WsBroadcaster;
use crate::error::AppError;

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
    // 方案：使用 tokio::select! 在同一个任务中处理收发
    let (mut sender, mut receiver) = socket.split();
    let mut broadcast_rx = broadcaster.subscribe();

    loop {
        tokio::select! {
            // 接收客户端消息
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

            // 发送广播消息
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