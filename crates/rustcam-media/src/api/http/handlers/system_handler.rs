use std::sync::Arc;

use axum::{extract::State, Extension, Json};

use crate::api::response::ApiResponse;
use crate::api::state::FullState;
use crate::auth::middleware::CurrentUser;
use crate::auth::models::UserInfo;
use crate::error::{AppError, Result};

/// casbin 对象名：系统配置（含连接串里的密码）。
const SYSTEM_OBJECT: &str = "system";

/// 把连接串里的密码打码：`postgres://user:secret@host/db` → `postgres://user:******@host/db`。
/// 没有密码（如 `redis://127.0.0.1:6379/`）时原样返回。
pub fn mask_url_secret(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let scheme = &url[..scheme_end + 3];
    let rest = &url[scheme_end + 3..];
    let auth_end = rest.find('/').unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(auth_end);
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return url.to_string();
    };
    let masked_userinfo = match userinfo.split_once(':') {
        Some((user, _password)) => format!("{user}:******"),
        None => userinfo.to_string(),
    };
    format!("{scheme}{masked_userinfo}@{host}{tail}")
}

/// 运行时配置（数据库/Redis 连接串的密码已打码）。
pub async fn get_system_config_handler(
    State(state): State<Arc<FullState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let config = state.app.registry.infra.config();
    Json(ApiResponse::success(serde_json::json!({
        "server": {
            "host": config.server.host,
            "port": config.server.port,
            "tls_enabled": config.server.tls_enabled,
        },
        "database": {
            "url": mask_url_secret(&config.database.url),
            "max_connections": config.database.max_connections,
        },
        "redis": {
            "url": mask_url_secret(&config.redis.url),
            "pool_size": config.redis.pool_size,
        },
    })))
}

/// 运行时配置里的连接串明文（需要 system:config 权限）。
pub async fn get_system_config_secret_handler(
    State(state): State<Arc<FullState>>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<serde_json::Value>>> {
    require_system_permission(&state, &user.user, "config").await?;
    let config = state.app.registry.infra.config();
    Ok(Json(ApiResponse::success(serde_json::json!({
        "database": { "url": config.database.url },
        "redis": { "url": config.redis.url },
    }))))
}

/// 校验当前用户是否具备系统配置相关权限。
async fn require_system_permission(
    state: &Arc<FullState>,
    user: &UserInfo,
    act: &str,
) -> Result<()> {
    for role in &user.roles {
        if state.auth.casbin.enforce(role, SYSTEM_OBJECT, act).await {
            return Ok(());
        }
    }
    Err(AppError::Forbidden(format!(
        "权限不足：需要 {}:{}",
        SYSTEM_OBJECT, act
    )))
}

#[cfg(test)]
mod tests {
    use super::mask_url_secret;

    #[test]
    fn masks_password_in_url() {
        assert_eq!(
            mask_url_secret("postgres://postgres:123456@192.168.1.7:5435/rustcam"),
            "postgres://postgres:******@192.168.1.7:5435/rustcam"
        );
        assert_eq!(
            mask_url_secret("redis://:secret@127.0.0.1:6379/"),
            "redis://:******@127.0.0.1:6379/"
        );
    }

    #[test]
    fn keeps_urls_without_password() {
        assert_eq!(
            mask_url_secret("redis://127.0.0.1:6379/"),
            "redis://127.0.0.1:6379/"
        );
        assert_eq!(
            mask_url_secret("http://127.0.0.1:8080"),
            "http://127.0.0.1:8080"
        );
        assert_eq!(mask_url_secret(""), "");
    }
}
