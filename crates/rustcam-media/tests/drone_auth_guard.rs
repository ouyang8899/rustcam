use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use rustcam_media::auth::casbin::CasbinManager;
use rustcam_media::auth::config::JwtConfig;
use rustcam_media::auth::middleware::{jwt_auth_layer, AuthState};
use rustcam_media::auth::models::{Permission, Role};
use rustcam_media::auth::repository::{
    InMemoryAuthRepository, PermissionRepository, RoleRepository,
};
use rustcam_media::drone_gateway::api::router;
use rustcam_media::drone_gateway::client::{
    AdapterDto, AlertDto, AuditDto, CommandDto, CommandRequest, ControlDto, DroneApi,
    DroneApiError, DroneDto, EventStream, FenceDto, RouteDto, TelemetryDto,
};
use rustcam_media::drone_gateway::ticket::TicketStore;

struct FakeDroneApi;

#[async_trait]
impl DroneApi for FakeDroneApi {
    async fn ping(&self) -> Result<(), DroneApiError> {
        Ok(())
    }
    async fn list_drones(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        Ok(vec![DroneDto {
            drone_tag: "drone-a".into(),
            ..Default::default()
        }])
    }
    async fn get_drone(&self, drone_tag: &str) -> Result<DroneDto, DroneApiError> {
        Ok(DroneDto {
            drone_tag: drone_tag.into(),
            ..Default::default()
        })
    }
    async fn save_drone(
        &self,
        drone: DroneDto,
        _operator: &str,
    ) -> Result<DroneDto, DroneApiError> {
        Ok(drone)
    }
    async fn delete_drone(&self, _drone_tag: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn list_drone_profiles(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        Ok(vec![DroneDto {
            drone_tag: "drone-a".into(),
            ..Default::default()
        }])
    }
    async fn list_adapters(&self, _include_disabled: bool) -> Result<Vec<AdapterDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn get_adapter(&self, adapter_id: &str) -> Result<AdapterDto, DroneApiError> {
        Ok(AdapterDto {
            adapter_id: adapter_id.into(),
            ..Default::default()
        })
    }
    async fn save_adapter(
        &self,
        adapter: AdapterDto,
        _operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        Ok(adapter)
    }
    async fn set_adapter_enabled(
        &self,
        adapter_id: &str,
        enabled: bool,
        _operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        Ok(AdapterDto {
            adapter_id: adapter_id.into(),
            enabled,
            ..Default::default()
        })
    }
    async fn list_routes(&self, _drone_tag: &str) -> Result<Vec<RouteDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn save_route(
        &self,
        route: RouteDto,
        _operator: &str,
    ) -> Result<RouteDto, DroneApiError> {
        Ok(route)
    }
    async fn delete_route(&self, _id: &str, _operator: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn execute_route(
        &self,
        _id: &str,
        _drone_tag: &str,
        _lateral_offset_m: f64,
        _operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        Ok(CommandDto::default())
    }
    async fn list_fences(&self) -> Result<Vec<FenceDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn save_fence(
        &self,
        fence: FenceDto,
        _operator: &str,
    ) -> Result<FenceDto, DroneApiError> {
        Ok(fence)
    }
    async fn delete_fence(&self, _id: &str, _operator: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn send_command(
        &self,
        drone_tag: &str,
        req: CommandRequest,
        _operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        Ok(CommandDto {
            cmd_id: "cmd-1".into(),
            drone_tag: drone_tag.into(),
            status: req.kind,
            ..Default::default()
        })
    }
    async fn get_command(&self, cmd_id: &str) -> Result<CommandDto, DroneApiError> {
        Ok(CommandDto {
            cmd_id: cmd_id.into(),
            ..Default::default()
        })
    }
    async fn list_alerts(
        &self,
        _drone_tag: &str,
        _limit: i32,
    ) -> Result<Vec<AlertDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn list_audits(
        &self,
        _drone_tag: &str,
        _limit: i32,
    ) -> Result<Vec<AuditDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn telemetry_history(
        &self,
        _drone_tag: &str,
        _start_ms: i64,
        _end_ms: i64,
        _limit: i32,
    ) -> Result<Vec<TelemetryDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn acquire_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        priority: i32,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto {
            drone_tag: drone_tag.into(),
            control_type: control_type.into(),
            holder: operator.into(),
            priority,
            granted: true,
            ..Default::default()
        })
    }
    async fn renew_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto {
            drone_tag: drone_tag.into(),
            control_type: control_type.into(),
            holder: operator.into(),
            granted: true,
            held: true,
            ..Default::default()
        })
    }
    async fn preempt_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        priority: i32,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto {
            drone_tag: drone_tag.into(),
            control_type: control_type.into(),
            holder: operator.into(),
            priority,
            granted: true,
            held: true,
            ..Default::default()
        })
    }
    async fn release_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _force: bool,
    ) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn get_control(
        &self,
        drone_tag: &str,
        control_type: &str,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto {
            drone_tag: drone_tag.into(),
            control_type: control_type.into(),
            ..Default::default()
        })
    }
    async fn event_stream(&self, _drone_tags: Vec<String>) -> Result<EventStream, DroneApiError> {
        Err(DroneApiError::unavailable(
            "event stream not used in auth tests",
        ))
    }
}

