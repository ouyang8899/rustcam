//! 把 MAVLink 消息聚合成一条规范化的机体状态。
//!
//! 聚合器与 IO 解耦：输入是 `MavMessage`，输出是 `Telemetry` 与少量状态变更事件，
//! 因此可以用构造出来的消息做完整单测，不需要真实飞控。

use ::mavlink::dialects::ardupilotmega::{
    MavAutopilot, MavLandedState, MavMessage, MavModeFlag, MavResult, MavType,
};

use rustcam_drone_core::telemetry::Telemetry;

use super::autopilot::{ardupilot_mode_name, px4_mode_name, AutopilotHint, AutopilotKind};

/// 唯一标识规范化：`uid == 0` → 空串（飞控未提供）；否则 16 位小写十六进制。
///
/// 平台侧登记的唯一标识是小写十六进制字符串（drone-service 校验），
/// 两边同形才能逐字符比较（drone.md §12.4 L3）。
///
/// 注：MAVLink 2 的 `AUTOPILOT_VERSION.uid2`（18 字节）不在 ardupilotmega v1.0
/// 方言内，真实 PX4 / ArduPilot 把 UUID 放在 uid2，因此实机读到的仍是空的。
pub fn normalize_unique_id(uid: u64) -> String {
    if uid == 0 {
        String::new()
    } else {
        format!("{uid:016x}")
    }
}

/// 聚合器产生的、需要驱动副作用（回执 / 事件）的状态变更。
#[derive(Debug, Clone, PartialEq)]
pub enum StateChange {
    Heartbeat {
        autopilot: AutopilotKind,
        armed: bool,
        mode: String,
    },
    Ack {
        command: u16,
        result: MavResult,
    },
    StatusText {
        severity: u8,
        text: String,
    },
    AutopilotVersion {
        version: String,
        /// 飞控唯一标识（小写十六进制）；空 = 飞控未提供。
        unique_id: String,
    },
    Landed {
        on_ground: bool,
    },
}

/// 单机体状态聚合器。所有时间戳使用 `now_ms` 注入，便于测试。
#[derive(Debug, Clone)]
pub struct VehicleState {
    hint: AutopilotHint,
    pub autopilot: AutopilotKind,
    pub vehicle_type: MavType,
    pub armed: bool,
    pub mode: String,
    pub firmware: String,
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
    pub has_quaternion: bool,
    pub has_attitude: bool,
    pub battery_pct: f64,
    pub voltage: f64,
    pub satellites: u32,
    pub gps_fix: u32,
    pub hdop: f64,
    pub home_lat: f64,
    pub home_lon: f64,
    pub home_alt: f64,
    pub radio_rssi: Option<u8>,
    pub landed_state: Option<MavLandedState>,
    pub last_heartbeat_ms: i64,
    pub last_seen_ms: i64,
    /// 飞控上报的唯一标识（`AUTOPILOT_VERSION.uid`）；空 = 飞控未提供。
    ///
    /// 由 AUTOPILOT_VERSION 消息写入，随遥测上报给平台做唯一标识核验
    /// （drone.md §12.4 L3）。
    pub unique_id: String,
}

impl VehicleState {
    pub fn new(hint: AutopilotHint) -> VehicleState {
        VehicleState {
            hint,
            autopilot: AutopilotKind::Unknown,
            vehicle_type: MavType::MAV_TYPE_GENERIC,
            armed: false,
            mode: String::new(),
            firmware: String::new(),
            lat: 0.0,
            lon: 0.0,
            alt_amsl: 0.0,
            rel_alt: 0.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            qx: 0.0,
            qy: 0.0,
            qz: 0.0,
            qw: 1.0,
            has_quaternion: false,
            has_attitude: false,
            battery_pct: 0.0,
            voltage: 0.0,
            satellites: 0,
            gps_fix: 0,
            hdop: 0.0,
            home_lat: 0.0,
            home_lon: 0.0,
            home_alt: 0.0,
            radio_rssi: None,
            landed_state: None,
            last_heartbeat_ms: 0,
            last_seen_ms: 0,
            unique_id: String::new(),
        }
    }

    pub fn is_online(&self, now_ms: i64, timeout_ms: u64) -> bool {
        self.last_heartbeat_ms > 0 && now_ms - self.last_heartbeat_ms <= timeout_ms as i64
    }

