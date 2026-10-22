use std::sync::Arc;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{ Deserialize };
use crate::api::state::FullState;
use crate::api::response::ApiResponse;
use crate::error::AppError;
use crate::domain::server::{ServerType, ServerProtocolPorts};

#[derive(Debug, Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
    pub url: String,
    pub api_key: String,
    pub server_type: String,
    pub weight: Option<u32>,
    pub server_tag: String,
    #[serde(default)]
    pub protocol_ports: Option<ServerProtocolPorts>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateServerRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub server_type: Option<String>,
    #[serde(default)]
    pub weight: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub server_tag: Option<String>,
    #[serde(default)]
    pub protocol_ports: Option<ServerProtocolPorts>,
}

pub async fn list_servers_handler(
    State(state): State<Arc<FullState>>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, AppError> {
    let registry = &state.app.registry;
    let servers = registry.media_server_service.list();

    let mut result = Vec::new();
    for server in servers.iter() {
        let status = registry.infra.redis.get_server_status(&server.server_tag).await.ok().flatten();
        result.push(serde_json::json!({
            "id": server.id,
            "name": server.name,
            "url": server.url,
            "api_key": server.api_key,
            "server_type": server.server_type,
            "server_tag": server.server_tag,
            "weight": server.weight,
            "enabled": server.enabled,
            "online": status.as_ref().map(|s| s.online).unwrap_or(false),
            "session_count": status.as_ref().map(|s| s.session_count).unwrap_or(0),
            "cpu_usage": status.as_ref().map(|s| s.cpu_usage).unwrap_or(0.0),
            "memory_usage": status.as_ref().map(|s| s.memory_usage).unwrap_or(0.0),
            "protocol_ports": server.protocol_ports,
        }));
    }

    Ok(Json(ApiResponse::success(result)))
}

    pub async fn create_server_handler(
        State(state): State<Arc<FullState>>,
        Json(req): Json<CreateServerRequest>,
    ) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
        let registry = &state.app.registry;

        if req.name.trim().is_empty() {
            return Err(AppError::BadRequest("Server name cannot be empty".to_string()));
        }

        let server_type = ServerType::from_str(&req.server_type)
            .ok_or_else(|| AppError::BadRequest(format!("Invalid server_type: {}", req.server_type)))?;

        let weight = req.weight.unwrap_or(100);
        let protocol_ports = req.protocol_ports.clone().unwrap_or_default();
        let server_tag = req.server_tag.trim().to_string();

        let server = registry.media_server_service.create(
            req.name, req.url, req.api_key, server_type, weight, server_tag, protocol_ports,
        ).await;

        registry.media.cluster.register_server(server.to_config());

        if server.enabled {
            registry.stream_recovery_service.mark_streams_recovering_for_media_server(&server.server_tag).await;
            registry.stream_recovery_service.recover_streams_for_media_server(&server.server_tag).await;
        }

        Ok(Json(ApiResponse::success(serde_json::json!({
            "id": server.id,
            "name": server.name,
            "url": server.url,
            "api_key": server.api_key,
            "server_type": server.server_type,
            "server_tag": server.server_tag,
            "weight": server.weight,
            "enabled": server.enabled,
            "created_at": server.created_at,
        }))))
    }

pub async fn get_server_handler(
    State(state): State<Arc<FullState>>,
    Path(tag): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let registry = &state.app.registry;
    let server = registry.media_server_service.get(&tag)
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", tag)))?;

    let status = registry.media.cluster.get_server_status(&server.server_tag).await;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "id": server.id,
        "name": server.name,
        "url": server.url,
        "api_key": server.api_key,
        "server_type": server.server_type,
        "server_tag": server.server_tag,
        "weight": server.weight,
        "enabled": server.enabled,
        "online": status.as_ref().map(|s| s.online).unwrap_or(false),
        "session_count": status.as_ref().map(|s| s.session_count).unwrap_or(0),
        "cpu_usage": status.as_ref().map(|s| s.cpu_usage).unwrap_or(0.0),
        "memory_usage": status.as_ref().map(|s| s.memory_usage).unwrap_or(0.0),
        "protocol_ports": server.protocol_ports,
    }))))
}