fn model_path() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../casbin_model.conf").to_string()
}

/// 构造带指定角色与权限点的 Casbin 管理器（策略来自内存库）。
async fn casbin_with(role_name: &str, permissions: &[&str]) -> Arc<CasbinManager> {
    let repo = Arc::new(InMemoryAuthRepository::new());
    let role = Role::new(role_name.into(), None);
    RoleRepository::create(repo.as_ref(), &role).await.unwrap();

    let mut permission_ids = Vec::new();
    for name in permissions {
        let permission = Permission::new((*name).into(), None);
        PermissionRepository::create(repo.as_ref(), &permission)
            .await
            .unwrap();
        permission_ids.push(permission.id);
    }
    RoleRepository::set_role_permissions(repo.as_ref(), role.id, &permission_ids)
        .await
        .unwrap();

    Arc::new(
        CasbinManager::new_with_model_path(model_path(), repo)
            .await
            .unwrap(),
    )
}

fn token_for(role: &str) -> String {
    rustcam_media::auth::jwt::encode_token(
        uuid::Uuid::new_v4(),
        "alice",
        vec![role.to_string()],
        "access",
        chrono::Duration::minutes(10),
        &JwtConfig::default().secret,
    )
    .unwrap()
}

fn guarded_router(casbin: Arc<CasbinManager>) -> axum::Router {
    let api: Arc<dyn DroneApi> = Arc::new(FakeDroneApi);
    let tickets = Arc::new(TicketStore::new(60));
    router(api, tickets, casbin, None)
}

async fn auth_state(casbin: Arc<CasbinManager>) -> AuthState {
    AuthState {
        jwt_config: JwtConfig::default(),
        casbin,
        repo: Arc::new(
            rustcam_media::auth::auth_db::PostgresAuthRepository::with_rb(rbatis::RBatis::new()),
        ),
    }
}

async fn call(app: axum::Router, uri: &str, token: Option<&str>) -> StatusCode {
    call_with_body(app, uri, token, r#"{"drone_tag":"drone-a"}"#).await
}

async fn call_with_body(
    app: axum::Router,
    uri: &str,
    token: Option<&str>,
    body: &'static str,
) -> StatusCode {
    request(app, uri, token, body, "POST").await
}

async fn call_get(app: axum::Router, uri: &str, token: Option<&str>) -> StatusCode {
    request(app, uri, token, "", "GET").await
}

async fn request(
    app: axum::Router,
    uri: &str,
    token: Option<&str>,
    body: &'static str,
    method: &str,
) -> StatusCode {
    let mut builder = Request::builder()
        .uri(uri)
        .method(method)
        .header("Content-Type", "application/json");
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {t}"));
    }
    let req = builder.body(Body::from(body)).unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn drone_api_rejects_request_without_token() {
    let casbin = casbin_with("Admin", &["drone:view"]).await;
    let app = guarded_router(casbin.clone()).route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ));

    let status = call(app, "/api/v1/drone/control/acquire", None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn drone_api_rejects_viewer_on_control_acquire() {
    let casbin = casbin_with("Viewer", &["drone:view"]).await;
    let app = guarded_router(casbin.clone()).route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ));

    let status = call(
        app,
        "/api/v1/drone/control/acquire",
        Some(&token_for("Viewer")),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "viewer 角色不应能获取飞行控制权"
    );
}

