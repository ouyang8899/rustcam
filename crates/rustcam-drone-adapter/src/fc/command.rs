//! 指令映射与回执：把平台中立的 `Command` 翻译成 PX4 / ArduPilot 都能执行的
//! MAVLink 消息序列，并把 `COMMAND_ACK` 折回统一状态。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use ::mavlink::dialects::ardupilotmega::{
    MavCmd, MavFrame, MavMessage, MavModeFlag, MavResult, MavType, PositionTargetTypemask,
    COMMAND_LONG_DATA, SET_POSITION_TARGET_GLOBAL_INT_DATA,
};
use ::mavlink::MavHeader;

use super::FcLink;
use tokio::sync::mpsc;

use rustcam_drone_core::command::status;
use rustcam_drone_proto::common as proto;

use super::autopilot::{
    ardupilot_guided_mode, px4_custom_mode, AutopilotKind, PX4_MAIN_MODE_AUTO,
    PX4_SUB_MODE_AUTO_TAKEOFF,
};

const DEFAULT_TAKEOFF_ALT_M: f64 = 10.0;
const DEFAULT_GOTO_ALT_M: f64 = 30.0;
/// MAV_CMD_DO_MOUNT_CONTROL 的 param7：MAV_MOUNT_MODE_MAVLINK_TARGETING = 2。
const MOUNT_MODE_MAVLINK_TARGETING: f32 = 2.0;
/// 只发送位置（忽略速度 / 加速度 / 偏航 / 偏航角速度）的 type_mask。
const POSITION_ONLY_MASK: PositionTargetTypemask = PositionTargetTypemask::from_bits_truncate(
    PositionTargetTypemask::POSITION_TARGET_TYPEMASK_VX_IGNORE.bits()
        | PositionTargetTypemask::POSITION_TARGET_TYPEMASK_VY_IGNORE.bits()
        | PositionTargetTypemask::POSITION_TARGET_TYPEMASK_VZ_IGNORE.bits()
        | PositionTargetTypemask::POSITION_TARGET_TYPEMASK_AX_IGNORE.bits()
        | PositionTargetTypemask::POSITION_TARGET_TYPEMASK_AY_IGNORE.bits()
        | PositionTargetTypemask::POSITION_TARGET_TYPEMASK_AZ_IGNORE.bits()
        | PositionTargetTypemask::POSITION_TARGET_TYPEMASK_YAW_IGNORE.bits()
        | PositionTargetTypemask::POSITION_TARGET_TYPEMASK_YAW_RATE_IGNORE.bits(),
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanError(pub String);

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PlanError {}

/// 指令执行序列中的一步；只有最后一步可能等待 ACK。
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub message: MavMessage,
    pub wait_ack: Option<u16>,
    pub label: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandPlan {
    pub steps: Vec<Step>,
    /// 是否至少有一个等待 ACK 的步骤（决定回执是 accepted 还是 timeout）。
    pub expects_ack: bool,
}

#[derive(Debug, Default)]
pub struct PendingCommands {
    inner: Mutex<HashMap<u16, PendingEntry>>,
}

#[derive(Debug)]
struct PendingEntry {
    cmd_id: String,
    tx: mpsc::UnboundedSender<MavResult>,
}

impl PendingCommands {
    pub fn new() -> PendingCommands {
        PendingCommands::default()
    }

    /// 登记一条等待 ACK 的指令；同一 MAV_CMD 同时只允许一条在途。
    pub fn register(
        &self,
        command: u16,
        cmd_id: &str,
    ) -> Result<mpsc::UnboundedReceiver<MavResult>, PlanError> {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = map.get(&command) {
            return Err(PlanError(format!(
                "command {} is already pending (cmd_id={})",
                command, existing.cmd_id
            )));
        }
        let (tx, rx) = mpsc::unbounded_channel();
        map.insert(
            command,
            PendingEntry {
                cmd_id: cmd_id.to_string(),
                tx,
            },
        );
        Ok(rx)
    }

    /// 把 ACK 投递给等待中的指令；`IN_PROGRESS` 保留登记，终态则移除。
    pub fn resolve(&self, command: u16, result: MavResult) -> bool {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = map.get(&command) else {
            return false;
        };
        let sent = entry.tx.send(result).is_ok();
        if result != MavResult::MAV_RESULT_IN_PROGRESS {
            map.remove(&command);
        }
        sent
    }

    pub fn remove(&self, command: u16) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&command);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// 指令 → MAVLink
// ---------------------------------------------------------------------------

#[allow(deprecated)] // MAV_CMD_DO_MOUNT_CONTROL 仍是老云台兼容性最好的入口
pub fn plan(
    autopilot: AutopilotKind,
    vehicle_type: MavType,
    target_system: u8,
    target_component: u8,
    home_alt: f64,
    kind: &str,
    params: &HashMap<String, String>,
) -> Result<CommandPlan, PlanError> {
    match kind {
        "takeoff" => plan_takeoff(
            autopilot,
            vehicle_type,
            target_system,
            target_component,
            params,
        ),
        "land" => Ok(CommandPlan {
            steps: vec![Step {
                message: command_long(
                    target_system,
                    target_component,
                    MavCmd::MAV_CMD_NAV_LAND,
                    [0.0, 0.0, 0.0, f32::NAN, 0.0, 0.0, 0.0],
                ),
                wait_ack: Some(MavCmd::MAV_CMD_NAV_LAND as u32 as u16),
                label: "nav_land",
            }],
            expects_ack: true,
        }),
        "rtl" => Ok(CommandPlan {
            steps: vec![Step {
                message: command_long(
                    target_system,
                    target_component,
                    MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH,
                    [0.0; 7],
                ),
                wait_ack: Some(MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH as u32 as u16),
                label: "nav_return_to_launch",
            }],
            expects_ack: true,
        }),
        "goto" => plan_goto(
            autopilot,
            vehicle_type,
            target_system,
            target_component,
            home_alt,
            params,
        ),
        "gimbal" => plan_gimbal(target_system, target_component, params),
        "yaw" => plan_yaw(target_system, target_component, params),
        "camera" => plan_camera(target_system, target_component, params),
        other => Err(PlanError(format!("unsupported command kind `{other}`"))),
    }
}

fn plan_takeoff(
    autopilot: AutopilotKind,
    vehicle_type: MavType,
    target_system: u8,
    target_component: u8,
    params: &HashMap<String, String>,
) -> Result<CommandPlan, PlanError> {
    let alt = param_f64(params, "alt")?.unwrap_or(DEFAULT_TAKEOFF_ALT_M);
    if !(0.5..=500.0).contains(&alt) {
        return Err(PlanError(format!(
            "takeoff alt {alt} is out of range [0.5, 500]"
        )));
    }

    let mut steps = Vec::new();
    match autopilot {
        AutopilotKind::Px4 => steps.push(Step {
            message: set_mode(
                target_system,
                px4_custom_mode(PX4_MAIN_MODE_AUTO, PX4_SUB_MODE_AUTO_TAKEOFF),
            ),
            wait_ack: None,
            label: "px4_mode_auto_takeoff",
        }),
        AutopilotKind::ArduPilot => steps.push(Step {
            message: set_mode(target_system, ardupilot_guided_mode(vehicle_type)),
            wait_ack: None,
            label: "ardupilot_mode_guided",
        }),
        _ => {}
    }
    steps.push(Step {
        message: command_long(
            target_system,
            target_component,
            MavCmd::MAV_CMD_NAV_TAKEOFF,
            [0.0, 0.0, 0.0, f32::NAN, 0.0, 0.0, alt as f32],
        ),
        wait_ack: Some(MavCmd::MAV_CMD_NAV_TAKEOFF as u32 as u16),
        label: "nav_takeoff",
    });

    Ok(CommandPlan {
        steps,
        expects_ack: true,
    })
}

fn plan_goto(
    autopilot: AutopilotKind,
    vehicle_type: MavType,
    target_system: u8,
    target_component: u8,
    home_alt: f64,
    params: &HashMap<String, String>,
) -> Result<CommandPlan, PlanError> {
    let lat = param_f64(params, "lat")?.ok_or_else(|| PlanError("goto requires `lat`".into()))?;
    let lon = param_f64(params, "lon")?.ok_or_else(|| PlanError("goto requires `lon`".into()))?;
    let alt = param_f64(params, "alt")?.unwrap_or(DEFAULT_GOTO_ALT_M);
    if !(-90.0..=90.0).contains(&lat) {
        return Err(PlanError(format!("goto lat {lat} is out of range")));
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err(PlanError(format!("goto lon {lon} is out of range")));
    }

    match autopilot {
        // PX4 的 MAVLink 位置指令走 DO_REPOSITION；SET_POSITION_TARGET 只在 OFFBOARD
        // 下生效，直接发会被静默忽略。
        //
        // DO_REPOSITION 的 alt 参数是 **AMSL**（海拔），而产品侧给的是相对起飞点高度，
        // 因此这里要加上 home_alt 换算（规则 §7.4）。
        AutopilotKind::Px4 => {
            let alt_amsl = if home_alt > 0.0 { home_alt + alt } else { alt };
            Ok(CommandPlan {
                steps: vec![Step {
                    message: command_long(
                        target_system,
                        target_component,
                        MavCmd::MAV_CMD_DO_REPOSITION,
                        [
                            -1.0, // 使用飞控默认速度
                            0.0,  // bitmask
                            0.0,  // loiter 半径
                            f32::NAN,
                            lat as f32,
                            lon as f32,
                            alt_amsl as f32,
                        ],
                    ),
                    wait_ack: Some(MavCmd::MAV_CMD_DO_REPOSITION as u32 as u16),
                    label: "px4_do_reposition",
                }],
                expects_ack: true,
            })
        }
        // ArduPilot 先在 GUIDED 下接受 SET_POSITION_TARGET_GLOBAL_INT（无 ACK）。
        AutopilotKind::ArduPilot => Ok(CommandPlan {
            steps: vec![
                Step {
                    message: set_mode(target_system, ardupilot_guided_mode(vehicle_type)),
                    wait_ack: None,
                    label: "ardupilot_mode_guided",
                },
                Step {
                    message: position_target(target_system, target_component, lat, lon, alt),
                    wait_ack: None,
                    label: "ardupilot_set_position_target",
                },
            ],
            expects_ack: false,
        }),
        _ => Ok(CommandPlan {
            steps: vec![Step {
                message: position_target(target_system, target_component, lat, lon, alt),
                wait_ack: None,
                label: "set_position_target",
            }],
            expects_ack: false,
        }),
    }
}

#[allow(deprecated)]
fn plan_gimbal(
    target_system: u8,
    target_component: u8,
    params: &HashMap<String, String>,
) -> Result<CommandPlan, PlanError> {
    let pitch = param_f64(params, "pitch")?.unwrap_or(0.0);
    let roll = param_f64(params, "roll")?.unwrap_or(0.0);
    let yaw = param_f64(params, "yaw")?.unwrap_or(0.0);
    Ok(CommandPlan {
        steps: vec![Step {
            message: command_long(
                target_system,
                target_component,
                MavCmd::MAV_CMD_DO_MOUNT_CONTROL,
                [
                    pitch as f32,
                    roll as f32,
                    yaw as f32,
                    0.0,
                    0.0,
                    0.0,
                    MOUNT_MODE_MAVLINK_TARGETING,
                ],
            ),
            wait_ack: Some(MavCmd::MAV_CMD_DO_MOUNT_CONTROL as u32 as u16),
            label: "do_mount_control",
        }],
        expects_ack: true,
    })
}

/// 机体转向：绝对角度（度），走 `MAV_CMD_CONDITION_YAW`。
fn plan_yaw(
    target_system: u8,
    target_component: u8,
    params: &HashMap<String, String>,
) -> Result<CommandPlan, PlanError> {
    let angle = param_f64(params, "angle")?
        .or_else(|| param_f64(params, "yaw").ok().flatten())
        .unwrap_or(0.0)
        .rem_euclid(360.0);
    let speed = param_f64(params, "speed")?
        .unwrap_or(30.0)
        .clamp(1.0, 180.0);
    Ok(CommandPlan {
        steps: vec![Step {
            message: command_long(
                target_system,
                target_component,
                MavCmd::MAV_CMD_CONDITION_YAW,
                [
                    angle as f32,
                    speed as f32,
                    0.0, // 0 = 最短方向
                    0.0, // 0 = 绝对角度
                    0.0,
                    0.0,
                    0.0,
                ],
            ),
            wait_ack: Some(MavCmd::MAV_CMD_CONDITION_YAW as u32 as u16),
            label: "condition_yaw",
        }],
        expects_ack: true,
    })
}

fn plan_camera(
    target_system: u8,
    target_component: u8,
    params: &HashMap<String, String>,
) -> Result<CommandPlan, PlanError> {
    let action = params
        .get("action")
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "photo".to_string());

    let (command, values, label) = match action.as_str() {
        "photo" | "picture" | "capture" => {
            let interval = param_f64(params, "interval")?.unwrap_or(0.0).max(0.0) as f32;
            let count = param_f64(params, "count")?.unwrap_or(1.0).max(1.0) as f32;
            (
                MavCmd::MAV_CMD_IMAGE_START_CAPTURE,
                [0.0, interval, count, 0.0, 0.0, 0.0, 0.0],
                "image_start_capture",
            )
        }
        "video_start" | "video" => (
            MavCmd::MAV_CMD_VIDEO_START_CAPTURE,
            [0.0; 7],
            "video_start_capture",
        ),
        "video_stop" => (
            MavCmd::MAV_CMD_VIDEO_STOP_CAPTURE,
            [0.0; 7],
            "video_stop_capture",
        ),
        other => {
            return Err(PlanError(format!(
                "unsupported camera action `{other}` (photo|video_start|video_stop)"
            )))
        }
    };

    Ok(CommandPlan {
        steps: vec![Step {
            message: command_long(target_system, target_component, command, values),
            wait_ack: Some(command as u32 as u16),
            label,
        }],
        expects_ack: true,
    })
}

