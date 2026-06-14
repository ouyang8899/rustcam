use std::sync::Arc;
use axum::{
    extract::{Path, State, Query},
    Json,
};
use serde::Deserialize;
use crate::api::response::ApiResponse;
use crate::api::http::handlers::device_handler::PaginatedResponse;
use crate::error::AppError;
use crate::domain::Stream;
use crate::adapter::media_server::StreamInfo;
use crate::api::state::FullState;

/// 分页查询
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl PaginationQuery {
    pub fn limit(&self) -> usize { self.limit.unwrap_or(50).min(500) }
    pub fn offset(&self) -> usize { self.offset.unwrap_or(0) }
}

/// 开始推流请求
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StartStreamRequest {
    pub device_id: i64,
    pub rtsp_url: Option<String>,
}

/// 播放查询
#[derive(Debug, Deserialize)]
pub struct StreamPlayQuery {
    pub protocol: crate::adapter::media_server::Protocol,
}

/// GET /api/v1/streams
pub async fn list_streams_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<ApiResponse<PaginatedResponse<Stream>>>, AppError> {
    let registry = &state.app.registry;
    let streams = registry.stream_service.list_streams_paginated(q.limit(), q.offset()).await;
    let total = registry.stream_service.count_streams().await;

    Ok(Json(ApiResponse::success(PaginatedResponse {
        items: streams,
        total,
        limit: q.limit(),
        offset: q.offset(),
    })))
}

/// GET /api/v1/streams/:id
pub async fn get_stream_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Stream>>, AppError> {
    let stream = state.app.registry.stream_service.get_stream(id).await?
        .ok_or_else(|| AppError::NotFound(format!("Stream {} not found", id)))?;
    Ok(Json(ApiResponse::success(stream)))
}

/// POST /api/v1/streams
pub async fn start_stream_handler(
    State(state): State<Arc<FullState>>,
    Json(req): Json<StartStreamRequest>,
) -> Result<Json<ApiResponse<StreamInfo>>, AppError> {
    let registry = &state.app.registry;
    let stream_info = registry.stream_service.start_pull_stream(req.device_id,
                                                                 &req.rtsp_url.unwrap_or_default()).await?;
    Ok(Json(ApiResponse::success(stream_info)))
}

/// DELETE /api/v1/streams/:id
pub async fn stop_stream_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let stream = state.app.registry.stream_service.get_stream(id).await?
        .ok_or_else(|| AppError::NotFound(format!("Stream {} not found", id)))?;
    state.app.registry.stream_service.stop_stream(&stream.app, &stream.stream_key).await?;
    Ok(Json(ApiResponse::success(())))
}

/// GET /api/v1/streams/:id/play
pub async fn get_stream_play_url(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
    Query(query): Query<StreamPlayQuery>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let stream = state.app.registry.stream_service.get_stream(id).await?
        .ok_or_else(|| AppError::NotFound(format!("Stream {} not found", id)))?;
    let url = state.app.registry.stream_service.get_play_url(&stream.stream_key, query.protocol).await?;
    Ok(Json(ApiResponse::success(url)))
}

/// GET /api/v1/streams/:id/play-links
pub async fn get_stream_play_links_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<crate::domain::device::PlayLinks>>, AppError> {
    let registry = &state.app.registry;
    let stream = registry.stream_service.get_stream(id).await?
        .ok_or_else(|| AppError::NotFound(format!("Stream {} not found", id)))?;
    let device = registry.device_service.get_device(stream.device_id).await?
        .ok_or_else(|| AppError::NotFound(format!("Device {} not found", stream.device_id)))?;
    let play_links = registry.play_service.build_play_links_with_server(
        &device, &stream.token, &stream.stream_key, Some(&stream.media_server_tag), None,
    ).await;
    Ok(Json(ApiResponse::success(play_links)))
}

/// GET /api/v1/streams/:id/online
pub async fn is_stream_online_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<bool>>, AppError> {
    let stream = state.app.registry.stream_service.get_stream(id).await?
        .ok_or_else(|| AppError::NotFound(format!("Stream {} not found", id)))?;
    let online = state.app.registry.media.cluster
        .is_stream_online(&stream.app, &stream.stream_key, &stream.media_server_tag).await;
    Ok(Json(ApiResponse::success(online)))
}