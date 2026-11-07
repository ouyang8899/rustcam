//! `rustcam-drone-core::telemetry::Telemetry` → gRPC `drone.v1.common.Telemetry`。

use rustcam_drone_core::telemetry::Telemetry;
use rustcam_drone_proto::common::Telemetry as ProtoTelemetry;

pub fn to_proto(t: &Telemetry) -> ProtoTelemetry {
    ProtoTelemetry {
        drone_tag: t.drone_tag.clone(),
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
        unique_id: t.unique_id.clone(),
        autopilot: t.autopilot.clone(),
        armed: t.armed,
        mode: t.mode.clone(),
        satellites: t.satellites,
        gps_fix: t.gps_fix,
        hdop: t.hdop,
        home_lat: t.home_lat,
        home_lon: t.home_lon,
        home_alt: t.home_alt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fields_are_carried_over() {
        let t = Telemetry {
            drone_tag: "drone-001".into(),
            unique_id: "0123456789abcdef".into(),
            ts_ms: 42,
            lat: 31.2304,
            lon: 121.4737,
            alt_amsl: 62.5,
            rel_alt: 12.5,
            roll: 0.1,
            pitch: -0.2,
            yaw: 1.5,
            qx: 0.1,
            qy: 0.2,
            qz: 0.3,
            qw: 0.9,
            vx: 1.0,
            vy: 2.0,
            vz: -0.5,
            battery_pct: 87.0,
            voltage: 12.4,
            link_quality: 90,
            seq: 7,
            autopilot: "ardupilot".into(),
            armed: true,
            mode: "GUIDED".into(),
            satellites: 14,
            gps_fix: 3,
            hdop: 1.2,
            home_lat: 31.23,
            home_lon: 121.47,
            home_alt: 50.0,
        };
        let p = to_proto(&t);
        assert_eq!(p.drone_tag, "drone-001");
        assert_eq!(p.seq, 7);
        assert!((p.qw - 0.9).abs() < 1e-9);
        assert_eq!(p.autopilot, "ardupilot");
        assert!(p.armed);
        assert_eq!(p.mode, "GUIDED");
        assert_eq!(p.satellites, 14);
        assert!((p.home_lat - 31.23).abs() < 1e-9);
        assert_eq!(p.unique_id, "0123456789abcdef");
    }
}
