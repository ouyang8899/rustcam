use std::sync::Arc;
use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use argon2::{
    Argon2, PasswordHash, PasswordVerifier,
};
use password_hash::phc::SaltString;
use password_hash::PasswordHasher;
use serde::Deserialize;
use uuid::Uuid;
use crate::api::FullState;
use crate::api::http::handlers::device_handler::PaginatedResponse;
use crate::error::{AppError, Result as AppResult};
use crate::auth::models::*;
use crate::auth::middleware::CurrentUser;
use crate::auth::repository::{UserRepository, RoleRepository, PermissionRepository};

/// 列表分页参数。
///
/// 与设备/流等列表同一套口径：默认每页 50 条，最多 500 条。
#[derive(Debug, Deserialize)]
pub struct PageQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl PageQuery {
    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(50).min(500)
    }
    pub fn offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

/// 校验当前用户是否具备指定权限；任一角色命中即放行。
async fn require_permission(
    state: &Arc<FullState>,
    current_user: &CurrentUser,
    obj: &str,
    act: &str,
) -> AppResult<()> {
    for role in &current_user.0.roles {
        if state.auth.casbin.enforce(role, obj, act).await {
            return Ok(());
        }
    }
    Err(AppError::Auth(format!(
        "Permission denied: {}:{}",
        obj, act
    )))
}

/// 角色/权限变更后立即刷新内存策略，避免权限改动延迟生效。
async fn reload_casbin(state: &Arc<FullState>) -> AppResult<()> {
    state
        .auth
        .casbin
        .reload_from_db(state.auth.repo.as_ref())
        .await
        .map_err(|e| AppError::Internal(format!("Failed to reload casbin policy: {}", e)))
}

pub async fn login(
    State(state): State<Arc<FullState>>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<ApiResponse<LoginResponse>>> {

    let user = UserRepository::find_by_username(&*state.auth.repo, &req.username)
        .await
        .map_err(|e| {
            AppError::Auth(e.to_string())
        })?
        .ok_or_else(|| {
            AppError::Auth("Invalid credentials".to_string())
        })?;

    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|e| {
            AppError::Auth(format!("Password hash error: {}", e))
        })?;

    let password_bytes = req.password.as_bytes().to_vec();
    let password_valid = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .verify_password(&password_bytes, &parsed_hash)
            .is_ok()
    })
    .await
    .unwrap_or(false);

    if !password_valid {
        return Err(AppError::Auth("Invalid credentials".to_string()));
    }

    let roles = UserRepository::get_user_roles(&*state.auth.repo, user.id)
        .await
        .map_err(|e| {
            AppError::Auth(e.to_string())
        })?;

    let user_info = UserInfo {
        id: user.id,
        username: user.username.clone(),
        roles: roles.clone(),
    };

    let (access_token, refresh_token) =
        crate::auth::jwt::generate_tokens(&user_info, &state.auth.jwt_config)
            .map_err(|e| {
                AppError::Auth(e.to_string())
            })?;

    Ok(Json(ApiResponse::success(LoginResponse {
        access_token,
        refresh_token,
        expires_in: state.auth.jwt_config.access_token_expire_minutes * 60,
        token_type: "Bearer".to_string(),
        user: user_info,
    })))
}

