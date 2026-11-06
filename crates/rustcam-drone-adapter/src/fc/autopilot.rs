//! 飞控差异层：识别 PX4 / ArduPilot，并把各自的模式编码翻译成统一名称。
//!
//! 两者的差异集中在这里，`telemetry` 与 `command` 只依赖统一的 `AutopilotKind`：
//! - 自动识别来自 `HEARTBEAT.autopilot`（PX4 = 12，ArduPilot = 3）；
//! - 模式名：PX4 用 custom_mode 的 main/sub 两段，ArduPilot 用单一 custom_mode 且随机型变化；
//! - 指令：PX4 的定点用 `DO_REPOSITION`、起飞前先进 AUTO_TAKEOFF，ArduPilot 先切 GUIDED。

use ::mavlink::dialects::ardupilotmega::{MavAutopilot, MavType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutopilotKind {
    Px4,
    ArduPilot,
    Generic,
    Unknown,
}

impl AutopilotKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutopilotKind::Px4 => "px4",
            AutopilotKind::ArduPilot => "ardupilot",
            AutopilotKind::Generic => "generic",
            AutopilotKind::Unknown => "unknown",
        }
    }

    pub fn from_mav(autopilot: MavAutopilot) -> AutopilotKind {
        match autopilot {
            MavAutopilot::MAV_AUTOPILOT_PX4 => AutopilotKind::Px4,
            MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA => AutopilotKind::ArduPilot,
            MavAutopilot::MAV_AUTOPILOT_GENERIC => AutopilotKind::Generic,
            MavAutopilot::MAV_AUTOPILOT_INVALID => AutopilotKind::Unknown,
            _ => AutopilotKind::Generic,
        }
    }
}

/// 配置里的飞控类型提示；`auto` 表示完全按心跳识别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutopilotHint {
    #[default]
    Auto,
    Px4,
    ArduPilot,
}

