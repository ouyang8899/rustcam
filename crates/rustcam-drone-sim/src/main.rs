//! 轻量 MAVLink 机体模拟器：用于在无 PX4/ArduPilot SITL 的机器上跑通
//! adapter → drone-service → gateway → 前端 的完整链路。
//!
//! 默认从 `udpout:127.0.0.1:14540` 向 adapter 发送心跳/遥测（adapter 默认监听
//! `udpin:0.0.0.0:14540`），并响应起飞、降落、返航、定点、模式、云台、拍照指令。

#![allow(deprecated)] // 模拟器需要兼容 SET_MODE / DO_MOUNT_CONTROL 等旧指令

use std::time::Duration;

use ::mavlink::dialects::ardupilotmega::{
    GpsFixType, MavAutopilot, MavCmd, MavFrame, MavLandedState, MavMessage, MavModeFlag,
    MavProtocolCapability, MavResult, MavState, MavSysStatusSensor, MavType, ATTITUDE_DATA,
    COMMAND_ACK_DATA, EXTENDED_SYS_STATE_DATA, GLOBAL_POSITION_INT_DATA, GPS_RAW_INT_DATA,
    HEARTBEAT_DATA, HOME_POSITION_DATA, SYS_STATUS_DATA,
};
use ::mavlink::MavHeader;
use ::mavlink::{connect_async, AsyncConnection, AsyncMavConnection};
use anyhow::Context;

const TICK_SECS: f64 = 0.1;
const HOME_ALT_M: f64 = 30.0;
/// 唯一标识默认值（飞控侧 `AUTOPILOT_VERSION.uid`）。
///
/// 平台侧登记的唯一标识必须与本值一致，否则指令会被唯一标识核验拒绝。
/// 可用 `--uid` 覆盖。
const DEFAULT_UID: u64 = 0x0011_2233_4455_6677;
/// `AUTOPILOT_VERSION` 重发周期（秒）。
const VERSION_REPORT_SECS: u64 = 5;

