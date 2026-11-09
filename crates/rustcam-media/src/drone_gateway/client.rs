use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use rustcam_drone_core::reconnect::Backoff;
use rustcam_drone_proto::common as proto;
use rustcam_drone_proto::control::admin_service_client::AdminServiceClient;
use rustcam_drone_proto::control::control_service_client::ControlServiceClient;
use rustcam_drone_proto::control::event_service_client::EventServiceClient;
use rustcam_drone_proto::control::geofence_service_client::GeofenceServiceClient;
use rustcam_drone_proto::control::query_service_client::QueryServiceClient;
use rustcam_drone_proto::control::{
    AcquireRequest, Adapter as AdapterMsg, AdapterRequest, AlertListRequest, AuditListRequest,
    CommandStatusRequest, DeviceRequest, DroneProfile as ProfileMsg, ForceReleaseRequest,
    Geofence as FenceMsg, GeofenceDeleteRequest, GeofenceListRequest, GeofenceUpsertRequest,
    ListAdaptersRequest, PreemptRequest, ReleaseRequest, RenewRequest, Route as RouteMsg,
    RouteDeleteRequest, RouteExecuteRequest, RouteListRequest, RouteWaypoint,
    SetAdapterEnabledRequest, StateRequest, SubscribeRequest, TelemetryHistoryRequest,
};
use rustcam_drone_proto::link::adapter_link_client::AdapterLinkClient;
use serde::{Deserialize, Serialize};
use tonic::transport::{Channel, Endpoint};

use crate::config::DroneClientConfig;

pub type EventStream = Pin<Box<dyn Stream<Item = Result<DroneEventDto, String>> + Send>>;

/// 网关调用 drone-service 时返回的错误。
///
/// 携带 HTTP 状态码，避免把 gRPC 的内部细节原样透给浏览器（规则 §16：
/// gRPC 错误映射为 5xx）。
#[derive(Debug, Clone)]
pub struct DroneApiError {
    pub status: u16,
    pub message: String,
}

impl DroneApiError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: 503,
            message: message.into(),
        }
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: 404,
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            message: message.into(),
        }
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: 409,
            message: message.into(),
        }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DroneApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DroneApiError {}

/// 把 tonic 状态码映射成网关的 HTTP 状态码。
///
/// 4xx 保留服务端的说明文字（对操作员有用）；5xx 只给通用文案，
/// 不泄露内部实现细节。
pub fn map_grpc_status(status: &tonic::Status) -> DroneApiError {
    use tonic::Code;
    // 连接层错误（drone-service 未启动 / 正在重启）一律按“暂时不可用”处理，
    // 否则浏览器会看到 500 而不是可重试的 503。
    if is_transport_failure(status) {
        return DroneApiError::unavailable("drone-service 不可用");
    }
    match status.code() {
        Code::NotFound => DroneApiError::not_found(status.message()),
        Code::InvalidArgument | Code::FailedPrecondition | Code::OutOfRange => {
            DroneApiError::bad_request(status.message())
        }
        Code::PermissionDenied | Code::Unauthenticated => DroneApiError {
            status: 403,
            message: status.message().to_string(),
        },
        Code::AlreadyExists => DroneApiError::conflict(status.message()),
        Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted => {
            DroneApiError::unavailable("drone-service 不可用")
        }
        _ => DroneApiError::internal("drone-service 内部错误"),
    }
}