#[tokio::test]
async fn drone_api_rejects_viewer_on_command_send() {
    let casbin = casbin_with("Viewer", &["drone:view"]).await;
    let app = guarded_router(casbin.clone()).route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ));

    let status = call(
        app,
        "/api/v1/drone/drone-a/command",
        Some(&token_for("Viewer")),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "viewer 角色不应能下发指令");
}

#[tokio::test]
async fn drone_api_allows_operator_with_control_acquire() {
    let casbin = casbin_with("Operator", &["drone:view", "drone:control:acquire"]).await;
    let app = guarded_router(casbin.clone()).route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ));

    let status = call(
        app,
        "/api/v1/drone/control/acquire",
        Some(&token_for("Operator")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn drone_api_rejects_fence_management_without_manage_permission() {
    let casbin = casbin_with("Operator", &["drone:view"]).await;
    let app = guarded_router(casbin.clone()).route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ));

    let status = call(app, "/api/v1/drone/fences", Some(&token_for("Operator"))).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "没有 drone:manage 权限不应能新建围栏"
    );
}

#[tokio::test]
async fn ws_ticket_requires_drone_view_permission() {
    let casbin = casbin_with("Guest", &[]).await;
    let app = guarded_router(casbin.clone()).route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ));

    let status = call(app, "/api/v1/drone/ws-ticket", Some(&token_for("Guest"))).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "没有任何无人机权限不应能换取 WS 票据"
    );
}

#[tokio::test]
async fn ws_ticket_issued_for_viewer_with_drone_view() {
    let casbin = casbin_with("Viewer", &["drone:view"]).await;
    let app = guarded_router(casbin.clone()).route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ));

    let status = call(app, "/api/v1/drone/ws-ticket", Some(&token_for("Viewer"))).await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn permission_is_checked_before_body_is_parsed() {
    let casbin = casbin_with("Viewer", &["drone:view"]).await;
    let app = guarded_router(casbin.clone()).route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ));

    // 请求体不是合法 JSON：未授权时必须先返回 403，而不是解析失败。
    let status = call_with_body(
        app,
        "/api/v1/drone/control/acquire",
        Some(&token_for("Viewer")),
        "not-json",
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "鉴权必须先于请求体解析");
}

// ---------- 错误状态码（规则 §11.3 / §11.6） ----------

/// drone-service 不可用时的假实现。
struct UnavailableDroneApi;

#[async_trait]
impl DroneApi for UnavailableDroneApi {
    async fn ping(&self) -> Result<(), DroneApiError> {
        Err(DroneApiError::unavailable("drone-service not configured"))
    }
    async fn list_drones(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service not configured"))
    }
    async fn get_drone(&self, _drone_tag: &str) -> Result<DroneDto, DroneApiError> {
        Err(DroneApiError::not_found("unknown device"))
    }
    async fn save_drone(
        &self,
        drone: DroneDto,
        _operator: &str,
    ) -> Result<DroneDto, DroneApiError> {
        Ok(drone)
    }
    async fn delete_drone(&self, _drone_tag: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn list_drone_profiles(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        Ok(vec![DroneDto {
            drone_tag: "drone-a".into(),
            ..Default::default()
        }])
    }
    async fn list_adapters(&self, _include_disabled: bool) -> Result<Vec<AdapterDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn get_adapter(&self, adapter_id: &str) -> Result<AdapterDto, DroneApiError> {
        Ok(AdapterDto {
            adapter_id: adapter_id.into(),
            ..Default::default()
        })
    }
    async fn save_adapter(
        &self,
        adapter: AdapterDto,
        _operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        Ok(adapter)
    }
    async fn set_adapter_enabled(
        &self,
        adapter_id: &str,
        enabled: bool,
        _operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        Ok(AdapterDto {
            adapter_id: adapter_id.into(),
            enabled,
            ..Default::default()
        })
    }
    async fn list_routes(&self, _drone_tag: &str) -> Result<Vec<RouteDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn save_route(
        &self,
        route: RouteDto,
        _operator: &str,
    ) -> Result<RouteDto, DroneApiError> {
        Ok(route)
    }
    async fn delete_route(&self, _id: &str, _operator: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn execute_route(
        &self,
        _id: &str,
        _drone_tag: &str,
        _lateral_offset_m: f64,
        _operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        Ok(CommandDto::default())
    }
    async fn list_fences(&self) -> Result<Vec<FenceDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn save_fence(
        &self,
        fence: FenceDto,
        _operator: &str,
    ) -> Result<FenceDto, DroneApiError> {
        Ok(fence)
    }
    async fn delete_fence(&self, _id: &str, _operator: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn send_command(
        &self,
        _drone_tag: &str,
        _req: CommandRequest,
        _operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        Ok(CommandDto::default())
    }
    async fn get_command(&self, _cmd_id: &str) -> Result<CommandDto, DroneApiError> {
        Err(DroneApiError::not_found("unknown cmd"))
    }
    async fn list_alerts(
        &self,
        _drone_tag: &str,
        _limit: i32,
    ) -> Result<Vec<AlertDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn list_audits(
        &self,
        _drone_tag: &str,
        _limit: i32,
    ) -> Result<Vec<AuditDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn telemetry_history(
        &self,
        _drone_tag: &str,
        _start_ms: i64,
        _end_ms: i64,
        _limit: i32,
    ) -> Result<Vec<TelemetryDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn acquire_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        priority: i32,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto {
            drone_tag: drone_tag.into(),
            control_type: control_type.into(),
            holder: operator.into(),
            priority,
            granted: false,
            held: true,
            reason: "控制权由 `alice` 持有".into(),
            ..Default::default()
        })
    }
    async fn renew_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto::default())
    }
    async fn preempt_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _priority: i32,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto::default())
    }
    async fn release_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _force: bool,
    ) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn get_control(
        &self,
        drone_tag: &str,
        control_type: &str,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto {
            drone_tag: drone_tag.into(),
            control_type: control_type.into(),
            ..Default::default()
        })
    }
    async fn event_stream(&self, _drone_tags: Vec<String>) -> Result<EventStream, DroneApiError> {
        Err(DroneApiError::unavailable("event stream not used"))
    }
}

