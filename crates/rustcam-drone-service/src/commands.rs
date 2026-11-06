//! 指令状态机：service 侧受理、下发、回执与超时回收。
//!
//! 状态流转：`pending` → `sent` →（adapter 回执）`accepted` / `failed` / `timeout`。
//! adapter 离线或参数非法时 service 直接 `rejected`，不回执到 adapter。

use rustcam_drone_core::command::{is_terminal_status, status, Command as CoreCommand};
use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_core::metrics::names;
use rustcam_drone_proto::common as proto;
use rustcam_drone_proto::link::service_message;
use tokio::sync::mpsc::error::TrySendError;

use crate::state::{AuditRecord, CommandRecord, DroneState};
use crate::{control, geofence};

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn control_type_for(kind: &str) -> Option<&'static str> {
    match kind {
        "gimbal" | "camera" => Some(control::CONTROL_GIMBAL),
        "takeoff" | "land" | "rtl" | "goto" => Some(control::CONTROL_FLIGHT),
        _ => None,
    }
}

async fn audit_command(state: &DroneState, cmd: &proto::Command, result: &str, detail: &str) {
    state
        .write_audit(AuditRecord {
            ts_ms: now_ms(),
            device_id: cmd.device_id.clone(),
            category: "command".into(),
            action: cmd.r#type.clone(),
            operator: cmd.operator.clone(),
            result: result.to_string(),
            detail: format!("cmd_id={} {detail}", cmd.cmd_id),
            cmd_id: cmd.cmd_id.clone(),
            params: serde_json::to_string(&cmd.params).unwrap_or_default(),
            ..Default::default()
        })
        .await;
    state.persist();
}

fn reply(
    cmd: &proto::Command,
    status: &str,
    code: i32,
    message: impl Into<String>,
) -> proto::CommandReply {
    proto::CommandReply {
        cmd_id: cmd.cmd_id.clone(),
        device_id: cmd.device_id.clone(),
        status: status.to_string(),
        code,
        message: message.into(),
        ts_ms: now_ms(),
    }
}

