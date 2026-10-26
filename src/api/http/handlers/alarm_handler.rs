use std::sync::Arc;
use chrono::{DateTime, Utc};
use axum::{
    extract::{Path, State, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use crate::api::state::FullState;
use crate::api::response::ApiResponse;
use crate::error::AppError;
use crate::domain::alarm::{ Alarm };

#[derive(Debug, Deserialize)]
pub struct AlarmQuery {
    pub device_id: Option<i64>,
    /// 设备标识过滤（客户端回放用，与录像查询口径一致）
    pub device_tag: Option<String>,
    /// 通道标识过滤
    pub channel_tag: Option<String>,
    /// 报警类型过滤
    pub alarm_type: Option<String>,
    /// 时间范围（RFC3339）
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

impl AlarmQuery {
    pub fn limit(&self) -> usize { self.page_size.unwrap_or(20).min(100) }
    pub fn offset(&self) -> usize { self.page.unwrap_or(1).saturating_sub(1) * self.limit() }
    pub fn device_tag(&self) -> Option<&str> { self.device_tag.as_deref().filter(|s| !s.is_empty()) }
    pub fn channel_tag(&self) -> Option<&str> { self.channel_tag.as_deref().filter(|s| !s.is_empty()) }
    pub fn alarm_type(&self) -> Option<&str> { self.alarm_type.as_deref().filter(|s| !s.is_empty()) }
    pub fn has_filter(&self) -> bool {
        self.device_tag().is_some() || self.channel_tag().is_some() || self.alarm_type().is_some()
            || self.start_time.is_some() || self.end_time.is_some()
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedAlarms {
    pub items: Vec<Alarm>,
    pub total: i64,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct MarkProcessedRequest {
    pub processed: bool,
}

pub async fn list_alarms_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<AlarmQuery>,
) -> Result<Json<ApiResponse<PaginatedAlarms>>, AppError> {
    let registry = &state.app.registry;
    let (alarms, total) = if q.has_filter() {
        let alarms = registry.infra.db.list_alarms_filtered(
            q.device_tag(),
            q.channel_tag(),
            q.alarm_type(),
            q.start_time,
            q.end_time,
            q.limit(),
            q.offset(),
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        let total = registry.infra.db.count_alarms_filtered(
            q.device_tag(),
            q.channel_tag(),
            q.alarm_type(),
            q.start_time,
            q.end_time,
        ).await.map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        (alarms, total)
    } else {
        let alarms = registry.infra.db.list_alarms(q.device_id, q.limit(), q.offset()).await
            .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        let total = registry.infra.db.count_alarms(q.device_id).await
            .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
        (alarms, total)
    };

    Ok(Json(ApiResponse::success(PaginatedAlarms {
        items: alarms,
        total,
        page: q.page.unwrap_or(1),
        page_size: q.limit(),
    })))
}

pub async fn mark_alarm_processed_handler(
    State(state): State<Arc<FullState>>,
    Path(id): Path<i64>,
    Json(req): Json<MarkProcessedRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let registry = &state.app.registry;

    if registry.infra.db.get_alarm(id).await?.is_none() {
        return Err(AppError::NotFound(format!("Alarm {} not found", id)));
    }

    registry.infra.db.mark_alarm_processed(id, req.processed).await
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?;
    Ok(Json(ApiResponse::success(())))
}