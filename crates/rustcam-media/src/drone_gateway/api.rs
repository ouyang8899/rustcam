use std::sync::Arc;

use axum::extract::{Extension, Path, Query};
use axum::routing::{delete, get, post, put};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::response::ApiResponse;
use crate::auth::casbin::CasbinManager;
use crate::drone_gateway::auth::{ws_scope, DroneUser};
use crate::drone_gateway::client::{
    AdapterDto, AlertDto, AuditDto, CommandDto, CommandRequest, ControlDto, DroneApi, DroneApiError,
    DroneDto, FenceDto, RouteDto, TelemetryDto,
};
use crate::drone_gateway::ticket::TicketStore;
use crate::error::AppError;
use crate::infrastructure::db_repository::DbRepository;

/// drone 接口的统一返回：出错时用真实 HTTP 状态码（规则 §11.3 / §11.6）。
type ApiResult<T> = Result<Json<ApiResponse<T>>, AppError>;

impl From<DroneApiError> for AppError {
    fn from(e: DroneApiError) -> AppError {
        match e.status {
            400 => AppError::BadRequest(e.message),
            403 => AppError::Forbidden(e.message),
            404 => AppError::NotFound(e.message),
            409 => AppError::Conflict(e.message),
            503 => AppError::ServiceUnavailable(e.message),
            _ => AppError::Internal(e.message),
        }
    }
}

/// 构造 drone REST 路由。
///
/// 鉴权由 `route_layer(authorize)` 统一完成（先于请求体解析）：路由到权限点的
/// 映射见 `permissions::required_permissions`；`Extension` 层在鉴权之后注入，
/// 因此必须后于 `route_layer` 调用。
pub fn router<S>(
    api: Arc<dyn DroneApi>,
    tickets: Arc<TicketStore>,
    casbin: Arc<CasbinManager>,
    db: Option<Arc<DbRepository>>,
) -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    axum::Router::new()
        .route("/api/v1/drone/ping", get(ping))
        .route("/api/v1/drone/drones", get(list_drones).post(create_drone))
        .route(
            "/api/v1/drone/drones/{drone_tag}",
            get(get_drone).put(update_drone).delete(delete_drone),
        )
        .route(
            "/api/v1/drone/drones/{drone_tag}/cameras",
            get(list_drone_cameras).post(add_drone_camera),
        )
        .route(
            "/api/v1/drone/drones/{drone_tag}/cameras/{camera_device_tag}",
            delete(remove_drone_camera),
        )
        .route("/api/v1/drone/devices", get(list_drones))
        .route("/api/v1/drone/devices/{drone_tag}", get(get_drone))
        // 机载链路实体（adapter）：配置登记与启停（软删除）。
        .route("/api/v1/drone/adapters", get(list_adapters).post(create_adapter))
        .route(
            "/api/v1/drone/adapters/{adapter_id}",
            get(get_adapter).put(update_adapter).delete(disable_adapter),
        )
        .route(
            "/api/v1/drone/adapters/{adapter_id}/restore",
            post(restore_adapter),
        )
        .route("/api/v1/drone/{drone_tag}/command", post(send_command))
        .route("/api/v1/drone/{drone_tag}/telemetry", get(get_telemetry))
        // 规则 §6.12 的路径别名（与上面的简写并存）。
        .route(
            "/api/v1/drone/devices/{drone_tag}/telemetry",
            get(get_telemetry),
        )
        .route("/api/v1/drone/commands/{cmd_id}", get(get_command))
        .route(
            "/api/v1/drone/{drone_tag}/commands/{cmd_id}",
            get(get_command),
        )
        .route("/api/v1/drone/routes", get(list_routes).post(save_route))
        .route(
            "/api/v1/drone/routes/{id}",
            put(update_route).delete(delete_route),
        )
        .route("/api/v1/drone/routes/{id}/execute", post(execute_route))
        .route("/api/v1/drone/fences", get(list_fences).post(save_fence))
        .route(
            "/api/v1/drone/fences/{id}",
            put(update_fence).delete(delete_fence),
        )
        .route("/api/v1/drone/geofences", get(list_fences).post(save_fence))
        .route(
            "/api/v1/drone/geofences/{id}",
            put(update_fence).delete(delete_fence),
        )
        .route("/api/v1/drone/alerts", get(list_alerts))
        .route("/api/v1/drone/audits", get(list_audits))
        .route("/api/v1/drone/replay", get(get_replay))
        .route("/api/v1/drone/control/acquire", post(acquire_control))
        .route("/api/v1/drone/control/preempt", post(preempt_control))
        .route("/api/v1/drone/control/release", post(release_control))
        .route("/api/v1/drone/control/renew", post(renew_control))
        .route(
            "/api/v1/drone/control/force_release",
            post(force_release_control),
        )
        .route(
            "/api/v1/drone/control/{drone_tag}/{control_type}",
            get(get_control),
        )
        .route("/api/v1/drone/ws-ticket", post(issue_ticket))
        .route_layer(axum::middleware::from_fn(
            crate::drone_gateway::auth::authorize,
        ))
        .layer(Extension(api))
        .layer(Extension(tickets))
        .layer(Extension(casbin))
        .layer(Extension(db))
}