/// 指令参数校验（规则 §6.8 步骤 3、§12.3）。
///
/// 只接受白名单参数，并对取值范围做硬校验：
/// - 经纬度：lat ∈ [-90, 90]，lon ∈ [-180, 180]
/// - 高度：alt ∈ [0, 500] 米（相对起飞点）
/// - 云台：pitch ∈ [-90, 90]，yaw ∈ [-180, 180]，zoom ∈ [1, 30]
/// - 相机：action ∈ {start, stop, photo, video_start, video_stop}
pub fn validate_params(cmd: &proto::Command) -> Result<(), String> {
    match cmd.r#type.as_str() {
        "goto" => {
            let lat = required_f64(cmd, "lat")?;
            let lon = required_f64(cmd, "lon")?;
            if !(-90.0..=90.0).contains(&lat) {
                return Err(format!("lat 超出范围：{lat}"));
            }
            if !(-180.0..=180.0).contains(&lon) {
                return Err(format!("lon 超出范围：{lon}"));
            }
            let alt = optional_f64(cmd, "alt").unwrap_or(DEFAULT_ALT_M);
            check_alt(alt)?;
            Ok(())
        }
        "takeoff" => {
            let alt = optional_f64(cmd, "alt").unwrap_or(DEFAULT_ALT_M);
            check_alt(alt)
        }
        "gimbal" => {
            if let Some(pitch) = optional_f64(cmd, "pitch") {
                if !(-90.0..=90.0).contains(&pitch) {
                    return Err(format!("pitch 超出范围：{pitch}"));
                }
            }
            if let Some(yaw) = optional_f64(cmd, "yaw") {
                if !(-180.0..=180.0).contains(&yaw) {
                    return Err(format!("yaw 超出范围：{yaw}"));
                }
            }
            if let Some(zoom) = optional_f64(cmd, "zoom") {
                if !(1.0..=30.0).contains(&zoom) {
                    return Err(format!("zoom 超出范围：{zoom}"));
                }
            }
            Ok(())
        }
        "camera" => {
            let action = cmd
                .params
                .get("action")
                .map(|v| v.trim().to_ascii_lowercase())
                .unwrap_or_default();
            if !CAMERA_ACTIONS.contains(&action.as_str()) {
                return Err(format!("unsupported camera action `{action}`"));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 相对起飞点的默认高度（米）。
pub const DEFAULT_ALT_M: f64 = 30.0;
/// 相机动作白名单。
pub const CAMERA_ACTIONS: &[&str] = &["start", "stop", "photo", "video_start", "video_stop"];

fn check_alt(alt: f64) -> Result<(), String> {
    if !(0.0..=500.0).contains(&alt) {
        return Err(format!("alt 超出范围：{alt}"));
    }
    Ok(())
}

/// 取必填数值参数；兼容 `alt` 与 `alt_m` 两种命名。
fn required_f64(cmd: &proto::Command, key: &str) -> Result<f64, String> {
    optional_f64(cmd, key).ok_or_else(|| format!("{key} is required"))
}

fn optional_f64(cmd: &proto::Command, key: &str) -> Option<f64> {
    let raw = cmd.params.get(key).or_else(|| {
        if key == "alt" {
            cmd.params.get("alt_m")
        } else {
            None
        }
    })?;
    raw.trim().parse::<f64>().ok()
}

/// 领取幂等键：首次返回 `true`，重复返回 `false`。
///
/// Redis 可用时用 `SET cmd:idem:{cmd_id} NX EX 86400`（规则 §8.2）；
/// Redis 不可用时退回进程内记录（只保证单实例）。
async fn claim_idempotency(state: &DroneState, cmd_id: &str) -> anyhow::Result<bool> {
    if let Some(redis) = state.redis.clone() {
        let key = rustcam_drone_core::keys::cmd_idem_key(cmd_id);
        return redis.set_nx_ex(&key, "1", IDEMPOTENCY_TTL_SECS).await;
    }
    let mut guard = state.idempotency.lock().unwrap_or_else(|e| e.into_inner());
    let now = now_ms();
    guard.retain(|_, exp| *exp > now);
    if guard.contains_key(cmd_id) {
        return Ok(false);
    }
    guard.insert(
        cmd_id.to_string(),
        now + (IDEMPOTENCY_TTL_SECS as i64) * 1000,
    );
    Ok(true)
}

/// 幂等键保留时长（24 小时，规则 §6.8 步骤 4）。
pub const IDEMPOTENCY_TTL_SECS: u64 = 86_400;

/// 受理一条指令并下发给所属 adapter。
pub async fn submit(state: &DroneState, cmd: proto::Command) -> proto::CommandReply {
    submit_inner(state, cmd, true).await
}

/// 安全指令（如控制权丢失后的自动返航）：跳过控制权校验。
///
/// 参数校验、设备在线、adapter 连接等约束仍然生效；只有“必须持有控制权”
/// 这一条被放开，避免租约过期导致无人机无人指挥。
pub async fn submit_safety(state: &DroneState, cmd: proto::Command) -> proto::CommandReply {
    submit_inner(state, cmd, false).await
}

/// 唯一标识校验（drone.md §12.4 L3）。
///
/// 档案登记了唯一标识时，adapter 实测值必须非空且相等；否则拒绝下发指令。
pub fn check_identity(state: &DroneState, device_id: &str) -> Result<(), String> {
    let Some(profile) = state.profiles.get(device_id) else {
        return Ok(());
    };
    let registered = profile.unique_id.trim().to_string();
    if registered.is_empty() {
        return Ok(());
    }
    let observed = state
        .devices
        .get(device_id)
        .map(|d| d.unique_id.trim().to_string())
        .unwrap_or_default();
    if crate::registry::identity_matches(&registered, &observed) {
        Ok(())
    } else {
        Err(format!(
            "unique_id mismatch: registered `{registered}`, observed `{observed}`"
        ))
    }
}

async fn submit_inner(
    state: &DroneState,
    mut cmd: proto::Command,
    enforce_control: bool,
) -> proto::CommandReply {
    if cmd.cmd_id.trim().is_empty() {
        cmd.cmd_id = uuid::Uuid::new_v4().to_string();
    }
    if !CoreCommand::is_supported_kind(&cmd.r#type) {
        return reply(
            &cmd,
            status::REJECTED,
            400,
            format!("unsupported command kind `{}`", cmd.r#type),
        );
    }
    if cmd.device_id.trim().is_empty() {
        return reply(&cmd, status::REJECTED, 400, "device_id is required");
    }
    if let Err(e) = validate_params(&cmd) {
        audit_command(state, &cmd, "rejected", &e).await;
        return reply(&cmd, status::REJECTED, 400, e);
    }

    if let Some(existing) = state.commands.get(&cmd.cmd_id) {
        return proto::CommandReply {
            cmd_id: existing.cmd_id.clone(),
            device_id: existing.device_id.clone(),
            status: existing.status.clone(),
            code: 200,
            message: format!("duplicate cmd_id, current status: {}", existing.message),
            ts_ms: now_ms(),
        };
    }

    let now = now_ms();
    if cmd.deadline_ms == 0 {
        cmd.deadline_ms = now + (state.config.default_command_timeout_sec as i64) * 1000;
    }
    if cmd.deadline_ms <= now {
        return reply(
            &cmd,
            status::REJECTED,
            400,
            "command deadline already exceeded",
        );
    }

    let Some(device) = state.devices.get(&cmd.device_id) else {
        return reply(
            &cmd,
            status::REJECTED,
            404,
            format!("unknown device `{}`", cmd.device_id),
        );
    };
    if !device.online {
        return reply(
            &cmd,
            status::REJECTED,
            503,
            format!("device `{}` is offline", cmd.device_id),
        );
    }
    let adapter_id = device.adapter_id.clone();
    let protocol = device.protocol.clone();
    drop(device);

    // 唯一标识核验（drone.md §12.4 L3）：档案登记了标识时，实测值必须非空且相等。
    if let Err(e) = check_identity(state, &cmd.device_id) {
        audit_command(state, &cmd, "rejected", &e).await;
        return reply(&cmd, status::REJECTED, 409, e);
    }

    if enforce_control {
        if let Some(control_type) = control_type_for(&cmd.r#type) {
            if let Err(e) =
                control::ensure_holder(state, &cmd.device_id, control_type, &cmd.operator).await
            {
                audit_command(state, &cmd, "rejected", &e).await;
                return reply(&cmd, status::REJECTED, 403, e);
            }
        }
    }

    if cmd.r#type == "goto" {
        // 围栏预检不可绕过：坐标缺失或非法一律拒绝（§6.8 步骤 5、§12.3）。
        let lat = optional_f64(&cmd, "lat").unwrap_or_default();
        let lon = optional_f64(&cmd, "lon").unwrap_or_default();
        let alt = optional_f64(&cmd, "alt").unwrap_or(DEFAULT_ALT_M);
        if let Err(e) = geofence::check_target(state, &cmd.device_id, lat, lon, alt) {
            audit_command(state, &cmd, "rejected", &e).await;
            return reply(&cmd, status::REJECTED, 403, e);
        }
    }

    let Some(tx) = state.adapter_tx(&adapter_id) else {
        return reply(
            &cmd,
            status::REJECTED,
            503,
            format!("adapter `{adapter_id}` is not connected"),
        );
    };
    let inflight = state
        .commands
        .iter()
        .filter(|e| {
            let r = e.value();
            r.adapter_id.as_deref() == Some(adapter_id.as_str()) && !is_terminal_status(&r.status)
        })
        .count();
    if inflight >= state.config.max_inflight_commands.max(1) {
        return reply(
            &cmd,
            status::REJECTED,
            429,
            format!("adapter `{adapter_id}` has {inflight} in-flight commands"),
        );
    }

    // 幂等键（规则 §6.8 步骤 4）：`cmd:idem:{cmd_id}` NX EX 24h。
    // 只有真正要下发的指令才占用幂等键，被拒绝的指令不占用。
    // Redis 不可用时退回进程内记录（只保证单实例）。
    match claim_idempotency(state, &cmd.cmd_id).await {
        Ok(false) => {
            return proto::CommandReply {
                cmd_id: cmd.cmd_id.clone(),
                device_id: cmd.device_id.clone(),
                status: status::REJECTED.to_string(),
                code: 409,
                message: "duplicate cmd_id, already submitted".to_string(),
                ts_ms: now_ms(),
            };
        }
        Ok(true) => {}
        Err(e) => {
            tracing::warn!(error = %e, "[Command] idempotency check failed");
        }
    }

    state.commands.insert(
        cmd.cmd_id.clone(),
        CommandRecord {
            cmd_id: cmd.cmd_id.clone(),
            device_id: cmd.device_id.clone(),
            kind: cmd.r#type.clone(),
            operator: cmd.operator.clone(),
            status: status::PENDING.to_string(),
            message: "queued".into(),
            created_ms: now,
            updated_ms: now,
            deadline_ms: cmd.deadline_ms,
            adapter_id: Some(adapter_id.clone()),
        },
    );

    match tx.try_send(service_message::Payload::Command(cmd.clone())) {
        Ok(()) => {
            if let Some(mut record) = state.commands.get_mut(&cmd.cmd_id) {
                record.status = status::SENT.to_string();
                record.message = format!("sent to adapter `{adapter_id}`");
                record.updated_ms = now_ms();
            }
            state.metrics_inc_labeled(names::COMMAND_SENT_TOTAL, &[("type", cmd.r#type.as_str())]);
            audit_command(state, &cmd, "sent", &format!("adapter={adapter_id}")).await;
            tracing::info!(
                cmd_id = %cmd.cmd_id,
                kind = %cmd.r#type,
                device = %cmd.device_id,
                adapter = %adapter_id,
                protocol = %protocol,
                operator = %cmd.operator,
                "command dispatched"
            );
            reply(
                &cmd,
                status::SENT,
                202,
                format!("dispatched to adapter `{adapter_id}`"),
            )
        }
        Err(TrySendError::Full(_)) => {
            state.commands.insert(
                cmd.cmd_id.clone(),
                record_with_status(&cmd, status::REJECTED, "adapter queue is full", &adapter_id),
            );
            audit_command(state, &cmd, "rejected", "adapter queue is full").await;
            reply(&cmd, status::REJECTED, 429, "adapter command queue is full")
        }
        Err(TrySendError::Closed(_)) => {
            state.commands.insert(
                cmd.cmd_id.clone(),
                record_with_status(&cmd, status::REJECTED, "adapter link closed", &adapter_id),
            );
            audit_command(state, &cmd, "rejected", "adapter link closed").await;
            reply(&cmd, status::REJECTED, 503, "adapter link is closed")
        }
    }
}

fn record_with_status(
    cmd: &proto::Command,
    status: &str,
    message: &str,
    adapter_id: &str,
) -> CommandRecord {
    let now = now_ms();
    CommandRecord {
        cmd_id: cmd.cmd_id.clone(),
        device_id: cmd.device_id.clone(),
        kind: cmd.r#type.clone(),
        operator: cmd.operator.clone(),
        status: status.to_string(),
        message: message.to_string(),
        created_ms: now,
        updated_ms: now,
        deadline_ms: cmd.deadline_ms,
        adapter_id: Some(adapter_id.to_string()),
    }
}

/// 处理 adapter 回执；重复或未知回执返回 false。
pub async fn on_reply(state: &DroneState, reply: proto::CommandReply) -> bool {
    let Some(mut record) = state.commands.get_mut(&reply.cmd_id) else {
        tracing::warn!(
            cmd_id = %reply.cmd_id,
            status = %reply.status,
            "command reply for unknown cmd_id"
        );
        return false;
    };

    if is_terminal_status(&record.status) {
        tracing::debug!(
            cmd_id = %reply.cmd_id,
            status = %record.status,
            "ignoring duplicate command reply"
        );
        return false;
    }

    let new_status = if reply.status.trim().is_empty() {
        status::FAILED.to_string()
    } else {
        reply.status.clone()
    };
    let kind = record.kind.clone();
    let operator = record.operator.clone();
    let device_id = record.device_id.clone();
    let created_ms = record.created_ms;
    record.status = new_status.clone();
    record.message = reply.message.clone();
    record.updated_ms = now_ms();
    drop(record);

    state.metrics_inc_labeled(
        names::COMMAND_REPLY_TOTAL,
        &[("type", kind.as_str()), ("status", new_status.as_str())],
    );
    // 指令耗时（最近一次观测值）；§14.2 指出需要 P99 时再补固定桶直方图。
    let duration_secs = (now_ms() - created_ms).max(0) as f64 / 1000.0;
    state.metrics_set_labeled(
        names::COMMAND_DURATION_SECONDS,
        duration_secs,
        &[("type", kind.as_str()), ("status", new_status.as_str())],
    );
    state
        .write_audit(AuditRecord {
            ts_ms: now_ms(),
            device_id: device_id.clone(),
            category: "command".into(),
            action: kind,
            operator,
            result: new_status.clone(),
            detail: format!("cmd_id={} {}", reply.cmd_id, reply.message),
            cmd_id: reply.cmd_id.clone(),
            finished_at_ms: now_ms(),
            ..Default::default()
        })
        .await;
    state.persist();
    let _ = state.event_tx.send(DroneEvent::CommandReply {
        cmd_id: reply.cmd_id,
        device_id,
        status: new_status,
        message: reply.message,
    });
    true
}

/// 超时回收：把超过 deadline 且仍非终态的指令置为 timeout。
pub async fn reap_expired(state: &DroneState, now_ms: i64) -> usize {
    let expired: Vec<String> = state
        .commands
        .iter()
        .filter(|e| {
            let r = e.value();
            !is_terminal_status(&r.status) && r.deadline_ms > 0 && now_ms > r.deadline_ms
        })
        .map(|e| e.key().clone())
        .collect();

    let mut count = 0;
    for cmd_id in expired {
        let Some(mut record) = state.commands.get_mut(&cmd_id) else {
            continue;
        };
        record.status = status::TIMEOUT.to_string();
        record.message = format!("no reply before deadline {}", record.deadline_ms);
        record.updated_ms = now_ms;
        let device_id = record.device_id.clone();
        let message = record.message.clone();
        let kind = record.kind.clone();
        let operator = record.operator.clone();
        drop(record);

        state
            .write_audit(AuditRecord {
                ts_ms: now_ms,
                device_id: device_id.clone(),
                category: "command".into(),
                action: kind.clone(),
                operator,
                result: status::TIMEOUT.to_string(),
                detail: format!("cmd_id={cmd_id} timeout"),
                cmd_id: cmd_id.clone(),
                finished_at_ms: now_ms,
                ..Default::default()
            })
            .await;
        let _ = state.event_tx.send(DroneEvent::CommandReply {
            cmd_id,
            device_id,
            status: status::TIMEOUT.to_string(),
            message,
        });
        state.metrics_inc_labeled(names::COMMAND_TIMEOUT_TOTAL, &[("type", kind.as_str())]);
        count += 1;
    }
    if count > 0 {
        state.persist();
    }
    count
}

/// 后台超时回收任务（每秒扫描一次，每分钟清理一次过期终态记录）。
pub async fn run_reaper(state: DroneState) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut ticks: u64 = 0;
    loop {
        ticker.tick().await;
        reap_expired(&state, now_ms()).await;
        ticks += 1;
        if ticks.is_multiple_of(60) {
            prune_terminal(&state, now_ms(), 3_600_000);
        }
    }
}

/// 清理超过保留期的终态指令记录，避免长期运行后内存无界增长。
pub fn prune_terminal(state: &DroneState, now_ms: i64, retention_ms: i64) -> usize {
    let stale: Vec<String> = state
        .commands
        .iter()
        .filter(|e| {
            let r = e.value();
            is_terminal_status(&r.status) && now_ms - r.updated_ms > retention_ms
        })
        .map(|e| e.key().clone())
        .collect();
    let count = stale.len();
    for cmd_id in stale {
        state.commands.remove(&cmd_id);
    }
    count
}

/// 设备下线时，把它在途指令全部置为 failed，避免永久 pending。
pub async fn fail_inflight_for_device(state: &DroneState, device_id: &str, reason: &str) -> usize {
    let inflight: Vec<String> = state
        .commands
        .iter()
        .filter(|e| e.value().device_id == device_id && !is_terminal_status(&e.value().status))
        .map(|e| e.key().clone())
        .collect();
    let mut count = 0;
    for cmd_id in inflight {
        if let Some(mut record) = state.commands.get_mut(&cmd_id) {
            record.status = status::FAILED.to_string();
            record.message = reason.to_string();
            record.updated_ms = now_ms();
            let kind = record.kind.clone();
            let operator = record.operator.clone();
            drop(record);
            state
                .write_audit(AuditRecord {
                    ts_ms: now_ms(),
                    device_id: device_id.to_string(),
                    category: "command".into(),
                    action: kind,
                    operator,
                    result: status::FAILED.to_string(),
                    detail: format!("cmd_id={cmd_id} {reason}"),
                    cmd_id: cmd_id.clone(),
                    finished_at_ms: now_ms(),
                    ..Default::default()
                })
                .await;
            count += 1;
        }
    }
    if count > 0 {
        state.persist();
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use crate::state::{AdapterEntry, DeviceEntry, DroneState};
    use rustcam_drone_proto::link::service_message::Payload;
    use std::sync::atomic::AtomicI64;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn state_with_device(online: bool) -> (DroneState, mpsc::Receiver<Payload>) {
        state_with_control(online, false)
    }

    fn state_with_control(
        online: bool,
        require_control: bool,
    ) -> (DroneState, mpsc::Receiver<Payload>) {
        let state = DroneState::new(DroneConfig {
            require_control,
            ..Default::default()
        });
        let (tx, rx) = mpsc::channel(8);
        state.adapters.insert(
            "a-1".into(),
            AdapterEntry {
                tx,
                generation: 1,
                protocol: "mavlink".into(),
                connected_at_ms: 1,
                last_heartbeat_ms: Arc::new(AtomicI64::new(1)),
            },
        );
        state
            .device_to_adapter
            .insert("drone-001".into(), "a-1".into());
        let mut device = DeviceEntry::new("drone-001", "mavlink", "a-1", 1);
        device.online = online;
        state.devices.insert("drone-001".into(), device);
        (state, rx)
    }

    fn command(kind: &str) -> proto::Command {
        proto::Command {
            cmd_id: "cmd-1".into(),
            device_id: "drone-001".into(),
            r#type: kind.into(),
            params: Default::default(),
            deadline_ms: 0,
            operator: "alice".into(),
        }
    }

    #[test]
    fn identity_check_rejects_when_observed_differs() {
        let (state, _rx) = state_with_device(true);
        state.profiles.insert(
            "drone-001".into(),
            crate::state::DroneProfileRecord {
                device_id: "drone-001".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-001", 0)
            },
        );
        if let Some(mut device) = state.devices.get_mut("drone-001") {
            device.unique_id = "observed".into();
        }
        assert!(check_identity(&state, "drone-001").is_err());
    }

    #[test]
    fn identity_check_passes_when_observed_matches() {
        let (state, _rx) = state_with_device(true);
        state.profiles.insert(
            "drone-001".into(),
            crate::state::DroneProfileRecord {
                device_id: "drone-001".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-001", 0)
            },
        );
        if let Some(mut device) = state.devices.get_mut("drone-001") {
            device.unique_id = "registered".into();
        }
        assert!(check_identity(&state, "drone-001").is_ok());
    }

    #[test]
    fn identity_check_skips_when_profile_has_no_unique_id() {
        let (state, _rx) = state_with_device(true);
        assert!(check_identity(&state, "drone-001").is_ok());
    }

    #[tokio::test]
    async fn submit_is_rejected_when_identity_mismatches() {
        let (state, _rx) = state_with_device(true);
        state.profiles.insert(
            "drone-001".into(),
            crate::state::DroneProfileRecord {
                device_id: "drone-001".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-001", 0)
            },
        );
        if let Some(mut device) = state.devices.get_mut("drone-001") {
            device.unique_id = "observed".into();
        }
        let reply = submit(&state, command("rtl")).await;
        assert_eq!(reply.status, status::REJECTED);
        assert_eq!(reply.code, 409);
    }

    #[tokio::test]
    async fn submit_dispatches_to_adapter_and_records_sent() {
        let (state, mut rx) = state_with_device(true);
        let reply = submit(&state, command("rtl")).await;
        assert_eq!(reply.status, status::SENT);
        assert_eq!(reply.code, 202);
        let record = state.commands.get("cmd-1").unwrap();
        assert_eq!(record.status, status::SENT);
        assert_eq!(record.adapter_id.as_deref(), Some("a-1"));
        assert!(record.deadline_ms > 0);
        drop(record);

        match rx.try_recv().unwrap() {
            Payload::Command(cmd) => {
                assert_eq!(cmd.cmd_id, "cmd-1");
                assert_eq!(cmd.r#type, "rtl");
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test]
    async fn submit_generates_cmd_id_when_missing() {
        let (state, _rx) = state_with_device(true);
        let mut cmd = command("land");
        cmd.cmd_id.clear();
        let reply = submit(&state, cmd).await;
        assert!(!reply.cmd_id.is_empty());
        assert_eq!(reply.status, status::SENT);
        assert!(state.commands.contains_key(&reply.cmd_id));
    }

    #[tokio::test]
    async fn submit_rejects_unknown_kind_and_unknown_device() {
        let (state, _rx) = state_with_device(true);
        let reply = submit(&state, command("self_destruct")).await;
        assert_eq!(reply.status, status::REJECTED);
        assert_eq!(reply.code, 400);

        let mut cmd = command("rtl");
        cmd.device_id = "drone-404".into();
        let reply = submit(&state, cmd).await;
        assert_eq!(reply.status, status::REJECTED);
        assert_eq!(reply.code, 404);
    }

    #[tokio::test]
    async fn submit_rejects_offline_device() {
        let (state, _rx) = state_with_device(false);
        let reply = submit(&state, command("rtl")).await;
        assert_eq!(reply.status, status::REJECTED);
        assert_eq!(reply.code, 503);
    }

    #[tokio::test]
    async fn duplicate_cmd_id_returns_current_status() {
        let (state, _rx) = state_with_device(true);
        assert_eq!(submit(&state, command("rtl")).await.status, status::SENT);
        let second = submit(&state, command("land")).await;
        assert_eq!(second.status, status::SENT);
        assert_eq!(second.code, 200, "重复 cmd_id 不重复下发");
    }

    #[tokio::test]
    async fn reply_moves_command_to_terminal_and_broadcasts() {
        let (state, _rx) = state_with_device(true);
        submit(&state, command("rtl")).await;
        let mut events = state.event_tx.subscribe();
        assert!(
            on_reply(
                &state,
                proto::CommandReply {
                    cmd_id: "cmd-1".into(),
                    device_id: "drone-001".into(),
                    status: status::ACCEPTED.into(),
                    code: 200,
                    message: "COMMAND_ACK accepted".into(),
                    ts_ms: 2,
                }
            )
            .await
        );
        assert_eq!(
            state.commands.get("cmd-1").unwrap().status,
            status::ACCEPTED
        );
        let event = events.try_recv().unwrap();
        assert_eq!(event.msg_type(), "command");

        assert!(
            !on_reply(
                &state,
                proto::CommandReply {
                    cmd_id: "cmd-1".into(),
                    device_id: "drone-001".into(),
                    status: status::FAILED.into(),
                    code: 500,
                    message: "late".into(),
                    ts_ms: 3,
                }
            )
            .await,
            "终态后重复回执必须被忽略"
        );
        assert_eq!(
            state.commands.get("cmd-1").unwrap().status,
            status::ACCEPTED
        );
    }

    #[tokio::test]
    async fn reaper_times_out_inflight_commands_only() {
        let (state, _rx) = state_with_device(true);
        submit(&state, command("rtl")).await;
        assert_eq!(
            reap_expired(&state, now_ms()).await,
            0,
            "deadline 未到不应超时"
        );
        let deadline = state.commands.get("cmd-1").unwrap().deadline_ms;
        assert_eq!(reap_expired(&state, deadline + 1).await, 1);
        assert_eq!(state.commands.get("cmd-1").unwrap().status, status::TIMEOUT);
        // 已完成指令不会被回收
        state.commands.get_mut("cmd-1").unwrap().status = status::ACCEPTED.into();
        assert_eq!(reap_expired(&state, deadline + 10_000).await, 0);
    }

    #[tokio::test]
    async fn device_offline_fails_inflight_commands() {
        let (state, _rx) = state_with_device(true);
        submit(&state, command("rtl")).await;
        assert_eq!(
            fail_inflight_for_device(&state, "drone-001", "adapter disconnected").await,
            1
        );
        assert_eq!(state.commands.get("cmd-1").unwrap().status, status::FAILED);
    }

    #[tokio::test]
    async fn prune_terminal_drops_only_old_finished_commands() {
        let (state, _rx) = state_with_device(true);
        submit(&state, command("rtl")).await;
        // 未到终态：即使很旧也不能删
        state.commands.get_mut("cmd-1").unwrap().updated_ms = 1;
        assert_eq!(prune_terminal(&state, 10_000_000, 3_600_000), 0);

        // 终态且超过保留期：删除
        {
            let mut record = state.commands.get_mut("cmd-1").unwrap();
            record.status = status::ACCEPTED.into();
            record.updated_ms = 1;
        }
        assert_eq!(prune_terminal(&state, 10_000_000, 3_600_000), 1);
        assert!(state.commands.is_empty());
    }

    #[tokio::test]
    async fn flight_command_requires_control_when_enabled() {
        let (state, _rx) = state_with_control(true, true);
        let denied = submit(&state, command("rtl")).await;
        assert_eq!(denied.status, status::REJECTED);
        assert_eq!(denied.code, 403);

        crate::control::acquire(&state, "drone-001", "flight", "alice", 10, 60)
            .await
            .unwrap();
        let allowed = submit(&state, command("rtl")).await;
        assert_eq!(allowed.status, status::SENT);
    }

    #[tokio::test]
    async fn goto_into_no_fly_fence_is_rejected() {
        let (state, _rx) = state_with_control(true, true);
        crate::control::acquire(&state, "drone-001", "flight", "alice", 10, 60)
            .await
            .unwrap();
        state.fences.insert(
            "f1".into(),
            crate::state::FenceRecord {
                id: "f1".into(),
                name: "禁飞区".into(),
                rule_type: "no_fly".into(),
                shape: "polygon".into(),
                geojson: r#"{"type":"Polygon","coordinates":[[[121.40,31.20],[121.50,31.20],[121.50,31.30],[121.40,31.30],[121.40,31.20]]]}"#.into(),
                center_lat: 0.0,
                center_lon: 0.0,
                radius_m: 0.0,
                min_alt: 0.0,
                max_alt: 120.0,
                device_id: String::new(),
                device_ids: Vec::new(),
                enabled: true,
                version: 1,
                updated_ms: 0,
            },
        );
        let mut cmd = command("goto");
        cmd.params.insert("lat".into(), "31.25".into());
        cmd.params.insert("lon".into(), "121.45".into());
        cmd.params.insert("alt".into(), "30".into());
        let reply = submit(&state, cmd).await;
        assert_eq!(reply.status, status::REJECTED);
        assert_eq!(reply.code, 403);
        assert!(reply.message.contains("禁飞区"));
    }

    // ---------- 规则 §6.8 步骤 3 / §12.3：参数范围校验 ----------

    #[tokio::test]
    async fn goto_without_coordinates_is_rejected() {
        let (state, _rx) = state_with_control(true, true);
        crate::control::acquire(&state, "drone-001", "flight", "alice", 10, 60)
            .await
            .unwrap();
        let reply = submit(&state, command("goto")).await;
        assert_eq!(
            reply.status,
            status::REJECTED,
            "缺少坐标的 goto 必须被拒绝，不能绕过围栏预检"
        );
        assert_eq!(reply.code, 400);
    }

    #[tokio::test]
    async fn out_of_range_coordinates_are_rejected() {
        let (state, _rx) = state_with_control(true, true);
        crate::control::acquire(&state, "drone-001", "flight", "alice", 10, 60)
            .await
            .unwrap();

        let mut cmd = command("goto");
        cmd.params.insert("lat".into(), "95.0".into());
        cmd.params.insert("lon".into(), "121.4".into());
        let reply = submit(&state, cmd).await;
        assert_eq!(reply.status, status::REJECTED);
        assert!(reply.message.contains("lat"));

        let mut cmd = command("goto");
        cmd.params.insert("lat".into(), "31.2".into());
        cmd.params.insert("lon".into(), "200.0".into());
        let reply = submit(&state, cmd).await;
        assert_eq!(reply.status, status::REJECTED);
        assert!(reply.message.contains("lon"));
    }

    #[tokio::test]
    async fn out_of_range_altitude_is_rejected() {
        let (state, _rx) = state_with_control(true, true);
        crate::control::acquire(&state, "drone-001", "flight", "alice", 10, 60)
            .await
            .unwrap();

        let mut cmd = command("takeoff");
        cmd.params.insert("alt".into(), "9000".into());
        let reply = submit(&state, cmd).await;
        assert_eq!(reply.status, status::REJECTED);
        assert!(reply.message.contains("alt"));
    }

    #[tokio::test]
    async fn out_of_range_gimbal_angles_are_rejected() {
        let (state, _rx) = state_with_control(true, true);
        crate::control::acquire(&state, "drone-001", "gimbal", "alice", 10, 60)
            .await
            .unwrap();

        let mut cmd = command("gimbal");
        cmd.params.insert("pitch".into(), "120".into());
        let reply = submit(&state, cmd).await;
        assert_eq!(reply.status, status::REJECTED);
        assert!(reply.message.contains("pitch"));

        let mut cmd = command("gimbal");
        cmd.params.insert("yaw".into(), "400".into());
        let reply = submit(&state, cmd).await;
        assert_eq!(reply.status, status::REJECTED);
        assert!(reply.message.contains("yaw"));
    }

    #[tokio::test]
    async fn unknown_camera_action_is_rejected() {
        let (state, _rx) = state_with_control(true, true);
        crate::control::acquire(&state, "drone-001", "gimbal", "alice", 10, 60)
            .await
            .unwrap();

        let mut cmd = command("camera");
        cmd.params.insert("action".into(), "self_destruct".into());
        let reply = submit(&state, cmd).await;
        assert_eq!(reply.status, status::REJECTED);
        assert!(reply.message.contains("camera action"));
    }

    // ---------- 规则 §6.8 步骤 4：幂等 ----------

    #[tokio::test]
    async fn duplicate_cmd_id_is_not_dispatched_twice() {
        let (state, mut rx) = state_with_device(true);
        let first = submit(&state, command("rtl")).await;
        assert_eq!(first.status, status::SENT);
        assert!(rx.try_recv().is_ok(), "第一条指令应已下发");

        let mut second_cmd = command("land");
        second_cmd.cmd_id = "cmd-1".into();
        let second = submit(&state, second_cmd).await;
        assert_eq!(second.cmd_id, "cmd-1");
        assert!(rx.try_recv().is_err(), "重复 cmd_id 不得再次下发到 adapter");
    }

    #[test]
    fn parameter_validation_rejects_out_of_range_values() {
        let mut cmd = command("goto");
        cmd.params.insert("lat".into(), "31.2".into());
        cmd.params.insert("lon".into(), "121.4".into());
        assert!(validate_params(&cmd).is_ok());

        let mut bad = command("goto");
        bad.params.insert("lat".into(), "200".into());
        bad.params.insert("lon".into(), "121.4".into());
        assert!(validate_params(&bad).is_err());

        let mut bad = command("camera");
        bad.params.insert("action".into(), "boom".into());
        assert!(validate_params(&bad).is_err());

        let mut bad = command("gimbal");
        bad.params.insert("zoom".into(), "0.1".into());
        assert!(validate_params(&bad).is_err());
    }
}
