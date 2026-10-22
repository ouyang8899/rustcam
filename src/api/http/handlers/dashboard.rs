use std::sync::Arc;
use axum::{extract::State, Json};
use crate::api::response::ApiResponse;
use crate::api::state::FullState;

pub async fn dashboard_handler(
    State(state): State<Arc<FullState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let data = state.app.registry.get_dashboard_data().await;
    Json(ApiResponse::success(data))
}

pub async fn reload_cache_handler(
    State(state): State<Arc<FullState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.app.registry.infra.db.reload_devices_cache().await {
        Ok(count) => Json(ApiResponse::success(serde_json::json!({ "count": count }))),
        Err(e) => Json(ApiResponse::error(500, &e.to_string())),
    }
}
