use std::sync::Arc;
use axum::{
    extract::{Path, State, Query},
    Json,
};
use serde::Deserialize;
use chrono::{DateTime, Utc};
use crate::api::response::ApiResponse;
use crate::api::state::FullState;
use crate::error::AppError;
use crate::domain::recording_policy::{
    RecordingSegmentQuery, SegmentType, PaginatedRecordingFiles, PlayUrlResponse,
};

#[derive(Debug, Deserialize)]
pub struct RecordingFilesQuery {
    /// 设备标识，为空时列出全部设备的录像文件
    pub device_tag: Option<String>,

    pub channel_tag: Option<String>,
    /// 录像开始时间，为空时不按时间过滤
    pub start_time: Option<DateTime<Utc>>,
    /// 录像结束时间，为空时不按时间过滤
    pub end_time: Option<DateTime<Utc>>,
    pub segment_type: Option<String>,
    pub limit: Option<usize>,
    pub page: Option<usize>,
}

impl RecordingFilesQuery {

    pub fn channel_tag(&self) -> Option<&str> {
        self.channel_tag.as_deref().filter(|s| !s.is_empty())
    }

    pub fn device_tag(&self) -> Option<&str> {
        self.device_tag.as_deref().filter(|s| !s.is_empty())
    }

    pub fn segment_type(&self) -> Option<SegmentType> {
        self.segment_type.as_ref().and_then(|s| {
            SegmentType::from_str(s)
        })
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            if start >= end {
                return Err(AppError::BadRequest("start_time must be before end_time".to_string()));
            }
        }
        if let Some(p) = self.page {
            if p < 1 {
                return Err(AppError::BadRequest("page must be >= 1".to_string()));
            }
        }
        if let Some(l) = self.limit {
            if l < 1 || l > 500 {
                return Err(AppError::BadRequest("limit must be 1-500".to_string()));
            }
        }
        if self.segment_type.is_some() && self.segment_type().is_none() {
            return Err(AppError::BadRequest("invalid segment_type".to_string()));
        }
        Ok(())
    }
}

pub async fn list_recording_files_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<RecordingFilesQuery>,
) -> Result<Json<ApiResponse<PaginatedRecordingFiles>>, AppError> {
    let registry = &state.app.registry;

    q.validate()?;

    let query = RecordingSegmentQuery {
        device_tag: q.device_tag().map(|s| s.to_string()),
        channel_tag: q.channel_tag().map(|s| s.to_string()),
        start_time: q.start_time,
        end_time: q.end_time,
        segment_type: q.segment_type(),
        limit: q.limit,
        page: q.page,
    };

    let result = registry.recording_policy_service.list_recording_segments(query).await?;

    Ok(Json(ApiResponse::success(result)))
}

pub async fn get_recording_file_play_url_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<PlayUrlResponse>>, AppError> {
    let registry = &state.app.registry;

    let play_url = registry.recording_policy_service.get_play_url(&id).await?;

    Ok(Json(ApiResponse::success(play_url)))
}
