use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    pub device_id: String,
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
}

impl Telemetry {
    pub fn is_valid(&self) -> bool {
        !self.device_id.is_empty()
            && self.lat.is_finite()
            && self.lon.is_finite()
            && self.alt_amsl.is_finite()
            && self.roll.is_finite()
            && self.pitch.is_finite()
            && self.yaw.is_finite()
            && (0.0..=100.0).contains(&self.battery_pct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_telemetry_passes() {
        let t = Telemetry {
            device_id: "drone-001".into(),
            lat: 31.2304,
            lon: 121.4737,
            alt_amsl: 62.5,
            roll: 0.01,
            pitch: -0.02,
            yaw: 1.57,
            battery_pct: 87.0,
            ..Default::default()
        };
        assert!(t.is_valid());
    }

    #[test]
    fn empty_device_id_is_invalid() {
        let t = Telemetry { battery_pct: 50.0, ..Default::default() };
        assert!(!t.is_valid());
    }

    #[test]
    fn out_of_range_battery_is_invalid() {
        let t = Telemetry {
            device_id: "drone-001".into(),
            battery_pct: 150.0,
            ..Default::default()
        };
        assert!(!t.is_valid());
    }
}