/// 解析唯一标识：十六进制文本（可带 `0x` 前缀）→ u64。
///
/// 平台侧登记值是**小写十六进制字符串**，因此这里只接受十六进制字符，
/// 长度上限 16 位（u64 范围）。
fn parse_uid(text: &str) -> anyhow::Result<u64> {
    let t = text.trim();
    let hex = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t)
        .trim();
    if hex.is_empty() {
        anyhow::bail!("uid is empty");
    }
    if hex.len() > 16 {
        anyhow::bail!("uid `{hex}` exceeds 16 hex digits (u64)");
    }
    u64::from_str_radix(hex, 16).with_context(|| format!("invalid uid `{hex}`"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Autopilot {
    Px4,
    ArduPilot,
}

impl Autopilot {
    fn mav(self) -> MavAutopilot {
        match self {
            Autopilot::Px4 => MavAutopilot::MAV_AUTOPILOT_PX4,
            Autopilot::ArduPilot => MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Takeoff,
    GoTo,
    Land,
    Rtl,
    Hover,
}

#[derive(Debug, Clone, Copy)]
struct Target {
    lat: f64,
    lon: f64,
    alt: f64,
}

struct Vehicle {
    autopilot: Autopilot,
    lat: f64,
    lon: f64,
    alt: f64,
    home_lat: f64,
    home_lon: f64,
    home_alt: f64,
    armed: bool,
    battery: f64,
    phase: Phase,
    target: Option<Target>,
    takeoff_alt: f64,
    custom_mode: u32,
    yaw_deg: f64,
    /// 飞控唯一标识（`AUTOPILOT_VERSION.uid`）。
    uid: u64,
    /// 收到 `AUTOPILOT_VERSION_REQUEST`：需要回一条 `AUTOPILOT_VERSION`。
    version_requested: bool,
}

impl Vehicle {
    fn new(autopilot: Autopilot, lat: f64, lon: f64, uid: u64) -> Vehicle {
        Vehicle {
            autopilot,
            lat,
            lon,
            alt: HOME_ALT_M,
            home_lat: lat,
            home_lon: lon,
            home_alt: HOME_ALT_M,
            armed: false,
            battery: 96.0,
            phase: Phase::Idle,
            target: None,
            takeoff_alt: HOME_ALT_M + 10.0,
            custom_mode: default_mode(autopilot),
            yaw_deg: 0.0,
            uid,
            version_requested: false,
        }
    }

    fn set_px4_mode(&mut self, main: u32, sub: u32) {
        self.custom_mode = ((sub & 0xFF) << 24) | ((main & 0xFF) << 16);
    }

    fn set_guided(&mut self) {
        match self.autopilot {
            Autopilot::Px4 => self.set_px4_mode(4, 3), // AUTO.LOITER
            Autopilot::ArduPilot => self.custom_mode = 4,
        }
    }

    fn heartbeat(&self) -> MavMessage {
        let mut base = MavModeFlag::MAV_MODE_FLAG_CUSTOM_MODE_ENABLED;
        if self.armed {
            base |= MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED;
        }
        MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            custom_mode: self.custom_mode,
            mavtype: MavType::MAV_TYPE_QUADROTOR,
            autopilot: self.autopilot.mav(),
            base_mode: base,
            system_status: if self.armed {
                MavState::MAV_STATE_ACTIVE
            } else {
                MavState::MAV_STATE_STANDBY
            },
            mavlink_version: 3,
        })
    }

    /// 固件版本信息：携带唯一标识，供平台做唯一标识核验。
    ///
    /// `uid` 字段是 MAVLink 2 `AUTOPILOT_VERSION` 的 64 位唯一标识；
    /// 平台侧登记值需与之相等（登记为空时不做判定）。
    fn autopilot_version(&self) -> MavMessage {
        let capabilities = MavProtocolCapability::MAV_PROTOCOL_CAPABILITY_MISSION_FLOAT
            | MavProtocolCapability::MAV_PROTOCOL_CAPABILITY_PARAM_FLOAT
            | MavProtocolCapability::MAV_PROTOCOL_CAPABILITY_SET_POSITION_TARGET_GLOBAL_INT
            | MavProtocolCapability::MAV_PROTOCOL_CAPABILITY_MAVLINK2;
        MavMessage::AUTOPILOT_VERSION(::mavlink::dialects::ardupilotmega::AUTOPILOT_VERSION_DATA {
            capabilities,
            uid: self.uid,
            // 版本号按 MAVLink 编码：major<<24 | minor<<16 | patch<<8 | type。
            flight_sw_version: 1 << 24,
            middleware_sw_version: 0,
            os_sw_version: 0,
            board_version: 0,
            vendor_id: 0,
            product_id: 0,
            flight_custom_version: *b"SIMUATOR",
            middleware_custom_version: [0; 8],
            os_custom_version: [0; 8],
        })
    }

    fn telemetry(&self) -> Vec<MavMessage> {
        let rel_alt = (self.alt - self.home_alt).max(0.0);
        vec![
            MavMessage::ATTITUDE(ATTITUDE_DATA {
                time_boot_ms: 0,
                roll: 0.0,
                pitch: 0.0,
                yaw: (self.yaw_deg).to_radians() as f32,
                rollspeed: 0.0,
                pitchspeed: 0.0,
                yawspeed: 0.0,
            }),
            MavMessage::GLOBAL_POSITION_INT(GLOBAL_POSITION_INT_DATA {
                time_boot_ms: 0,
                lat: (self.lat * 1e7).round() as i32,
                lon: (self.lon * 1e7).round() as i32,
                alt: (self.alt * 1000.0).round() as i32,
                relative_alt: (rel_alt * 1000.0).round() as i32,
                vx: 0,
                vy: 0,
                vz: 0,
                hdg: ((self.yaw_deg.rem_euclid(360.0)) * 100.0).round() as u16,
            }),
            MavMessage::GPS_RAW_INT(GPS_RAW_INT_DATA {
                time_usec: 0,
                lat: (self.lat * 1e7).round() as i32,
                lon: (self.lon * 1e7).round() as i32,
                alt: (self.alt * 1000.0).round() as i32,
                eph: 80,
                epv: 120,
                vel: 0,
                cog: 0,
                fix_type: GpsFixType::GPS_FIX_TYPE_3D_FIX,
                satellites_visible: 16,
            }),
            MavMessage::SYS_STATUS(SYS_STATUS_DATA {
                onboard_control_sensors_present: MavSysStatusSensor::empty(),
                onboard_control_sensors_enabled: MavSysStatusSensor::empty(),
                onboard_control_sensors_health: MavSysStatusSensor::empty(),
                load: 120,
                voltage_battery: 16_600,
                current_battery: 800,
                battery_remaining: self.battery.clamp(0.0, 100.0) as i8,
                drop_rate_comm: 0,
                errors_comm: 0,
                errors_count1: 0,
                errors_count2: 0,
                errors_count3: 0,
                errors_count4: 0,
            }),
            MavMessage::HOME_POSITION(HOME_POSITION_DATA {
                latitude: (self.home_lat * 1e7).round() as i32,
                longitude: (self.home_lon * 1e7).round() as i32,
                altitude: (self.home_alt * 1000.0).round() as i32,
                x: 0.0,
                y: 0.0,
                z: 0.0,
                q: [1.0, 0.0, 0.0, 0.0],
                approach_x: 0.0,
                approach_y: 0.0,
                approach_z: 0.0,
            }),
            MavMessage::EXTENDED_SYS_STATE(EXTENDED_SYS_STATE_DATA {
                vtol_state:
                    ::mavlink::dialects::ardupilotmega::MavVtolState::MAV_VTOL_STATE_UNDEFINED,
                landed_state: if self.armed && rel_alt > 0.5 {
                    MavLandedState::MAV_LANDED_STATE_IN_AIR
                } else {
                    MavLandedState::MAV_LANDED_STATE_ON_GROUND
                },
            }),
        ]
    }

    fn step(&mut self, dt: f64) {
        if self.armed {
            self.battery = (self.battery - 0.02 * dt).max(0.0);
        }
        match self.phase {
            Phase::Idle | Phase::Hover => {}
            Phase::Takeoff => {
                self.alt += 2.0 * dt;
                if self.alt >= self.takeoff_alt {
                    self.alt = self.takeoff_alt;
                    self.phase = Phase::Hover;
                }
            }
            Phase::GoTo => {
                if let Some(t) = self.target {
                    let reached = self.move_towards(t, 8.0, dt);
                    if reached {
                        self.phase = Phase::Hover;
                    }
                } else {
                    self.phase = Phase::Hover;
                }
            }
            Phase::Rtl => {
                let home = Target {
                    lat: self.home_lat,
                    lon: self.home_lon,
                    alt: self.home_alt + 15.0,
                };
                let reached = self.move_towards(home, 10.0, dt);
                if reached {
                    self.phase = Phase::Land;
                }
            }
            Phase::Land => {
                self.alt -= 1.5 * dt;
                if self.alt <= self.home_alt {
                    self.alt = self.home_alt;
                    self.armed = false;
                    self.phase = Phase::Idle;
                    self.set_guided();
                }
            }
        }
    }

    fn move_towards(&mut self, target: Target, speed: f64, dt: f64) -> bool {
        let d_lat = target.lat - self.lat;
        let d_lon = target.lon - self.lon;
        let meters_lat = d_lat * 111_320.0;
        let meters_lon = d_lon * 111_320.0 * self.lat.to_radians().cos().abs().max(0.1);
        let horizontal = (meters_lat * meters_lat + meters_lon * meters_lon).sqrt();
        let vertical = target.alt - self.alt;
        let total = (horizontal * horizontal + vertical * vertical).sqrt();

        if horizontal < 1.0 && vertical.abs() < 0.5 {
            self.lat = target.lat;
            self.lon = target.lon;
            self.alt = target.alt;
            return true;
        }
        if total > 0.001 {
            let step = (speed * dt).min(total);
            self.lat += d_lat * step / total;
            self.lon += d_lon * step / total;
            self.alt += vertical * step / total;
            self.yaw_deg = bearing_deg(self.lat, self.lon, target.lat, target.lon);
        }
        false
    }

    fn on_message(&mut self, msg: MavMessage) -> Option<(MavCmd, MavResult)> {
        match msg {
            MavMessage::COMMAND_LONG(cmd) => {
                let result = self.apply_command(cmd.command, &cmd);
                Some((cmd.command, result))
            }
            MavMessage::SET_POSITION_TARGET_GLOBAL_INT(target) => {
                if target.coordinate_frame == MavFrame::MAV_FRAME_GLOBAL_RELATIVE_ALT_INT
                    || target.coordinate_frame == MavFrame::MAV_FRAME_GLOBAL_INT
                {
                    self.target = Some(Target {
                        lat: target.lat_int as f64 / 1e7,
                        lon: target.lon_int as f64 / 1e7,
                        alt: self.home_alt + target.alt as f64,
                    });
                    self.phase = Phase::GoTo;
                    self.set_guided();
                }
                None
            }
            MavMessage::SET_MODE(mode) => {
                self.custom_mode = mode.custom_mode;
                None
            }
            MavMessage::AUTOPILOT_VERSION_REQUEST(_) => {
                // adapter 建链后会主动索取固件版本：回一条带 uid 的 AUTOPILOT_VERSION。
                self.version_requested = true;
                None
            }
            _ => None,
        }
    }

    fn apply_command(
        &mut self,
        command: MavCmd,
        cmd: &::mavlink::dialects::ardupilotmega::COMMAND_LONG_DATA,
    ) -> MavResult {
        match command {
            MavCmd::MAV_CMD_COMPONENT_ARM_DISARM => {
                self.armed = cmd.param1 > 0.5;
                if self.armed {
                    self.phase = Phase::Hover;
                }
                MavResult::MAV_RESULT_ACCEPTED
            }
            MavCmd::MAV_CMD_NAV_TAKEOFF => {
                let rel = if cmd.param7 > 0.5 {
                    cmd.param7 as f64
                } else {
                    10.0
                };
                self.takeoff_alt = self.home_alt + rel;
                self.armed = true;
                self.phase = Phase::Takeoff;
                match self.autopilot {
                    Autopilot::Px4 => self.set_px4_mode(4, 2), // AUTO.TAKEOFF
                    Autopilot::ArduPilot => self.custom_mode = 4,
                }
                MavResult::MAV_RESULT_ACCEPTED
            }
            MavCmd::MAV_CMD_NAV_LAND => {
                self.armed = true;
                self.phase = Phase::Land;
                match self.autopilot {
                    Autopilot::Px4 => self.set_px4_mode(4, 6),
                    Autopilot::ArduPilot => self.custom_mode = 9,
                }
                MavResult::MAV_RESULT_ACCEPTED
            }
            MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH => {
                self.armed = true;
                self.phase = Phase::Rtl;
                match self.autopilot {
                    Autopilot::Px4 => self.set_px4_mode(4, 5),
                    Autopilot::ArduPilot => self.custom_mode = 6,
                }
                MavResult::MAV_RESULT_ACCEPTED
            }
            MavCmd::MAV_CMD_DO_REPOSITION => {
                // DO_REPOSITION 的 param7 按 AMSL 解释，与 rustcam-drone-adapter
                // 的换算（相对高度 + home_alt）保持一致；param7 无效时保持当前高度。
                let alt_amsl = if cmd.param7.is_finite() && cmd.param7 > 0.0 {
                    cmd.param7 as f64
                } else {
                    self.alt
                };
                self.target = Some(Target {
                    lat: cmd.param5 as f64,
                    lon: cmd.param6 as f64,
                    alt: alt_amsl,
                });
                self.armed = true;
                self.phase = Phase::GoTo;
                match self.autopilot {
                    Autopilot::Px4 => self.set_px4_mode(4, 3),
                    Autopilot::ArduPilot => self.custom_mode = 4,
                }
                MavResult::MAV_RESULT_ACCEPTED
            }
            MavCmd::MAV_CMD_CONDITION_YAW => {
                // 机体绝对航向：param1 角度（度），param2 角速度（度/秒）。
                self.yaw_deg = (cmd.param1 as f64).rem_euclid(360.0);
                MavResult::MAV_RESULT_ACCEPTED
            }
            MavCmd::MAV_CMD_DO_SET_MODE => {
                let custom = cmd.param2 as u32;
                if custom > 0 {
                    self.custom_mode = custom;
                }
                MavResult::MAV_RESULT_ACCEPTED
            }
            MavCmd::MAV_CMD_DO_MOUNT_CONTROL
            | MavCmd::MAV_CMD_IMAGE_START_CAPTURE
            | MavCmd::MAV_CMD_VIDEO_START_CAPTURE
            | MavCmd::MAV_CMD_VIDEO_STOP_CAPTURE => MavResult::MAV_RESULT_ACCEPTED,
            _ => MavResult::MAV_RESULT_UNSUPPORTED,
        }
    }
}

fn default_mode(autopilot: Autopilot) -> u32 {
    match autopilot {
        Autopilot::Px4 => (3 << 24) | (4 << 16), // AUTO.LOITER
        Autopilot::ArduPilot => 4,               // GUIDED
    }
}

fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let dlambda = (lon2 - lon1).to_radians();
    let y = dlambda.sin() * phi2.cos();
    let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * dlambda.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

fn header(system_id: u32) -> MavHeader {
    MavHeader {
        system_id,
        component_id: 1,
        sequence: 0,
    }
}

async fn send_many(conn: &AsyncConnection<MavMessage>, header: &MavHeader, msgs: &[MavMessage]) -> anyhow::Result<()> {
    for m in msgs {
        conn.send(header, m)
            .await
            .map_err(|e| anyhow::anyhow!("send failed: {e}"))?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut addr = "udpout:127.0.0.1:14540".to_string();
    let mut autopilot = Autopilot::Px4;
    let mut lat = 31.2304_f64;
    let mut lon = 121.4737_f64;
    let mut uid = DEFAULT_UID;
    let mut sysid = 1_u32;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--addr" if i + 1 < args.len() => {
                addr = args[i + 1].clone();
                i += 2;
            }
            "--autopilot" if i + 1 < args.len() => {
                autopilot = match args[i + 1].to_ascii_lowercase().as_str() {
                    "ardupilot" | "apm" => Autopilot::ArduPilot,
                    _ => Autopilot::Px4,
                };
                i += 2;
            }
            "--lat" if i + 1 < args.len() => {
                lat = args[i + 1].parse().context("invalid --lat")?;
                i += 2;
            }
            "--lon" if i + 1 < args.len() => {
                lon = args[i + 1].parse().context("invalid --lon")?;
                i += 2;
            }
            "--uid" if i + 1 < args.len() => {
                uid = parse_uid(&args[i + 1]).context("invalid --uid")?;
                i += 2;
            }
            "--sysid" if i + 1 < args.len() => {
                sysid = args[i + 1].parse().context("invalid --sysid")?;
                i += 2;
            }
            "--help" | "-h" => {
                println!(
                    "rustcam-drone-sim [--addr udpout:127.0.0.1:14540] [--autopilot px4|ardupilot] \
                     [--lat 31.23] [--lon 121.47] [--uid 0011223344556677] [--sysid 1]"
                );
                println!(
                    "  --uid    飞控唯一标识（十六进制，≤16 位）。平台登记值需与之一致，\n\
                     \x20          否则唯一标识核验会拒绝下发指令。"
                );
                return Ok(());
            }
            other => {
                tracing::warn!(arg = %other, "unknown argument");
                i += 1;
            }
        }
    }

    // 环境变量优先级高于默认值，但不覆盖命令行显式给出的值。
    if !args.iter().any(|a| a == "--uid") {
        if let Ok(v) = std::env::var("DRONE_SIM_UID") {
            uid = parse_uid(&v).context("invalid DRONE_SIM_UID")?;
        }
    }

    tracing::info!(
        addr = %addr,
        autopilot = ?autopilot,
        lat,
        lon,
        uid = format_args!("{uid:016x}"),
        sysid,
        "drone simulator starting"
    );

    loop {
        match run_once(&addr, autopilot, lat, lon, uid, sysid).await {
            Ok(()) => {}
            Err(e) => tracing::warn!(error = %e, "simulator link ended"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn run_once(
    addr: &str,
    autopilot: Autopilot,
    lat: f64,
    lon: f64,
    uid: u64,
    sysid: u32,
) -> anyhow::Result<()> {
    let conn = connect_async::<MavMessage>(addr)
        .await
        .map_err(|e| anyhow::anyhow!("connect {addr} failed: {e}"))?;
    let mut vehicle = Vehicle::new(autopilot, lat, lon, uid);
    let header = header(sysid);

    let mut tick = tokio::time::interval(Duration::from_secs_f64(TICK_SECS));
    let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
    let mut telemetry = tokio::time::interval(Duration::from_millis(200));
    let mut version = tokio::time::interval(Duration::from_secs(VERSION_REPORT_SECS));

    // 建链后先报一次版本与唯一标识，adapter 可能在首个心跳后才发起版本请求。
    conn.send(&header, &vehicle.autopilot_version())
        .await
        .map_err(|e| anyhow::anyhow!("autopilot_version failed: {e}"))?;

    loop {
        tokio::select! {
            recv = conn.recv() => {
                match recv {
                    Ok((_header, msg)) => {
                        if let Some((command, result)) = vehicle.on_message(msg) {
                            let ack = MavMessage::COMMAND_ACK(COMMAND_ACK_DATA { command, result });
                            conn.send(&header, &ack).await.map_err(|e| anyhow::anyhow!("ack failed: {e}"))?;
                            tracing::info!(command = ?command, result = ?result, "command acknowledged");
                        }
                        if vehicle.version_requested {
                            vehicle.version_requested = false;
                            conn.send(&header, &vehicle.autopilot_version())
                                .await
                                .map_err(|e| anyhow::anyhow!("autopilot_version failed: {e}"))?;
                            tracing::info!(uid = format_args!("{uid:016x}"), "autopilot version reported");
                        }
                    }
                    Err(e) => return Err(anyhow::anyhow!("recv failed: {e}")),
                }
            }
            _ = heartbeat.tick() => {
                conn.send(&header, &vehicle.heartbeat())
                    .await
                    .map_err(|e| anyhow::anyhow!("heartbeat failed: {e}"))?;
            }
            _ = telemetry.tick() => {
                send_many(&conn, &header, &vehicle.telemetry()).await?;
            }
            _ = version.tick() => {
                conn.send(&header, &vehicle.autopilot_version())
                    .await
                    .map_err(|e| anyhow::anyhow!("autopilot_version failed: {e}"))?;
            }
            _ = tick.tick() => vehicle.step(TICK_SECS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takeoff_climbs_and_hovers() {
        let mut v = Vehicle::new(Autopilot::Px4, 31.0, 121.0, DEFAULT_UID);
        v.takeoff_alt = v.home_alt + 5.0;
        v.phase = Phase::Takeoff;
        for _ in 0..100 {
            v.step(0.1);
        }
        assert!((v.alt - v.takeoff_alt).abs() < 0.01);
        assert_eq!(v.phase, Phase::Hover);
    }

    #[test]
    fn goto_moves_towards_target() {
        let mut v = Vehicle::new(Autopilot::ArduPilot, 31.0, 121.0, DEFAULT_UID);
        v.target = Some(Target {
            lat: 31.001,
            lon: 121.001,
            alt: v.home_alt + 10.0,
        });
        v.phase = Phase::GoTo;
        for _ in 0..2000 {
            v.step(0.1);
            if v.phase == Phase::Hover {
                break;
            }
        }
        assert_eq!(v.phase, Phase::Hover);
        assert!((v.lat - 31.001).abs() < 1e-6);
        assert!((v.lon - 121.001).abs() < 1e-6);
    }

    #[test]
    fn land_disarms_at_home_alt() {
        let mut v = Vehicle::new(Autopilot::Px4, 31.0, 121.0, DEFAULT_UID);
        v.armed = true;
        v.alt = v.home_alt + 3.0;
        v.phase = Phase::Land;
        for _ in 0..100 {
            v.step(0.1);
        }
        assert!(!v.armed);
        assert_eq!(v.alt, v.home_alt);
        assert_eq!(v.phase, Phase::Idle);
    }

    #[test]
    fn parse_uid_accepts_plain_hex() {
        assert_eq!(parse_uid("0011223344556677").unwrap(), 0x0011_2233_4455_6677);
        assert_eq!(parse_uid("  0011223344556677  ").unwrap(), 0x0011_2233_4455_6677);
    }

    #[test]
    fn parse_uid_accepts_0x_prefix() {
        assert_eq!(parse_uid("0x1122334455667788").unwrap(), 0x1122_3344_5566_7788);
        assert_eq!(parse_uid("0X1122334455667788").unwrap(), 0x1122_3344_5566_7788);
    }

    #[test]
    fn parse_uid_rejects_invalid_text() {
        assert!(parse_uid("").is_err());
        assert!(parse_uid("zz").is_err());
        assert!(parse_uid("0x").is_err());
        assert!(parse_uid("0011 2233").is_err());
    }

    #[test]
    fn parse_uid_rejects_overflow_beyond_u64() {
        // 17 位十六进制超出 u64 范围。
        assert!(parse_uid("112233445566778899").is_err());
    }

    #[test]
    fn parse_uid_rejects_empty_after_trim() {
        assert!(parse_uid("   ").is_err());
    }

    #[test]
    fn autopilot_version_message_carries_uid() {
        let v = Vehicle::new(Autopilot::Px4, 31.0, 121.0, 0x0011_2233_4455_6677);
        match v.autopilot_version() {
            MavMessage::AUTOPILOT_VERSION(ver) => {
                assert_eq!(ver.uid, 0x0011_2233_4455_6677);
            }
            other => panic!("expected AUTOPILOT_VERSION, got {other:?}"),
        }
    }

    #[test]
    fn autopilot_version_request_asks_for_version_reply() {
        let mut v = Vehicle::new(Autopilot::Px4, 31.0, 121.0, 0x0011_2233_4455_6677);
        assert!(!v.version_requested);
        v.on_message(MavMessage::AUTOPILOT_VERSION_REQUEST(
            ::mavlink::dialects::ardupilotmega::AUTOPILOT_VERSION_REQUEST_DATA {
                target_system: 1,
                target_component: 1,
            },
        ));
        assert!(v.version_requested);
    }

    #[test]
    fn commands_update_state_and_ack() {
        let mut v = Vehicle::new(Autopilot::Px4, 31.0, 121.0, DEFAULT_UID);
        let (cmd, result) = v
            .on_message(MavMessage::COMMAND_LONG(
                ::mavlink::dialects::ardupilotmega::COMMAND_LONG_DATA {
                    param1: 0.0,
                    param2: 0.0,
                    param3: 0.0,
                    param4: 0.0,
                    param5: 0.0,
                    param6: 0.0,
                    param7: 12.0,
                    command: MavCmd::MAV_CMD_NAV_TAKEOFF,
                    target_system: 1,
                    target_component: 1,
                    confirmation: 0,
                },
            ))
            .unwrap();
        assert_eq!(cmd, MavCmd::MAV_CMD_NAV_TAKEOFF);
        assert_eq!(result, MavResult::MAV_RESULT_ACCEPTED);
        assert!(v.armed);
        assert_eq!(v.phase, Phase::Takeoff);
        assert!((v.takeoff_alt - (v.home_alt + 12.0)).abs() < 1e-9);
    }
}
