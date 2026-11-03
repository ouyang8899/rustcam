use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;

/// 本期仅登记路由：真正的遥测推送需要订阅 drone-service 的 EventService，在后续计划实现。
pub async fn telemetry_handler(
    ws: WebSocketUpgrade,
) -> Result<Response, axum::http::StatusCode> {
    Ok(ws.on_upgrade(|mut socket: WebSocket| async move {
        let _ = socket
            .send(Message::Text(
                serde_json::json!({ "msg_type": "hello", "data": {}, "timestamp": "" })
                    .to_string()
                    .into(),
            ))
            .await;
    }))
}