/// 判断错误是否来自连接层（而非 drone-service 的业务处理）。
///
/// tonic 在连接失败时把错误包成 `tonic::transport::Error`，
/// 其 message 固定为 "transport error"。
pub fn is_transport_failure(status: &tonic::Status) -> bool {
    use std::error::Error as _;

    if status.code() == tonic::Code::Unavailable {
        return true;
    }
    let mut source = status.source();
    while let Some(err) = source {
        if err.is::<tonic::transport::Error>() {
            return true;
        }
        source = err.source();
    }
    status.message() == "transport error"
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryDto {
    pub drone_tag: String,
    pub ts_ms: i64,
    pub lat: f64,
    pub lon: f64,
    pub alt_amsl: f64,
    pub rel_alt: f64,
    pub roll: f64,
    pub pitch: f64,
    pub yaw: f64,
    pub qx: f64,
    pub qy: f64,
    pub qz: f64,
    pub qw: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub battery_pct: f64,
    pub voltage: f64,
    pub link_quality: u32,
    pub seq: u32,
    pub autopilot: String,
    pub armed: bool,
    pub mode: String,
    pub satellites: u32,
    pub gps_fix: u32,
    pub hdop: f64,
    pub home_lat: f64,
    pub home_lon: f64,
    pub home_alt: f64,
    /// 飞控实测的唯一标识（`AUTOPILOT_VERSION.uid`）；空 = 飞控未提供。
    ///
    /// 与档案登记的 `unique_id` 比对即可判断链路身份是否一致。
    pub unique_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DroneDto {
    pub name: String,
    pub model: String,
    pub protocol: String,
    pub adapter_id: String,
    /// drone-service 生成的无人机唯一标识。
    #[serde(default)]
    pub drone_tag: String,
    /// 关联的摄像头设备 tag；由 media REST 层回填。
    #[serde(default)]
    pub camera_device_tags: Vec<String>,
    pub online: bool,
    pub enabled: bool,
    pub autopilot: String,
    pub armed: bool,
    pub mode: String,
    pub video_stream: String,
    pub notes: String,
    pub last_seen_ms: i64,
    pub last_telemetry: Option<TelemetryDto>,
    /// 登记的无人机唯一标识（`AUTOPILOT_VERSION.uid`）。
    pub unique_id: String,
    /// 该机的 MAVLink 端点（隧道内地址）。
    pub mavlink_addr: String,
    /// 该机的 MAVLink 2 签名密钥（服务端加密存储，接口返回密文）。
    pub signing_secret: String,
    /// 是否放行未签名帧（含 MAVLink 1 帧）。
    pub signing_allow_unsigned: bool,
    /// MAVLink 协议版本：1 / 2。
    pub mavlink_version: i32,
    /// 该机的 system id。
    pub mavlink_sysid: i32,
    /// 该机的 component id。
    pub mavlink_compid: i32,
}

/// 一台机载链路实体（adapter）的接入配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AdapterDto {
    /// 唯一标识；与适配器进程配置一致，创建后不可改。
    pub adapter_id: String,
    /// 显示名。
    pub name: String,
    /// 适配协议：mavlink / 厂商 SDK 等。
    pub protocol: String,
    /// 监听地址。
    pub bind_addr: String,
    /// 可用端口段起点。
    pub port_start: i32,
    /// 可用端口段终点。
    pub port_end: i32,
    /// 飞控类型：auto / px4 / ardupilot。
    pub autopilot: String,
    /// 签名密钥（服务端加密存储，接口返回密文）。
    pub signing_secret: String,
    /// 是否放行未签名帧。
    pub signing_allow_unsigned: bool,
    /// 重连退避下限（秒）。
    pub reconnect_min_secs: i32,
    /// 重连退避上限（秒）。
    pub reconnect_max_secs: i32,
    /// 适配器当前是否已接入。
    pub online: bool,
    /// 启停状态：false = 已停用（软删除）。
    pub enabled: bool,
    /// 绑定的无人机数量（REST 层统计回填）。
    pub bound_drones: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WaypointDto {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    #[serde(default)]
    pub speed: f64,
    #[serde(default)]
    pub loiter_sec: i32,
    #[serde(default)]
    pub action: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RouteDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub drone_tag: String,
    /// 适用无人机列表；为空表示所有无人机都可执行（模板）。
    #[serde(default)]
    pub drone_tags: Vec<String>,
    pub waypoints: Vec<WaypointDto>,
    pub enabled: bool,
    pub updated_ms: i64,
    /// 任务完成后的动作：`hold` / `rtl` / `land`。
    #[serde(default)]
    pub end_action: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FenceDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub rule_type: String,
    pub shape: String,
    pub geojson: String,
    pub center_lat: f64,
    pub center_lon: f64,
    pub radius_m: f64,
    pub min_alt: f64,
    pub max_alt: f64,
    pub drone_tag: String,
    /// 适用无人机列表；为空表示全部。
    #[serde(default)]
    pub drone_tags: Vec<String>,
    pub enabled: bool,
    pub version: i32,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandDto {
    pub cmd_id: String,
    pub drone_tag: String,
    pub status: String,
    pub message: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandRequest {
    pub kind: String,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub deadline_ms: i64,
    /// 客户端可自带 cmd_id 以获得幂等语义（规则 §6.8 步骤 4）；
    /// 为空时由服务端生成。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmd_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlertDto {
    pub drone_tag: String,
    pub rule: String,
    pub level: String,
    pub message: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditDto {
    pub ts_ms: i64,
    pub drone_tag: String,
    pub category: String,
    pub action: String,
    pub operator: String,
    pub result: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControlDto {
    pub drone_tag: String,
    pub control_type: String,
    pub holder: String,
    pub priority: i32,
    pub expire_at_ms: i64,
    pub granted: bool,
    pub reason: String,
    pub held: bool,
    /// 抢占成功时为被抢占者（规则 §6.7）。
    pub preempted: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DroneEventDto {
    pub msg_type: String,
    pub data: serde_json::Value,
    pub ts_ms: i64,
}

#[async_trait]
pub trait DroneApi: Send + Sync + 'static {
    async fn ping(&self) -> Result<(), DroneApiError>;
    async fn list_drones(&self) -> Result<Vec<DroneDto>, DroneApiError>;
    /// 列出无人机档案（含链路身份：端点、系统号等）。
    async fn list_drone_profiles(&self) -> Result<Vec<DroneDto>, DroneApiError>;
    async fn get_drone(&self, drone_tag: &str) -> Result<DroneDto, DroneApiError>;
    async fn save_drone(&self, drone: DroneDto, operator: &str) -> Result<DroneDto, DroneApiError>;
    async fn delete_drone(&self, drone_tag: &str) -> Result<bool, DroneApiError>;

    /// 列出机载链路实体（adapter）；`include_disabled` 为真时包含已停用的。
    async fn list_adapters(
        &self,
        include_disabled: bool,
    ) -> Result<Vec<AdapterDto>, DroneApiError>;
    /// 读取单个 adapter 配置。
    async fn get_adapter(&self, adapter_id: &str) -> Result<AdapterDto, DroneApiError>;
    /// 新建或更新 adapter 配置。
    async fn save_adapter(
        &self,
        adapter: AdapterDto,
        operator: &str,
    ) -> Result<AdapterDto, DroneApiError>;
    /// 停用（软删除）或恢复 adapter。
    async fn set_adapter_enabled(
        &self,
        adapter_id: &str,
        enabled: bool,
        operator: &str,
    ) -> Result<AdapterDto, DroneApiError>;

    async fn list_routes(&self, drone_tag: &str) -> Result<Vec<RouteDto>, DroneApiError>;
    async fn save_route(&self, route: RouteDto, operator: &str) -> Result<RouteDto, DroneApiError>;
    async fn delete_route(&self, id: &str, operator: &str) -> Result<bool, DroneApiError>;
    async fn execute_route(
        &self,
        id: &str,
        drone_tag: &str,
        lateral_offset_m: f64,
        operator: &str,
    ) -> Result<CommandDto, DroneApiError>;

    async fn list_fences(&self) -> Result<Vec<FenceDto>, DroneApiError>;
    async fn save_fence(&self, fence: FenceDto, operator: &str) -> Result<FenceDto, DroneApiError>;
    async fn delete_fence(&self, id: &str, operator: &str) -> Result<bool, DroneApiError>;

    async fn send_command(
        &self,
        drone_tag: &str,
        req: CommandRequest,
        operator: &str,
    ) -> Result<CommandDto, DroneApiError>;
    async fn get_command(&self, cmd_id: &str) -> Result<CommandDto, DroneApiError>;
    async fn list_alerts(
        &self,
        drone_tag: &str,
        limit: i32,
    ) -> Result<Vec<AlertDto>, DroneApiError>;
    async fn list_audits(
        &self,
        drone_tag: &str,
        limit: i32,
    ) -> Result<Vec<AuditDto>, DroneApiError>;
    /// 轨迹回放：按时间窗查询遥测历史。
    async fn telemetry_history(
        &self,
        drone_tag: &str,
        start_ms: i64,
        end_ms: i64,
        limit: i32,
    ) -> Result<Vec<TelemetryDto>, DroneApiError>;

    async fn acquire_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        priority: i32,
        lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError>;
    /// 续约控制权：只有当前持有人可以续约。
    async fn renew_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError>;
    /// 抢占控制权：需要更高的优先级。
    async fn preempt_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        priority: i32,
        lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError>;
    async fn release_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        force: bool,
    ) -> Result<bool, DroneApiError>;
    async fn get_control(
        &self,
        drone_tag: &str,
        control_type: &str,
    ) -> Result<ControlDto, DroneApiError>;

    async fn event_stream(&self, drone_tags: Vec<String>) -> Result<EventStream, DroneApiError>;
}

pub struct NoopDroneApi;

#[async_trait]
impl DroneApi for NoopDroneApi {
    async fn ping(&self) -> Result<(), DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn list_drones(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn list_drone_profiles(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn get_drone(&self, _drone_tag: &str) -> Result<DroneDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn save_drone(
        &self,
        _drone: DroneDto,
        _operator: &str,
    ) -> Result<DroneDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn delete_drone(&self, _drone_tag: &str) -> Result<bool, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn list_adapters(&self, _include_disabled: bool) -> Result<Vec<AdapterDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn get_adapter(&self, _adapter_id: &str) -> Result<AdapterDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn save_adapter(
        &self,
        _adapter: AdapterDto,
        _operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn set_adapter_enabled(
        &self,
        _adapter_id: &str,
        _enabled: bool,
        _operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn list_routes(&self, _drone_tag: &str) -> Result<Vec<RouteDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn save_route(
        &self,
        _route: RouteDto,
        _operator: &str,
    ) -> Result<RouteDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn delete_route(&self, _id: &str, _operator: &str) -> Result<bool, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn execute_route(
        &self,
        _id: &str,
        _drone_tag: &str,
        _lateral_offset_m: f64,
        _operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn list_fences(&self) -> Result<Vec<FenceDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn save_fence(
        &self,
        _fence: FenceDto,
        _operator: &str,
    ) -> Result<FenceDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn delete_fence(&self, _id: &str, _operator: &str) -> Result<bool, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn send_command(
        &self,
        _drone_tag: &str,
        _req: CommandRequest,
        _operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn get_command(&self, _cmd_id: &str) -> Result<CommandDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn list_alerts(
        &self,
        _drone_tag: &str,
        _limit: i32,
    ) -> Result<Vec<AlertDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn list_audits(
        &self,
        _drone_tag: &str,
        _limit: i32,
    ) -> Result<Vec<AuditDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn telemetry_history(
        &self,
        _drone_tag: &str,
        _start_ms: i64,
        _end_ms: i64,
        _limit: i32,
    ) -> Result<Vec<TelemetryDto>, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn acquire_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _priority: i32,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn renew_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn preempt_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _priority: i32,
        _lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn release_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
        _operator: &str,
        _force: bool,
    ) -> Result<bool, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn get_control(
        &self,
        _drone_tag: &str,
        _control_type: &str,
    ) -> Result<ControlDto, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
    async fn event_stream(&self, _drone_tags: Vec<String>) -> Result<EventStream, DroneApiError> {
        Err(DroneApiError::unavailable("drone-service 未配置"))
    }
}

#[derive(Clone)]
pub struct DroneClient {
    pub control: ControlServiceClient<Channel>,
    pub query: QueryServiceClient<Channel>,
    pub geofence: GeofenceServiceClient<Channel>,
    pub admin: AdminServiceClient<Channel>,
    pub events: EventServiceClient<Channel>,
    pub adapter_link: AdapterLinkClient<Channel>,
    /// 事件流重连的退避下限
    reconnect_min: Duration,
    /// 事件流重连的退避上限
    reconnect_max: Duration,
}

impl DroneClient {
    /// 建立到 drone-service 的客户端。
    ///
    /// 使用惰性连接：drone-service 尚未启动或中途重启都不会让主服务启动失败，
    /// 真正的建连推迟到第一次调用；之后由 tonic 的 Channel 负责断线重连。
    /// 只有地址本身非法（无法解析 URI）才算致命错误。
    pub async fn connect(config: &DroneClientConfig) -> anyhow::Result<DroneClient> {
        let endpoint = Endpoint::from_shared(config.service_addr.clone())?
            .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
            .timeout(Duration::from_secs(config.request_timeout_secs));
        let channel = endpoint.connect_lazy();
        Ok(DroneClient {
            control: ControlServiceClient::new(channel.clone()),
            query: QueryServiceClient::new(channel.clone()),
            geofence: GeofenceServiceClient::new(channel.clone()),
            admin: AdminServiceClient::new(channel.clone()),
            events: EventServiceClient::new(channel.clone()),
            adapter_link: AdapterLinkClient::new(channel),
            reconnect_min: Duration::from_secs(config.reconnect_min_secs.max(1)),
            reconnect_max: Duration::from_secs(
                config.reconnect_max_secs.max(config.reconnect_min_secs.max(1)),
            ),
        })
    }

    /// 订阅 drone-service 事件流，断线后自动退避重连。
    ///
    /// drone-service 尚未启动时不会失败：订阅在后台任务里按需发起，
    /// 失败后指数退避重试，直到成功为止。浏览器侧的连接因此可以一直保持，
    /// drone-service 恢复后事件自动续上。
    fn resilient_event_stream(&self, drone_tags: Vec<String>) -> EventStream {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<DroneEventDto, String>>(256);
        let client = self.events.clone();
        let reconnect_min = self.reconnect_min;
        let reconnect_max = self.reconnect_max;
        tokio::spawn(async move {
            let mut backoff = Backoff::with_jitter(reconnect_min, reconnect_max, 0.2);
            let mut current: Option<tonic::Streaming<rustcam_drone_proto::control::EventMessage>> =
                None;
            loop {
                if current.is_none() {
                    let mut client = client.clone();
                    match client
                        .subscribe(SubscribeRequest {
                            drone_tags: drone_tags.clone(),
                        })
                        .await
                    {
                        Ok(resp) => {
                            backoff.reset();
                            current = Some(resp.into_inner());
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "drone-service 事件订阅失败，退避后重试");
                            tokio::select! {
                                _ = tokio::time::sleep(backoff.next_delay()) => {}
                                _ = tx.closed() => return,
                            }
                            continue;
                        }
                    }
                }

                let mut stream = current.take().expect("事件流已建立");
                // 消费端（浏览器）断开时立即退出，避免留下悬挂任务。
                let message = tokio::select! {
                    msg = stream.message() => msg,
                    _ = tx.closed() => return,
                };
                match message {
                    Ok(Some(msg)) => {
                        current = Some(stream);
                        let dto = DroneEventDto {
                            msg_type: msg.msg_type,
                            data: serde_json::from_str(&msg.payload_json)
                                .unwrap_or(serde_json::Value::Null),
                            ts_ms: msg.ts_ms,
                        };
                        if tx.send(Ok(dto)).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("drone-service 事件流结束，退避后重连");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "drone-service 事件流中断，退避后重连");
                    }
                }
                if tx.is_closed() {
                    return;
                }
                tokio::select! {
                    _ = tokio::time::sleep(backoff.next_delay()) => {}
                    _ = tx.closed() => return,
                }
            }
        });
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

fn telemetry_dto(t: proto::Telemetry) -> TelemetryDto {
    TelemetryDto {
        drone_tag: t.drone_tag,
        ts_ms: t.ts_ms,
        lat: t.lat,
        lon: t.lon,
        alt_amsl: t.alt_amsl,
        rel_alt: t.rel_alt,
        roll: t.roll,
        pitch: t.pitch,
        yaw: t.yaw,
        qx: t.qx,
        qy: t.qy,
        qz: t.qz,
        qw: t.qw,
        vx: t.vx,
        vy: t.vy,
        vz: t.vz,
        battery_pct: t.battery_pct,
        voltage: t.voltage,
        link_quality: t.link_quality,
        seq: t.seq,
        autopilot: t.autopilot,
        armed: t.armed,
        mode: t.mode,
        satellites: t.satellites,
        gps_fix: t.gps_fix,
        hdop: t.hdop,
        home_lat: t.home_lat,
        home_lon: t.home_lon,
        home_alt: t.home_alt,
        unique_id: t.unique_id,
    }
}

fn drone_dto(info: rustcam_drone_proto::control::DeviceInfo) -> DroneDto {
    DroneDto {
        name: info.name,
        model: info.model,
        protocol: info.protocol,
        adapter_id: info.adapter_id,
        drone_tag: info.drone_tag,
        online: info.online,
        enabled: info.enabled,
        autopilot: info.autopilot,
        armed: info.armed,
        mode: info.mode,
        video_stream: info.video_stream,
        notes: info.notes,
        last_seen_ms: info.last_seen_ms,
        last_telemetry: info.last_telemetry.map(telemetry_dto),
        // 设备列表不含链路身份字段；档案接口（profile_dto）才带。
        ..Default::default()
    }
}

fn route_dto(route: RouteMsg) -> RouteDto {
    RouteDto {
        id: route.id,
        name: route.name,
        drone_tag: route.drone_tag,
        waypoints: route
            .waypoints
            .into_iter()
            .map(|w| WaypointDto {
                lat: w.lat,
                lon: w.lon,
                alt: w.alt,
                speed: w.speed,
                loiter_sec: w.loiter_sec,
                action: w.action,
            })
            .collect(),
        enabled: route.enabled,
        updated_ms: route.updated_ms,
        end_action: route.end_action,
        drone_tags: route.drone_tags,
    }
}

fn route_msg(route: RouteDto, operator: &str) -> RouteMsg {
    RouteMsg {
        id: route.id,
        name: route.name,
        drone_tag: route.drone_tag,
        waypoints: route
            .waypoints
            .into_iter()
            .map(|w| RouteWaypoint {
                lat: w.lat,
                lon: w.lon,
                alt: w.alt,
                speed: w.speed,
                loiter_sec: w.loiter_sec,
                action: w.action,
            })
            .collect(),
        enabled: route.enabled,
        updated_ms: route.updated_ms,
        operator: operator.to_string(),
        end_action: route.end_action,
        drone_tags: route.drone_tags,
    }
}

fn fence_dto(fence: FenceMsg) -> FenceDto {
    FenceDto {
        id: fence.id,
        name: fence.name,
        rule_type: fence.r#type,
        shape: fence.shape,
        geojson: fence.geojson,
        center_lat: fence.center_lat,
        center_lon: fence.center_lon,
        radius_m: fence.radius_m,
        min_alt: fence.min_alt,
        max_alt: fence.max_alt,
        drone_tag: fence.drone_tag,
        drone_tags: fence.drone_tags,
        enabled: fence.enabled,
        version: fence.version,
        updated_ms: fence.updated_ms,
    }
}

fn fence_msg(fence: FenceDto) -> FenceMsg {
    FenceMsg {
        id: fence.id,
        name: fence.name,
        r#type: fence.rule_type,
        geojson: fence.geojson,
        min_alt: fence.min_alt,
        max_alt: fence.max_alt,
        enabled: fence.enabled,
        version: fence.version,
        center_lat: fence.center_lat,
        center_lon: fence.center_lon,
        radius_m: fence.radius_m,
        drone_tag: fence.drone_tag,
        drone_tags: fence.drone_tags,
        shape: fence.shape,
        updated_ms: fence.updated_ms,
    }
}

fn command_dto(reply: proto::CommandReply) -> CommandDto {
    CommandDto {
        cmd_id: reply.cmd_id,
        drone_tag: reply.drone_tag,
        status: reply.status,
        message: reply.message,
        ts_ms: reply.ts_ms,
    }
}

fn profile_msg(drone: DroneDto, operator: &str) -> ProfileMsg {
    ProfileMsg {
        name: drone.name,
        model: drone.model,
        autopilot: drone.autopilot,
        video_stream: drone.video_stream,
        notes: drone.notes,
        enabled: drone.enabled,
        created_ms: 0,
        updated_ms: 0,
        operator: operator.to_string(),
        unique_id: drone.unique_id,
        mavlink_addr: drone.mavlink_addr,
        signing_secret: drone.signing_secret,
        signing_allow_unsigned: drone.signing_allow_unsigned,
        mavlink_version: drone.mavlink_version,
        mavlink_sysid: drone.mavlink_sysid,
        mavlink_compid: drone.mavlink_compid,
        adapter_id: drone.adapter_id,
        drone_tag: drone.drone_tag,
    }
}

fn profile_dto(profile: ProfileMsg) -> DroneDto {
    DroneDto {
        name: profile.name,
        model: profile.model,
        protocol: "mavlink".into(),
        autopilot: profile.autopilot,
        video_stream: profile.video_stream,
        notes: profile.notes,
        enabled: profile.enabled,
        last_seen_ms: 0,
        unique_id: profile.unique_id,
        mavlink_addr: profile.mavlink_addr,
        signing_secret: profile.signing_secret,
        signing_allow_unsigned: profile.signing_allow_unsigned,
        mavlink_version: profile.mavlink_version,
        mavlink_sysid: profile.mavlink_sysid,
        mavlink_compid: profile.mavlink_compid,
        adapter_id: profile.adapter_id,
        drone_tag: profile.drone_tag,
        ..Default::default()
    }
}

/// adapter 配置转协议消息。
fn adapter_msg(adapter: AdapterDto) -> AdapterMsg {
    AdapterMsg {
        adapter_id: adapter.adapter_id,
        name: adapter.name,
        protocol: adapter.protocol,
        bind_addr: adapter.bind_addr,
        port_start: adapter.port_start,
        port_end: adapter.port_end,
        autopilot: adapter.autopilot,
        signing_secret: adapter.signing_secret,
        signing_allow_unsigned: adapter.signing_allow_unsigned,
        reconnect_min_secs: adapter.reconnect_min_secs,
        reconnect_max_secs: adapter.reconnect_max_secs,
        online: adapter.online,
        enabled: adapter.enabled,
        operator: String::new(),
    }
}

/// 协议消息转 adapter 配置。
fn adapter_dto(adapter: AdapterMsg) -> AdapterDto {
    AdapterDto {
        adapter_id: adapter.adapter_id,
        name: adapter.name,
        protocol: adapter.protocol,
        bind_addr: adapter.bind_addr,
        port_start: adapter.port_start,
        port_end: adapter.port_end,
        autopilot: adapter.autopilot,
        signing_secret: adapter.signing_secret,
        signing_allow_unsigned: adapter.signing_allow_unsigned,
        reconnect_min_secs: adapter.reconnect_min_secs,
        reconnect_max_secs: adapter.reconnect_max_secs,
        online: adapter.online,
        enabled: adapter.enabled,
        bound_drones: 0,
    }
}

#[async_trait]
impl DroneApi for DroneClient {
    async fn ping(&self) -> Result<(), DroneApiError> {
        let mut query = self.query.clone();
        query
            .list_devices(proto::Empty {})
            .await
            .map(|_| ())
            .map_err(|e| map_grpc_status(&e))
    }

    async fn list_drones(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        let mut query = self.query.clone();
        let resp = query
            .list_devices(proto::Empty {})
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp
            .into_inner()
            .devices
            .into_iter()
            .map(drone_dto)
            .collect())
    }

    async fn list_drone_profiles(&self) -> Result<Vec<DroneDto>, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .list_drone_profiles(proto::Empty {})
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp
            .into_inner()
            .profiles
            .into_iter()
            .map(profile_dto)
            .collect())
    }

    async fn get_drone(&self, drone_tag: &str) -> Result<DroneDto, DroneApiError> {
        let mut query = self.query.clone();
        let resp = query
            .get_device(DeviceRequest {
                drone_tag: drone_tag.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(drone_dto(resp.into_inner()))
    }

    async fn save_drone(&self, drone: DroneDto, operator: &str) -> Result<DroneDto, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .upsert_drone_profile(profile_msg(drone, operator))
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(profile_dto(resp.into_inner()))
    }

    async fn delete_drone(&self, drone_tag: &str) -> Result<bool, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .delete_drone_profile(DeviceRequest {
                drone_tag: drone_tag.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp.into_inner().deleted)
    }

    async fn list_adapters(&self, include_disabled: bool) -> Result<Vec<AdapterDto>, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .list_adapters(ListAdaptersRequest { include_disabled })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp
            .into_inner()
            .adapters
            .into_iter()
            .map(adapter_dto)
            .collect())
    }

    async fn get_adapter(&self, adapter_id: &str) -> Result<AdapterDto, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .get_adapter(AdapterRequest {
                adapter_id: adapter_id.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(adapter_dto(resp.into_inner()))
    }

    async fn save_adapter(
        &self,
        adapter: AdapterDto,
        operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        let mut admin = self.admin.clone();
        let mut msg = adapter_msg(adapter);
        msg.operator = operator.to_string();
        let resp = admin
            .upsert_adapter(msg)
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(adapter_dto(resp.into_inner()))
    }

    async fn set_adapter_enabled(
        &self,
        adapter_id: &str,
        enabled: bool,
        operator: &str,
    ) -> Result<AdapterDto, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .set_adapter_enabled(SetAdapterEnabledRequest {
                adapter_id: adapter_id.to_string(),
                enabled,
                operator: operator.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(adapter_dto(resp.into_inner()))
    }

    async fn list_routes(&self, drone_tag: &str) -> Result<Vec<RouteDto>, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .list_routes(RouteListRequest {
                drone_tag: drone_tag.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp
            .into_inner()
            .routes
            .into_iter()
            .map(route_dto)
            .collect())
    }

    async fn save_route(&self, route: RouteDto, operator: &str) -> Result<RouteDto, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .upsert_route(route_msg(route, operator))
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(route_dto(resp.into_inner()))
    }

    async fn delete_route(&self, id: &str, operator: &str) -> Result<bool, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .delete_route(RouteDeleteRequest {
                id: id.to_string(),
                operator: operator.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp.into_inner().deleted)
    }

    async fn execute_route(
        &self,
        id: &str,
        drone_tag: &str,
        lateral_offset_m: f64,
        operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .execute_route(RouteExecuteRequest {
                route_id: id.to_string(),
                operator: operator.to_string(),
                drone_tag: drone_tag.to_string(),
                lateral_offset_m,
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(command_dto(resp.into_inner()))
    }

    async fn list_fences(&self) -> Result<Vec<FenceDto>, DroneApiError> {
        let mut geofence = self.geofence.clone();
        let resp = geofence
            .list_geofences(GeofenceListRequest {
                only_enabled: false,
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp
            .into_inner()
            .geofences
            .into_iter()
            .map(fence_dto)
            .collect())
    }

    async fn save_fence(&self, fence: FenceDto, operator: &str) -> Result<FenceDto, DroneApiError> {
        let mut geofence = self.geofence.clone();
        let resp = geofence
            .create_geofence(GeofenceUpsertRequest {
                geofence: Some(fence_msg(fence)),
                operator: operator.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(fence_dto(resp.into_inner()))
    }

    async fn delete_fence(&self, id: &str, operator: &str) -> Result<bool, DroneApiError> {
        let mut geofence = self.geofence.clone();
        let resp = geofence
            .delete_geofence(GeofenceDeleteRequest {
                id: id.to_string(),
                operator: operator.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp.into_inner().deleted)
    }

    async fn send_command(
        &self,
        drone_tag: &str,
        req: CommandRequest,
        operator: &str,
    ) -> Result<CommandDto, DroneApiError> {
        let mut control = self.control.clone();
        // 客户端自带 cmd_id 时透传，保证重试幂等（规则 §6.8 步骤 4）。
        let cmd_id = req
            .cmd_id
            .as_deref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let resp = control
            .send_command(proto::Command {
                cmd_id,
                drone_tag: drone_tag.to_string(),
                r#type: req.kind,
                params: req.params,
                deadline_ms: req.deadline_ms,
                operator: operator.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(command_dto(resp.into_inner()))
    }

    async fn get_command(&self, cmd_id: &str) -> Result<CommandDto, DroneApiError> {
        let mut query = self.query.clone();
        let resp = query
            .get_command_status(CommandStatusRequest {
                cmd_id: cmd_id.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        let status = resp.into_inner();
        Ok(CommandDto {
            cmd_id: status.cmd_id,
            drone_tag: status.drone_tag,
            status: status.status,
            message: status.message,
            ts_ms: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn list_alerts(
        &self,
        drone_tag: &str,
        limit: i32,
    ) -> Result<Vec<AlertDto>, DroneApiError> {
        let mut query = self.query.clone();
        let resp = query
            .list_alerts(AlertListRequest {
                drone_tag: drone_tag.to_string(),
                limit,
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp
            .into_inner()
            .alerts
            .into_iter()
            .map(|a| AlertDto {
                drone_tag: a.drone_tag,
                rule: a.rule,
                level: a.level,
                message: a.message,
                ts_ms: a.ts_ms,
            })
            .collect())
    }

    async fn list_audits(
        &self,
        drone_tag: &str,
        limit: i32,
    ) -> Result<Vec<AuditDto>, DroneApiError> {
        let mut admin = self.admin.clone();
        let resp = admin
            .list_audits(AuditListRequest {
                drone_tag: drone_tag.to_string(),
                limit,
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp
            .into_inner()
            .records
            .into_iter()
            .map(|a| AuditDto {
                ts_ms: a.ts_ms,
                drone_tag: a.drone_tag,
                category: a.category,
                action: a.action,
                operator: a.operator,
                result: a.result,
                detail: a.detail,
            })
            .collect())
    }

    async fn telemetry_history(
        &self,
        drone_tag: &str,
        start_ms: i64,
        end_ms: i64,
        limit: i32,
    ) -> Result<Vec<TelemetryDto>, DroneApiError> {
        let mut query = self.query.clone();
        let resp = query
            .get_telemetry_history(TelemetryHistoryRequest {
                drone_tag: drone_tag.to_string(),
                start_ms,
                end_ms,
                limit,
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp
            .into_inner()
            .samples
            .into_iter()
            .map(telemetry_dto)
            .collect())
    }

    async fn acquire_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        priority: i32,
        lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        let mut control = self.control.clone();
        let resp = control
            .acquire_control(AcquireRequest {
                drone_tag: drone_tag.to_string(),
                control_type: control_type.to_string(),
                operator: operator.to_string(),
                priority,
                lease_seconds,
            })
            .await
            .map_err(|e| map_grpc_status(&e))?
            .into_inner();
        Ok(ControlDto {
            drone_tag: resp.drone_tag,
            control_type: resp.control_type,
            holder: resp.holder,
            priority: resp.priority,
            expire_at_ms: resp.expire_at_ms,
            granted: resp.granted,
            reason: resp.reason,
            held: true,
            preempted: resp.preempted,
        })
    }

    async fn renew_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        let mut control = self.control.clone();
        let resp = control
            .renew_control(RenewRequest {
                drone_tag: drone_tag.to_string(),
                control_type: control_type.to_string(),
                operator: operator.to_string(),
                lease_seconds,
            })
            .await
            .map_err(|e| map_grpc_status(&e))?
            .into_inner();
        Ok(ControlDto {
            drone_tag: resp.drone_tag,
            control_type: resp.control_type,
            holder: resp.holder,
            priority: resp.priority,
            expire_at_ms: resp.expire_at_ms,
            granted: resp.granted,
            reason: resp.reason,
            held: resp.granted,
            preempted: resp.preempted,
        })
    }

    async fn preempt_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        priority: i32,
        lease_seconds: i32,
    ) -> Result<ControlDto, DroneApiError> {
        let mut control = self.control.clone();
        let resp = control
            .preempt_control(PreemptRequest {
                drone_tag: drone_tag.to_string(),
                control_type: control_type.to_string(),
                operator: operator.to_string(),
                priority,
                lease_seconds,
            })
            .await
            .map_err(|e| map_grpc_status(&e))?
            .into_inner();
        Ok(ControlDto {
            drone_tag: resp.drone_tag,
            control_type: resp.control_type,
            holder: resp.holder,
            priority: resp.priority,
            expire_at_ms: resp.expire_at_ms,
            granted: resp.granted,
            reason: resp.reason,
            held: resp.granted,
            preempted: resp.preempted,
        })
    }

    async fn release_control(
        &self,
        drone_tag: &str,
        control_type: &str,
        operator: &str,
        force: bool,
    ) -> Result<bool, DroneApiError> {
        let mut control = self.control.clone();
        if force {
            let resp = control
                .force_release_control(ForceReleaseRequest {
                    drone_tag: drone_tag.to_string(),
                    control_type: control_type.to_string(),
                    operator: operator.to_string(),
                })
                .await
                .map_err(|e| map_grpc_status(&e))?
                .into_inner();
            return Ok(resp.released);
        }
        let resp = control
            .release_control(ReleaseRequest {
                drone_tag: drone_tag.to_string(),
                control_type: control_type.to_string(),
                operator: operator.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?;
        Ok(resp.into_inner().released)
    }

    async fn get_control(
        &self,
        drone_tag: &str,
        control_type: &str,
    ) -> Result<ControlDto, DroneApiError> {
        let mut control = self.control.clone();
        let resp = control
            .get_control_state(StateRequest {
                drone_tag: drone_tag.to_string(),
                control_type: control_type.to_string(),
            })
            .await
            .map_err(|e| map_grpc_status(&e))?
            .into_inner();
        match resp.lease {
            Some(lease) => Ok(ControlDto {
                drone_tag: lease.drone_tag,
                control_type: lease.control_type,
                holder: lease.holder,
                priority: lease.priority,
                expire_at_ms: lease.expire_at_ms,
                granted: resp.held,
                reason: lease.reason,
                held: resp.held,
                preempted: lease.preempted,
            }),
            None => Ok(ControlDto {
                drone_tag: drone_tag.to_string(),
                control_type: control_type.to_string(),
                held: false,
                ..Default::default()
            }),
        }
    }

    async fn event_stream(&self, drone_tags: Vec<String>) -> Result<EventStream, DroneApiError> {
        // 返回自愈流：drone-service 未启动或重启时内部退避重订阅，
        // 浏览器连接不会因此断开。
        Ok(self.resilient_event_stream(drone_tags))
    }
}

/// 兼容旧调用：设备列表 DTO 与 DroneDto 同构，保留别名。
pub type DeviceInfoDto = DroneDto;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_dto_carries_drone_tag() {
        let dto = telemetry_dto(proto::Telemetry {
            drone_tag: "drone-a".into(),
            unique_id: "0011223344556677".into(),
            ..Default::default()
        });
        assert_eq!(dto.drone_tag, "drone-a");
        assert_eq!(dto.unique_id, "0011223344556677");
    }

    #[test]
    fn telemetry_dto_carries_observed_unique_id() {
        // 实测唯一标识必须透出到 REST 层：运维要靠它和档案登记值比对。
        let msg = proto::Telemetry {
            drone_tag: "drone-001".into(),
            unique_id: "0011223344556677".into(),
            ..Default::default()
        };
        let dto = telemetry_dto(msg);
        assert_eq!(dto.unique_id, "0011223344556677");
    }

    #[test]
    fn profile_round_trip_keeps_link_identity_fields() {
        let dto = DroneDto {
            unique_id: "0123456789abcdef".into(),
            mavlink_addr: "udpin:10.88.0.2:14540".into(),
            signing_secret: "v1:00112233".into(),
            signing_allow_unsigned: true,
            mavlink_version: 2,
            mavlink_sysid: 3,
            mavlink_compid: 1,
            adapter_id: "adapter-1".into(),
            drone_tag: "drone-abc".into(),
            camera_device_tags: vec!["cam-001".into()],
            ..Default::default()
        };
        let msg = profile_msg(dto.clone(), "alice");
        assert_eq!(msg.unique_id, "0123456789abcdef");
        assert_eq!(msg.mavlink_addr, "udpin:10.88.0.2:14540");
        assert_eq!(msg.signing_secret, "v1:00112233");
        assert_eq!(msg.mavlink_version, 2);
        assert_eq!(msg.mavlink_sysid, 3);
        assert_eq!(msg.adapter_id, "adapter-1");
        assert_eq!(msg.drone_tag, "drone-abc");

        let back = profile_dto(msg);
        assert_eq!(back.unique_id, dto.unique_id);
        assert_eq!(back.mavlink_addr, dto.mavlink_addr);
        assert_eq!(back.signing_secret, dto.signing_secret);
        assert_eq!(back.mavlink_version, dto.mavlink_version);
        assert_eq!(back.mavlink_sysid, dto.mavlink_sysid);
        assert_eq!(back.mavlink_compid, dto.mavlink_compid);
        assert_eq!(back.adapter_id, dto.adapter_id);
        assert_eq!(back.drone_tag, dto.drone_tag);
    }

    #[test]
    fn drone_dto_json_round_trip_keeps_drone_tag_and_camera_tags() {
        let dto = DroneDto {
            drone_tag: "drone-abc".into(),
            camera_device_tags: vec!["cam-001".into(), "cam-002".into()],
            ..Default::default()
        };
        let json = serde_json::to_string(&dto).expect("serialize");
        let round: DroneDto = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round.drone_tag, "drone-abc");
        assert_eq!(round.camera_device_tags, vec!["cam-001", "cam-002"]);
    }

    #[test]
    fn drone_dto_without_camera_tags_deserializes_with_empty_list() {
        let round: DroneDto =
            serde_json::from_str(r#"{"drone_tag":"drone-001"}"#).expect("deserialize");
        assert_eq!(round.drone_tag, "drone-001");
        assert!(round.camera_device_tags.is_empty());
    }

    #[test]
    fn adapter_dto_round_trip_keeps_config_fields() {
        let dto = AdapterDto {
            adapter_id: "adapter-1".into(),
            name: "东场链路".into(),
            protocol: "mavlink".into(),
            bind_addr: "0.0.0.0".into(),
            port_start: 14540,
            port_end: 14560,
            autopilot: "auto".into(),
            signing_secret: "00112233".into(),
            signing_allow_unsigned: true,
            reconnect_min_secs: 1,
            reconnect_max_secs: 30,
            online: true,
            enabled: true,
            bound_drones: 3,
        };
        let msg = adapter_msg(dto.clone());
        assert_eq!(msg.adapter_id, "adapter-1");
        assert_eq!(msg.port_start, 14540);
        assert_eq!(msg.port_end, 14560);
        assert!(msg.enabled);

        let back = adapter_dto(msg);
        assert_eq!(back.adapter_id, dto.adapter_id);
        assert_eq!(back.name, dto.name);
        assert_eq!(back.protocol, dto.protocol);
        assert_eq!(back.bind_addr, dto.bind_addr);
        assert_eq!(back.port_start, dto.port_start);
        assert_eq!(back.port_end, dto.port_end);
        assert_eq!(back.autopilot, dto.autopilot);
        assert_eq!(back.signing_allow_unsigned, dto.signing_allow_unsigned);
        assert_eq!(back.reconnect_min_secs, dto.reconnect_min_secs);
        assert_eq!(back.reconnect_max_secs, dto.reconnect_max_secs);
        assert!(back.online);
        assert!(back.enabled);
    }

    #[test]
    fn adapter_dto_json_round_trip_keeps_enabled_flag() {
        let dto = AdapterDto {
            adapter_id: "adapter-1".into(),
            enabled: false,
            ..Default::default()
        };
        let json = serde_json::to_string(&dto).expect("serialize");
        let round: AdapterDto = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round.adapter_id, "adapter-1");
        assert!(!round.enabled);
    }
}