pub async fn update_server_handler(
    State(state): State<Arc<FullState>>,
    Path(tag): Path<String>,
    Json(req): Json<UpdateServerRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let registry = &state.app.registry;
    let old = registry.media_server_service.get(&tag)
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", tag)))?;

    let server_type = match req.server_type.as_deref() {
        Some(st) => Some(
            ServerType::from_str(st)
                .ok_or_else(|| AppError::BadRequest(format!("Invalid server_type: {}", st)))?,
        ),
        None => None,
    };

    let server = registry.media_server_service.update(
        &tag,
        req.name,
        req.url,
        req.api_key,
        server_type,
        req.weight,
        req.enabled,
        req.server_tag.map(|t| t.trim().to_string()),
        req.protocol_ports,
    ).await.ok_or_else(|| AppError::NotFound(format!("Server {} not found", tag)))?;

    registry.media.cluster.unregister_server(&old.name);
    registry.media.cluster.register_server(server.to_config());

    if server.enabled {
        registry.stream_recovery_service.mark_streams_recovering_for_media_server(&server.server_tag).await;
        registry.stream_recovery_service.recover_streams_for_media_server(&server.server_tag).await;
    }

    Ok(Json(ApiResponse::success(serde_json::json!({
        "id": server.id,
        "name": server.name,
        "url": server.url,
        "api_key": server.api_key,
        "server_type": server.server_type,
        "server_tag": server.server_tag,
        "enabled": server.enabled,
    }))))
}

pub async fn delete_server_handler(
    State(state): State<Arc<FullState>>,
    Path(tag): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let registry = &state.app.registry;
    let server = registry.media_server_service.get(&tag)
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", tag)))?;

    registry.media.cluster.unregister_server(&server.name);
    registry.media_server_service.delete(&tag).await;

    Ok(Json(ApiResponse::success(())))
}

pub async fn server_status_handler(
    State(state): State<Arc<FullState>>,
    Path(tag): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let registry = &state.app.registry;
    let server = registry.media_server_service.get(&tag)
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", tag)))?;

    let status = registry.media.cluster.get_server_status(&server.server_tag).await;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "online": status.as_ref().map(|s| s.online).unwrap_or(false),
        "session_count": status.as_ref().map(|s| s.session_count).unwrap_or(0),
        "cpu_usage": status.as_ref().map(|s| s.cpu_usage).unwrap_or(0.0),
        "memory_usage": status.as_ref().map(|s| s.memory_usage).unwrap_or(0.0),
    }))))
}

pub async fn refresh_server_handler(
    State(state): State<Arc<FullState>>,
    Path(tag): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let registry = &state.app.registry;
    let server = registry.media_server_service.get(&tag)
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", tag)))?;

    let status = registry.media.cluster.get_server_status(&server.server_tag).await;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "online": status.as_ref().map(|s| s.online).unwrap_or(false),
        "session_count": status.as_ref().map(|s| s.session_count).unwrap_or(0),
    }))))
}

pub async fn enable_server_handler(
    State(state): State<Arc<FullState>>,
    Path(tag): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let registry = &state.app.registry;
    let updated = registry.media_server_service.update_enabled(&tag, true)
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", tag)))?;
    registry.media.cluster.set_server_enabled(&updated.name, true);

    registry.stream_recovery_service.mark_streams_recovering_for_media_server(&updated.server_tag).await;
    registry.stream_recovery_service.recover_streams_for_media_server(&updated.server_tag).await;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "id": updated.id, "name": updated.name, "enabled": updated.enabled,
    }))))
}

pub async fn disable_server_handler(
    State(state): State<Arc<FullState>>,
    Path(tag): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let registry = &state.app.registry;
    let updated = registry.media_server_service.update_enabled(&tag, false)
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", tag)))?;
    registry.media.cluster.set_server_enabled(&updated.name, false);

    Ok(Json(ApiResponse::success(serde_json::json!({
        "id": updated.id, "name": updated.name, "enabled": updated.enabled,
    }))))
}

pub async fn server_sessions_handler(
    State(state): State<Arc<FullState>>,
    Path(tag): Path<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, AppError> {
    let registry = &state.app.registry;
    if let Some(adapter) = registry.media.cluster.get_server(&tag) {
        let sessions = adapter.get_sessions().await.unwrap_or_default();
        return Ok(Json(ApiResponse::success(sessions)));
    }
    Ok(Json(ApiResponse::success(vec![])))
}