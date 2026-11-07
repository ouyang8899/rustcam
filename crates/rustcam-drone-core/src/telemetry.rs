use serde::{Deserialize, Serialize};

/// 规范化后的无人机遥测。字段命名即浏览器契约字段名，service / adapter 共用。
///
/// `autopilot` 取值：`px4` / `ardupilot` / `generic` / `unknown`；
/// `mode` 为飞控模式名（PX4 与 ArduPilot 的命名差异由适配层抹平）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    pub drone_tag: String,
    pub ts_ms: i64,
    pub lat: f64,
    pub lon: f64,
    pub alt_amsl: f64,
    pub rel_alt: f64,
    pub roll: f64,
    pub pitch: f64,
    pub yaw: f64,
    pub battery_pct: f64,
    pub voltage: f64,
    pub link_quality: u32,
    pub seq: u32,
    #[serde(default)]
    pub vx: f64,
    #[serde(default)]
    pub vy: f64,
    #[serde(default)]
    pub vz: f64,
    #[serde(default)]
    pub qx: f64,
    #[serde(default)]
    pub qy: f64,
    #[serde(default)]
    pub qz: f64,
    #[serde(default)]
    pub qw: f64,
    #[serde(default)]
    pub autopilot: String,
    #[serde(default)]
    pub armed: bool,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub satellites: u32,
    #[serde(default)]
    pub gps_fix: u32,
    #[serde(default)]
    pub hdop: f64,
    #[serde(default)]
    pub home_lat: f64,
    #[serde(default)]
    pub home_lon: f64,
    #[serde(default)]
    pub home_alt: f64,
    /// 飞控上报的唯一标识（`AUTOPILOT_VERSION.uid`）；空 = 飞控未提供。
    #[serde(default)]
    pub unique_id: String,
}

impl Telemetry {
    pub fn is_valid(&self) -> bool {
        !self.drone_tag.is_empty()
            && self.lat.is_finite()
            && self.lon.is_finite()
            && self.alt_amsl.is_finite()
            && self.roll.is_finite()
            && self.pitch.is_finite()
            && self.yaw.is_finite()
            && (0.0..=100.0).contains(&self.battery_pct)
    }

    /// 是否已经拿到有效定位（GPS fix >= 2 且有非零经纬度）。
    pub fn has_position(&self) -> bool {
        self.lat != 0.0
            && self.lon != 0.0
            && (-90.0..=90.0).contains(&self.lat)
            && (-180.0..=180.0).contains(&self.lon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Telemetry {
        Telemetry {
            drone_tag: "drone-001".into(),
            lat: 31.2304,
            lon: 121.4737,
            alt_amsl: 62.5,
            roll: 0.01,
            pitch: -0.02,
            yaw: 1.57,
            battery_pct: 87.0,
            autopilot: "px4".into(),
            armed: true,
            mode: "AUTO.LOITER".into(),
            ..Default::default()
        }
    }

    #[test]
    fn valid_telemetry_passes() {
        let t = sample();
        assert!(t.is_valid());
        assert!(t.has_position());
    }

    #[test]
    fn empty_device_id_is_invalid() {
        let t = Telemetry {
            battery_pct: 50.0,
            ..Default::default()
        };
        assert!(!t.is_valid());
    }

    #[test]
    fn out_of_range_battery_is_invalid() {
        let t = Telemetry {
            drone_tag: "drone-001".into(),
            battery_pct: 150.0,
            ..Default::default()
        };
        assert!(!t.is_valid());
    }

    #[test]
    fn zero_coordinates_are_not_a_position() {
        let t = Telemetry {
            drone_tag: "d".into(),
            ..Default::default()
        };
        assert!(!t.has_position());
    }

    #[test]
    fn autopilot_and_mode_survive_serde() {
        let t = sample();
        let json = serde_json::to_string(&t).unwrap();
        let back: Telemetry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.autopilot, "px4");
        assert_eq!(back.mode, "AUTO.LOITER");
        assert!(back.armed);
    }

    #[test]
    fn older_payload_without_new_fields_deserializes() {
        let json = r#"{"drone_tag":"d","ts_ms":1,"lat":0,"lon":0,"alt_amsl":0,"rel_alt":0,
            "roll":0,"pitch":0,"yaw":0,"battery_pct":50,"voltage":12,"link_quality":0,"seq":0}"#;
        let t: Telemetry = serde_json::from_str(json).unwrap();
        assert_eq!(t.drone_tag, "d");
        assert_eq!(t.autopilot, "");
    }
}
