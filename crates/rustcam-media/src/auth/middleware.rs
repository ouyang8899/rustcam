use std::sync::Arc;

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::{
    auth_db::PostgresAuthRepository,
    casbin::CasbinManager,
    config::{AuthConfig, JwtConfig},
    errors::AuthError,
    jwt::decode_token,
    models::{UserInfo, UserSession},
    session::SessionRepository,
    session_cache::SessionCache,
};

#[derive(Clone)]
pub struct CurrentUser {
    pub user: UserInfo,
    pub sid: Uuid,
}

#[derive(Clone)]
pub struct AuthState {
    pub jwt_config: JwtConfig,
    pub auth_config: AuthConfig,
    pub casbin: Arc<CasbinManager>,
    pub repo: Arc<PostgresAuthRepository>,
    pub session_cache: Arc<SessionCache>,
    pub revoker: broadcast::Sender<Uuid>,
}

impl AuthState {
    pub fn new(
        jwt_config: JwtConfig,
        auth_config: AuthConfig,
        casbin: Arc<CasbinManager>,
        repo: Arc<PostgresAuthRepository>,
    ) -> Self {
        let (revoker, _) = broadcast::channel(256);
        let session_cache = Arc::new(SessionCache::new(auth_config.session_cache_ttl_seconds));
        Self {
            jwt_config,
            auth_config,
            casbin,
            repo,
            session_cache,
            revoker,
        }
    }

    pub async fn is_session_active(&self, sid: Uuid) -> crate::error::Result<bool> {
        if self.session_cache.is_fresh(sid) {
            return Ok(true);
        }
        let active = SessionRepository::find_active(&*self.repo, sid)
            .await?
            .is_some();
        if active {
            self.session_cache.mark(sid);
        }
        Ok(active)
    }

    pub async fn load_active_session(&self, sid: Uuid) -> crate::error::Result<Option<UserSession>> {
        let session = SessionRepository::find_active(&*self.repo, sid).await?;
        if session.is_some() {
            self.session_cache.mark(sid);
        }
        Ok(session)
    }

    pub async fn revoke_session(&self, sid: Uuid) -> crate::error::Result<bool> {
        let revoked = SessionRepository::revoke(&*self.repo, sid).await?;
        self.session_cache.remove(sid);
        if revoked {
            let _ = self.revoker.send(sid);
        }
        Ok(revoked)
    }
}

pub async fn jwt_auth_layer(
    State(auth_state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Response {
    let auth_header = req.headers().get("Authorization").and_then(|v| v.to_str().ok());

    let auth_header = match auth_header {
        Some(h) if h.starts_with("Bearer ") => h,
        _ => {
            return AuthError::Unauthorized("Missing or invalid Authorization header".to_string())
                .into_response()
        }
    };

    let token = &auth_header[7..];
    let claims = match decode_token(token, &auth_state.jwt_config.secret) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };

    if claims.token_type != "access" {
        return AuthError::InvalidToken("Invalid token type".to_string()).into_response();
    }

    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(_) => return AuthError::InvalidToken("Invalid user ID".to_string()).into_response(),
    };

    let sid = match Uuid::parse_str(&claims.sid) {
        Ok(id) => id,
        Err(_) => return AuthError::Unauthorized("Missing session id".to_string()).into_response(),
    };

    match auth_state.is_session_active(sid).await {
        Ok(true) => {}
        Ok(false) => {
            return AuthError::Unauthorized("Session revoked or expired".to_string())
                .into_response()
        }
        Err(e) => return AuthError::Unauthorized(e.to_string()).into_response(),
    }

    let user_info = UserInfo {
        id: user_id,
        username: claims.username,
        roles: claims.roles,
        is_super: claims.is_super,
    };
    req.extensions_mut().insert(CurrentUser { user: user_info, sid });
    next.run(req).await
}
