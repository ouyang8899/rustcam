use crate::api::response::ApiResponse;
use crate::api::state::FullState;
use crate::domain::region::{Region, RegionNode};
use crate::error::AppError;
use axum::extract::Query;
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct RegionQuery {
    pub parent: Option<String>,
}

pub async fn list_regions_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<RegionQuery>,
) -> Result<Json<ApiResponse<Vec<Region>>>, AppError> {
    let regions = if let Some(parent) = &q.parent {
        state
            .app
            .registry
            .device_service
            .list_region_children(parent)
            .await
    } else {
        state
            .app
            .registry
            .device_service
            .list_region_children("")
            .await
    };
    Ok(Json(ApiResponse::success(regions)))
}

pub async fn list_region_tree_handler(
    State(state): State<Arc<FullState>>,
) -> Result<Json<ApiResponse<Vec<RegionNode>>>, AppError> {
    let tree = state.app.registry.device_service.get_region_tree().await;
    Ok(Json(ApiResponse::success(tree)))
}

pub async fn unassigned_region_count_handler(
    State(state): State<Arc<FullState>>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let count = state
        .app
        .registry
        .device_service
        .count_unassigned_region_devices();
    Ok(Json(ApiResponse::success(
        serde_json::json!({ "count": count }),
    )))
}
