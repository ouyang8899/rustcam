//! 归档写入：遥测与告警落 PostgreSQL（遥测走 TimescaleDB 超表）。
//!
//! 设计约束：
//! - ingest 侧只做 `try_send` 投递，绝不等待写库——飞控链路不能被数据库拖慢；
//!   队列满时直接丢弃并计数（宁可丢样本，也不能丢链路）。
//! - 后台 worker 攒批写入：遥测按批（时间或行数触发），告警低频单独即时写。
//! - 写库失败只计数 + 记日志，不影响主链路。

use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc;

use rustcam_drone_core::metrics::names;
use rustcam_drone_core::telemetry::Telemetry;

use crate::state::{AlertRecord, DroneState};

/// 归档队列容量：满了就丢。
pub const ARCHIVE_QUEUE_CAPACITY: usize = 8192;
/// 单批最大行数。
const BATCH_MAX: usize = 500;
/// 单批最大等待时间。
const BATCH_INTERVAL: Duration = Duration::from_millis(200);

/// 待归档的事件。
#[derive(Debug, Clone)]
pub enum ArchiveEvent {
    Telemetry(Telemetry),
    Alert(AlertRecord),
}

/// 遥测行：字段名与 `drone_telemetry` 表列名一一对应。
///
/// 只存逐帧变化的量；`unique_id` / `autopilot` / `home_*` 每机恒定，
/// 档案表里已有，不进时序表。
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryRow {
    drone_tag: String,
    ts: i64,
    lat: f64,
    lon: f64,
    alt_amsl: f64,
    rel_alt: f64,
    roll: f64,
    pitch: f64,
    yaw: f64,
    vx: f64,
    vy: f64,
    vz: f64,
    qx: f64,
    qy: f64,
    qz: f64,
    qw: f64,
    battery_pct: f64,
    voltage: f64,
    link_quality: i32,
    seq: i32,
    satellites: i32,
    gps_fix: i32,
    hdop: f64,
    armed: bool,
    mode: String,
}

impl TelemetryRow {
    pub fn from(t: &Telemetry) -> TelemetryRow {
        TelemetryRow {
            drone_tag: t.drone_tag.clone(),
            ts: t.ts_ms,
            lat: t.lat,
            lon: t.lon,
            alt_amsl: t.alt_amsl,
            rel_alt: t.rel_alt,
            roll: t.roll,
            pitch: t.pitch,
            yaw: t.yaw,
            vx: t.vx,
            vy: t.vy,
            vz: t.vz,
            qx: t.qx,
            qy: t.qy,
            qz: t.qz,
            qw: t.qw,
            battery_pct: t.battery_pct,
            voltage: t.voltage,
            link_quality: t.link_quality as i32,
            seq: t.seq as i32,
            satellites: t.satellites as i32,
            gps_fix: t.gps_fix as i32,
            hdop: t.hdop,
            armed: t.armed,
            mode: t.mode.clone(),
        }
    }
}

/// 投递一条遥测到归档队列；队列满则丢弃并计数。
pub fn offer_telemetry(state: &DroneState, telemetry: &Telemetry) {
    let Some(tx) = state.archive_tx() else {
        tracing::warn!("[Drone] archive channel missing");
        return;
    };
    if tx.try_send(ArchiveEvent::Telemetry(telemetry.clone())).is_err() {
        state.metrics_inc(names::ARCHIVE_DROPPED_TOTAL);
    }
}

/// 投递一条告警到归档队列；队列满则丢弃并计数。
pub fn offer_alert(state: &DroneState, alert: &AlertRecord) {
    let Some(tx) = state.archive_tx() else {
        return;
    };
    if tx.try_send(ArchiveEvent::Alert(alert.clone())).is_err() {
        state.metrics_inc(names::ARCHIVE_DROPPED_TOTAL);
    }
}

/// 归档 worker：收队列事件，攒批写库。
pub async fn run_archive_worker(state: DroneState, mut rx: mpsc::Receiver<ArchiveEvent>) {
    let mut batch: Vec<TelemetryRow> = Vec::with_capacity(BATCH_MAX);

    loop {
        // 每批从开始计时；行数或时间任一到达上限就落库。
        // 注意超时必须是「距本批开始还剩多久」，而不是每次 recv 的独立超时——
        // 否则连续流量下永远等不到 idle，批次会一直攒到 BATCH_MAX。
        let deadline = tokio::time::Instant::now() + BATCH_INTERVAL;
        while batch.len() < BATCH_MAX {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(ArchiveEvent::Telemetry(t))) => {
                    if t.ts_ms <= 0 {
                        tracing::warn!("[Drone] telemetry without timestamp skipped");
                        continue;
                    }
                    batch.push(TelemetryRow::from(&t));
                }
                Ok(Some(ArchiveEvent::Alert(alert))) => {
                    write_alert(&state, &alert).await;
                }
                Ok(None) => {
                    // 通道关闭：把剩余的批次写完再退出。
                    flush(&state, &mut batch).await;
                    return;
                }
                Err(_) => break,
            }
        }
        flush(&state, &mut batch).await;
        // 队列深度作为指标暴露，便于发现写库跟不上。
        state.metrics_set(names::ARCHIVE_QUEUE_DEPTH, rx.len() as f64);
    }
}