async fn router_with(api: Arc<dyn DroneApi>) -> axum::Router {
    let casbin = casbin_with("Operator", &["drone:view", "drone:control:acquire"]).await;
    let tickets = Arc::new(TicketStore::new(60));
    let app = router(api, tickets, casbin.clone(), None);
    app.route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ))
}

#[tokio::test]
async fn drone_service_unavailable_returns_503() {
    let app = router_with(Arc::new(UnavailableDroneApi)).await;

    let status = call_get(app, "/api/v1/drone/ping", Some(&token_for("Operator"))).await;

    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "drone-service 不可用必须返回 503，不能是 200"
    );
}

#[tokio::test]
async fn unknown_device_returns_404() {
    let app = router_with(Arc::new(UnavailableDroneApi)).await;

    let status = call_get(
        app,
        "/api/v1/drone/devices/drone-a",
        Some(&token_for("Operator")),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn drone_route_path_uses_drone_tag() {
    let app = router_with(Arc::new(FakeDroneApi)).await;

    let status = call_get(
        app,
        "/api/v1/drone/devices/drone-a",
        Some(&token_for("Operator")),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "路径参数改为 drone_tag 后仍应按无人机权限放行"
    );
}

#[tokio::test]
async fn drone_control_path_uses_drone_tag() {
    let app = router_with(Arc::new(FakeDroneApi)).await;

    let status = call_get(
        app,
        "/api/v1/drone/control/drone-a/flight",
        Some(&token_for("Operator")),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "控制权查询路径参数改为 drone_tag 后仍应按无人机权限放行"
    );
}

// ---------------------------------------------------------------------------
// 机载链路实体（adapter）的 REST 接口
// ---------------------------------------------------------------------------

/// adapter 接口用的假实现：已登记的 adapter 只有一个。
struct AdapterDroneApi;

#[async_trait]
impl DroneApi for AdapterDroneApi {
    async fn ping(&self) -> Result<(), DroneApiError> {
        Ok(())
    }
    async fn list_drones(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        Ok(vec![DroneDto {
            drone_tag: "drone-a".into(),
            adapter_id: "adapter-1".into(),
            ..Default::default()
        }])
    }
    async fn list_drone_profiles(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn get_drone(&self, drone_tag: &str) -> Result<DroneDto, DroneApiError> {
        Ok(DroneDto {
            drone_tag: drone_tag.into(),
            ..Default::default()
        })
    }
    async fn save_drone(
        &self,
        drone: DroneDto,
        _operator: &str,
    ) -> Result<DroneDto, DroneApiError> {
        Ok(drone)
    }
    async fn delete_drone(&self, _drone_tag: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn list_adapters(&self, _include_disabled: bool) -> Result<Vec<AdapterDto>, DroneApiError> {
        Ok(vec![AdapterDto {
            adapter_id: "adapter-1".into(),
            name: "东场链路".into(),
            protocol: "mavlink".into(),
            enabled: true,
            ..Default::default()
        }])
    }
    async fn get_adapter(&self, adapter_id: &str) -> Result<AdapterDto, DroneApiError> {
        if adapter_id == "adapter-1" {
            Ok(AdapterDto {
                adapter_id: adapter_id.into(),
                enabled: true,
                ..Default::default()
            })
        } else {
            Err(DroneApiError::not_found("adapter not found"))
        }
    }
    async fn save_adapter(
        &self,
        adapter: AdapterDto,
        _operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        Ok(adapter)
    }
    async fn set_adapter_enabled(
        &self,
        adapter_id: &str,
        enabled: bool,
        _operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        Ok(AdapterDto {
            adapter_id: adapter_id.into(),
            enabled,
            ..Default::default()
        })
    }
    async fn list_routes(&self, _drone_tag: &str) -> Result<Vec<RouteDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn save_route(
        &self,
        route: RouteDto,
        _operator: &str,
    ) -> Result<RouteDto, DroneApiError> {
        Ok(route)
    }
    async fn delete_route(&self, _id: &str, _operator: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn execute_route(
        &self,
        _id: &str,
        _drone_tag: &str,
        _lateral_offset_m: f64,
        _operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        Ok(CommandDto::default())
    }
    async fn list_fences(&self) -> Result<Vec<FenceDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn save_fence(&self, fence: FenceDto, _operator: &str) -> Result<FenceDto, DroneApiError> {
        Ok(fence)
    }
    async fn delete_fence(&self, _id: &str, _operator: &str) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn send_command(
        &self,
        _drone_tag: &str,
        _req: CommandRequest,
        _operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        Ok(CommandDto::default())
    }
    async fn get_command(&self, _cmd_id: &str) -> Result<CommandDto, DroneApiError> {
        Ok(CommandDto::default())
    }
    async fn list_alerts(&self, _drone_tag: &str, _limit: i32) -> Result<Vec<AlertDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn list_audits(&self, _drone_tag: &str, _limit: i32) -> Result<Vec<AuditDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn telemetry_history(
        &self,
        _drone_tag: &str,
        _start_ms: i64,
        _end_ms: i64,
        _limit: i32,
    ) -> Result<Vec<TelemetryDto>, DroneApiError> {
        Ok(vec![])
    }
    async fn acquire_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _priority: i32,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto::default())
    }
    async fn renew_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto::default())
    }
    async fn preempt_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _priority: i32,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto::default())
    }
    async fn release_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _force: bool,
    ) -> Result<bool, DroneApiError> {
        Ok(true)
    }
    async fn get_control(&self, _drone_tag: &str, _control_type: &str) -> Result<ControlDto, DroneApiError> {
        Ok(ControlDto::default())
    }
    async fn event_stream(&self, _drone_tags: Vec<String>) -> Result<EventStream, DroneApiError> {
        Err(DroneApiError::unavailable("event stream not used in auth tests"))
    }
}