fn ok<T: Serialize>(data: T) -> Json<ApiResponse<T>> {
    Json(ApiResponse::success(data))
}

fn fail<T: Serialize>(code: u16, message: &str) -> Json<ApiResponse<T>> {
    Json(ApiResponse::error(code, message))
}

async fn ping(Extension(api): Extension<Arc<dyn DroneApi>>) -> ApiResult<serde_json::Value> {
    match api.ping().await {
        Ok(()) => Ok(ok(serde_json::json!({ "status": "ok" }))),
        Err(e) => Err(e.into()),
    }
}

async fn list_drones(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Extension(db): Extension<Option<Arc<DbRepository>>>,
) -> ApiResult<Vec<DroneDto>> {
    match api.list_drones().await {
        Ok(list) => {
            let mut out = Vec::with_capacity(list.len());
            for drone in list {
                out.push(with_camera_tags(db.as_ref(), drone).await);
            }
            Ok(ok(merge_link_identity(&api, out).await))
        }
        Err(e) => Err(e.into()),
    }
}

/// 把档案里的链路身份（端点、系统号、唯一标识）补到设备列表上。
///
/// 设备列表来自运行时注册信息，不含链路身份；档案里有登记值。
/// 档案读失败不阻断列表返回：链路信息缺失不应让整个列表不可用。
async fn merge_link_identity(api: &Arc<dyn DroneApi>, drones: Vec<DroneDto>) -> Vec<DroneDto> {
    let Ok(profiles) = api.list_drone_profiles().await else {
        return drones;
    };
    drones
        .into_iter()
        .map(|mut drone| {
            if let Some(profile) = profiles.iter().find(|p| p.drone_tag == drone.drone_tag) {
                if !profile.mavlink_addr.is_empty() {
                    drone.mavlink_addr = profile.mavlink_addr.clone();
                }
                if profile.mavlink_sysid > 0 {
                    drone.mavlink_sysid = profile.mavlink_sysid;
                }
                if profile.mavlink_compid > 0 {
                    drone.mavlink_compid = profile.mavlink_compid;
                }
                if !profile.unique_id.is_empty() {
                    drone.unique_id = profile.unique_id.clone();
                }
                if !profile.adapter_id.is_empty() {
                    drone.adapter_id = profile.adapter_id.clone();
                }
            }
            drone
        })
        .collect()
}

async fn get_drone(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Extension(db): Extension<Option<Arc<DbRepository>>>,
    Path(drone_tag): Path<String>,
) -> ApiResult<DroneDto> {
    match api.get_drone(&drone_tag).await {
        Ok(v) => Ok(ok(with_camera_tags(db.as_ref(), v).await)),
        Err(e) => Err(e.into()),
    }
}

