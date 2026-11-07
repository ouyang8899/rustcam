//! 设备影子：接收 adapter 上报的遥测，做去重/乱序过滤后写入内存并广播。
//!
//! 去重键是 adapter 自增的 `seq`（同一帧重发会被丢弃），乱序则按 `ts_ms` 回退
//! 阈值判断；`online` 由注册与遥测共同维护，落库不在本层（后续 archive worker）。

use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_core::metrics::names;
use rustcam_drone_core::telemetry::Telemetry;
use rustcam_drone_proto::common as proto;

use crate::state::{DeviceEntry, DroneState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ingest {
    Accepted,
    Duplicate,
    Stale,
    Invalid,
}

impl Ingest {
    pub fn accepted(&self) -> bool {
        *self == Ingest::Accepted
    }
}

pub fn ingest(state: &DroneState, telemetry: Telemetry) -> Ingest {
    if !telemetry.is_valid() {
        state.metrics_inc(names::TELEMETRY_REJECTED_TOTAL);
        return Ingest::Invalid;
    }

    let ts = if telemetry.ts_ms > 0 {
        telemetry.ts_ms
    } else {
        chrono::Utc::now().timestamp_millis()
    };

    if let Some(mut entry) = state.devices.get_mut(&telemetry.drone_tag) {
        if telemetry.seq == entry.last_seq && ts <= entry.last_ts_ms {
            state.metrics_inc(names::TELEMETRY_DEDUP_DROPPED_TOTAL);
            return Ingest::Duplicate;
        }
        if entry.last_ts_ms > 0 && ts + 1_000 < entry.last_ts_ms {
            state.metrics_inc(names::TELEMETRY_STALE_TOTAL);
            return Ingest::Stale;
        }
        entry.online = true;
        entry.last_seen_ms = ts;
        entry.last_seq = telemetry.seq;
        entry.last_ts_ms = ts;
        // 实测唯一标识（drone.md §12.4 L3）：只在上报非空时覆盖，避免未提供时清掉已有值。
        if !telemetry.unique_id.trim().is_empty() {
            entry.unique_id = telemetry.unique_id.trim().to_string();
        }
        entry.last_telemetry = Some(telemetry.clone());
    } else {
        let (adapter_id, protocol) = match state.adapter_for(&telemetry.drone_tag) {
            Some(adapter_id) => {
                let protocol = state
                    .adapters
                    .get(&adapter_id)
                    .map(|a| a.protocol.clone())
                    .unwrap_or_else(|| "mavlink".to_string());
                (adapter_id, protocol)
            }
            None => ("unknown".to_string(), "mavlink".to_string()),
        };
        let mut entry = DeviceEntry::new(&telemetry.drone_tag, &protocol, &adapter_id, ts);
        entry.last_seq = telemetry.seq;
        entry.last_ts_ms = ts;
        entry.unique_id = telemetry.unique_id.trim().to_string();
        entry.last_telemetry = Some(telemetry.clone());
        state.devices.insert(telemetry.drone_tag.clone(), entry);
    }

    crate::geofence::evaluate(state, &telemetry);
    crate::risk::evaluate(state, &telemetry);

    // 轨迹回放样本需要可用的时间戳：adapter 未带 ts_ms 时用接收时间补齐。
    let mut sample = telemetry.clone();
    if sample.ts_ms <= 0 {
        sample.ts_ms = ts;
    }
    state.record_telemetry(&sample);

    let _ = state
        .event_tx
        .send(DroneEvent::Telemetry(telemetry.clone()));
    state.metrics_inc(names::TELEMETRY_TOTAL);
    // 遥测滞后：上报时间与接收时间的差值，用于发现链路拥塞。
    let lag_secs = (chrono::Utc::now().timestamp_millis() - ts).max(0) as f64 / 1000.0;
    state.metrics_set_labeled(
        names::TELEMETRY_LAG_SECONDS,
        lag_secs,
        &[("drone_tag", telemetry.drone_tag.as_str())],
    );
    Ingest::Accepted
}

pub fn from_proto(t: &proto::Telemetry) -> Telemetry {
    Telemetry {
        drone_tag: t.drone_tag.clone(),
        ts_ms: t.ts_ms,
        lat: t.lat,
        lon: t.lon,
        alt_amsl: t.alt_amsl,
        rel_alt: t.rel_alt,
        roll: t.roll,
        pitch: t.pitch,
        yaw: t.yaw,
        battery_pct: t.battery_pct,
        voltage: t.voltage,
        link_quality: t.link_quality,
        seq: t.seq,
        vx: t.vx,
        vy: t.vy,
        vz: t.vz,
        qx: t.qx,
        qy: t.qy,
        qz: t.qz,
        qw: t.qw,
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

pub fn to_proto(t: &Telemetry) -> proto::Telemetry {
    proto::Telemetry {
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
    use crate::config::DroneConfig;
    use crate::state::{AdapterEntry, DroneState};
    use std::sync::atomic::AtomicI64;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn telemetry(seq: u32, ts_ms: i64) -> Telemetry {
        Telemetry {
            drone_tag: "drone-001".into(),
            ts_ms,
            lat: 31.2304,
            lon: 121.4737,
            alt_amsl: 62.5,
            roll: 0.1,
            pitch: -0.2,
            yaw: 1.5,
            battery_pct: 88.0,
            autopilot: "px4".into(),
            mode: "AUTO.LOITER".into(),
            seq,
            ..Default::default()
        }
    }

    fn state_with_adapter() -> DroneState {
        let state = DroneState::new(DroneConfig::default());
        let (tx, _rx) = mpsc::channel(8);
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
        state
    }

    #[test]
    fn first_telemetry_creates_shadow_and_broadcasts() {
        let state = state_with_adapter();
        let mut rx = state.event_tx.subscribe();
        assert_eq!(ingest(&state, telemetry(1, 1_000)), Ingest::Accepted);
        let entry = state.devices.get("drone-001").unwrap();
        assert!(entry.online);
        assert_eq!(entry.last_seq, 1);
        assert_eq!(entry.adapter_id, "a-1");
        assert_eq!(entry.protocol, "mavlink");
        let event = rx.try_recv().unwrap();
        assert_eq!(event.msg_type(), "telemetry");
    }

    #[test]
    fn duplicate_and_stale_frames_are_dropped() {
        let state = state_with_adapter();
        assert_eq!(ingest(&state, telemetry(1, 3_000)), Ingest::Accepted);
        assert_eq!(ingest(&state, telemetry(1, 3_000)), Ingest::Duplicate);
        assert_eq!(ingest(&state, telemetry(2, 500)), Ingest::Stale);
        assert_eq!(ingest(&state, telemetry(2, 3_100)), Ingest::Accepted);
        assert_eq!(
            state.devices.get("drone-001").unwrap().last_seq,
            2,
            "只有被接受的帧才推进 seq"
        );
    }

    #[test]
    fn accepted_frames_are_recorded_for_replay() {
        let state = state_with_adapter();
        assert_eq!(ingest(&state, telemetry(1, 1_000)), Ingest::Accepted);
        assert_eq!(ingest(&state, telemetry(2, 2_000)), Ingest::Accepted);
        assert_eq!(ingest(&state, telemetry(2, 2_000)), Ingest::Duplicate);

        let all = state.telemetry_history("drone-001", 0, 0, 10);
        assert_eq!(all.len(), 2, "重复帧不应进入轨迹历史");
        assert_eq!(all[0].ts_ms, 1_000);
        assert_eq!(all[1].ts_ms, 2_000);

        let window = state.telemetry_history("drone-001", 1_500, 0, 10);
        assert_eq!(window.len(), 1);
        assert_eq!(window[0].ts_ms, 2_000);
        assert!(state.telemetry_history("unknown", 0, 0, 10).is_empty());
    }

    #[test]
    fn replay_history_is_downsampled_while_keeping_endpoints() {
        let state = DroneState::new(DroneConfig::default());
        for seq in 1..=100 {
            state.record_telemetry(&telemetry(seq, seq as i64 * 1_000));
        }
        let samples = state.telemetry_history("drone-001", 0, 0, 10);
        assert!(samples.len() <= 11);
        assert_eq!(samples.first().map(|t| t.ts_ms), Some(1_000));
        assert_eq!(samples.last().map(|t| t.ts_ms), Some(100_000));
    }

    #[test]
    fn invalid_telemetry_is_rejected() {
        let state = state_with_adapter();
        let mut bad = telemetry(1, 1_000);
        bad.drone_tag.clear();
        assert_eq!(ingest(&state, bad), Ingest::Invalid);
        assert!(state.devices.is_empty());
    }

    #[test]
    fn telemetry_for_unknown_adapter_still_creates_a_shadow() {
        let state = DroneState::new(DroneConfig::default());
        assert_eq!(ingest(&state, telemetry(1, 1_000)), Ingest::Accepted);
        let entry = state.devices.get("drone-001").unwrap();
        assert_eq!(entry.adapter_id, "unknown");
    }

    #[test]
    fn proto_round_trip_keeps_flight_controller_fields() {
        let mut t = telemetry(9, 2_000);
        t.armed = true;
        t.satellites = 16;
        t.gps_fix = 3;
        t.home_lat = 31.2;
        t.vx = 1.5;
        t.qw = 0.9;
        let p = to_proto(&t);
        let back = from_proto(&p);
        assert_eq!(back, t);
    }
}
