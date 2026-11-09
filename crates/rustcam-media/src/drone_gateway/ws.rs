use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Path, Query};
use axum::http::request::Parts;
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};
use futures::StreamExt;
use serde::Deserialize;

use crate::drone_gateway::client::DroneApi;
use crate::drone_gateway::permissions;
use crate::drone_gateway::ticket::TicketStore;

#[derive(Debug, Deserialize)]
struct TicketQuery {
    #[serde(default)]
    ticket: String,
}

/// 校验 WS 票据并确定订阅范围。
///
/// 作为提取器先于 WebSocketUpgrade 执行，保证「票据无效 / 越权订阅」分别返回
/// 401 / 403，而不是被握手失败（400）掩盖。
pub struct DroneTicket {
    pub kind: String,
}

impl<S: Send + Sync> FromRequestParts<S> for DroneTicket {
    type Rejection = axum::http::StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let tickets = parts
            .extensions
            .get::<Arc<TicketStore>>()
            .cloned()
            .ok_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        let query = Query::<TicketQuery>::from_request_parts(parts, state)
            .await
            .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
        let scope = tickets
            .consume(&query.ticket)
            .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

        let kind = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| axum::http::StatusCode::NOT_FOUND)?
            .0;
        if !permissions::is_ws_kind(&kind) {
            return Err(axum::http::StatusCode::NOT_FOUND);
        }

        // 票据在签发时绑定了可订阅的消息类型；越权订阅一律拒绝。
        let allowed = permissions::ws_msg_types(&kind);
        if !allowed.iter().all(|m| scope.iter().any(|s| s == m)) {
            return Err(axum::http::StatusCode::FORBIDDEN);
        }

        Ok(DroneTicket { kind })
    }
}

pub fn router<S>(api: Arc<dyn DroneApi>, tickets: Arc<TicketStore>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/ws/drone/{kind}", get(handler))
        .layer(Extension(api))
        .layer(Extension(tickets))
}

async fn handler(
    ticket: DroneTicket,
    ws: WebSocketUpgrade,
    Extension(api): Extension<Arc<dyn DroneApi>>,
) -> Result<Response, axum::http::StatusCode> {
    Ok(ws.on_upgrade(move |socket| stream_events(socket, api, ticket.kind)))
}

fn accepts(kind: &str, msg_type: &str) -> bool {
    match kind {
        "telemetry" => msg_type == "telemetry",
        "alerts" => msg_type == "alert",
        "commands" => msg_type == "command",
        "control" => msg_type == "control",
        _ => true,
    }
}

async fn stream_events(mut socket: WebSocket, api: Arc<dyn DroneApi>, kind: String) {
    // 事件流自带退避重订阅：drone-service 未启动或重启时，浏览器连接保持不断，
    // 恢复后事件自动续上。
    let mut stream = match api.event_stream(Vec::new()).await {
        Ok(s) => s,
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({ "msg_type": "error", "data": { "message": e.to_string() } })
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    };

    let hello = serde_json::json!({
        "msg_type": "hello",
        "data": { "kind": kind },
        "timestamp": chrono::Utc::now().timestamp_millis(),
    });
    if socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    let mut heartbeat = tokio::time::interval(Duration::from_secs(25));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            event = stream.next() => {
                match event {
                    Some(Ok(event)) => {
                        if !accepts(&kind, &event.msg_type) {
                            continue;
                        }
                        let payload = serde_json::json!({
                            "msg_type": event.msg_type,
                            "data": event.data,
                            "timestamp": event.ts_ms,
                        });
                        if socket.send(Message::Text(payload.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        // 单条事件解析失败只上报，不断开连接。
                        tracing::warn!(error = %e, "drone 事件解析失败");
                        let _ = socket
                            .send(Message::Text(
                    serde_json::json!({ "msg_type": "error", "data": { "message": e.to_string() } })
                                    .to_string()
                                    .into(),
                            ))
                            .await;
                    }
                    None => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        if socket.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) if text.as_str() == "ping" => {
                        let _ = socket.send(Message::Text("pong".into())).await;
                    }
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            _ = heartbeat.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}