/// 回填无人机已关联的摄像头 tag。
///
/// 关联查询失败不阻断列表返回：关联信息缺失不应让整个列表不可用。
async fn with_camera_tags(db: Option<&Arc<DbRepository>>, mut drone: DroneDto) -> DroneDto {
    if let Some(db) = db {
        if !drone.drone_tag.is_empty() {
            if let Ok(tags) = db.list_drone_cameras(&drone.drone_tag).await {
                drone.camera_device_tags = tags;
            }
        }
    }
    drone
}

/// 关联的摄像头设备；已删除的设备不再出现在列表里。
#[derive(Debug, Serialize)]
struct DroneCameraDto {
    camera_device_tag: String,
    name: String,
    protocol: String,
    status: String,
}

async fn camera_list(
    db: &Arc<DbRepository>,
    drone_tag: &str,
) -> Result<Vec<DroneCameraDto>, AppError> {
    let tags = db.list_drone_cameras(drone_tag).await?;
    let mut out = Vec::with_capacity(tags.len());
    for tag in tags {
        if let Some(device) = db.get_device_by_device_tag(&tag) {
            out.push(DroneCameraDto {
                camera_device_tag: tag,
                name: device.name,
                protocol: device.protocol.to_string(),
                status: device.status.to_string(),
            });
        }
    }
    Ok(out)
}

fn require_db(db: &Option<Arc<DbRepository>>) -> Result<&Arc<DbRepository>, AppError> {
    db.as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("数据库未就绪".into()))
}

#[derive(Debug, Deserialize)]
struct AddDroneCameraRequest {
    camera_device_tag: String,
}

async fn list_drone_cameras(
    Extension(db): Extension<Option<Arc<DbRepository>>>,
    Path(drone_tag): Path<String>,
) -> ApiResult<Vec<DroneCameraDto>> {
    let db = require_db(&db)?;
    Ok(ok(camera_list(db, &drone_tag).await?))
}

async fn add_drone_camera(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Extension(db): Extension<Option<Arc<DbRepository>>>,
    Path(drone_tag): Path<String>,
    Json(body): Json<AddDroneCameraRequest>,
) -> ApiResult<Vec<DroneCameraDto>> {
    let camera_tag = body.camera_device_tag.trim().to_string();
    if camera_tag.is_empty() {
        return Err(AppError::BadRequest("camera_device_tag 不能为空".into()));
    }

    let db = require_db(&db)?;

    let drones = api.list_drones().await?;
    if !drones.iter().any(|d| d.drone_tag == drone_tag) {
        return Err(AppError::NotFound(format!("无人机 {drone_tag} 不存在")));
    }

    if db.get_device_by_device_tag(&camera_tag).is_none() {
        return Err(AppError::NotFound(format!("摄像头设备 {camera_tag} 不存在")));
    }

    db.add_drone_camera(&drone_tag, &camera_tag).await?;
    Ok(ok(camera_list(db, &drone_tag).await?))
}

async fn remove_drone_camera(
    Extension(db): Extension<Option<Arc<DbRepository>>>,
    Path((drone_tag, camera_device_tag)): Path<(String, String)>,
) -> ApiResult<bool> {
    let db = require_db(&db)?;
    db.remove_drone_camera(&drone_tag, &camera_device_tag)
        .await?;
    Ok(ok(true))
}

