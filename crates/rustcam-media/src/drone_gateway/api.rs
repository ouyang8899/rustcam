use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::routing::{get, post};
use axum::Json;
use serde::Serialize;

use crate::api::response::ApiResponse;
use crate::drone_gateway::client::{DeviceInfoDto, DroneApi};
use crate::drone_gateway::ticket::TicketStore;

/// 构造 drone REST 路由（无状态依赖，依赖用 Extension 注入）。
/// 必须在主服务的 `protected_routes` 内、`.route_layer(jwt_auth_layer)` 之前 merge。
pub fn router<S>(api: Arc<dyn DroneApi>) -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let tickets = Arc::new(TicketStore::new(60));
    axum::Router::new()
        .route("/api/v1/drone/ping", get(ping))
        .route("/api/v1/drone/devices", get(list_devices))
        .route("/api/v1/drone/devices/{device_id}", get(get_device))
        .route("/api/v1/drone/ws-ticket", post(issue_ticket))
        .layer(Extension(api))
        .layer(Extension(tickets))
}

async fn ping(Extension(api): Extension<Arc<dyn DroneApi>>) -> Json<ApiResponse<serde_json::Value>> {
    match api.ping().await {
        Ok(()) => Json(ApiResponse::success(serde_json::json!({ "status": "ok" }))),
        Err(e) => Json(ApiResponse::error(503, &format!("drone-service unavailable: {e}"))),
    }
}

async fn list_devices(
    Extension(api): Extension<Arc<dyn DroneApi>>,
) -> Json<ApiResponse<Vec<DeviceInfoDto>>> {
    match api.list_devices().await {
        Ok(v) => Json(ApiResponse::success(v)),
        Err(e) => Json(ApiResponse::error(503, &format!("drone-service unavailable: {e}"))),
    }
}

async fn get_device(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Path(device_id): Path<String>,
) -> Json<ApiResponse<DeviceInfoDto>> {
    match api.get_device(&device_id).await {
        Ok(v) => Json(ApiResponse::success(v)),
        Err(e) => Json(ApiResponse::error(
            404,
            &format!("device {device_id} not found: {e}"),
        )),
    }
}

#[derive(Serialize)]
struct TicketResponse {
    ticket: String,
    expires_in_secs: u64,
}

async fn issue_ticket(
    Extension(tickets): Extension<Arc<TicketStore>>,
) -> Json<ApiResponse<TicketResponse>> {
    let ticket = tickets.issue();
    Json(ApiResponse::success(TicketResponse {
        ticket,
        expires_in_secs: 60,
    }))
}
