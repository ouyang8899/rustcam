use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_core::metrics::{names, MetricsText};
use rustcam_drone_core::telemetry::Telemetry;
use rustcam_drone_proto::link::service_message::Payload;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::config::DroneConfig;

pub type AdapterTx = mpsc::Sender<Payload>;

const MAX_ALERTS: usize = 2_000;
const MAX_AUDITS: usize = 5_000;
/// 每台设备保留的遥测历史样本数（内存环，重启即失；TimescaleDB 后置）。
const MAX_TELEMETRY_HISTORY: usize = 20_000;

#[derive(Clone)]
pub struct AdapterEntry {
    pub tx: AdapterTx,
    pub generation: u64,
    pub protocol: String,
    pub connected_at_ms: i64,
    /// adapter 最后一次心跳；用于查询链路是否还活着。
    pub last_heartbeat_ms: Arc<AtomicI64>,
}

#[derive(Clone, Debug)]
pub struct DeviceEntry {
    pub drone_tag: String,
    pub protocol: String,
    pub adapter_id: String,
    pub online: bool,
    pub last_seen_ms: i64,
    pub last_telemetry: Option<Telemetry>,
    pub last_seq: u32,
    pub last_ts_ms: i64,
    /// adapter 实测的飞控唯一标识（`AUTOPILOT_VERSION.uid`）；空 = 未上报。
    pub unique_id: String,
}

