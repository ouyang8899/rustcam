use std::sync::Arc;
use axum::{extract::State, extract::Query, Json};
use serde::Deserialize;
use crate::api::response::ApiResponse;
use crate::api::http::handlers::device_handler::{PaginatedResponse, SessionWithMediaServer};
use crate::api::state::FullState;
use crate::error::AppError;

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search: Option<String>,
    pub state: Option<String>,
}

impl PaginationQuery {
    pub fn limit(&self) -> usize { self.limit.unwrap_or(50).min(500) }
    pub fn offset(&self) -> usize { self.offset.unwrap_or(0) }
    pub fn search(&self) -> Option<&str> { self.search.as_deref() }
    pub fn state(&self) -> Option<&str> { self.state.as_deref() }
}

pub async fn list_sessions_handler(
    State(state): State<Arc<FullState>>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<ApiResponse<PaginatedResponse<SessionWithMediaServer>>>, AppError> {
    let registry = &state.app.registry;
    let sessions = registry.session_service.list_sessions_paginated(
        q.search(), q.state(), q.limit(), q.offset()
    ).await;
    let total = registry.session_service.count_sessions(q.search(), q.state()).await;

    Ok(Json(ApiResponse::success(PaginatedResponse {
        items: sessions.into_iter().map(|s| SessionWithMediaServer::from_session(s, registry)).collect(),
        total,
        limit: q.limit(),
        offset: q.offset(),
    })))
}