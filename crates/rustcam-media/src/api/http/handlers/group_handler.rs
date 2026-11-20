use crate::api::response::ApiResponse;
use crate::api::state::FullState;
use crate::domain::device_group::{
    AssignGroupRequest, CreateGroupRequest, DeviceGroup, DeviceGroupNode, UpdateGroupRequest,
};
use crate::error::AppError;
use axum::{extract::Path, extract::Query, extract::State, Json};
use serde::Deserialize;
use std::sync::Arc;

pub async fn list_group_tree_handler(
    State(state): State<Arc<FullState>>,
) -> Result<Json<ApiResponse<Vec<DeviceGroupNode>>>, AppError> {
    let tree = state.app.registry.device_service.get_group_tree().await;
    Ok(Json(ApiResponse::success(tree)))
}

#[derive(Debug, Deserialize)]
pub struct DeviceTreeQuery {
    /// 按设备/通道名称或 tag 搜索。
    pub search: Option<String>,
    /// 预留：是否展开通道层。
    pub include_channels: Option<bool>,
}

/// 设备分组树：`分组 → 设备 → 通道` 三层结构，"未分组" 节点永远排在最后。
pub async fn device_tree_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<DeviceTreeQuery>,
) -> Result<Json<ApiResponse<crate::domain::device_tree::DeviceTreeResponse>>, AppError> {
    let search = q
        .search
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if let Some(s) = search {
        if s.len() > 128 {
            return Err(AppError::BadRequest("Search keyword too long".to_string()));
        }
    }
    let tree = state
        .app
        .registry
        .device_service
        .get_device_tree(search)
        .await?;
    Ok(Json(ApiResponse::success(tree)))
}

pub async fn list_groups_handler(
    State(state): State<Arc<FullState>>,
) -> Result<Json<ApiResponse<Vec<DeviceGroup>>>, AppError> {
    let groups = state.app.registry.device_service.list_groups().await;
    Ok(Json(ApiResponse::success(groups)))
}

pub async fn create_group_handler(
    State(state): State<Arc<FullState>>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<Json<ApiResponse<i64>>, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Group name cannot be empty".to_string(),
        ));
    }
    let id = state.app.registry.device_service.create_group(req).await?;
    Ok(Json(ApiResponse::success(id)))
}

pub async fn update_group_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    state
        .app
        .registry
        .device_service
        .update_group(id, req)
        .await?;
    Ok(Json(ApiResponse::success(())))
}

pub async fn delete_group_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    state.app.registry.device_service.delete_group(id).await?;
    Ok(Json(ApiResponse::success(())))
}

pub async fn assign_device_group_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
    Json(req): Json<AssignGroupRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    state
        .app
        .registry
        .device_service
        .assign_device_group(id, req.group_id)
        .await?;
    Ok(Json(ApiResponse::success(())))
}
