use crate::api::response::ApiResponse;
use crate::api::state::FullState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::http::handlers::device_handler::ChannelWithMediaServer;
use crate::common::normalize_status;
use crate::domain::{Channel, CreateChannelRequest, DeviceStatus, Protocol, UpdateChannelRequest};
use crate::error::AppError;
use crate::protocol::adapter_manager;

#[derive(Debug, Deserialize)]
pub struct ChannelPaginationQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub device_tag: Option<String>,
    pub search: Option<String>,
    pub status: Option<String>,
    /// 分组过滤：`0` 表示未分组，`>0` 表示该分组及其全部子孙分组。
    pub group_id: Option<i64>,
    /// 地区过滤：按地区编码前缀匹配，覆盖该地区及其下属地区。
    pub region_code: Option<String>,
    /// 只要没有归属地区的设备（地区树上的「未分组」）。
    pub unassigned_region: Option<bool>,
}

impl ChannelPaginationQuery {
    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(50).min(500)
    }
    pub fn offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedChannels {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub items: Vec<ChannelWithMediaServer>,
}

pub async fn list_channels_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<ChannelPaginationQuery>,
) -> Result<Json<ApiResponse<PaginatedChannels>>, AppError> {
    let registry = &state.app.registry;
    let status = normalize_status(q.status.as_deref());

    let channels = registry
        .channel_service
        .list_channels_paginated_db(
            q.device_tag.as_deref(),
            q.search.as_deref(),
            status.as_deref(),
            q.group_id,
            q.region_code.as_deref(),
            q.unassigned_region,
            q.limit(),
            q.offset(),
        )
        .await?;

    let total = registry
        .channel_service
        .count_channels_db(
            q.device_tag.as_deref(),
            q.search.as_deref(),
            status.as_deref(),
            q.group_id,
            q.region_code.as_deref(),
            q.unassigned_region,
        )
        .await?;

    let items: Vec<ChannelWithMediaServer> = channels
        .into_iter()
        .map(|c| ChannelWithMediaServer::from_channel(c, registry))
        .collect();

    Ok(Json(ApiResponse::success(PaginatedChannels {
        total,
        limit: q.limit(),
        offset: q.offset(),
        items,
    })))
}

pub async fn get_channel_handler(
    State(state): State<Arc<FullState>>,
    Path((device_tag, channel_tag)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Channel>>, AppError> {
    let channel = state
        .app
        .registry
        .channel_service
        .get_channel(&device_tag, &channel_tag)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!("Channel {}/{} not found", device_tag, channel_tag))
        })?;
    Ok(Json(ApiResponse::success(channel)))
}

/// 更新通道。
///
/// 直接复用 domain 层的 `UpdateChannelRequest`——它已经带好了
/// `double_option` 的反序列化配置，能区分"字段缺省"与"显式 null"。
pub async fn update_channel_handler(
    State(state): State<Arc<FullState>>,
    Path((device_tag, channel_tag)): Path<(String, String)>,
    Json(payload): Json<UpdateChannelRequest>,
) -> Result<Json<ApiResponse<Channel>>, AppError> {
    let channel = state
        .app
        .registry
        .channel_service
        .update_channel_info(&device_tag, &channel_tag, payload)
        .await?;
    Ok(Json(ApiResponse::success(channel)))
}

pub async fn create_channel_handler(
    State(state): State<Arc<FullState>>,
    Json(payload): Json<CreateChannelRequest>,
) -> Result<Json<ApiResponse<Channel>>, AppError> {
    if payload.device_tag.trim().is_empty() || payload.channel_tag.trim().is_empty() {
        return Err(AppError::BadRequest(
            "device_tag and channel_tag are required".to_string(),
        ));
    }
    let channel = state
        .app
        .registry
        .channel_service
        .create_channel_info(payload)
        .await?;
    Ok(Json(ApiResponse::success(channel)))
}

pub async fn get_channels_by_device_handler(
    State(state): State<Arc<FullState>>,
    Path(device_tag): Path<String>,
) -> Result<Json<ApiResponse<Vec<Channel>>>, AppError> {
    let channels = state
        .app
        .registry
        .channel_service
        .get_channels_by_device(&device_tag);
    Ok(Json(ApiResponse::success(channels)))
}

#[derive(Debug, serde::Deserialize)]
pub struct ChannelPlayQuery {
    pub stream_key: Option<String>,
}

