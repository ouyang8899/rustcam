use rbatis::html_sql;
use serde::Deserialize;

use crate::infra::db::{AdapterRow, DeviceLinkRow};

#[derive(Debug, Clone, Deserialize)]
pub struct PresentRow {
    pub present: i32,
}

/// 占用链路身份的无人机（用于唯一性冲突提示）。
#[derive(Debug, Clone, Deserialize)]
pub struct LinkIdentityOwner {
    pub drone_tag: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteRow {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub end_action: String,
    pub updated_ms: i64,
    pub waypoints: String,
    pub drone_tags: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FenceRow {
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
    pub drone_tags: String,
    pub enabled: bool,
    pub version: i32,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelemetryPointRow {
    pub drone_tag: String,
    pub ts_ms: i64,
    pub lat: f64,
    pub lon: f64,
    pub alt_amsl: f64,
    pub rel_alt: f64,
    pub roll: f64,
    pub pitch: f64,
    pub yaw: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub qx: f64,
    pub qy: f64,
    pub qz: f64,
    pub qw: f64,
    pub battery_pct: f64,
    pub voltage: f64,
    pub link_quality: i32,
    pub seq: i32,
    pub satellites: i32,
    pub gps_fix: i32,
    pub hdop: f64,
    pub armed: bool,
    pub mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlertRow {
    pub ts_ms: i64,
    pub drone_tag: String,
    pub rule: String,
    pub level: String,
    pub message: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt: Option<f64>,
}

#[html_sql("sql/drone_device.html")]
pub async fn drone_device_link_select_by_adapter(
    rb: &dyn rbatis::Executor,
    adapter_id: &str,
) -> rbatis::Result<Vec<DeviceLinkRow>> {
    impled!()
}

#[html_sql("sql/drone_device.html")]
pub async fn drone_device_link_select_by_tag(
    rb: &dyn rbatis::Executor,
    drone_tag: &str,
) -> rbatis::Result<Vec<DeviceLinkRow>> {
    impled!()
}

#[html_sql("sql/drone_device.html")]
pub async fn drone_device_link_identity_taken(
    rb: &dyn rbatis::Executor,
    adapter_id: &str,
    link_family: &str,
    link_endpoint: &str,
    fc_sysid: i32,
    drone_tag: &str,
) -> rbatis::Result<Vec<LinkIdentityOwner>> {
    impled!()
}

#[html_sql("sql/drone_device.html")]
pub async fn drone_device_link_upsert(
    rb: &dyn rbatis::Executor,
    drone_tag: &str,
    adapter_id: &str,
    unique_id: &str,
    mavlink_addr: &str,
    signing_secret: &str,
    signing_allow_unsigned: bool,
    mavlink_version: i32,
    mavlink_sysid: i32,
    mavlink_compid: i32,
    link_family: &str,
    link_endpoint: &str,
    fc_sysid: i32,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_device.html")]
pub async fn drone_device_link_delete(
    rb: &dyn rbatis::Executor,
    drone_tag: &str,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_adapter.html")]
pub async fn drone_adapter_select_by_id(
    rb: &dyn rbatis::Executor,
    adapter_id: &str,
) -> rbatis::Result<Vec<AdapterRow>> {
    impled!()
}

#[html_sql("sql/drone_adapter.html")]
pub async fn drone_adapter_select_all(
    rb: &dyn rbatis::Executor,
    include_disabled: bool,
) -> rbatis::Result<Vec<AdapterRow>> {
    impled!()
}

#[html_sql("sql/drone_adapter.html")]
pub async fn drone_adapter_upsert(
    rb: &dyn rbatis::Executor,
    adapter_id: &str,
    name: &str,
    protocol: &str,
    bind_addr: &str,
    port_start: i32,
    port_end: i32,
    autopilot: &str,
    signing_secret: &str,
    signing_allow_unsigned: bool,
    reconnect_min_secs: i32,
    reconnect_max_secs: i32,
    online: bool,
    enabled: bool,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_device.html")]
pub async fn drone_device_legacy_column_exists(
    rb: &dyn rbatis::Executor,
    table_name: &str,
    column_name: &str,
) -> rbatis::Result<Vec<PresentRow>> {
    impled!()
}

#[html_sql("sql/drone_route.html")]
pub async fn drone_route_select_all(
    rb: &dyn rbatis::Executor,
) -> rbatis::Result<Vec<RouteRow>> {
    impled!()
}

#[html_sql("sql/drone_route.html")]
pub async fn drone_route_upsert(
    rb: &dyn rbatis::Executor,
    id: &str,
    name: &str,
    waypoints: &str,
    enabled: bool,
    end_action: &str,
    updated_ms: i64,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_route.html")]
pub async fn drone_route_devices_replace(
    rb: &dyn rbatis::Executor,
    route_id: &str,
    tags: &str,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_route.html")]
pub async fn drone_route_delete(
    rb: &dyn rbatis::Executor,
    id: &str,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_fence.html")]
pub async fn drone_fence_select_all(
    rb: &dyn rbatis::Executor,
) -> rbatis::Result<Vec<FenceRow>> {
    impled!()
}

#[html_sql("sql/drone_fence.html")]
#[allow(clippy::too_many_arguments)]
pub async fn drone_fence_upsert(
    rb: &dyn rbatis::Executor,
    id: &str,
    name: &str,
    rule_type: &str,
    shape: &str,
    geojson: &str,
    center_lat: f64,
    center_lon: f64,
    radius_m: f64,
    min_alt: f64,
    max_alt: f64,
    enabled: bool,
    version: i32,
    updated_ms: i64,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_fence.html")]
pub async fn drone_fence_devices_replace(
    rb: &dyn rbatis::Executor,
    fence_id: &str,
    tags: &str,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_fence.html")]
pub async fn drone_fence_delete(
    rb: &dyn rbatis::Executor,
    id: &str,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_telemetry.html")]
pub async fn drone_telemetry_insert_batch(
    rb: &dyn rbatis::Executor,
    payload: &str,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_telemetry.html")]
pub async fn drone_telemetry_select_window(
    rb: &dyn rbatis::Executor,
    drone_tag: &str,
    start_ms: i64,
    end_ms: i64,
    limit: i64,
) -> rbatis::Result<Vec<TelemetryPointRow>> {
    impled!()
}

#[html_sql("sql/drone_alert.html")]
pub async fn drone_alert_insert(
    rb: &dyn rbatis::Executor,
    ts_ms: i64,
    drone_tag: &str,
    rule: &str,
    level: &str,
    message: &str,
    lat: Option<f64>,
    lon: Option<f64>,
    alt: Option<f64>,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_alert.html")]
pub async fn drone_alert_select_window(
    rb: &dyn rbatis::Executor,
    drone_tag: &str,
    start_ms: i64,
    end_ms: i64,
    limit: i64,
) -> rbatis::Result<Vec<AlertRow>> {
    impled!()
}

#[html_sql("sql/drone_audit.html")]
pub async fn drone_control_audit_insert(
    rb: &dyn rbatis::Executor,
    ts_ms: i64,
    drone_tag: &str,
    control_type: &str,
    action: &str,
    holder: &str,
    priority: i32,
    result: &str,
    operator: &str,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}

#[html_sql("sql/drone_audit.html")]
pub async fn drone_command_audit_upsert(
    rb: &dyn rbatis::Executor,
    ts_ms: i64,
    drone_tag: &str,
    cmd_id: &str,
    action: &str,
    params: &str,
    result: &str,
    operator: &str,
    finished_at_ms: i64,
) -> rbatis::Result<rbatis::rbdc::ExecResult> {
    impled!()
}