fn command_long(
    target_system: u8,
    target_component: u8,
    command: MavCmd,
    params: [f32; 7],
) -> MavMessage {
    MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
        param1: params[0],
        param2: params[1],
        param3: params[2],
        param4: params[3],
        param5: params[4],
        param6: params[5],
        param7: params[6],
        command,
        target_system,
        target_component,
        confirmation: 0,
    })
}

fn set_mode(target_system: u8, custom_mode: u32) -> MavMessage {
    // SET_MODE 自 2015 年起被 MAV_CMD_DO_SET_MODE 取代；PX4 与 ArduPilot 均支持后者。
    command_long(
        target_system,
        1,
        MavCmd::MAV_CMD_DO_SET_MODE,
        [
            MavModeFlag::MAV_MODE_FLAG_CUSTOM_MODE_ENABLED.bits() as f32,
            custom_mode as f32,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ],
    )
}

#[allow(deprecated)] // MAV_FRAME_GLOBAL_RELATIVE_ALT_INT 是 GLOBAL_INT 消息的既有取值
fn position_target(
    target_system: u8,
    target_component: u8,
    lat: f64,
    lon: f64,
    alt: f64,
) -> MavMessage {
    MavMessage::SET_POSITION_TARGET_GLOBAL_INT(SET_POSITION_TARGET_GLOBAL_INT_DATA {
        time_boot_ms: 0,
        lat_int: (lat * 1e7).round() as i32,
        lon_int: (lon * 1e7).round() as i32,
        alt: alt as f32,
        vx: 0.0,
        vy: 0.0,
        vz: 0.0,
        afx: 0.0,
        afy: 0.0,
        afz: 0.0,
        yaw: 0.0,
        yaw_rate: 0.0,
        type_mask: POSITION_ONLY_MASK,
        target_system,
        target_component,
        coordinate_frame: MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT_INT,
    })
}

