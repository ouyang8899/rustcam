use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub auto_init_schema: bool,
    #[serde(default)]
    pub max_sessions_per_user: usize,
    #[serde(default)]
    pub max_sessions_per_client_type: usize,
    #[serde(default = "default_session_limit_policy")]
    pub session_limit_policy: String,
    #[serde(default = "default_session_cache_ttl_seconds")]
    pub session_cache_ttl_seconds: u64,
    #[serde(default = "default_ws_ticket_ttl_seconds")]
    pub ws_ticket_ttl_seconds: u64,
}

fn default_session_limit_policy() -> String {
    "evict".to_string()
}

fn default_session_cache_ttl_seconds() -> u64 {
    30
}

fn default_ws_ticket_ttl_seconds() -> u64 {
    60
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auto_init_schema: false,
            max_sessions_per_user: 0,
            max_sessions_per_client_type: 0,
            session_limit_policy: default_session_limit_policy(),
            session_cache_ttl_seconds: default_session_cache_ttl_seconds(),
            ws_ticket_ttl_seconds: default_ws_ticket_ttl_seconds(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JwtConfig {
    pub secret: String,
    pub access_token_expire_minutes: i64,
    pub refresh_token_expire_days: i64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-change-in-production".to_string()),
            access_token_expire_minutes: 60,
            refresh_token_expire_days: 30,
        }
    }
}