/// 按指定角色与权限点构造网关路由（adapter 接口要区分查看权与管理权）。
async fn adapter_router(
    api: Arc<dyn DroneApi>,
    role: &str,
    permissions: &[&str],
) -> axum::Router {
    let casbin = casbin_with(role, permissions).await;
    let tickets = Arc::new(TicketStore::new(60));
    let app = router(api, tickets, casbin.clone(), None);
    app.route_layer(axum::middleware::from_fn_with_state(
        auth_state(casbin).await,
        jwt_auth_layer,
    ))
}

#[tokio::test]
async fn adapter_list_is_readable_with_view_permission() {
    let app = adapter_router(
        Arc::new(AdapterDroneApi),
        "Viewer",
        &["drone:view"],
    )
    .await;

    let status = call_get(app, "/api/v1/drone/adapters", Some(&token_for("Viewer"))).await;

    assert_eq!(status, StatusCode::OK, "查看权应能读 adapter 列表");
}

#[tokio::test]
async fn adapter_write_requires_manage_permission() {
    let app = adapter_router(
        Arc::new(AdapterDroneApi),
        "Viewer",
        &["drone:view"],
    )
    .await;

    let status = call_with_body(
        app,
        "/api/v1/drone/adapters",
        Some(&token_for("Viewer")),
        r#"{"adapter_id":"adapter-2"}"#,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "只有查看权时不应能新建 adapter"
    );
}