fn param_f64(params: &HashMap<String, String>, key: &str) -> Result<Option<f64>, PlanError> {
    match params.get(key) {
        None => Ok(None),
        Some(raw) if raw.trim().is_empty() => Ok(None),
        Some(raw) => raw
            .trim()
            .parse::<f64>()
            .map(Some)
            .map_err(|_| PlanError(format!("parameter `{key}` is not a number: `{raw}`"))),
    }
}

// ---------------------------------------------------------------------------
// 执行
// ---------------------------------------------------------------------------

fn reply(
    cmd: &proto::Command,
    status: &str,
    code: i32,
    message: impl Into<String>,
) -> proto::CommandReply {
    proto::CommandReply {
        cmd_id: cmd.cmd_id.clone(),
        drone_tag: cmd.drone_tag.clone(),
        status: status.to_string(),
        code,
        message: message.into(),
        ts_ms: chrono::Utc::now().timestamp_millis(),
    }
}

/// 执行一条指令：发送 MAVLink 序列并等待 COMMAND_ACK。
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    link: &FcLink,
    header: MavHeader,
    pending: &PendingCommands,
    autopilot: AutopilotKind,
    vehicle_type: MavType,
    target_system: u8,
    target_component: u8,
    home_alt: f64,
    timeout: Duration,
    cmd: &proto::Command,
) -> proto::CommandReply {
    let now = chrono::Utc::now().timestamp_millis();
    if cmd.deadline_ms > 0 && cmd.deadline_ms <= now {
        return reply(
            cmd,
            status::TIMEOUT,
            408,
            "command deadline already exceeded before dispatch",
        );
    }
    let budget = if cmd.deadline_ms > 0 {
        Duration::from_millis((cmd.deadline_ms - now).max(0) as u64).min(timeout)
    } else {
        timeout
    };
    let started = std::time::Instant::now();

    let plan = match plan(
        autopilot,
        vehicle_type,
        target_system,
        target_component,
        home_alt,
        &cmd.r#type,
        &cmd.params,
    ) {
        Ok(p) => p,
        Err(e) => return reply(cmd, status::REJECTED, 400, e.to_string()),
    };

    let mut ack_rx = None;
    for (idx, step) in plan.steps.iter().enumerate() {
        if let Some(command) = step.wait_ack {
            match pending.register(command, &cmd.cmd_id) {
                Ok(rx) => ack_rx = Some((command, rx)),
                Err(e) => return reply(cmd, status::FAILED, 409, e.to_string()),
            }
        }

        if let Err(e) = link.send(&header, &step.message).await {
            if let Some(command) = step.wait_ack {
                pending.remove(command);
            }
            return reply(
                cmd,
                status::FAILED,
                502,
                format!("failed to send {}: {e}", step.label),
            );
        }

        // 模式切换与随后的动作指令之间留一点时间，避免飞控还没切完模式就收到动作。
        if step.wait_ack.is_none() && idx + 1 < plan.steps.len() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    let Some((ack_command, mut rx)) = ack_rx else {
        return reply(
            cmd,
            status::ACCEPTED,
            202,
            "dispatched (no MAVLink ACK for this command)",
        );
    };

    loop {
        let elapsed = started.elapsed();
        let remaining = budget.saturating_sub(elapsed);
        if remaining.is_zero() {
            pending.remove(ack_command);
            return reply(
                cmd,
                status::TIMEOUT,
                408,
                format!("no COMMAND_ACK for command {ack_command} within {budget:?}"),
            );
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Err(_) => {
                pending.remove(ack_command);
                return reply(
                    cmd,
                    status::TIMEOUT,
                    408,
                    format!("no COMMAND_ACK for command {ack_command} within {budget:?}"),
                );
            }
            Ok(None) => {
                pending.remove(ack_command);
                return reply(
                    cmd,
                    status::FAILED,
                    502,
                    format!("ACK channel closed for command {ack_command}"),
                );
            }
            Ok(Some(MavResult::MAV_RESULT_IN_PROGRESS)) => continue,
            Ok(Some(MavResult::MAV_RESULT_ACCEPTED)) => {
                return reply(cmd, status::ACCEPTED, 200, "COMMAND_ACK accepted");
            }
            Ok(Some(result)) => {
                return reply(
                    cmd,
                    status::FAILED,
                    502,
                    format!("COMMAND_ACK rejected: {result:?}"),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(deprecated)]

    use super::*;
    use ::mavlink::dialects::ardupilotmega::MavMountMode;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn command_long_of(msg: &MavMessage) -> &COMMAND_LONG_DATA {
        match msg {
            MavMessage::COMMAND_LONG(c) => c,
            other => panic!("expected COMMAND_LONG, got {other:?}"),
        }
    }

    #[test]
    fn px4_takeoff_sets_auto_takeoff_then_commands() {
        let plan = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "takeoff",
            &params(&[("alt", "15")]),
        )
        .unwrap();
        assert!(plan.expects_ack);
        assert_eq!(plan.steps.len(), 2);

        let mode = command_long_of(&plan.steps[0].message);
        assert_eq!(mode.command, MavCmd::MAV_CMD_DO_SET_MODE);
        assert!(
            (mode.param2 - px4_custom_mode(PX4_MAIN_MODE_AUTO, PX4_SUB_MODE_AUTO_TAKEOFF) as f32)
                .abs()
                < 1e-3
        );
        assert_eq!(mode.target_system, 1);
        let takeoff = command_long_of(&plan.steps[1].message);
        assert_eq!(takeoff.command, MavCmd::MAV_CMD_NAV_TAKEOFF);
        assert!((takeoff.param7 - 15.0).abs() < 1e-6);
        assert_eq!(
            plan.steps[1].wait_ack,
            Some(MavCmd::MAV_CMD_NAV_TAKEOFF as u32 as u16)
        );
    }

    #[test]
    fn ardupilot_takeoff_enters_guided_first() {
        let plan = plan(
            AutopilotKind::ArduPilot,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "takeoff",
            &HashMap::new(),
        )
        .unwrap();
        let mode = command_long_of(&plan.steps[0].message);
        assert_eq!(mode.command, MavCmd::MAV_CMD_DO_SET_MODE);
        assert!((mode.param2 - 4.0).abs() < 1e-6);
        assert!(
            (command_long_of(&plan.steps[1].message).param7 - DEFAULT_TAKEOFF_ALT_M as f32).abs()
                < 1e-6
        );
    }

    #[test]
    fn takeoff_alt_range_is_validated() {
        let err = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "takeoff",
            &params(&[("alt", "900")]),
        )
        .unwrap_err();
        assert!(err.0.contains("out of range"));
        assert!(plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "takeoff",
            &params(&[("alt", "abc")]),
        )
        .is_err());
    }

    #[test]
    fn land_and_rtl_map_to_nav_commands() {
        let land = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "land",
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(
            command_long_of(&land.steps[0].message).command,
            MavCmd::MAV_CMD_NAV_LAND
        );

        let rtl = plan(
            AutopilotKind::ArduPilot,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "rtl",
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(
            command_long_of(&rtl.steps[0].message).command,
            MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH
        );
    }

    #[test]
    fn px4_goto_uses_do_reposition() {
        let plan = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "goto",
            &params(&[("lat", "31.2304"), ("lon", "121.4737"), ("alt", "25")]),
        )
        .unwrap();
        assert_eq!(plan.steps.len(), 1);
        let c = command_long_of(&plan.steps[0].message);
        assert_eq!(c.command, MavCmd::MAV_CMD_DO_REPOSITION);
        assert!((c.param5 - 31.2304).abs() < 1e-4);
        assert!((c.param6 - 121.4737).abs() < 1e-4);
        assert!((c.param7 - 25.0).abs() < 1e-6);
        assert_eq!(
            plan.steps[0].wait_ack,
            Some(MavCmd::MAV_CMD_DO_REPOSITION as u32 as u16)
        );
    }

    #[test]
    fn px4_goto_converts_relative_alt_to_amsl() {
        // DO_REPOSITION 的 alt 是 AMSL；产品给的是相对起飞点高度，需加 home_alt。
        let plan = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            100.0,
            "goto",
            &params(&[("lat", "31.2304"), ("lon", "121.4737"), ("alt", "25")]),
        )
        .unwrap();
        let c = command_long_of(&plan.steps[0].message);
        assert!(
            (c.param7 - 125.0).abs() < 1e-6,
            "相对高度 25m + home 100m 应换算为 AMSL 125m，实际 {}",
            c.param7
        );
    }

    #[test]
    fn ardupilot_goto_sets_guided_and_position_target() {
        let plan = plan(
            AutopilotKind::ArduPilot,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "goto",
            &params(&[("lat", "31.2304"), ("lon", "121.4737")]),
        )
        .unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert!(!plan.expects_ack);
        match &plan.steps[0].message {
            MavMessage::COMMAND_LONG(m) => {
                assert_eq!(m.command, MavCmd::MAV_CMD_DO_SET_MODE);
                assert!((m.param2 - 4.0).abs() < 1e-6);
            }
            other => panic!("expected DO_SET_MODE, got {other:?}"),
        }
        match &plan.steps[1].message {
            MavMessage::SET_POSITION_TARGET_GLOBAL_INT(p) => {
                assert_eq!(p.lat_int, 312_304_000);
                assert_eq!(p.lon_int, 1_214_737_000);
                assert_eq!(
                    p.coordinate_frame,
                    MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT_INT
                );
                assert!((p.alt - DEFAULT_GOTO_ALT_M as f32).abs() < 1e-6);
                assert!(p
                    .type_mask
                    .contains(PositionTargetTypemask::POSITION_TARGET_TYPEMASK_VX_IGNORE));
            }
            other => panic!("expected SET_POSITION_TARGET_GLOBAL_INT, got {other:?}"),
        }
    }

    #[test]
    fn goto_requires_valid_coordinates() {
        assert!(plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "goto",
            &params(&[("lat", "31.0")]),
        )
        .is_err());
        assert!(plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "goto",
            &params(&[("lat", "91.0"), ("lon", "121.0")]),
        )
        .is_err());
    }

    #[test]
    fn gimbal_maps_to_do_mount_control() {
        let plan = plan(
            AutopilotKind::ArduPilot,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "gimbal",
            &params(&[("pitch", "-30"), ("yaw", "45")]),
        )
        .unwrap();
        let c = command_long_of(&plan.steps[0].message);
        assert_eq!(c.command, MavCmd::MAV_CMD_DO_MOUNT_CONTROL);
        assert!((c.param1 + 30.0).abs() < 1e-6);
        assert!((c.param3 - 45.0).abs() < 1e-6);
        assert!((c.param7 - MOUNT_MODE_MAVLINK_TARGETING).abs() < 1e-6);
        // 与 mavlink 枚举保持一致，避免魔法数字漂移
        assert_eq!(
            MOUNT_MODE_MAVLINK_TARGETING as u32,
            MavMountMode::MAV_MOUNT_MODE_MAVLINK_TARGETING as u32
        );
    }

    #[test]
    fn camera_actions_map_to_capture_commands() {
        let photo = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "camera",
            &params(&[("action", "photo"), ("count", "3")]),
        )
        .unwrap();
        let c = command_long_of(&photo.steps[0].message);
        assert_eq!(c.command, MavCmd::MAV_CMD_IMAGE_START_CAPTURE);
        assert!((c.param3 - 3.0).abs() < 1e-6);

        let start = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "camera",
            &params(&[("action", "video_start")]),
        )
        .unwrap();
        assert_eq!(
            command_long_of(&start.steps[0].message).command,
            MavCmd::MAV_CMD_VIDEO_START_CAPTURE
        );

        let stop = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "camera",
            &params(&[("action", "video_stop")]),
        )
        .unwrap();
        assert_eq!(
            command_long_of(&stop.steps[0].message).command,
            MavCmd::MAV_CMD_VIDEO_STOP_CAPTURE
        );

        assert!(plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "camera",
            &params(&[("action", "selfie")]),
        )
        .is_err());
    }

    #[test]
    fn unknown_kind_is_rejected() {
        let err = plan(
            AutopilotKind::Px4,
            MavType::MAV_TYPE_QUADROTOR,
            1,
            1,
            0.0,
            "self_destruct",
            &HashMap::new(),
        )
        .unwrap_err();
        assert!(err.0.contains("unsupported"));
    }

    #[test]
    fn pending_commands_reject_duplicates_and_resolve() {
        let pending = PendingCommands::new();
        let mut rx = pending.register(84, "cmd-1").unwrap();
        assert!(pending
            .register(84, "cmd-2")
            .unwrap_err()
            .0
            .contains("already pending"));
        assert!(pending.resolve(84, MavResult::MAV_RESULT_IN_PROGRESS));
        assert_eq!(
            pending.len(),
            1,
            "in-progress ACK keeps the command pending"
        );
        assert_eq!(rx.try_recv().unwrap(), MavResult::MAV_RESULT_IN_PROGRESS);
        assert!(pending.resolve(84, MavResult::MAV_RESULT_ACCEPTED));
        assert!(pending.is_empty());
        assert_eq!(rx.try_recv().unwrap(), MavResult::MAV_RESULT_ACCEPTED);
        assert!(!pending.resolve(84, MavResult::MAV_RESULT_ACCEPTED));
    }
}