pub async fn get_channel_play_links_handler(
    State(state): State<Arc<FullState>>,
    Path((device_tag, channel_tag)): Path<(String, String)>,
    Query(query): Query<ChannelPlayQuery>,
) -> Result<Json<ApiResponse<crate::domain::device::PlayLinks>>, AppError> {
    let channel = state
        .app
        .registry
        .channel_service
        .get_channel(&device_tag, &channel_tag)
        .await?;

    let device = state
        .app
        .registry
        .device_service
        .get_device_by_device_tag(&device_tag)?
        .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_tag)))?;

    let stream_key = query.stream_key.unwrap_or_else(|| {
        channel
            .as_ref()
            .map(|c| c.channel_tag.clone())
            .unwrap_or_else(|| channel_tag.clone())
    });

    let stream_id =
        crate::common::device_utils::format_stream_key(&device_tag, stream_key.as_str());
    let token = state
        .app
        .registry
        .play_service
        .generate_token(&device.id.to_string())
        .await?;
    let play_links = state
        .app
        .registry
        .play_service
        .build_play_links(&device, &token, &stream_id)
        .await;

    Ok(Json(ApiResponse::success(play_links)))
}

pub async fn start_channel_stream_handler(
    State(state): State<Arc<FullState>>,
    Path((device_tag, channel_tag)): Path<(String, String)>,
) -> Result<Json<ApiResponse<StartChannelResult>>, AppError> {
    let device = state
        .app
        .registry
        .device_service
        .get_device_by_device_tag(&device_tag)?
        .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_tag)))?;

    if device.protocol != Protocol::Gb28181 {
        return Err(AppError::BadRequest(
            "Only GB28181 devices support this operation".to_string(),
        ));
    }

    let adapter = adapter_manager::get_adapter(&device_tag).ok_or_else(|| {
        AppError::BadRequest(format!(
            "GB28181 adapter not found for device {}",
            device_tag
        ))
    })?;

    let mut guard = adapter.lock().await;
    guard
        .start(&device_tag, &channel_tag)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to start stream: {}", e)))?;

    Ok(Json(ApiResponse::success(StartChannelResult {
        device_tag: device_tag.clone(),
        channel_tag: channel_tag.clone(),
        message: "INVITE sent, waiting for device response".to_string(),
    })))
}

#[derive(Debug, Serialize)]
pub struct StartChannelResult {
    pub device_tag: String,
    pub channel_tag: String,
    pub message: String,
}

pub async fn get_channel_status_handler(
    State(state): State<Arc<FullState>>,
    Path((device_tag, channel_tag)): Path<(String, String)>,
) -> Result<Json<ApiResponse<ChannelStatus>>, AppError> {
    let channel = state
        .app
        .registry
        .channel_service
        .get_channel(&device_tag, &channel_tag)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!("Channel {}/{} not found", device_tag, channel_tag))
        })?;

    let cache = state.app.registry.infra.db.streams_cache();
    let has_stream = cache
        .iter()
        .any(|s| s.channel_tag.as_ref() == Some(&channel.channel_tag));

    let status = ChannelStatus {
        channel_tag: channel.channel_tag,
        device_tag: channel.device_tag,
        name: channel.name,
        status: channel.status,
        has_stream,
        stream_count: if has_stream { 1 } else { 0 },
    };

    Ok(Json(ApiResponse::success(status)))
}

#[derive(Debug, serde::Serialize)]
pub struct ChannelStatus {
    pub channel_tag: String,
    pub device_tag: String,
    pub name: String,
    pub status: DeviceStatus,
    pub has_stream: bool,
    pub stream_count: u32,
}

#[derive(Debug, serde::Deserialize)]
pub struct DeviceChannelsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub status: Option<String>,
}

pub async fn get_device_channels_handler(
    State(state): State<Arc<FullState>>,
    Path(device_tag): Path<String>,
    Query(q): Query<DeviceChannelsQuery>,
) -> Result<Json<ApiResponse<Vec<Channel>>>, AppError> {
    let status = normalize_status(q.status.as_deref());
    let limit = q.limit.unwrap_or(500).min(500);
    let offset = q.offset.unwrap_or(0);
    // 走数据库分页而不是遍历内存缓存：缓存里的通道没有分页能力，
    // 设备多的时候会造成 N+1 请求与响应体膨胀。
    let channels = state
        .app
        .registry
        .channel_service
        .list_channels_paginated_db(
            Some(device_tag.as_str()),
            None,
            status.as_deref(),
            None,
            None,
            None,
            limit,
            offset,
        )
        .await?;
    Ok(Json(ApiResponse::success(channels)))
}
