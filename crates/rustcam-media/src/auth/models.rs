use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    /// 超级用户标志：置位后自动拥有全部权限，且只有超级用户能改这个标志。
    #[serde(default)]
    pub is_super: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(username: String, password_hash: String) -> Self {
        let now = Utc::now();
        Self { id: Uuid::new_v4(), username, password_hash, is_super: false, created_at: now, updated_at: now }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub roles: Vec<String>,
    /// 超级用户标志（随令牌下发，标志变更后强制重新登录以生效）。
    #[serde(default)]
    pub is_super: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Role {
    pub fn new(name: String, description: Option<String>) -> Self {
        let now = Utc::now();
        Self { id: Uuid::new_v4(), name, description, created_at: now, updated_at: now }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Permission {
    pub fn new(name: String, description: Option<String>) -> Self {
        Self { id: Uuid::new_v4(), name, description, created_at: Utc::now() }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub client_type: Option<String>,
    #[serde(default)]
    pub client_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String, pub refresh_token: String,
    pub expires_in: i64, pub token_type: String, pub user: UserInfo,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest { pub refresh_token: String }

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String, pub password: String, pub role_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserRequest { pub username: Option<String>, pub password: Option<String> }

#[derive(Debug, Serialize, Deserialize)]
pub struct AssignRolesRequest { pub role_ids: Vec<Uuid> }

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String, pub description: Option<String>, pub permission_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub name: Option<String>, pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetPermissionsRequest { pub permission_ids: Vec<Uuid> }

#[derive(Debug, Deserialize)]
pub struct SetRoleParentsRequest { pub parent_role_ids: Vec<Uuid> }

#[derive(Debug, Deserialize)]
pub struct CreatePermissionRequest { pub name: String, pub description: Option<String> }

#[derive(Debug, Deserialize)]
pub struct UpdatePermissionRequest {
    pub name: Option<String>, pub description: Option<String>,
}

/// 角色详情：角色本身 + 已授权权限 + 直接继承的父角色。
#[derive(Debug, Deserialize)]
pub struct SetSuperRequest {
    pub is_super: bool,
}

#[derive(Debug, Serialize)]
pub struct RoleDetail {
    pub role: Role,
    pub permissions: Vec<String>,
    pub parent_roles: Vec<Role>,
}

#[derive(Debug, Serialize)]
pub struct UserWithRoles {
    #[serde(flatten)]
    pub user: User,
    pub roles: Vec<String>,
    pub role_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub code: u16, pub message: String, pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self { Self { code: 200, message: "success".to_string(), data: Some(data) } }
    pub fn error(code: u16, message: &str) -> Self { Self { code, message: message.to_string(), data: None } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub client_type: String,
    pub client_name: Option<String>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
}

/// 带用户名的会话，供管理员查看全部会话时使用。
#[derive(Debug, Clone)]
pub struct UserSessionWithUser {
    pub session: UserSession,
    pub username: String,
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub client_type: String,
    pub client_name: Option<String>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
}