impl AutopilotHint {
    pub fn parse(raw: &str) -> anyhow::Result<AutopilotHint> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(AutopilotHint::Auto),
            "px4" => Ok(AutopilotHint::Px4),
            "ardupilot" | "ardupilotmega" | "apm" => Ok(AutopilotHint::ArduPilot),
            other => anyhow::bail!("unsupported autopilot hint `{other}`"),
        }
    }

    /// 心跳识别出的类型优先；识别不出时回落到配置提示。
    pub fn resolve(self, detected: MavAutopilot) -> AutopilotKind {
        match AutopilotKind::from_mav(detected) {
            AutopilotKind::Px4 => AutopilotKind::Px4,
            AutopilotKind::ArduPilot => AutopilotKind::ArduPilot,
            _ => match self {
                AutopilotHint::Px4 => AutopilotKind::Px4,
                AutopilotHint::ArduPilot => AutopilotKind::ArduPilot,
                AutopilotHint::Auto => AutopilotKind::from_mav(detected),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// PX4 custom_mode
// ---------------------------------------------------------------------------

pub const PX4_MAIN_MODE_MANUAL: u32 = 1;
pub const PX4_MAIN_MODE_ALTCTL: u32 = 2;
pub const PX4_MAIN_MODE_POSCTL: u32 = 3;
pub const PX4_MAIN_MODE_AUTO: u32 = 4;
pub const PX4_MAIN_MODE_ACRO: u32 = 5;
pub const PX4_MAIN_MODE_OFFBOARD: u32 = 6;
pub const PX4_MAIN_MODE_STABILIZED: u32 = 7;
pub const PX4_MAIN_MODE_RATTITUDE: u32 = 8;

pub const PX4_SUB_MODE_AUTO_READY: u32 = 1;
pub const PX4_SUB_MODE_AUTO_TAKEOFF: u32 = 2;
pub const PX4_SUB_MODE_AUTO_LOITER: u32 = 3;
pub const PX4_SUB_MODE_AUTO_MISSION: u32 = 4;
pub const PX4_SUB_MODE_AUTO_RTL: u32 = 5;
pub const PX4_SUB_MODE_AUTO_LAND: u32 = 6;
pub const PX4_SUB_MODE_AUTO_RTGS: u32 = 7;
pub const PX4_SUB_MODE_AUTO_FOLLOW_TARGET: u32 = 8;
pub const PX4_SUB_MODE_AUTO_PRECLAND: u32 = 9;

/// PX4 custom_mode = (sub_mode << 24) | (main_mode << 16)。
pub fn px4_custom_mode(main_mode: u32, sub_mode: u32) -> u32 {
    ((sub_mode & 0xFF) << 24) | ((main_mode & 0xFF) << 16)
}

pub fn px4_mode_name(custom_mode: u32) -> String {
    let main = (custom_mode >> 16) & 0xFF;
    let sub = (custom_mode >> 24) & 0xFF;
    let main_name = match main {
        PX4_MAIN_MODE_MANUAL => "MANUAL",
        PX4_MAIN_MODE_ALTCTL => "ALTCTL",
        PX4_MAIN_MODE_POSCTL => "POSCTL",
        PX4_MAIN_MODE_AUTO => "AUTO",
        PX4_MAIN_MODE_ACRO => "ACRO",
        PX4_MAIN_MODE_OFFBOARD => "OFFBOARD",
        PX4_MAIN_MODE_STABILIZED => "STABILIZED",
        PX4_MAIN_MODE_RATTITUDE => "RATTITUDE",
        other => return format!("MODE_{other}"),
    };
    if main != PX4_MAIN_MODE_AUTO {
        return main_name.to_string();
    }
    let sub_name = match sub {
        PX4_SUB_MODE_AUTO_READY => "READY",
        PX4_SUB_MODE_AUTO_TAKEOFF => "TAKEOFF",
        PX4_SUB_MODE_AUTO_LOITER => "LOITER",
        PX4_SUB_MODE_AUTO_MISSION => "MISSION",
        PX4_SUB_MODE_AUTO_RTL => "RTL",
        PX4_SUB_MODE_AUTO_LAND => "LAND",
        PX4_SUB_MODE_AUTO_RTGS => "RTGS",
        PX4_SUB_MODE_AUTO_FOLLOW_TARGET => "FOLLOW_TARGET",
        PX4_SUB_MODE_AUTO_PRECLAND => "PRECLAND",
        other => return format!("AUTO.{other}"),
    };
    format!("AUTO.{sub_name}")
}

// ---------------------------------------------------------------------------
// ArduPilot custom_mode（随机型变化）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleClass {
    Copter,
    Plane,
    Rover,
    Sub,
    Unknown,
}

pub fn ardupilot_vehicle_class(vehicle_type: MavType) -> VehicleClass {
    match vehicle_type {
        MavType::MAV_TYPE_FIXED_WING
        | MavType::MAV_TYPE_VTOL_TAILSITTER_DUOROTOR
        | MavType::MAV_TYPE_VTOL_TAILSITTER_QUADROTOR
        | MavType::MAV_TYPE_VTOL_TILTROTOR
        | MavType::MAV_TYPE_VTOL_TILTWING
        | MavType::MAV_TYPE_VTOL_FIXEDROTOR
        | MavType::MAV_TYPE_VTOL_TAILSITTER
        | MavType::MAV_TYPE_VTOL_GYRODYNE
        | MavType::MAV_TYPE_VTOL_RESERVED5 => VehicleClass::Plane,
        MavType::MAV_TYPE_GROUND_ROVER | MavType::MAV_TYPE_SURFACE_BOAT => VehicleClass::Rover,
        MavType::MAV_TYPE_SUBMARINE => VehicleClass::Sub,
        MavType::MAV_TYPE_QUADROTOR
        | MavType::MAV_TYPE_COAXIAL
        | MavType::MAV_TYPE_HELICOPTER
        | MavType::MAV_TYPE_HEXAROTOR
        | MavType::MAV_TYPE_OCTOROTOR
        | MavType::MAV_TYPE_TRICOPTER
        | MavType::MAV_TYPE_DODECAROTOR => VehicleClass::Copter,
        _ => VehicleClass::Unknown,
    }
}

/// ArduPilot 进入 GUIDED 所需的 custom_mode 值。
pub fn ardupilot_guided_mode(vehicle_type: MavType) -> u32 {
    // Copter / Sub 的 GUIDED = 4；Plane / Rover 的 GUIDED = 15。
    match ardupilot_vehicle_class(vehicle_type) {
        VehicleClass::Plane | VehicleClass::Rover => 15,
        _ => 4,
    }
}

pub fn ardupilot_mode_name(vehicle_type: MavType, custom_mode: u32) -> String {
    let name = match ardupilot_vehicle_class(vehicle_type) {
        VehicleClass::Copter => copter_mode_name(custom_mode),
        VehicleClass::Plane => plane_mode_name(custom_mode),
        VehicleClass::Rover => rover_mode_name(custom_mode),
        VehicleClass::Sub => sub_mode_name(custom_mode),
        VehicleClass::Unknown => None,
    };
    name.map(|s| s.to_string())
        .unwrap_or_else(|| format!("MODE_{custom_mode}"))
}

fn copter_mode_name(mode: u32) -> Option<&'static str> {
    Some(match mode {
        0 => "STABILIZE",
        1 => "ACRO",
        2 => "ALT_HOLD",
        3 => "AUTO",
        4 => "GUIDED",
        5 => "LOITER",
        6 => "RTL",
        7 => "CIRCLE",
        9 => "LAND",
        11 => "DRIFT",
        13 => "SPORT",
        14 => "FLIP",
        15 => "AUTOTUNE",
        16 => "POSHOLD",
        17 => "BRAKE",
        18 => "THROW",
        19 => "AVOID_ADSB",
        20 => "GUIDED_NOGPS",
        21 => "SMART_RTL",
        22 => "FLOWHOLD",
        23 => "FOLLOW",
        24 => "ZIGZAG",
        25 => "SYSTEMID",
        26 => "AUTOROTATE",
        27 => "AUTO_RTL",
        28 => "TURTLE",
        _ => return None,
    })
}

fn plane_mode_name(mode: u32) -> Option<&'static str> {
    Some(match mode {
        0 => "MANUAL",
        1 => "CIRCLE",
        2 => "STABILIZE",
        3 => "TRAINING",
        4 => "ACRO",
        5 => "FLY_BY_WIRE_A",
        6 => "FLY_BY_WIRE_B",
        7 => "CRUISE",
        8 => "AUTOTUNE",
        10 => "AUTO",
        11 => "RTL",
        12 => "LOITER",
        13 => "TAKEOFF",
        14 => "AVOID_ADSB",
        15 => "GUIDED",
        16 => "INITIALISING",
        17 => "QSTABILIZE",
        18 => "QHOVER",
        19 => "QLOITER",
        20 => "QLAND",
        21 => "QRTL",
        22 => "QAUTOTUNE",
        23 => "QACRO",
        24 => "THERMAL",
        25 => "LOITER_ALT_QLAND",
        _ => return None,
    })
}