impl DeviceEntry {
    pub fn new(drone_tag: &str, protocol: &str, adapter_id: &str, now_ms: i64) -> DeviceEntry {
        DeviceEntry {
            drone_tag: drone_tag.to_string(),
            protocol: protocol.to_string(),
            adapter_id: adapter_id.to_string(),
            online: true,
            last_seen_ms: now_ms,
            last_telemetry: None,
            last_seq: 0,
            last_ts_ms: 0,
            unique_id: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CommandRecord {
    pub cmd_id: String,
    pub drone_tag: String,
    pub kind: String,
    pub operator: String,
    pub status: String,
    pub message: String,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub deadline_ms: i64,
    pub adapter_id: Option<String>,
}

/// 无人机业务档案：与实时设备影子分开，允许“先建档、后上线”。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DroneProfileRecord {
    pub drone_tag: String,
    pub name: String,
    pub model: String,
    pub autopilot: String,
    pub device_tag: String,
    pub channel_tag: String,
    pub video_stream: String,
    pub notes: String,
    pub enabled: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
    /// 登记的无人机唯一标识（对应 `AUTOPILOT_VERSION.uid`）；空 = 未登记。
    #[serde(default)]
    pub unique_id: String,
    /// 该机的 MAVLink 端点（隧道内地址）。
    #[serde(default)]
    pub mavlink_addr: String,
    /// 该机的 MAVLink 2 签名密钥（密文 `v1:<hex>`）；空 = 该机不需要签名。
    #[serde(default)]
    pub signing_secret: String,
    /// 是否放行未签名帧（含 MAVLink 1 帧）；仅在有密钥时生效。
    #[serde(default = "default_true")]
    pub signing_allow_unsigned: bool,
    /// MAVLink 协议版本：1 / 2。版本 1 不能签名（MAVLink 1 帧无签名字段）。
    #[serde(default = "default_mavlink_version")]
    pub mavlink_version: i32,
    /// 该机的 system id；adapter 用作出站 `target_system`。
    #[serde(default = "default_mavlink_address")]
    pub mavlink_sysid: i32,
    /// 该机的 component id（飞控组件，通常 `MAV_COMP_ID_AUTOPILOT1` = 1）。
    #[serde(default = "default_mavlink_address")]
    pub mavlink_compid: i32,
    /// 该机归属的 adapter（决定谁拿到分配）；空 = 未归属。
    #[serde(default)]
    pub adapter_id: String,
}

fn default_true() -> bool {
    true
}

fn default_mavlink_version() -> i32 {
    2
}

fn default_mavlink_address() -> i32 {
    1
}

impl DroneProfileRecord {
    pub fn new(drone_tag: &str, now_ms: i64) -> DroneProfileRecord {
        DroneProfileRecord {
            drone_tag: drone_tag.to_string(),
            name: drone_tag.to_string(),
            model: String::new(),
            autopilot: "auto".into(),
            device_tag: String::new(),
            channel_tag: String::new(),
            video_stream: String::new(),
            notes: String::new(),
            enabled: true,
            created_ms: now_ms,
            updated_ms: now_ms,
            unique_id: String::new(),
            mavlink_addr: String::new(),
            signing_secret: String::new(),
            signing_allow_unsigned: default_true(),
            mavlink_version: default_mavlink_version(),
            mavlink_sysid: default_mavlink_address(),
            mavlink_compid: default_mavlink_address(),
            adapter_id: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaypointRecord {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub speed: f64,
    pub loiter_sec: i32,
    pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteRecord {
    pub id: String,
    pub name: String,
    pub drone_tag: String,
    /// 适用无人机列表；为空表示所有无人机都可用（航线模板）。
    #[serde(default)]
    pub drone_tags: Vec<String>,
    pub waypoints: Vec<WaypointRecord>,
    pub enabled: bool,
    pub updated_ms: i64,
    /// 任务完成后的动作：`hold`（默认，原地悬停）/ `rtl` / `land`。
    #[serde(default)]
    pub end_action: String,
}

/// 围栏：`rule_type = no_fly` 表示“进入即违规”，`allowed` 表示“离开即违规”。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FenceRecord {
    pub id: String,
    pub name: String,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AlertRecord {
    pub drone_tag: String,
    pub rule: String,
    pub level: String,
    pub message: String,
    pub ts_ms: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditRecord {
    pub ts_ms: i64,
    pub drone_tag: String,
    pub category: String,
    pub action: String,
    pub operator: String,
    pub result: String,
    pub detail: String,
    /// 控制权审计用：控制类型（flight/gimbal）。
    #[serde(default)]
    pub control_type: String,
    /// 指令审计用：cmd_id。
    #[serde(default)]
    pub cmd_id: String,
    /// 指令审计用：参数 JSON。
    #[serde(default)]
    pub params: String,
    /// 指令审计用：优先级。
    #[serde(default)]
    pub priority: i32,
    /// 指令审计用：完成时间戳。
    #[serde(default)]
    pub finished_at_ms: i64,
}

#[derive(Clone, Debug, Default)]
pub struct FenceDebounce {
    /// 连续违规采样数。
    pub consecutive: u32,
    /// 首次违规时间。
    pub first_violation_ms: i64,
    /// 最近一次违规时间。
    pub last_violation_ms: i64,
    /// 是否处于告警状态。
    pub alerting: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaseRecord {
    pub drone_tag: String,
    pub control_type: String,
    pub holder: String,
    pub priority: i32,
    pub expire_at_ms: i64,
}

/// 正在执行的航线任务：用于多机空域冲突检查。
#[derive(Clone, Debug)]
pub struct ActiveMission {
    pub route_id: String,
    /// 任务航点（lat, lon, alt）。
    pub waypoints: Vec<(f64, f64, f64)>,
    /// 预计结束时间；过期后自动从活动任务表清理。
    pub end_ms: i64,
}

#[derive(Clone)]
pub struct DroneState {
    pub config: Arc<DroneConfig>,
    pub adapters: Arc<DashMap<String, AdapterEntry>>,
    pub device_to_adapter: Arc<DashMap<String, String>>,
    pub adapter_generation: Arc<DashMap<String, u64>>,
    /// 设备影子：注册即建条目，遥测到达后补齐。
    pub devices: Arc<DashMap<String, DeviceEntry>>,
    /// 无人机业务档案（可先于设备上线创建）。
    pub profiles: Arc<DashMap<String, DroneProfileRecord>>,
    /// 航线。
    pub routes: Arc<DashMap<String, RouteRecord>>,
    /// 电子围栏。
    pub fences: Arc<DashMap<String, FenceRecord>>,
    /// 控制权租约（Redis 为权威源时这里是本地缓存）。
    pub leases: Arc<DashMap<String, LeaseRecord>>,
    /// 正在执行的航线任务（drone_tag → 任务），用于同空域互斥。
    pub active_missions: Arc<DashMap<String, ActiveMission>>,
    /// 任务注册/冲突检查的临界区锁。
    pub mission_lock: Arc<Mutex<()>>,
    /// 进程内降级仲裁的临界区锁（Redis 不可用时保证单实例互斥）。
    pub lease_lock: Arc<Mutex<()>>,
    /// 指令状态机（cmd_id → 记录）。
    pub commands: Arc<DashMap<String, CommandRecord>>,
    pub alerts: Arc<Mutex<VecDeque<AlertRecord>>>,
    pub audits: Arc<Mutex<VecDeque<AuditRecord>>>,
    /// 轨迹回放用的遥测历史：`drone_tag` → 按时间递增的样本环。
    pub telemetry_history: Arc<Mutex<HashMap<String, VecDeque<Telemetry>>>>,
    /// 围栏告警去抖状态：`{device}:{fence}` → 去抖记录。
    pub fence_debounce: Arc<Mutex<HashMap<String, FenceDebounce>>>,
    /// 幂等键（Redis 不可用时的进程内兜底）：`cmd_id` → 过期时间戳。
    pub idempotency: Arc<Mutex<HashMap<String, i64>>>,
    pub event_tx: broadcast::Sender<DroneEvent>,
    /// Redis 客户端；未连接时控制权降级为进程内锁（规则 §11.6）。
    pub redis: Option<Arc<crate::infra::redis::DroneRedis>>,
    /// PostgreSQL 客户端；未连接时审计只落内存（规则 §11.6）。
    pub db: Option<Arc<crate::infra::db::DroneDb>>,
    /// 控制权仲裁用的 Lua 脚本。
    pub scripts: Arc<crate::lease::LeaseScripts>,
    /// 当前活跃的事件订阅者数量（EventService）。
    pub ws_subscribers: Arc<AtomicI64>,
    pub metrics: Arc<Mutex<MetricsText>>,
    pub started_ms: i64,
    /// 串行化状态文件写入。
    pub persist_lock: Arc<Mutex<()>>,
    /// 快照写入请求（容量 1，天然合并）。
    pub persist_tx: mpsc::Sender<()>,
    /// 快照写入任务的接收端；由进程入口取走并启动写入任务。
    persist_rx: Arc<Mutex<Option<mpsc::Receiver<()>>>>,
    /// 写入任务是否已启动；未启动时 `persist` 退回同步写盘。
    pub persist_writer_started: Arc<AtomicBool>,
}

impl DroneState {
    pub fn new(config: DroneConfig) -> DroneState {
        let (event_tx, _) = broadcast::channel(4096);
        let (persist_tx, persist_rx) = mpsc::channel(1);
        let state = DroneState {
            config: Arc::new(config),
            adapters: Arc::new(DashMap::new()),
            device_to_adapter: Arc::new(DashMap::new()),
            adapter_generation: Arc::new(DashMap::new()),
            devices: Arc::new(DashMap::new()),
            profiles: Arc::new(DashMap::new()),
            routes: Arc::new(DashMap::new()),
            fences: Arc::new(DashMap::new()),
            leases: Arc::new(DashMap::new()),
            active_missions: Arc::new(DashMap::new()),
            mission_lock: Arc::new(Mutex::new(())),
            lease_lock: Arc::new(Mutex::new(())),
            commands: Arc::new(DashMap::new()),
            alerts: Arc::new(Mutex::new(VecDeque::new())),
            audits: Arc::new(Mutex::new(VecDeque::new())),
            telemetry_history: Arc::new(Mutex::new(HashMap::new())),
            fence_debounce: Arc::new(Mutex::new(HashMap::new())),
            idempotency: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
            redis: None,
            db: None,
            scripts: Arc::new(crate::lease::LeaseScripts::new()),
            ws_subscribers: Arc::new(AtomicI64::new(0)),
            metrics: Arc::new(Mutex::new(MetricsText::default())),
            started_ms: chrono::Utc::now().timestamp_millis(),
            persist_lock: Arc::new(Mutex::new(())),
            persist_tx,
            persist_rx: Arc::new(Mutex::new(Some(persist_rx))),
            persist_writer_started: Arc::new(AtomicBool::new(false)),
        };
        crate::store::load(&state);
        state
    }

    /// 取走快照写入任务的接收端；只能取一次。
    pub fn take_persist_receiver(&self) -> Option<mpsc::Receiver<()>> {
        let mut guard = self.persist_rx.lock().ok()?;
        guard.take()
    }

    /// 挂上 Redis 客户端；控制权仲裁随即走 Redis Lua。
    pub fn with_redis(mut self, redis: Option<Arc<crate::infra::redis::DroneRedis>>) -> DroneState {
        self.redis = redis;
        self
    }

    /// 挂上 PostgreSQL 客户端；审计随即落库。
    pub fn with_db(mut self, db: Option<Arc<crate::infra::db::DroneDb>>) -> DroneState {
        self.db = db;
        self
    }

    /// 从 PostgreSQL 载入航线/围栏；数据库为空时把现有 JSON 数据回填入库。
    ///
    /// PostgreSQL 是权威源：只要有数据就覆盖 state_file 的快照；
    /// 空库则把旧 `drone-state.json` 里的数据迁移进库，避免升级丢数据。
    pub async fn load_persistent(&self) -> anyhow::Result<()> {
        let Some(db) = self.db.clone() else {
            return Ok(());
        };

        let routes = db.load_routes().await?;
        if routes.is_empty() {
            let existing: Vec<RouteRecord> =
                self.routes.iter().map(|e| e.value().clone()).collect();
            for route in existing {
                db.upsert_route(&route).await?;
            }
        } else {
            self.routes.clear();
            for route in routes {
                self.routes.insert(route.id.clone(), route);
            }
        }

        let fences = db.load_fences().await?;
        if fences.is_empty() {
            let existing: Vec<FenceRecord> =
                self.fences.iter().map(|e| e.value().clone()).collect();
            for fence in existing {
                db.upsert_fence(&fence).await?;
            }
        } else {
            self.fences.clear();
            for fence in fences {
                self.fences.insert(fence.id.clone(), fence);
            }
        }

        self.persist();
        Ok(())
    }

    /// 自增并返回该 adapter 的连接代次；用于踢掉旧连接。
    pub fn next_generation(&self, adapter_id: &str) -> u64 {
        let mut entry = self
            .adapter_generation
            .entry(adapter_id.to_string())
            .or_insert(0);
        *entry += 1;
        *entry
    }

    /// 当前 adapter 的代次；未注册返回 None。
    pub fn generation_of(&self, adapter_id: &str) -> Option<u64> {
        self.adapter_generation.get(adapter_id).map(|v| *v)
    }

    pub fn touch_adapter_heartbeat(&self, adapter_id: &str, now_ms: i64) {
        if let Some(entry) = self.adapters.get(adapter_id) {
            entry.last_heartbeat_ms.store(now_ms, Ordering::Relaxed);
        }
    }

    /// adapter 距离上次心跳过去了多少毫秒；未注册返回 None。
    pub fn adapter_heartbeat_age_ms(&self, adapter_id: &str) -> Option<i64> {
        let entry = self.adapters.get(adapter_id)?;
        let last = entry.last_heartbeat_ms.load(Ordering::Relaxed);
        if last <= 0 {
            return None;
        }
        Some(chrono::Utc::now().timestamp_millis() - last)
    }

    pub fn adapter_for(&self, drone_tag: &str) -> Option<String> {
        self.device_to_adapter.get(drone_tag).map(|v| v.clone())
    }

    pub fn adapter_tx(&self, adapter_id: &str) -> Option<AdapterTx> {
        self.adapters.get(adapter_id).map(|e| e.tx.clone())
    }

    pub fn mark_device_online(&self, drone_tag: &str, now_ms: i64) -> bool {
        if let Some(mut entry) = self.devices.get_mut(drone_tag) {
            let was_offline = !entry.online;
            entry.online = true;
            entry.last_seen_ms = now_ms;
            return was_offline;
        }
        false
    }

    pub fn mark_device_offline(&self, drone_tag: &str) -> bool {
        if let Some(mut entry) = self.devices.get_mut(drone_tag) {
            let was_online = entry.online;
            entry.online = false;
            return was_online;
        }
        false
    }

    pub fn online_device_count(&self) -> usize {
        self.devices.iter().filter(|e| e.value().online).count()
    }

    pub fn push_alert(&self, alert: AlertRecord) {
        if let Ok(mut alerts) = self.alerts.lock() {
            alerts.push_back(alert);
            while alerts.len() > MAX_ALERTS {
                alerts.pop_front();
            }
        }
    }

    /// 写审计：内存环 + PostgreSQL（规则 §6.7 / §6.8）。
    ///
    /// PostgreSQL 不可用时只保留内存记录（并由状态快照兜底），
    /// 不影响主链路。
    pub async fn write_audit(&self, audit: AuditRecord) {
        self.push_audit(audit.clone());
        self.flush_audit_pg(&audit).await;
    }

    /// 只把审计写入 PostgreSQL（调用方已自行写入内存环）。
    ///
    /// 用于持有锁的临界区：先在临界区内写内存，出了临界区再落库，
    /// 避免 `MutexGuard` 跨 await 导致 future 不是 `Send`。
    pub async fn flush_audit_pg(&self, audit: &AuditRecord) {
        if let Some(db) = self.db.clone() {
            if let Err(e) = crate::infra::db::write_audit(&db, audit).await {
                tracing::warn!(error = %e, category = %audit.category, "[Drone] audit persist failed");
            }
        }
    }

    pub fn push_audit(&self, audit: AuditRecord) {
        if let Ok(mut audits) = self.audits.lock() {
            audits.push_back(audit);
            while audits.len() > MAX_AUDITS {
                audits.pop_front();
            }
        }
    }

    pub fn recent_alerts(&self, drone_tag: &str, limit: usize) -> Vec<AlertRecord> {
        let limit = limit.clamp(1, 500);
        let Ok(alerts) = self.alerts.lock() else {
            return vec![];
        };
        alerts
            .iter()
            .rev()
            .filter(|a| drone_tag.is_empty() || a.drone_tag == drone_tag)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn recent_audits(&self, drone_tag: &str, limit: usize) -> Vec<AuditRecord> {
        let limit = limit.clamp(1, 1000);
        let Ok(audits) = self.audits.lock() else {
            return vec![];
        };
        audits
            .iter()
            .rev()
            .filter(|a| drone_tag.is_empty() || a.drone_tag == drone_tag)
            .take(limit)
            .cloned()
            .collect()
    }

    /// 记录一帧遥测用于轨迹回放；超出上限时丢弃最旧样本。
    pub fn record_telemetry(&self, telemetry: &Telemetry) {
        let Ok(mut history) = self.telemetry_history.lock() else {
            return;
        };
        let samples = history
            .entry(telemetry.drone_tag.clone())
            .or_insert_with(VecDeque::new);
        samples.push_back(telemetry.clone());
        while samples.len() > MAX_TELEMETRY_HISTORY {
            samples.pop_front();
        }
    }

    /// 查询指定时间窗内的遥测历史，按时间升序返回。
    ///
    /// 匹配样本超过 `limit` 时按等步长降采样，保证首尾样本都在结果里。
    pub fn telemetry_history(
        &self,
        drone_tag: &str,
        start_ms: i64,
        end_ms: i64,
        limit: usize,
    ) -> Vec<Telemetry> {
        let limit = limit.clamp(1, 10_000);
        let Ok(history) = self.telemetry_history.lock() else {
            return vec![];
        };
        let Some(samples) = history.get(drone_tag) else {
            return vec![];
        };
        let matched: Vec<Telemetry> = samples
            .iter()
            .filter(|t| (start_ms <= 0 || t.ts_ms >= start_ms) && (end_ms <= 0 || t.ts_ms <= end_ms))
            .cloned()
            .collect();
        if matched.len() <= limit {
            return matched;
        }
        let step = matched.len().div_ceil(limit);
        let mut out = Vec::with_capacity(limit);
        for (i, sample) in matched.iter().enumerate() {
            if i % step == 0 {
                out.push(sample.clone());
            }
        }
        if let Some(last) = matched.last() {
            if out.last().map(|t| t.ts_ms) != Some(last.ts_ms) {
                out.push(last.clone());
            }
        }
        out
    }

    pub fn metrics_inc(&self, name: &str) {
        if let Ok(mut m) = self.metrics.lock() {
            m.inc(name);
        }
    }

    pub fn metrics_inc_labeled(&self, name: &str, labels: &[(&str, &str)]) {
        if let Ok(mut m) = self.metrics.lock() {
            m.inc_labeled(name, labels);
        }
    }

    pub fn metrics_set(&self, name: &str, value: f64) {
        if let Ok(mut m) = self.metrics.lock() {
            m.set_gauge(name, value);
        }
    }

    pub fn metrics_set_labeled(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        if let Ok(mut m) = self.metrics.lock() {
            m.set_gauge_labeled(name, value, labels);
        }
    }

    pub fn metrics_text(&self) -> String {
        let mut m = match self.metrics.lock() {
            Ok(m) => m,
            Err(_) => return String::new(),
        };
        m.set_gauge(names::ONLINE_DEVICES, self.online_device_count() as f64);
        m.set_gauge(names::ADAPTERS, self.adapters.len() as f64);
        m.set_gauge(names::FENCES, self.fences.len() as f64);
        m.set_gauge(
            names::COMMAND_INFLIGHT,
            self.inflight_command_count() as f64,
        );
        m.set_gauge_labeled(
            names::WS_SUBSCRIBERS,
            self.ws_subscribers.load(Ordering::Relaxed) as f64,
            &[("channel", "event")],
        );
        m.render()
    }

    /// 处于非终态（未回执）的指令数量。
    pub fn inflight_command_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|r| !rustcam_drone_core::command::is_terminal_status(&r.value().status))
            .count()
    }

    /// 保存状态快照。
    ///
    /// 写入任务已启动时只投递请求（合并写、磁盘 IO 在阻塞线程池）；
    /// 未启动时退回同步写盘，保证单测与嵌入式用法仍能落盘。
    pub fn persist(&self) {
        if self.persist_writer_started.load(Ordering::Relaxed) {
            let _ = self.persist_tx.try_send(());
            return;
        }
        crate::store::save(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> DroneState {
        DroneState::new(DroneConfig::default())
    }

    #[test]
    fn generation_increases_per_registration() {
        let state = test_state();
        assert_eq!(state.next_generation("a-1"), 1);
        assert_eq!(state.next_generation("a-1"), 2);
        assert_eq!(state.next_generation("a-2"), 1);
        assert_eq!(state.generation_of("a-1"), Some(2));
        assert_eq!(state.generation_of("nope"), None);
    }

    #[test]
    fn device_online_flags_are_tracked() {
        let state = test_state();
        state.devices.insert(
            "drone-001".into(),
            DeviceEntry::new("drone-001", "mavlink", "a-1", 1),
        );
        assert_eq!(state.online_device_count(), 1);
        assert!(state.mark_device_offline("drone-001"));
        assert!(!state.mark_device_offline("drone-001"));
        assert_eq!(state.online_device_count(), 0);
        assert!(state.mark_device_online("drone-001", 2));
        assert!(!state.mark_device_online("drone-001", 3));
        assert_eq!(state.online_device_count(), 1);
    }

    #[test]
    fn metrics_include_registry_gauges() {
        let state = test_state();
        state.metrics_inc("rustcam_drone_telemetry_total");
        let text = state.metrics_text();
        assert!(text.contains("rustcam_drone_telemetry_total 1"));
        assert!(text.contains("rustcam_drone_online_devices 0"));
    }

    #[test]
    fn alerts_and_audits_are_filtered() {
        let state = test_state();
        for i in 0..3 {
            state.push_alert(AlertRecord {
                drone_tag: if i == 0 { "d1" } else { "d2" }.into(),
                rule: "geofence".into(),
                level: "high".into(),
                message: format!("m{i}"),
                ts_ms: i,
            });
        }
        assert_eq!(state.recent_alerts("d1", 10).len(), 1);
        assert_eq!(state.recent_alerts("", 10).len(), 3);

        state.push_audit(AuditRecord {
            ts_ms: 1,
            drone_tag: "d1".into(),
            category: "command".into(),
            action: "takeoff".into(),
            operator: "alice".into(),
            result: "sent".into(),
            detail: String::new(),
            ..Default::default()
        });
        assert_eq!(state.recent_audits("d1", 10).len(), 1);
        assert_eq!(state.recent_audits("d2", 10).len(), 0);
    }
}
