use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::auth::casbin::CasbinManager;
use crate::auth::middleware::CurrentUser;
use crate::auth::models::UserInfo;
use crate::drone_gateway::permissions::{self, PERM_OBJECT};
use crate::error::AppError;

/// 无人机 REST 鉴权中间件。
///
/// 以 `route_layer` 方式挂在无人机路由上，因此**先于请求体解析**执行：
/// 未认证返回 401，权限不足返回 403，未登记权限点的路由一律拒绝。
pub async fn authorize(mut req: axum::extract::Request, next: Next) -> Response {
    let Some(casbin) = req.extensions().get::<Arc<CasbinManager>>().cloned() else {
        return AppError::Internal("无人机鉴权缺少 Casbin 管理器".to_string()).into_response();
    };
    let Some(user) = req.extensions().get::<CurrentUser>().cloned() else {
        return AppError::Auth("未认证：请求缺少用户身份".to_string()).into_response();
    };

    let path = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let method = req.method().as_str().to_string();

    let acts = match permissions::required_permissions(&path, &method) {
        Some(acts) => acts,
        None if permissions::is_drone_path(&path) => {
            return AppError::Forbidden(format!("权限不足：路由 {method} {path} 未登记权限点"))
                .into_response()
        }
        None => {
            let _ = &mut req;
            return next.run(req).await;
        }
    };

    if !any_allowed(&casbin, &user.user, acts).await {
        return AppError::Forbidden(format!(
            "权限不足：需要 {}:{}",
            PERM_OBJECT,
            acts.join(" 或 ")
        ))
        .into_response();
    }

    next.run(req).await
}

/// 任一权限点命中即放行。
pub async fn any_allowed(casbin: &CasbinManager, user: &UserInfo, acts: &[&str]) -> bool {
    for act in acts {
        if require_permission(casbin, user, act).await.is_ok() {
            return true;
        }
    }
    false
}

/// 带身份的用户提取器。
///
/// 请求上下文中没有 `CurrentUser` 时返回 401，绝不回落为默认账号。
pub struct DroneUser(pub UserInfo);

impl<S: Send + Sync> FromRequestParts<S> for DroneUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<CurrentUser>()
            .map(|u| DroneUser(u.user.clone()))
            .ok_or_else(|| AppError::Auth("未认证：请求缺少用户身份".to_string()))
    }
}

/// 校验用户是否具备指定无人机权限点；任一角色命中即放行。
pub async fn require_permission(
    casbin: &CasbinManager,
    user: &UserInfo,
    act: &str,
) -> Result<(), AppError> {
    for role in &user.roles {
        if casbin.enforce(role, PERM_OBJECT, act).await {
            return Ok(());
        }
    }
    Err(AppError::Forbidden(format!(
        "权限不足：需要 {}:{}",
        PERM_OBJECT, act
    )))
}

/// 计算用户在 WebSocket 上允许订阅的消息类型。
///
/// `drone:view` 覆盖告警/控制权/指令回执，`drone:telemetry:read` 覆盖遥测。
pub async fn ws_scope(casbin: &CasbinManager, user: &UserInfo) -> Vec<&'static str> {
    let mut scope = Vec::new();
    for role in &user.roles {
        if casbin.enforce(role, PERM_OBJECT, permissions::VIEW).await {
            for msg_type in ["alert", "control", "command"] {
                if !scope.contains(&msg_type) {
                    scope.push(msg_type);
                }
            }
        }
        if casbin
            .enforce(role, PERM_OBJECT, permissions::TELEMETRY_READ)
            .await
            && !scope.contains(&"telemetry")
        {
            scope.push("telemetry");
        }
    }
    scope
}
