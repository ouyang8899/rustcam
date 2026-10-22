use std::sync::Arc;
use axum::{
    extract::{Path, State, Query},
    Json,
};
use serde::Deserialize;
use crate::api::response::ApiResponse;
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
}

#[derive(Debug, Deserialize)]
pub struct PolicyPath {
    pub id: i64,
}

pub async fn list_policies_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<PolicyQuery>,
) -> Result<Json<ApiResponse<Vec<RecordingPolicy>>>, AppError> {
    let registry = &state.app.registry;

    let policies = if let Some(ref device_tag) = q.device_tag {
        registry.recording_policy_service.find_policies_by_device(device_tag).await?
    } else {
        registry.recording_policy_service.list_policies().await?
    };

    let filtered: Vec<RecordingPolicy> = if let Some(ref pt) = q.policy_type {
        let policy_type = PolicyType::from_str(pt)
            .ok_or_else(|| AppError::BadRequest(format!("Invalid policy type: {}", pt)))?;
        policies.into_iter().filter(|p| p.policy_type == policy_type).collect()
    } else {
        policies
    };

    Ok(Json(ApiResponse::success(filtered)))
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