    /// 应用一条 MAVLink 消息；返回需要上报的状态变更。
    ///
    /// 只信任飞控 HEARTBEAT（`autopilot != INVALID` 且 `type != GCS`），
    /// 避免把地面站或其它机载组件误认成被控对象。
    pub fn apply(&mut self, msg: MavMessage, now_ms: i64) -> Vec<StateChange> {
        let mut changes = Vec::new();
        self.last_seen_ms = now_ms;

        match msg {
            MavMessage::HEARTBEAT(hb) => {
                let is_fc = hb.autopilot != MavAutopilot::MAV_AUTOPILOT_INVALID
                    && !matches!(
                        hb.mavtype,
                        MavType::MAV_TYPE_GCS
                            | MavType::MAV_TYPE_ONBOARD_CONTROLLER
                            | MavType::MAV_TYPE_CAMERA
                            | MavType::MAV_TYPE_GIMBAL
                            | MavType::MAV_TYPE_ADSB
                            | MavType::MAV_TYPE_OSD
                    );
                if !is_fc {
                    return changes;
                }

                let autopilot = self.hint.resolve(hb.autopilot);
                let armed = hb
                    .base_mode
                    .contains(MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED);
                let mode = match autopilot {
                    AutopilotKind::Px4 => px4_mode_name(hb.custom_mode),
                    AutopilotKind::ArduPilot => ardupilot_mode_name(hb.mavtype, hb.custom_mode),
                    _ => format!("MODE_{}", hb.custom_mode),
                };

                self.autopilot = autopilot;
                self.vehicle_type = hb.mavtype;
                self.armed = armed;
                self.mode = mode.clone();
                self.last_heartbeat_ms = now_ms;

                // 每次心跳都上报，读者侧据此恢复 online；事件只在 off→on 时发。
                changes.push(StateChange::Heartbeat {
                    autopilot,
                    armed,
                    mode,
                });
            }
            MavMessage::ATTITUDE(att) => {
                self.roll = att.roll as f64;
                self.pitch = att.pitch as f64;
                self.yaw = normalize_yaw(att.yaw as f64);
                self.has_attitude = true;
            }
            MavMessage::ATTITUDE_QUATERNION(q) => {
                // MAVLink 顺序为 w, x, y, z
                self.qw = q.q1 as f64;
                self.qx = q.q2 as f64;
                self.qy = q.q3 as f64;
                self.qz = q.q4 as f64;
                self.has_quaternion = true;
            }
            MavMessage::GLOBAL_POSITION_INT(pos) => {
                self.lat = pos.lat as f64 / 1e7;
                self.lon = pos.lon as f64 / 1e7;
                self.alt_amsl = pos.alt as f64 / 1000.0;
                self.rel_alt = pos.relative_alt as f64 / 1000.0;
                self.vx = pos.vx as f64 / 100.0;
                self.vy = pos.vy as f64 / 100.0;
                self.vz = pos.vz as f64 / 100.0;
                if !self.has_attitude && pos.hdg != u16::MAX {
                    self.yaw = normalize_yaw((pos.hdg as f64 / 100.0).to_radians());
                }
            }
            MavMessage::GPS_RAW_INT(gps) => {
                self.gps_fix = gps.fix_type as u32;
                self.satellites = gps.satellites_visible as u32;
                if gps.eph != u16::MAX {
                    self.hdop = gps.eph as f64 / 100.0;
                }
                if self.last_heartbeat_ms == 0 || self.lat == 0.0 {
                    self.lat = gps.lat as f64 / 1e7;
                    self.lon = gps.lon as f64 / 1e7;
                }
                if self.alt_amsl == 0.0 {
                    self.alt_amsl = gps.alt as f64 / 1000.0;
                }
            }
            MavMessage::SYS_STATUS(sys) => {
                if sys.battery_remaining >= 0 {
                    self.battery_pct = sys.battery_remaining as f64;
                }
                if sys.voltage_battery != u16::MAX {
                    self.voltage = sys.voltage_battery as f64 / 1000.0;
                }
            }
            MavMessage::BATTERY_STATUS(bat) => {
                if bat.battery_remaining >= 0 {
                    self.battery_pct = bat.battery_remaining as f64;
                }
                if let Some(mv) = bat.voltages.iter().copied().find(|v| *v != u16::MAX) {
                    self.voltage = mv as f64 / 1000.0;
                }
            }
            MavMessage::VFR_HUD(hud) => {
                if self.alt_amsl == 0.0 {
                    self.alt_amsl = hud.alt as f64;
                }
                if !self.has_attitude && hud.heading >= 0 {
                    self.yaw = normalize_yaw((hud.heading as f64).to_radians());
                }
            }
            MavMessage::RADIO_STATUS(radio) => {
                if radio.rssi != u8::MAX {
                    self.radio_rssi = Some(radio.rssi);
                }
            }
            MavMessage::HOME_POSITION(home) => {
                self.home_lat = home.latitude as f64 / 1e7;
                self.home_lon = home.longitude as f64 / 1e7;
                self.home_alt = home.altitude as f64 / 1000.0;
            }
            MavMessage::GPS_GLOBAL_ORIGIN(origin) => {
                self.home_lat = origin.latitude as f64 / 1e7;
                self.home_lon = origin.longitude as f64 / 1e7;
                self.home_alt = origin.altitude as f64 / 1000.0;
            }
            MavMessage::COMMAND_ACK(ack) => {
                changes.push(StateChange::Ack {
                    command: ack.command as u32 as u16,
                    result: ack.result,
                });
            }
            MavMessage::STATUSTEXT(text) => {
                let raw = &text.text[..];
                let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
                changes.push(StateChange::StatusText {
                    severity: text.severity as u8,
                    text: String::from_utf8_lossy(&raw[..end]).trim().to_string(),
                });
            }
            MavMessage::AUTOPILOT_VERSION(ver) => {
                let v = ver.flight_sw_version;
                let version = format!(
                    "{}.{}.{}",
                    (v >> 24) & 0xFF,
                    (v >> 16) & 0xFF,
                    (v >> 8) & 0xFF
                );
                self.firmware = version.clone();
                // 唯一标识：`uid` 为 0 表示飞控未提供；否则规范成小写十六进制，
                // 与平台侧登记值同形（drone.md §12.4 L3）。
                let unique_id = normalize_unique_id(ver.uid);
                self.unique_id = unique_id.clone();
                changes.push(StateChange::AutopilotVersion { version, unique_id });
            }
            MavMessage::EXTENDED_SYS_STATE(state)
                if self.landed_state != Some(state.landed_state) =>
            {
                self.landed_state = Some(state.landed_state);
                changes.push(StateChange::Landed {
                    on_ground: state.landed_state == MavLandedState::MAV_LANDED_STATE_ON_GROUND,
                });
            }
            _ => {}
        }

        changes
    }

