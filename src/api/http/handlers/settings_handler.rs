use std::sync::Arc;
use axum::{extract::State, Json};
use crate::api::state::FullState;
use crate::api::response::ApiResponse;
use crate::error::AppError;
use crate::domain::map_settings::{MapSettings, UpdateMapSettingsRequest};

pub async fn get_map_settings_handler(
    State(state): State<Arc<FullState>>,
) -> Result<Json<ApiResponse<MapSettings>>, AppError> {
    let settings = state.app.registry.map_settings_service.get().await?;
    Ok(Json(ApiResponse::success(settings)))
}

pub async fn update_map_settings_handler(
    State(state): State<Arc<FullState>>,
    Json(req): Json<UpdateMapSettingsRequest>,
) -> Result<Json<ApiResponse<MapSettings>>, AppError> {
    let settings = state.app.registry.map_settings_service.update(req).await?;
    Ok(Json(ApiResponse::success(settings)))
}