pub async fn refresh(
    State(state): State<Arc<FullState>>,
    Json(req): Json<RefreshRequest>,
) -> AppResult<Json<ApiResponse<LoginResponse>>> {
    let claims = crate::auth::jwt::decode_token(&req.refresh_token, &state.auth.jwt_config.secret)
        .map_err(|e| AppError::Auth(e.to_string()))?;

    if claims.token_type != "refresh" {
        return Err(AppError::Auth("Invalid token type".to_string()));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Auth("Invalid user ID".to_string()))?;

    let user = UserRepository::find_by_id(&*state.auth.repo, user_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::Auth("User not found".to_string()))?;

    let roles = UserRepository::get_user_roles(&*state.auth.repo, user_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let user_info = UserInfo { id: user.id, username: user.username.clone(), roles };
    let (access_token, refresh_token) = crate::auth::jwt::generate_tokens(&user_info, &state.auth.jwt_config)
        .map_err(|e| AppError::Auth(e.to_string()))?;

    Ok(Json(ApiResponse::success(LoginResponse {
        access_token, refresh_token,
        expires_in: state.auth.jwt_config.access_token_expire_minutes * 60,
        token_type: "Bearer".to_string(), user: user_info,
    })))
}

pub async fn me(
    Extension(current_user): Extension<CurrentUser>,
) -> Json<ApiResponse<UserInfo>> {
    Json(ApiResponse::success(current_user.0))
}

pub async fn list_users(
    State(state): State<Arc<FullState>>,
    Extension(current_user): Extension<CurrentUser>,
    Query(q): Query<PageQuery>,
) -> AppResult<Json<ApiResponse<PaginatedResponse<UserWithRoles>>>> {

    require_permission(&state, &current_user, "user", "read").await?;

    let total = UserRepository::count(&*state.auth.repo)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let users = UserRepository::list_paginated(&*state.auth.repo, q.offset(), q.limit())
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let mut result = Vec::with_capacity(users.len());
    for user in users {
        let roles = UserRepository::get_user_roles(&*state.auth.repo, user.id)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?;
        let role_ids = UserRepository::get_user_role_ids(&*state.auth.repo, user.id)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?;

        let mut user_safe = user;
        user_safe.password_hash = "***".to_string();

        result.push(UserWithRoles {
            user: user_safe,
            roles,
            role_ids,
        });
    }

    Ok(Json(ApiResponse::success(PaginatedResponse {
        items: result,
        total,
        limit: q.limit(),
        offset: q.offset(),
    })))
}

pub async fn create_user(
    State(state): State<Arc<FullState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<Json<ApiResponse<User>>> {

    require_permission(&state, &current_user, "user", "write").await?;

    if UserRepository::find_by_username(&*state.auth.repo, &req.username)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .is_some()
    {
        return Err(AppError::BadRequest("Username already exists".to_string()));
    }

    let _salt = SaltString::generate();
    let pwd_bytes = req.password.as_bytes().to_vec();
    let password_hash = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(&pwd_bytes)
            .map(|h| h.to_string())
    })
    .await
    .map_err(|e| AppError::BadRequest(format!("Password hash error: {}", e)))?
    .map_err(|e| AppError::BadRequest(format!("Password hash error: {}", e)))?;

    let mut user = User::new(req.username, password_hash);
    UserRepository::create(&*state.auth.repo, &user)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    if let Some(role_ids) = req.role_ids {
        UserRepository::set_user_roles(&*state.auth.repo, user.id, &role_ids)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?;
    }

    user.password_hash = "***".to_string();
    Ok(Json(ApiResponse::success(user)))
}

