use std::sync::Arc;
use axum::{
    extract::{Path, State, Query},
    Json,
};
use serde::Deserialize;
use crate::api::response::ApiResponse;
use crate::api::http::handlers::device_handler::PaginatedResponse;
use crate::api::state::FullState;
use crate::error::AppError;
use crate::domain::recording_policy::{
    RecordingPolicy, PolicyType,
    CreateRecordingPolicyRequest, UpdateRecordingPolicyRequest,
};

#[derive(Debug, Deserialize)]
pub struct PolicyQuery {
    pub device_tag: Option<String>,
    pub policy_type: Option<String>,
    /// 分页大小
    pub limit: Option<usize>,
    /// 分页偏移
    pub offset: Option<usize>,
}

impl PolicyQuery {
    pub fn limit(&self) -> usize { self.limit.unwrap_or(50).min(500) }
    pub fn offset(&self) -> usize { self.offset.unwrap_or(0) }
    pub fn device_tag(&self) -> Option<&str> { self.device_tag.as_deref().filter(|s| !s.is_empty()) }
    pub fn policy_type(&self) -> Option<&str> { self.policy_type.as_deref().filter(|s| !s.is_empty()) }
}

#[derive(Debug, Deserialize)]
pub struct PolicyPath {
    pub id: i64,
}

pub async fn list_policies_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<PolicyQuery>,
) -> Result<Json<ApiResponse<PaginatedResponse<RecordingPolicy>>>, AppError> {
    let registry = &state.app.registry;

    // 过滤条件在 SQL 中执行，分页由数据库完成
    let policy_type = q.policy_type();
    if let Some(pt) = policy_type {
        PolicyType::from_str(pt)
            .ok_or_else(|| AppError::BadRequest(format!("Invalid policy type: {}", pt)))?;
    }

    let policies = registry
        .recording_policy_service
        .list_policies_paginated(q.device_tag(), policy_type, q.limit(), q.offset())
        .await?;
    let total = registry
        .recording_policy_service
        .count_policies(q.device_tag(), policy_type)
        .await?;

    Ok(Json(ApiResponse::success(PaginatedResponse {
        items: policies,
        total: total as usize,
        limit: q.limit(),
        offset: q.offset(),
    })))
}

pub async fn get_policy_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<RecordingPolicy>>, AppError> {
    let registry = &state.app.registry;

    let policy = registry.recording_policy_service.get_policy(id).await?
        .ok_or_else(|| AppError::NotFound(format!("Policy {} not found", id)))?;

    Ok(Json(ApiResponse::success(policy)))
}

pub async fn create_policy_handler(
    State(state): State<Arc<FullState>>,
    Json(req): Json<CreateRecordingPolicyRequest>,
) -> Result<Json<ApiResponse<RecordingPolicy>>, AppError> {
    let registry = &state.app.registry;

    if req.device_tag.is_empty() {
        return Err(AppError::BadRequest("device_tag is required".to_string()));
    }

    let policy = registry.recording_policy_service.create_policy(req).await?;

    Ok(Json(ApiResponse::success(policy)))
}

pub async fn update_policy_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateRecordingPolicyRequest>,
) -> Result<Json<ApiResponse<RecordingPolicy>>, AppError> {
    let registry = &state.app.registry;

    let policy = registry.recording_policy_service.update_policy(id, req).await?;

    Ok(Json(ApiResponse::success(policy)))
}

pub async fn delete_policy_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let registry = &state.app.registry;

    registry.recording_policy_service.delete_policy(id).await?;

    Ok(Json(ApiResponse::success(())))
}
