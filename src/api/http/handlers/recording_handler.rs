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
use crate::domain::recording::{Recording, RecordingFormat, CreateRecordingRequest};

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// 设备过滤，条件在 SQL 中执行
    pub device_tag: Option<String>,
    /// 通道过滤，条件在 SQL 中执行
    pub channel_tag: Option<String>,
    /// 状态过滤，条件在 SQL 中执行
    pub state: Option<String>,
    /// 关键字过滤（设备/通道/文件名），条件在 SQL 中执行
    pub search: Option<String>,
}

impl PaginationQuery {
    pub fn limit(&self) -> usize { self.limit.unwrap_or(50).min(500) }
    pub fn offset(&self) -> usize { self.offset.unwrap_or(0) }
    pub fn device_tag(&self) -> Option<&str> { self.device_tag.as_deref().filter(|s| !s.is_empty()) }
    pub fn channel_tag(&self) -> Option<&str> { self.channel_tag.as_deref().filter(|s| !s.is_empty()) }
    pub fn state(&self) -> Option<&str> { self.state.as_deref().filter(|s| !s.is_empty()) }
    pub fn search(&self) -> Option<&str> { self.search.as_deref().filter(|s| !s.is_empty()) }}

#[derive(Debug, Deserialize)]
pub struct CreateRecordingReq {
    pub device_tag: String,
    pub channel_tag: Option<String>,
    pub format: Option<String>,
    pub duration_secs: Option<u32>,
    pub max_file_size_mb: Option<u32>,
    pub output_path: Option<String>,
    pub labels: Option<Vec<String>>,
    pub pre_record_secs: Option<u32>,
}

pub async fn list_recordings_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<ApiResponse<PaginatedResponse<Recording>>>, AppError> {
    let registry = &state.app.registry;
    // 数据库里状态是小写的，前端传的是 PascalCase
    let state_filter = q.state().map(|s| s.to_lowercase());
    let recordings = registry
        .recording_service
        .list_recordings_filtered(
            q.device_tag(),
            q.channel_tag(),
            state_filter.as_deref(),
            q.search(),
            q.limit(),
            q.offset(),
        )
        .await?;
    let total = registry
        .recording_service
        .count_recordings_filtered(
            q.device_tag(),
            q.channel_tag(),
            state_filter.as_deref(),
            q.search(),
        )
        .await?;

    Ok(Json(ApiResponse::success(PaginatedResponse {
        items: recordings,
        total,
        limit: q.limit(),
        offset: q.offset(),
    })))
}

pub async fn create_recording_handler(
    State(state): State<Arc<FullState>>,
    Json(req): Json<CreateRecordingReq>,
) -> Result<Json<ApiResponse<Recording>>, AppError> {
    let registry = &state.app.registry;
    let format = match req.format.as_deref() {
        Some("hls") | Some("HLS") => RecordingFormat::Hls,
        Some("flv") | Some("FLV") => RecordingFormat::Flv,
        Some("ts") | Some("TS") => RecordingFormat::Ts,
        _ => RecordingFormat::Mp4,
    };

    let device = registry.device_service.get_device_by_device_tag(&req.device_tag)?
        .ok_or_else(|| AppError::NotFound(format!("Device {} not found", req.device_tag)))?;

    let channel_tag = req.channel_tag.clone().unwrap_or_else(|| {
        let cache = registry.infra.db.streams_cache();
        cache.iter()
            .find(|s| s.device_tag.as_ref() == Some(&req.device_tag))
            .and_then(|s| s.channel_tag.clone())
            .unwrap_or_else(|| "recording".to_string())
    });

    let recording_req = CreateRecordingRequest {
        device_tag: req.device_tag.clone(),
        channel_tag: channel_tag.clone(),
        format: Some(format),
        duration_secs: req.duration_secs,
        max_file_size_mb: req.max_file_size_mb,
        output_path: req.output_path,
        labels: req.labels,
        pre_record_secs: req.pre_record_secs,
    };

    let media_server = device.media_server_tag.clone().unwrap_or_else(|| "auto".to_string());

    let recording = registry.recording_service.create_recording(
        recording_req, channel_tag, media_server,
    ).await;

    Ok(Json(ApiResponse::success(recording)))
}

pub async fn get_recording_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Recording>>, AppError> {
    let recording = state.app.registry.recording_service.get_recording(id).await?
        .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;
    Ok(Json(ApiResponse::success(recording)))
}

pub async fn start_recording_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Recording>>, AppError> {
    let recording = state.app.registry.recording_service.start_recording(id).await?;
    Ok(Json(ApiResponse::success(recording)))
}

pub async fn stop_recording_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Recording>>, AppError> {
    let recording = state.app.registry.recording_service.stop_recording(id).await?;
    Ok(Json(ApiResponse::success(recording)))
}

pub async fn pause_recording_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Recording>>, AppError> {
    let recording = state.app.registry.recording_service.pause_recording(id).await?;
    Ok(Json(ApiResponse::success(recording)))
}

pub async fn resume_recording_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Recording>>, AppError> {
    let recording = state.app.registry.recording_service.resume_recording(id).await?;
    Ok(Json(ApiResponse::success(recording)))
}

pub async fn delete_recording_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    state.app.registry.recording_service.delete_recording(id).await?;
    Ok(Json(ApiResponse::success(())))
}

pub async fn recording_stats_handler(
    State(state): State<Arc<FullState>>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let stats = state.app.registry.recording_service.get_stats().await?;
    Ok(Json(ApiResponse::success(stats)))
}

pub async fn recording_files_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Vec<crate::adapter::media_server::RecordingFile>>>, AppError> {
    let files = state.app.registry.recording_service.list_recorded_files(id).await?;
    Ok(Json(ApiResponse::success(files)))
}

pub async fn all_recording_files_handler(
    State(state): State<Arc<FullState>>,
) -> Result<Json<ApiResponse<Vec<crate::adapter::media_server::RecordingFile>>>, AppError> {
    let registry = &state.app.registry;
    let all_recordings = registry.recording_service.list_recordings_paginated(1000, 0).await?;
    let mut all_files = Vec::new();
    for rec in all_recordings {
        if let Ok(files) = registry.recording_service.list_recorded_files(rec.id).await {
            all_files.extend(files);
        }
    }
    all_files.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(Json(ApiResponse::success(all_files)))
}