pub async fn get_user(
    State(state): State<Arc<FullState>>,
    Path(user_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
) -> AppResult<Json<ApiResponse<UserWithRoles>>> {

    require_permission(&state, &current_user, "user", "read").await?;

    let user = UserRepository::find_by_id(&*state.auth.repo, user_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let role_names = UserRepository::get_user_roles(&*state.auth.repo, user_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;
    let role_ids = UserRepository::get_user_role_ids(&*state.auth.repo, user_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let mut user_safe = user;
    user_safe.password_hash = "***".to_string();

    Ok(Json(ApiResponse::success(UserWithRoles {
        user: user_safe,
        roles: role_names,
        role_ids,
    })))
}

pub async fn update_user(
    State(state): State<Arc<FullState>>,
    Path(user_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<UpdateUserRequest>,
) -> AppResult<Json<ApiResponse<User>>> {

    require_permission(&state, &current_user, "user", "write").await?;

    let mut user = UserRepository::find_by_id(&*state.auth.repo, user_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    if let Some(username) = &req.username {
        if let Some(existing) = UserRepository::find_by_username(&*state.auth.repo, username)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?
        {
            if existing.id != user_id {
                return Err(AppError::BadRequest("Username already exists".to_string()));
            }
        }
        user.username = username.clone();
    }

    if let Some(password) = req.password {
        let _salt = SaltString::generate();
        let pwd_bytes = password.as_bytes().to_vec();
        user.password_hash = tokio::task::spawn_blocking(move || {
            Argon2::default()
                .hash_password(&pwd_bytes)
                .map(|h| h.to_string())
        })
        .await
        .map_err(|e| AppError::BadRequest(format!("Password hash error: {}", e)))?
        .map_err(|e| AppError::BadRequest(format!("Password hash error: {}", e)))?;
    }

    user.updated_at = chrono::Utc::now();

    UserRepository::update(&*state.auth.repo, &user)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    user.password_hash = "***".to_string();
    Ok(Json(ApiResponse::success(user)))
}

pub async fn delete_user(
    State(state): State<Arc<FullState>>,
    Path(user_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
) -> AppResult<Json<ApiResponse<()>>> {

    require_permission(&state, &current_user, "user", "write").await?;

    UserRepository::delete(&*state.auth.repo, user_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    Ok(Json(ApiResponse::success(())))
}

pub async fn assign_user_roles(
    State(state): State<Arc<FullState>>,
    Path(user_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<AssignRolesRequest>,
) -> AppResult<Json<ApiResponse<Vec<String>>>> {
    require_permission(&state, &current_user, "user", "write").await?;

    for role_id in &req.role_ids {
        if RoleRepository::find_by_id(&*state.auth.repo, *role_id)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?
            .is_none()
        {
            return Err(AppError::BadRequest(format!(
                "Role not found: {}",
                role_id
            )));
        }
    }

    UserRepository::set_user_roles(&*state.auth.repo, user_id, &req.role_ids)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let roles = UserRepository::get_user_roles(&*state.auth.repo, user_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    Ok(Json(ApiResponse::success(roles)))
}

pub async fn list_roles(
    State(state): State<Arc<FullState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> AppResult<Json<ApiResponse<Vec<Role>>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let roles = RoleRepository::list(&*state.auth.repo)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    Ok(Json(ApiResponse::success(roles)))
}

pub async fn get_role(
    State(state): State<Arc<FullState>>,
    Path(role_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
) -> AppResult<Json<ApiResponse<RoleDetail>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let role = RoleRepository::find_by_id(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    let permissions = RoleRepository::get_role_permissions(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let parent_roles = RoleRepository::get_role_parents(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    Ok(Json(ApiResponse::success(RoleDetail {
        role,
        permissions,
        parent_roles,
    })))
}

pub async fn create_role(
    State(state): State<Arc<FullState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateRoleRequest>,
) -> AppResult<Json<ApiResponse<Role>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Role name is empty".to_string()));
    }

    if RoleRepository::find_by_name(&*state.auth.repo, &name)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .is_some()
    {
        return Err(AppError::BadRequest(
            "Role name already exists".to_string(),
        ));
    }

    let role = Role::new(name, req.description);

    RoleRepository::create(&*state.auth.repo, &role)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    if let Some(perm_ids) = req.permission_ids {
        RoleRepository::set_role_permissions(&*state.auth.repo, role.id, &perm_ids)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?;
    }

    reload_casbin(&state).await?;

    Ok(Json(ApiResponse::success(role)))
}

pub async fn update_role(
    State(state): State<Arc<FullState>>,
    Path(role_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<UpdateRoleRequest>,
) -> AppResult<Json<ApiResponse<Role>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let mut role = RoleRepository::find_by_id(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    if let Some(name) = req.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::BadRequest("Role name is empty".to_string()));
        }
        if let Some(existing) = RoleRepository::find_by_name(&*state.auth.repo, &name)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?
        {
            if existing.id != role_id {
                return Err(AppError::BadRequest(
                    "Role name already exists".to_string(),
                ));
            }
        }
        role.name = name;
    }

    if let Some(description) = req.description {
        role.description = Some(description);
    }

    role.updated_at = chrono::Utc::now();

    RoleRepository::update(&*state.auth.repo, &role)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    reload_casbin(&state).await?;

    Ok(Json(ApiResponse::success(role)))
}

pub async fn delete_role(
    State(state): State<Arc<FullState>>,
    Path(role_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
) -> AppResult<Json<ApiResponse<()>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let role = RoleRepository::find_by_id(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    if role.name == "Admin" {
        return Err(AppError::BadRequest(
            "Admin role cannot be deleted".to_string(),
        ));
    }

    RoleRepository::delete(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    reload_casbin(&state).await?;

    Ok(Json(ApiResponse::success(())))
}

pub async fn set_role_permissions(
    State(state): State<Arc<FullState>>,
    Path(role_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<SetPermissionsRequest>,
) -> AppResult<Json<ApiResponse<()>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    if RoleRepository::find_by_id(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .is_none()
    {
        return Err(AppError::NotFound("Role not found".to_string()));
    }

    for permission_id in &req.permission_ids {
        if PermissionRepository::find_by_id(&*state.auth.repo, *permission_id)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?
            .is_none()
        {
            return Err(AppError::BadRequest(format!(
                "Permission not found: {}",
                permission_id
            )));
        }
    }

    RoleRepository::set_role_permissions(&*state.auth.repo, role_id, &req.permission_ids)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    reload_casbin(&state).await?;

    Ok(Json(ApiResponse::success(())))
}

/// 设置角色继承的父角色：子角色获得父角色的全部权限。
pub async fn set_role_parents(
    State(state): State<Arc<FullState>>,
    Path(role_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<SetRoleParentsRequest>,
) -> AppResult<Json<ApiResponse<Vec<Role>>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let role = RoleRepository::find_by_id(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Role not found".to_string()))?;

    if req.parent_role_ids.contains(&role_id) {
        return Err(AppError::BadRequest(
            "A role cannot inherit itself".to_string(),
        ));
    }

    let mut parent_names = Vec::with_capacity(req.parent_role_ids.len());
    for parent_id in &req.parent_role_ids {
        let parent = RoleRepository::find_by_id(&*state.auth.repo, *parent_id)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?
            .ok_or_else(|| {
                AppError::BadRequest(format!("Parent role not found: {}", parent_id))
            })?;
        parent_names.push(parent.name);
    }

    let existing = RoleRepository::list_role_inheritance(&*state.auth.repo)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let mut edges: Vec<(String, String)> = existing
        .into_iter()
        .filter(|(child, _)| child != &role.name)
        .collect();
    for parent_name in &parent_names {
        edges.push((role.name.clone(), parent_name.clone()));
    }

    crate::auth::casbin::validate_role_inheritance(&edges)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    RoleRepository::set_role_parents(&*state.auth.repo, role_id, &req.parent_role_ids)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    reload_casbin(&state).await?;

    let parent_roles = RoleRepository::get_role_parents(&*state.auth.repo, role_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    Ok(Json(ApiResponse::success(parent_roles)))
}

pub async fn list_permissions(
    State(state): State<Arc<FullState>>,
    Extension(current_user): Extension<CurrentUser>,
    Query(q): Query<PageQuery>,
) -> AppResult<Json<ApiResponse<PaginatedResponse<Permission>>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let total = PermissionRepository::count(&*state.auth.repo)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    let permissions = PermissionRepository::list_paginated(&*state.auth.repo, q.offset(), q.limit())
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    Ok(Json(ApiResponse::success(PaginatedResponse {
        items: permissions,
        total,
        limit: q.limit(),
        offset: q.offset(),
    })))
}

pub async fn create_permission(
    State(state): State<Arc<FullState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreatePermissionRequest>,
) -> AppResult<Json<ApiResponse<Permission>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let name = req.name.trim().to_string();
    crate::auth::casbin::parse_permission(&name)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    if PermissionRepository::find_by_name(&*state.auth.repo, &name)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .is_some()
    {
        return Err(AppError::BadRequest(
            "Permission name already exists".to_string(),
        ));
    }

    let permission = Permission::new(name, req.description);

    PermissionRepository::create(&*state.auth.repo, &permission)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    reload_casbin(&state).await?;

    Ok(Json(ApiResponse::success(permission)))
}

pub async fn update_permission(
    State(state): State<Arc<FullState>>,
    Path(permission_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<UpdatePermissionRequest>,
) -> AppResult<Json<ApiResponse<Permission>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    let mut permission = PermissionRepository::find_by_id(&*state.auth.repo, permission_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Permission not found".to_string()))?;

    if let Some(name) = req.name {
        let name = name.trim().to_string();
        crate::auth::casbin::parse_permission(&name)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        if let Some(existing) = PermissionRepository::find_by_name(&*state.auth.repo, &name)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?
        {
            if existing.id != permission_id {
                return Err(AppError::BadRequest(
                    "Permission name already exists".to_string(),
                ));
            }
        }
        permission.name = name;
    }

    if let Some(description) = req.description {
        permission.description = Some(description);
    }

    PermissionRepository::update(&*state.auth.repo, &permission)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    reload_casbin(&state).await?;

    Ok(Json(ApiResponse::success(permission)))
}

pub async fn delete_permission(
    State(state): State<Arc<FullState>>,
    Path(permission_id): Path<Uuid>,
    Extension(current_user): Extension<CurrentUser>,
) -> AppResult<Json<ApiResponse<()>>> {

    require_permission(&state, &current_user, "role", "manage").await?;

    PermissionRepository::delete(&*state.auth.repo, permission_id)
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;

    reload_casbin(&state).await?;

    Ok(Json(ApiResponse::success(())))
}