#[tokio::test]
async fn creating_existing_adapter_is_rejected_as_conflict() {
    let app = adapter_router(
        Arc::new(AdapterDroneApi),
        "Operator",
        &["drone:view", "drone:manage"],
    )
    .await;

    let status = call_with_body(
        app,
        "/api/v1/drone/adapters",
        Some(&token_for("Operator")),
        r#"{"adapter_id":"adapter-1"}"#,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "标识已存在时应拒绝覆盖"
    );
}

#[tokio::test]
async fn updating_missing_adapter_is_rejected_as_not_found() {
    let app = adapter_router(
        Arc::new(AdapterDroneApi),
        "Operator",
        &["drone:view", "drone:manage"],
    )
    .await;

    let status = request(
        app,
        "/api/v1/drone/adapters/adapter-404",
        Some(&token_for("Operator")),
        r#"{"name":"改名"}"#,
        "PUT",
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "改不存在的 adapter 应返回 404"
    );
}

#[tokio::test]
async fn disabling_adapter_soft_deletes_it() {
    let app = adapter_router(
        Arc::new(AdapterDroneApi),
        "Operator",
        &["drone:view", "drone:manage"],
    )
    .await;

    let status = request(
        app,
        "/api/v1/drone/adapters/adapter-1",
        Some(&token_for("Operator")),
        "",
        "DELETE",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "停用 adapter 应成功");
}

#[tokio::test]
async fn control_conflict_returns_409_with_holder() {
    let app = router_with(Arc::new(UnavailableDroneApi)).await;

    let status = call(
        app,
        "/api/v1/drone/control/acquire",
        Some(&token_for("Operator")),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "抢不到控制权必须返回 409");
}

// ---------- WebSocket 票据 ----------
fn ws_router(tickets: Arc<TicketStore>) -> axum::Router {
    let api: Arc<dyn DroneApi> = Arc::new(FakeDroneApi);
    rustcam_media::drone_gateway::ws::router(api, tickets)
}

async fn ws_status(app: axum::Router, uri: &str) -> StatusCode {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn ws_without_ticket_is_rejected() {
    let tickets = Arc::new(TicketStore::new(60));
    let app = ws_router(tickets);

    let status = ws_status(app, "/ws/drone/telemetry").await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ws_with_unknown_ticket_is_rejected() {
    let tickets = Arc::new(TicketStore::new(60));
    let app = ws_router(tickets);

    let status = ws_status(app, "/ws/drone/telemetry?ticket=nope").await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ws_ticket_cannot_be_replayed() {
    let tickets = Arc::new(TicketStore::new(60));
    let ticket = tickets.issue(vec!["telemetry".into()]);
    let app = ws_router(tickets);

    let first = ws_status(app.clone(), &format!("/ws/drone/telemetry?ticket={ticket}")).await;
    let second = ws_status(app, &format!("/ws/drone/telemetry?ticket={ticket}")).await;

    assert_ne!(first, StatusCode::UNAUTHORIZED, "首次使用应通过票据校验");
    assert_eq!(second, StatusCode::UNAUTHORIZED, "票据不可重放");
}

#[tokio::test]
async fn ws_ticket_scope_limits_subscription() {
    // 只有告警/控制权/指令回执范围，没有遥测。
    let tickets = Arc::new(TicketStore::new(60));
    let ticket = tickets.issue(vec!["alert".into()]);
    let app = ws_router(tickets);

    let status = ws_status(app, &format!("/ws/drone/telemetry?ticket={ticket}")).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "票据范围不含遥测时不得订阅遥测流"
    );
}

#[tokio::test]
async fn ws_expired_ticket_is_rejected() {
    let tickets = Arc::new(TicketStore::new(0));
    let ticket = tickets.issue(vec!["telemetry".into()]);
    let app = ws_router(tickets);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let status = ws_status(app, &format!("/ws/drone/telemetry?ticket={ticket}")).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED, "过期票据必须被拒绝");
}
