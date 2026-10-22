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
    pub device_tag: String,

    pub channel_tag: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub segment_type: Option<String>,
    pub limit: Option<usize>,
    pub page: Option<usize>,
}

impl RecordingFilesQuery {

    pub fn channel_tag(&self) -> Option<&str> {
        self.channel_tag.as_deref().filter(|s| !s.is_empty())
    }

    pub fn segment_type(&self) -> Option<SegmentType> {
        self.segment_type.as_ref().and_then(|s| {
            SegmentType::from_str(s)
        })
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.start_time >= self.end_time {
            return Err(AppError::BadRequest("start_time must be before end_time".to_string()));
        }
        if let Some(p) = self.page {
            if p < 1 {
                return Err(AppError::BadRequest("page must be >= 1".to_string()));
            }
        }
        if let Some(l) = self.limit {
            if l < 1 || l > 100 {
                return Err(AppError::BadRequest("limit must be 1-100".to_string()));
            }
        }
        if let Some(ref st) = self.segment_type {
            if SegmentType::from_str(st).is_none() {
                return Err(AppError::BadRequest("invalid segment_type".to_string()));
            }
        }
        Ok(())
    }
}

pub async fn list_recording_files_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<RecordingFilesQuery>,
) -> Result<Json<ApiResponse<PaginatedRecordingFiles>>, AppError> {
    let registry = &state.app.registry;

    if q.device_tag.is_empty() {
        return Err(AppError::BadRequest("device_tag is required".to_string()));
    }

    q.validate()?;

    let device_tag = q.device_tag.clone();
    let query = RecordingSegmentQuery {
        device_tag,
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