/// 写一批遥测。
async fn flush(state: &DroneState, batch: &mut Vec<TelemetryRow>) {
    if batch.is_empty() {
        return;
    }
    let rows = std::mem::take(batch);
    let count = rows.len();
    let Some(db) = state.db.clone() else {
        // 没有数据库时样本直接丢弃（内存环仍是热数据）。
        state.metrics_inc(names::ARCHIVE_DROPPED_TOTAL);
        return;
    };
    match db.insert_telemetry_batch(&rows).await {
        Ok(()) => {
            state.metrics_inc_labeled(names::ARCHIVE_ROWS_TOTAL, &[("kind", "telemetry")]);
        }
        Err(e) => {
            tracing::warn!(error = %e, rows = count, "[Drone] telemetry archive write failed");
            state.metrics_inc(names::ARCHIVE_ERRORS_TOTAL);
        }
    }
}

/// 写一条告警。
async fn write_alert(state: &DroneState, alert: &AlertRecord) {
    let Some(db) = state.db.clone() else {
        state.metrics_inc(names::ARCHIVE_DROPPED_TOTAL);
        return;
    };
    if let Err(e) = db.insert_alert(alert).await {
        tracing::warn!(error = %e, "[Drone] alert archive write failed");
        state.metrics_inc(names::ARCHIVE_ERRORS_TOTAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;

    #[test]
    fn telemetry_row_maps_only_changing_fields() {
        let t = Telemetry {
            drone_tag: "drone-001".into(),
            ts_ms: 1_000,
            lat: 31.2,
            lon: 121.4,
            alt_amsl: 60.0,
            battery_pct: 88.0,
            link_quality: 7,
            seq: 3,
            mode: "AUTO.LOITER".into(),
            unique_id: "0011223344556677".into(),
            autopilot: "px4".into(),
            home_lat: 31.1,
            ..Default::default()
        };
        let row = TelemetryRow::from(&t);
        assert_eq!(row.drone_tag, "drone-001");
        assert_eq!(row.ts, 1_000);
        assert_eq!(row.lat, 31.2);
        assert_eq!(row.link_quality, 7);
        assert_eq!(row.seq, 3);
        assert_eq!(row.mode, "AUTO.LOITER");

        // 恒定量不进时序表。
        let json = serde_json::to_value(&row).expect("serialize");
        assert!(json.get("unique_id").is_none());
        assert!(json.get("autopilot").is_none());
        assert!(json.get("home_lat").is_none());
    }

    #[tokio::test]
    async fn recorded_telemetry_reaches_the_archive_queue() {
        let (tx, mut rx) = mpsc::channel(8);
        let state = DroneState::new(DroneConfig::default()).with_archive(tx);
        state.record_telemetry(&Telemetry {
            drone_tag: "drone-001".into(),
            ts_ms: 1_000,
            ..Default::default()
        });
        match rx.try_recv() {
            Ok(ArchiveEvent::Telemetry(t)) => assert_eq!(t.drone_tag, "drone-001"),
            other => panic!("expected telemetry event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn archive_flushes_on_interval_under_continuous_load() {
        let (tx, rx) = mpsc::channel(1024);
        let state = DroneState::new(DroneConfig::default()).with_archive(tx.clone());

        // 连续到达（间隔远小于 BATCH_INTERVAL），永远等不到 idle 超时。
        let sender = tokio::spawn(async move {
            for seq in 0..10_000u32 {
                let telemetry = Telemetry {
                    drone_tag: "drone-001".into(),
                    ts_ms: 1_000 + seq as i64,
                    ..Default::default()
                };
                if tx.send(ArchiveEvent::Telemetry(telemetry)).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });

        let worker_state = state.clone();
        let worker = tokio::spawn(run_archive_worker(worker_state, rx));

        // 远小于攒满 500 行所需的 2.5s，但足够触发多次时间批次。
        tokio::time::sleep(Duration::from_millis(800)).await;
        let flushes = state
            .metrics
            .lock()
            .expect("metrics lock")
            .counter(names::ARCHIVE_DROPPED_TOTAL);

        worker.abort();
        sender.abort();

        assert!(
            flushes >= 1,
            "expected interval-triggered flushes within 800ms, got {flushes}"
        );
    }

    #[tokio::test]
    async fn offer_without_channel_is_a_noop() {
        let state = DroneState::new(DroneConfig::default());
        offer_telemetry(&state, &Telemetry::default());
        offer_alert(
            &state,
            &AlertRecord {
                drone_tag: "drone-001".into(),
                rule: "geofence_breach".into(),
                level: "high".into(),
                message: "越界".into(),
                ts_ms: 1,
                ..Default::default()
            },
        );
    }
}