async fn create_drone(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(body): Json<DroneDto>,
) -> ApiResult<DroneDto> {
    match api.save_drone(body, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn update_drone(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(drone_tag): Path<String>,
    Json(mut body): Json<DroneDto>,
) -> ApiResult<DroneDto> {
    body.drone_tag = drone_tag;
    match api.save_drone(body, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn delete_drone(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Extension(db): Extension<Option<Arc<DbRepository>>>,
    Path(drone_tag): Path<String>,
) -> ApiResult<bool> {
    let drone = api.get_drone(&drone_tag).await?;
    let deleted = api.delete_drone(&drone_tag).await?;
    if deleted && !drone.drone_tag.is_empty() {
        if let Some(db) = db.as_ref() {
            db.delete_drone_cameras_by_drone(&drone.drone_tag).await?;
        }
    }
    Ok(ok(deleted))
}

async fn get_telemetry(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Path(drone_tag): Path<String>,
) -> ApiResult<serde_json::Value> {
    match api.get_drone(&drone_tag).await {
        Ok(drone) => match drone.last_telemetry {
            Some(t) => Ok(ok(serde_json::to_value(t).unwrap_or_default())),
            None => Ok(fail(404, &format!("no telemetry for {drone_tag}"))),
        },
        Err(e) => Err(e.into()),
    }
}

// ---------------------------------------------------------------------------
// 机载链路实体（adapter）
// ---------------------------------------------------------------------------

/// adapter 列表查询参数。
#[derive(Debug, Deserialize, Default)]
struct AdapterListQuery {
    /// 是否包含已停用（软删除）的 adapter。
    #[serde(default)]
    include_disabled: bool,
}

/// 列出 adapter 配置，并回填每个 adapter 名下的无人机数量。
async fn list_adapters(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Query(q): Query<AdapterListQuery>,
) -> ApiResult<Vec<AdapterDto>> {
    let mut adapters = api.list_adapters(q.include_disabled).await?;
    // 绑定机数按无人机档案统计；统计失败不影响列表本身。
    if let Ok(drones) = api.list_drones().await {
        for adapter in adapters.iter_mut() {
            adapter.bound_drones = drones
                .iter()
                .filter(|d| d.adapter_id == adapter.adapter_id)
                .count() as i32;
        }
    }
    Ok(ok(adapters.into_iter().map(sanitize_adapter).collect()))
}

/// 抹掉密钥内容，只保留“是否已配置”标记。
///
/// 编辑时前端留空表示保持原密钥，由 drone-service 的合并逻辑处理。
fn sanitize_adapter(mut adapter: AdapterDto) -> AdapterDto {
    adapter.has_signing_secret = !adapter.signing_secret.trim().is_empty();
    adapter.signing_secret.clear();
    adapter
}

/// 读取单个 adapter 配置。
async fn get_adapter(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Path(adapter_id): Path<String>,
) -> ApiResult<AdapterDto> {
    Ok(ok(sanitize_adapter(api.get_adapter(&adapter_id).await?)))
}

/// 新建 adapter 配置。
///
/// 已存在同名 adapter 时拒绝：标识与适配器进程配置绑定，不能被悄悄覆盖。
async fn create_adapter(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(mut body): Json<AdapterDto>,
) -> ApiResult<AdapterDto> {
    body.adapter_id = body.adapter_id.trim().to_string();
    if body.adapter_id.is_empty() {
        return Err(AppError::BadRequest("adapter_id 不能为空".into()));
    }
    match api.get_adapter(&body.adapter_id).await {
        Ok(_) => {
            return Err(AppError::Conflict(format!(
                "adapter `{}` 已存在",
                body.adapter_id
            )))
        }
        Err(e) if e.status == 404 => {}
        Err(e) => return Err(e.into()),
    }
    // 新建一律启用；停用只能走停用接口。
    body.enabled = true;
    Ok(ok(sanitize_adapter(
        api.save_adapter(body, &user.0.username).await?,
    )))
}

/// 修改 adapter 配置。
///
/// 标识创建后不可改：它与适配器进程配置一一对应。
async fn update_adapter(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(adapter_id): Path<String>,
    Json(mut body): Json<AdapterDto>,
) -> ApiResult<AdapterDto> {
    let path_id = adapter_id.trim().to_string();
    if path_id.is_empty() {
        return Err(AppError::BadRequest("adapter_id 不能为空".into()));
    }
    let body_id = body.adapter_id.trim().to_string();
    if !body_id.is_empty() && body_id != path_id {
        return Err(AppError::BadRequest("adapter_id 不可修改".into()));
    }
    body.adapter_id = path_id.clone();
    match api.get_adapter(&path_id).await {
        Ok(_) => {}
        Err(e) if e.status == 404 => {
            return Err(AppError::NotFound(format!("adapter `{path_id}` 不存在")))
        }
        Err(e) => return Err(e.into()),
    }
    Ok(ok(sanitize_adapter(
        api.save_adapter(body, &user.0.username).await?,
    )))
}

/// 停用 adapter（软删除）：配置行与绑定关系都保留，可恢复。
async fn disable_adapter(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(adapter_id): Path<String>,
) -> ApiResult<AdapterDto> {
    Ok(ok(sanitize_adapter(
        api.set_adapter_enabled(&adapter_id, false, &user.0.username)
            .await?,
    )))
}

/// 恢复已停用的 adapter。
async fn restore_adapter(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(adapter_id): Path<String>,
) -> ApiResult<AdapterDto> {
    Ok(ok(sanitize_adapter(
        api.set_adapter_enabled(&adapter_id, true, &user.0.username)
            .await?,
    )))
}

#[derive(Debug, Deserialize)]
struct DeviceQuery {
    #[serde(default)]
    drone_tag: String,
    #[serde(default)]
    limit: i32,
}

async fn list_alerts(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Query(q): Query<DeviceQuery>,
) -> ApiResult<Vec<AlertDto>> {
    let limit = if q.limit <= 0 { 100 } else { q.limit };
    match api.list_alerts(&q.drone_tag, limit).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn list_audits(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Query(q): Query<DeviceQuery>,
) -> ApiResult<Vec<AuditDto>> {
    let limit = if q.limit <= 0 { 200 } else { q.limit };
    match api.list_audits(&q.drone_tag, limit).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Deserialize)]
struct ReplayQuery {
    #[serde(default)]
    drone_tag: String,
    #[serde(default)]
    start_ms: i64,
    #[serde(default)]
    end_ms: i64,
    #[serde(default)]
    limit: i32,
}

async fn get_replay(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Query(q): Query<ReplayQuery>,
) -> ApiResult<Vec<TelemetryDto>> {
    if q.drone_tag.trim().is_empty() {
        return Err(AppError::BadRequest("drone_tag 不能为空".into()));
    }
    let limit = if q.limit <= 0 { 5_000 } else { q.limit };
    match api
        .telemetry_history(&q.drone_tag, q.start_ms, q.end_ms, limit)
        .await
    {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn send_command(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(drone_tag): Path<String>,
    Json(req): Json<CommandRequest>,
) -> ApiResult<CommandDto> {
    match api.send_command(&drone_tag, req, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn get_command(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Path(cmd_id): Path<String>,
) -> ApiResult<CommandDto> {
    match api.get_command(&cmd_id).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn list_routes(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    Query(q): Query<DeviceQuery>,
) -> ApiResult<Vec<RouteDto>> {
    match api.list_routes(&q.drone_tag).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn save_route(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(body): Json<RouteDto>,
) -> ApiResult<RouteDto> {
    match api.save_route(body, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn update_route(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(id): Path<String>,
    Json(mut body): Json<RouteDto>,
) -> ApiResult<RouteDto> {
    body.id = id;
    match api.save_route(body, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn delete_route(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(id): Path<String>,
) -> ApiResult<bool> {
    match api.delete_route(&id, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Deserialize, Default)]
struct ExecuteRouteQuery {
    #[serde(default)]
    drone_tag: String,
    #[serde(default)]
    lateral_offset_m: f64,
}

async fn execute_route(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(id): Path<String>,
    Query(q): Query<ExecuteRouteQuery>,
) -> ApiResult<CommandDto> {
    match api
        .execute_route(&id, &q.drone_tag, q.lateral_offset_m, &user.0.username)
        .await
    {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn list_fences(Extension(api): Extension<Arc<dyn DroneApi>>) -> ApiResult<Vec<FenceDto>> {
    match api.list_fences().await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn save_fence(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(body): Json<FenceDto>,
) -> ApiResult<FenceDto> {
    match api.save_fence(body, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn update_fence(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(id): Path<String>,
    Json(mut body): Json<FenceDto>,
) -> ApiResult<FenceDto> {
    body.id = id;
    match api.save_fence(body, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn delete_fence(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path(id): Path<String>,
) -> ApiResult<bool> {
    match api.delete_fence(&id, &user.0.username).await {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Deserialize)]
struct ControlRequest {
    drone_tag: String,
    #[serde(default = "default_control_type")]
    control_type: String,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    lease_seconds: i32,
}

fn default_control_type() -> String {
    "flight".into()
}

async fn acquire_control(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(req): Json<ControlRequest>,
) -> ApiResult<ControlDto> {
    match api
        .acquire_control(
            &req.drone_tag,
            &req.control_type,
            &user.0.username,
            req.priority,
            req.lease_seconds,
        )
        .await
    {
        // 抢不到控制权时按规则 §11.3 返回 409，并带上当前持有人。
        Ok(v) if !v.granted => Err(AppError::Conflict(format!(
            "控制权由 `{}` 持有（优先级 {}，到期 {}）",
            v.holder, v.priority, v.expire_at_ms
        ))),
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn release_control(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(req): Json<ControlRequest>,
) -> ApiResult<bool> {
    match api
        .release_control(&req.drone_tag, &req.control_type, &user.0.username, false)
        .await
    {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn preempt_control(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(req): Json<ControlRequest>,
) -> ApiResult<ControlDto> {
    match api
        .preempt_control(
            &req.drone_tag,
            &req.control_type,
            &user.0.username,
            req.priority,
            req.lease_seconds,
        )
        .await
    {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn force_release_control(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(req): Json<ControlRequest>,
) -> ApiResult<bool> {
    match api
        .release_control(&req.drone_tag, &req.control_type, &user.0.username, true)
        .await
    {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Deserialize)]
struct RenewRequest {
    drone_tag: String,
    #[serde(default = "default_control_type")]
    control_type: String,
    #[serde(default)]
    lease_seconds: i32,
}

async fn renew_control(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Json(req): Json<RenewRequest>,
) -> ApiResult<ControlDto> {
    match api
        .renew_control(
            &req.drone_tag,
            &req.control_type,
            &user.0.username,
            req.lease_seconds,
        )
        .await
    {
        Ok(v) => Ok(ok(v)),
        Err(e) => Err(e.into()),
    }
}

async fn get_control(
    Extension(api): Extension<Arc<dyn DroneApi>>,
    user: DroneUser,
    Path((drone_tag, control_type)): Path<(String, String)>,
) -> ApiResult<ControlDto> {
    match api.get_control(&drone_tag, &control_type).await {
        Ok(mut v) => {
            // held 表示“存在租约”，granted 表示“当前用户就是持有人”。
            // 前端据此区分“控制权被他人持有”和“我可以释放/续约”。
            v.granted = v.held && v.holder == user.0.username;
            Ok(ok(v))
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Serialize)]
struct TicketResponse {
    ticket: String,
    expires_in_secs: u64,
}

async fn issue_ticket(
    Extension(tickets): Extension<Arc<TicketStore>>,
    Extension(casbin): Extension<Arc<CasbinManager>>,
    user: DroneUser,
) -> ApiResult<TicketResponse> {
    let scope = ws_scope(&casbin, &user.0).await;
    if scope.is_empty() {
        return Err(AppError::Forbidden(
            "权限不足：需要 drone:view 或 drone:telemetry:read".to_string(),
        ));
    }
    let ticket = tickets.issue(scope.iter().map(|s| s.to_string()).collect());
    Ok(ok(TicketResponse {
        ticket,
        expires_in_secs: 60,
    }))
}