    /// 链路质量：优先使用 RADIO_STATUS 的 RSSI；否则按心跳新鲜度折算。
    pub fn link_quality(&self, now_ms: i64, timeout_ms: u64) -> u32 {
        if let Some(rssi) = self.radio_rssi {
            return (rssi as u32 * 100 / 254).min(100);
        }
        if self.last_heartbeat_ms == 0 {
            return 0;
        }
        let age = now_ms - self.last_heartbeat_ms;
        if age <= 1_500 {
            return 100;
        }
        if age >= timeout_ms as i64 {
            return 0;
        }
        let remain = (timeout_ms as i64 - age) as f64 / ((timeout_ms as i64 - 1_500).max(1) as f64);
        (remain.clamp(0.0, 1.0) * 100.0).round() as u32
    }

    /// 生成一条可上报的规范遥测。
    pub fn to_telemetry(
        &self,
        device_id: &str,
        seq: u32,
        now_ms: i64,
        timeout_ms: u64,
    ) -> Telemetry {
        Telemetry {
            device_id: device_id.to_string(),
            ts_ms: now_ms,
            lat: self.lat,
            lon: self.lon,
            alt_amsl: self.alt_amsl,
            rel_alt: self.rel_alt,
            roll: self.roll,
            pitch: self.pitch,
            yaw: self.yaw,
            battery_pct: self.battery_pct.clamp(0.0, 100.0),
            voltage: self.voltage,
            link_quality: self.link_quality(now_ms, timeout_ms),
            seq,
            vx: self.vx,
            vy: self.vy,
            vz: self.vz,
            qx: self.qx,
            qy: self.qy,
            qz: self.qz,
            qw: self.qw,
            autopilot: self.autopilot.as_str().to_string(),
            armed: self.armed,
            mode: self.mode.clone(),
            satellites: self.satellites,
            gps_fix: self.gps_fix,
            hdop: self.hdop,
            home_lat: self.home_lat,
            home_lon: self.home_lon,
            home_alt: self.home_alt,
            unique_id: self.unique_id.clone(),
        }
    }
}