fn rover_mode_name(mode: u32) -> Option<&'static str> {
    Some(match mode {
        0 => "MANUAL",
        1 => "ACRO",
        3 => "STEERING",
        4 => "HOLD",
        5 => "LOITER",
        6 => "FOLLOW",
        7 => "SIMPLE",
        8 => "DOCK",
        9 => "CIRCLE",
        10 => "AUTO",
        11 => "RTL",
        12 => "SMART_RTL",
        15 => "GUIDED",
        16 => "INITIALISING",
        _ => return None,
    })
}

fn sub_mode_name(mode: u32) -> Option<&'static str> {
    Some(match mode {
        0 => "STABILIZE",
        1 => "ACRO",
        2 => "ALT_HOLD",
        3 => "AUTO",
        4 => "GUIDED",
        5 => "CIRCLE",
        6 => "SURFACE",
        7 => "POSHOLD",
        8 => "MANUAL",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_autopilot_is_recognized() {
        assert_eq!(
            AutopilotKind::from_mav(MavAutopilot::MAV_AUTOPILOT_PX4),
            AutopilotKind::Px4
        );
        assert_eq!(
            AutopilotKind::from_mav(MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA),
            AutopilotKind::ArduPilot
        );
        assert_eq!(
            AutopilotKind::from_mav(MavAutopilot::MAV_AUTOPILOT_INVALID),
            AutopilotKind::Unknown
        );
    }

    #[test]
    fn hint_is_only_a_fallback() {
        assert_eq!(
            AutopilotHint::Px4.resolve(MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA),
            AutopilotKind::ArduPilot
        );
        assert_eq!(
            AutopilotHint::Px4.resolve(MavAutopilot::MAV_AUTOPILOT_INVALID),
            AutopilotKind::Px4
        );
        assert_eq!(
            AutopilotHint::Auto.resolve(MavAutopilot::MAV_AUTOPILOT_INVALID),
            AutopilotKind::Unknown
        );
    }

    #[test]
    fn hint_parser_is_case_insensitive() {
        assert_eq!(AutopilotHint::parse("AUTO").unwrap(), AutopilotHint::Auto);
        assert_eq!(AutopilotHint::parse("PX4").unwrap(), AutopilotHint::Px4);
        assert_eq!(
            AutopilotHint::parse("ArduPilot").unwrap(),
            AutopilotHint::ArduPilot
        );
        assert_eq!(
            AutopilotHint::parse("apm").unwrap(),
            AutopilotHint::ArduPilot
        );
        assert!(AutopilotHint::parse("dji").is_err());
    }

    #[test]
    fn px4_mode_encoding_round_trips() {
        let mode = px4_custom_mode(PX4_MAIN_MODE_AUTO, PX4_SUB_MODE_AUTO_LOITER);
        assert_eq!(mode, (3 << 24) | (4 << 16));
        assert_eq!(px4_mode_name(mode), "AUTO.LOITER");
        assert_eq!(
            px4_mode_name(px4_custom_mode(PX4_MAIN_MODE_OFFBOARD, 0)),
            "OFFBOARD"
        );
        assert_eq!(
            px4_mode_name(px4_custom_mode(
                PX4_MAIN_MODE_AUTO,
                PX4_SUB_MODE_AUTO_TAKEOFF
            )),
            "AUTO.TAKEOFF"
        );
        assert_eq!(px4_mode_name(0), "MODE_0");
    }

    #[test]
    fn ardupilot_modes_depend_on_vehicle_type() {
        assert_eq!(
            ardupilot_mode_name(MavType::MAV_TYPE_QUADROTOR, 4),
            "GUIDED"
        );
        assert_eq!(ardupilot_mode_name(MavType::MAV_TYPE_QUADROTOR, 6), "RTL");
        assert_eq!(
            ardupilot_mode_name(MavType::MAV_TYPE_FIXED_WING, 15),
            "GUIDED"
        );
        assert_eq!(ardupilot_mode_name(MavType::MAV_TYPE_FIXED_WING, 11), "RTL");
        assert_eq!(
            ardupilot_mode_name(MavType::MAV_TYPE_GROUND_ROVER, 5),
            "LOITER"
        );
        assert_eq!(
            ardupilot_mode_name(MavType::MAV_TYPE_QUADROTOR, 200),
            "MODE_200"
        );
    }

    #[test]
    fn guided_mode_value_matches_vehicle_class() {
        assert_eq!(ardupilot_guided_mode(MavType::MAV_TYPE_QUADROTOR), 4);
        assert_eq!(ardupilot_guided_mode(MavType::MAV_TYPE_HELICOPTER), 4);
        assert_eq!(ardupilot_guided_mode(MavType::MAV_TYPE_FIXED_WING), 15);
        assert_eq!(ardupilot_guided_mode(MavType::MAV_TYPE_GROUND_ROVER), 15);
        assert_eq!(ardupilot_guided_mode(MavType::MAV_TYPE_SUBMARINE), 4);
    }
}
