use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Extension, Json,
};

use crate::api::response::ApiResponse;
use crate::api::state::FullState;
use crate::auth::middleware::CurrentUser;
use crate::auth::models::UserInfo;
use crate::domain::video_tour::{CreateTourSchemeRequest, TourScheme, UpdateTourSchemeRequest};
use crate::error::{AppError, Result};

const TOUR_OBJECT: &str = "tour";

/// 校验当前用户是否具备视频轮询权限；任一角色命中即放行。
async fn require_tour_permission(
    state: &Arc<FullState>,
    user: &UserInfo,
    act: &str,
) -> Result<()> {
    for role in &user.roles {
        if state.auth.casbin.enforce(role, TOUR_OBJECT, act).await {
            return Ok(());
        }
    }
    Err(AppError::Forbidden(format!(
        "权限不足：需要 {}:{}",
        TOUR_OBJECT, act
    )))
}

pub async fn list_tour_schemes_handler(
    State(state): State<Arc<FullState>>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<Vec<TourScheme>>>> {
    require_tour_permission(&state, &user.user, "view").await?;
    let schemes = state.app.registry.tour_scheme_service.list().await?;
    Ok(Json(ApiResponse::success(schemes)))
}

pub async fn get_tour_scheme_handler(
    State(state): State<Arc<FullState>>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<TourScheme>>> {
    require_tour_permission(&state, &user.user, "view").await?;
    let scheme = state
        .app
        .registry
        .tour_scheme_service
        .get(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("轮询方案 {} 不存在", id)))?;
    Ok(Json(ApiResponse::success(scheme)))
}

pub async fn create_tour_scheme_handler(
    State(state): State<Arc<FullState>>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateTourSchemeRequest>,
) -> Result<Json<ApiResponse<TourScheme>>> {
    require_tour_permission(&state, &user.user, "manage").await?;
    let scheme = state.app.registry.tour_scheme_service.create(req).await?;
    Ok(Json(ApiResponse::success(scheme)))
}

pub async fn update_tour_scheme_handler(
    State(state): State<Arc<FullState>>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateTourSchemeRequest>,
) -> Result<Json<ApiResponse<()>>> {
    require_tour_permission(&state, &user.user, "manage").await?;
    state
        .app
        .registry
        .tour_scheme_service
        .update(id, req)
        .await?;
    Ok(Json(ApiResponse::success(())))
}

pub async fn delete_tour_scheme_handler(
    State(state): State<Arc<FullState>>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>> {
    require_tour_permission(&state, &user.user, "manage").await?;
    state.app.registry.tour_scheme_service.delete(id).await?;
    Ok(Json(ApiResponse::success(())))
}