fn normalize_yaw(yaw: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let mut y = yaw % two_pi;
    if y < 0.0 {
        y += two_pi;
    }
    y
}

#[cfg(test)]
mod tests {
    #![allow(deprecated)]

    use super::*;
    use ::mavlink::dialects::ardupilotmega::*;
    use ::mavlink::types::CharArray;

    fn heartbeat(
        autopilot: MavAutopilot,
        mavtype: MavType,
        custom_mode: u32,
        armed: bool,
    ) -> MavMessage {
        let mut base_mode = MavModeFlag::empty();
        if armed {
            base_mode |= MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED;
        }
        MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            custom_mode,
            mavtype,
            autopilot,
            base_mode,
            system_status: MavState::MAV_STATE_ACTIVE,
            mavlink_version: 3,
        })
    }

    /// 带唯一标识的固件版本消息（`uid = 0` 表示飞控未提供）。
    fn autopilot_version(uid: u64) -> MavMessage {
        MavMessage::AUTOPILOT_VERSION(AUTOPILOT_VERSION_DATA {
            capabilities: MavProtocolCapability::empty(),
            uid,
            flight_sw_version: 1 << 24,
            middleware_sw_version: 0,
            os_sw_version: 0,
            board_version: 0,
            vendor_id: 0,
            product_id: 0,
            flight_custom_version: [0; 8],
            middleware_custom_version: [0; 8],
            os_custom_version: [0; 8],
        })
    }

    fn px4_loiter_heartbeat() -> MavMessage {        heartbeat(
            MavAutopilot::MAV_AUTOPILOT_PX4,
            MavType::MAV_TYPE_QUADROTOR,
            super::super::autopilot::px4_custom_mode(
                super::super::autopilot::PX4_MAIN_MODE_AUTO,
                super::super::autopilot::PX4_SUB_MODE_AUTO_LOITER,
            ),
            true,
        )
    }

    #[test]
    fn autopilot_version_uid_is_captured_as_unique_id() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        let changes = st.apply(autopilot_version(0x0011_2233_4455_6677), 1_000);
        assert_eq!(st.unique_id, "0011223344556677");
        assert!(changes.iter().any(|c| matches!(
            c,
            StateChange::AutopilotVersion { unique_id, .. } if unique_id == "0011223344556677"
        )));
    }

    #[test]
    fn autopilot_version_uid_zero_means_no_unique_id() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(autopilot_version(0), 1_000);
        assert_eq!(st.unique_id, "");
    }

    #[test]
    fn autopilot_version_uid_is_normalized_to_lowercase_hex() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(autopilot_version(0xDEAD_BEEF_1234_5678), 1_000);
        assert_eq!(st.unique_id, "deadbeef12345678");
    }

    #[test]
    fn heartbeat_detects_px4_and_mode() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        let changes = st.apply(px4_loiter_heartbeat(), 1_000);
        assert_eq!(st.autopilot, AutopilotKind::Px4);
        assert_eq!(st.mode, "AUTO.LOITER");
        assert!(st.armed);
        assert!(matches!(
            changes.as_slice(),
            [StateChange::Heartbeat { .. }]
        ));
        assert!(st.is_online(2_000, 5_000));
    }

    #[test]
    fn heartbeat_detects_ardupilot_guided() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(
            heartbeat(
                MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA,
                MavType::MAV_TYPE_QUADROTOR,
                4,
                false,
            ),
            1_000,
        );
        assert_eq!(st.autopilot, AutopilotKind::ArduPilot);
        assert_eq!(st.mode, "GUIDED");
        assert!(!st.armed);
    }

    #[test]
    fn gcs_heartbeat_is_ignored() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        let changes = st.apply(
            heartbeat(
                MavAutopilot::MAV_AUTOPILOT_INVALID,
                MavType::MAV_TYPE_GCS,
                0,
                false,
            ),
            1_000,
        );
        assert!(changes.is_empty());
        assert_eq!(st.autopilot, AutopilotKind::Unknown);
        assert_eq!(st.last_heartbeat_ms, 0);
    }

    #[test]
    fn every_heartbeat_reports_state() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        assert_eq!(st.apply(px4_loiter_heartbeat(), 1_000).len(), 1);
        // 心跳必须逐帧上报，否则掉线再上线时 reader 无法恢复 online。
        assert_eq!(st.apply(px4_loiter_heartbeat(), 2_000).len(), 1);
        assert_eq!(st.apply(px4_loiter_heartbeat(), 3_000).len(), 1);
    }

    #[test]
    fn global_position_and_attitude_fill_telemetry() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(px4_loiter_heartbeat(), 1_000);
        st.apply(
            MavMessage::ATTITUDE(ATTITUDE_DATA {
                time_boot_ms: 1,
                roll: 0.1,
                pitch: -0.2,
                yaw: -0.5,
                rollspeed: 0.0,
                pitchspeed: 0.0,
                yawspeed: 0.0,
            }),
            1_000,
        );
        st.apply(
            MavMessage::GLOBAL_POSITION_INT(GLOBAL_POSITION_INT_DATA {
                time_boot_ms: 1,
                lat: 312_304_000,
                lon: 1_214_737_000,
                alt: 62_500,
                relative_alt: 12_500,
                vx: 100,
                vy: -50,
                vz: 25,
                hdg: 9_000,
            }),
            1_000,
        );
        let t = st.to_telemetry("drone-001", 7, 1_000, 5_000);
        assert!((t.lat - 31.2304).abs() < 1e-9);
        assert!((t.lon - 121.4737).abs() < 1e-9);
        assert!((t.alt_amsl - 62.5).abs() < 1e-9);
        assert!((t.rel_alt - 12.5).abs() < 1e-9);
        assert!((t.roll - 0.1).abs() < 1e-6);
        assert!((t.yaw - (-0.5f64 + std::f64::consts::TAU)).abs() < 1e-6);
        assert!((t.vx - 1.0).abs() < 1e-9);
        assert_eq!(t.autopilot, "px4");
        assert_eq!(t.mode, "AUTO.LOITER");
        assert!(t.armed);
        assert_eq!(t.seq, 7);
        assert!(t.is_valid());
    }

    #[test]
    fn quaternion_is_converted_to_wxyz() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(
            MavMessage::ATTITUDE_QUATERNION(ATTITUDE_QUATERNION_DATA {
                time_boot_ms: 1,
                q1: 0.9,
                q2: 0.1,
                q3: 0.2,
                q4: 0.3,
                rollspeed: 0.0,
                pitchspeed: 0.0,
                yawspeed: 0.0,
            }),
            1_000,
        );
        let t = st.to_telemetry("d", 1, 1_000, 5_000);
        assert!((t.qw - 0.9).abs() < 1e-6);
        assert!((t.qx - 0.1).abs() < 1e-6);
        assert!((t.qy - 0.2).abs() < 1e-6);
        assert!((t.qz - 0.3).abs() < 1e-6);
    }

    #[test]
    fn battery_comes_from_sys_status_or_battery_status() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(
            MavMessage::SYS_STATUS(SYS_STATUS_DATA {
                onboard_control_sensors_present: MavSysStatusSensor::empty(),
                onboard_control_sensors_enabled: MavSysStatusSensor::empty(),
                onboard_control_sensors_health: MavSysStatusSensor::empty(),
                load: 100,
                voltage_battery: 12_400,
                current_battery: 500,
                battery_remaining: 73,
                drop_rate_comm: 0,
                errors_comm: 0,
                errors_count1: 0,
                errors_count2: 0,
                errors_count3: 0,
                errors_count4: 0,
            }),
            1_000,
        );
        let t = st.to_telemetry("d", 1, 1_000, 5_000);
        assert!((t.battery_pct - 73.0).abs() < 1e-9);
        assert!((t.voltage - 12.4).abs() < 1e-9);

        st.apply(
            MavMessage::BATTERY_STATUS(BATTERY_STATUS_DATA {
                current_consumed: -1,
                energy_consumed: -1,
                temperature: 0,
                voltages: [
                    11_100,
                    u16::MAX,
                    u16::MAX,
                    u16::MAX,
                    u16::MAX,
                    u16::MAX,
                    u16::MAX,
                    u16::MAX,
                    u16::MAX,
                    u16::MAX,
                ],
                current_battery: -1,
                id: 0,
                battery_function: MavBatteryFunction::MAV_BATTERY_FUNCTION_ALL,
                mavtype: MavBatteryType::MAV_BATTERY_TYPE_LIPO,
                battery_remaining: 64,
            }),
            2_000,
        );
        let t = st.to_telemetry("d", 2, 2_000, 5_000);
        assert!((t.battery_pct - 64.0).abs() < 1e-9);
        assert!((t.voltage - 11.1).abs() < 1e-9);
    }

    #[test]
    fn gps_fix_and_home_are_reported() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(
            MavMessage::GPS_RAW_INT(GPS_RAW_INT_DATA {
                time_usec: 0,
                lat: 312_304_000,
                lon: 1_214_737_000,
                alt: 62_500,
                eph: 120,
                epv: 200,
                vel: 0,
                cog: 0,
                fix_type: GpsFixType::GPS_FIX_TYPE_3D_FIX,
                satellites_visible: 14,
            }),
            1_000,
        );
        st.apply(
            MavMessage::HOME_POSITION(HOME_POSITION_DATA {
                latitude: 312_300_000,
                longitude: 1_214_730_000,
                altitude: 50_000,
                x: 0.0,
                y: 0.0,
                z: 0.0,
                q: [1.0, 0.0, 0.0, 0.0],
                approach_x: 0.0,
                approach_y: 0.0,
                approach_z: 0.0,
            }),
            1_000,
        );
        let t = st.to_telemetry("d", 1, 1_000, 5_000);
        assert_eq!(t.satellites, 14);
        assert_eq!(t.gps_fix, 3);
        assert!((t.hdop - 1.2).abs() < 1e-9);
        assert!((t.home_lat - 31.23).abs() < 1e-9);
        assert!((t.home_alt - 50.0).abs() < 1e-9);
    }

    #[test]
    fn command_ack_and_statustext_produce_events() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        let changes = st.apply(
            MavMessage::COMMAND_ACK(COMMAND_ACK_DATA {
                command: MavCmd::MAV_CMD_NAV_LAND,
                result: MavResult::MAV_RESULT_ACCEPTED,
            }),
            1_000,
        );
        assert_eq!(
            changes,
            vec![StateChange::Ack {
                command: MavCmd::MAV_CMD_NAV_LAND as u32 as u16,
                result: MavResult::MAV_RESULT_ACCEPTED
            }]
        );

        let text = CharArray::from("landing");
        let changes = st.apply(
            MavMessage::STATUSTEXT(STATUSTEXT_DATA {
                severity: MavSeverity::MAV_SEVERITY_WARNING,
                text,
            }),
            1_000,
        );
        match &changes[0] {
            StateChange::StatusText { severity, text } => {
                assert_eq!(*severity, MavSeverity::MAV_SEVERITY_WARNING as u8);
                assert_eq!(text, "landing");
            }
            other => panic!("unexpected change: {other:?}"),
        }
    }

    #[test]
    fn link_quality_reflects_heartbeat_age_and_rssi() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(px4_loiter_heartbeat(), 1_000);
        assert_eq!(st.link_quality(1_000, 5_000), 100);
        // 4s 时心跳已过 3s：剩余时间 (5-3)/(5-1.5) ≈ 57%
        assert_eq!(st.link_quality(4_000, 5_000), 57);
        assert_eq!(st.link_quality(9_000, 5_000), 0);
        st.apply(
            MavMessage::RADIO_STATUS(RADIO_STATUS_DATA {
                rxerrors: 0,
                fixed: 0,
                rssi: 127,
                remrssi: 0,
                txbuf: 100,
                noise: 0,
                remnoise: 0,
            }),
            2_000,
        );
        assert_eq!(st.link_quality(2_000, 5_000), 50);
    }

    #[test]
    fn offline_after_timeout() {
        let mut st = VehicleState::new(AutopilotHint::Auto);
        st.apply(px4_loiter_heartbeat(), 1_000);
        assert!(st.is_online(5_000, 5_000));
        assert!(!st.is_online(6_001, 5_000));
    }
}
