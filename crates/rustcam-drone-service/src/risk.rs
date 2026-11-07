//! 风控规则（规则 §6.11）。
//!
//! | 规则 | 条件 | 级别 |
//! |---|---|---|
//! | 低电量 | `battery_pct < 20` | medium |
//! | 严重低电量 | `battery_pct < 10` | high |
//! | 超速 | 水平速度 > 配置阈值 | medium |
//! | 失联 | 超过 3 个心跳周期无遥测 | high（由 presence 回收器发出） |
//!
//! 围栏相关规则见 `geofence` 模块。所有告警写入 `risk:alerts` 语义
//! （内存告警环 + 事件广播），并带 `rule` 便于前端区分。

use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_core::metrics::names;
use rustcam_drone_core::telemetry::Telemetry;

use crate::state::{AlertRecord, DroneState};

/// 低电量阈值（规则 §6.11）。
pub const LOW_BATTERY_PCT: f64 = 20.0;
/// 严重低电量阈值（规则 §6.11）。
pub const CRITICAL_BATTERY_PCT: f64 = 10.0;
/// 超速阈值缺省值（米/秒）。
pub const DEFAULT_MAX_SPEED_MPS: f64 = 30.0;

/// 水平速度（米/秒）。
pub fn horizontal_speed(telemetry: &Telemetry) -> f64 {
    (telemetry.vx.powi(2) + telemetry.vy.powi(2)).sqrt()
}

/// 评估风控规则；命中时产生告警。
pub fn evaluate(state: &DroneState, telemetry: &Telemetry) {
    if !state.config.risk_enabled {
        return;
    }

    let battery = telemetry.battery_pct;
    if battery < CRITICAL_BATTERY_PCT {
        emit(
            state,
            telemetry,
            "battery_critical",
            "high",
            format!("电量严重不足：{battery:.1}%"),
        );
    } else if battery < LOW_BATTERY_PCT {
        emit(
            state,
            telemetry,
            "battery_low",
            "medium",
            format!("电量偏低：{battery:.1}%"),
        );
    }

    let limit = if state.config.risk_max_speed_mps > 0.0 {
        state.config.risk_max_speed_mps
    } else {
        DEFAULT_MAX_SPEED_MPS
    };
    let speed = horizontal_speed(telemetry);
    if speed > limit {
        emit(
            state,
            telemetry,
            "overspeed",
            "medium",
            format!("水平速度 {speed:.1}m/s 超过阈值 {limit:.1}m/s"),
        );
    }
}

fn emit(state: &DroneState, telemetry: &Telemetry, rule: &str, level: &str, message: String) {
    let now = chrono::Utc::now().timestamp_millis();
    let alert = AlertRecord {
        drone_tag: telemetry.drone_tag.clone(),
        rule: rule.to_string(),
        level: level.to_string(),
        message: message.clone(),
        ts_ms: now,
    };
    state.push_alert(alert.clone());
    let _ = state.event_tx.send(DroneEvent::Alert {
        drone_tag: alert.drone_tag.clone(),
        rule: alert.rule,
        level: alert.level,
        message: alert.message,
    });
    state.metrics_inc_labeled(names::RISK_ALERT_TOTAL, &[("rule", rule)]);
    state.persist();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;

    fn state(enabled: bool) -> DroneState {
        DroneState::new(DroneConfig {
            risk_enabled: enabled,
            ..Default::default()
        })
    }

    fn telemetry(battery: f64, vx: f64) -> Telemetry {
        Telemetry {
            drone_tag: "d1".into(),
            battery_pct: battery,
            vx,
            vy: 0.0,
            ..Default::default()
        }
    }

    #[test]
    fn rules_are_disabled_by_default() {
        let s = state(false);
        evaluate(&s, &telemetry(5.0, 100.0));
        assert_eq!(s.recent_alerts("d1", 10).len(), 0);
    }

    #[test]
    fn low_battery_is_medium_and_critical_is_high() {
        let s = state(true);
        evaluate(&s, &telemetry(15.0, 0.0));
        let alerts = s.recent_alerts("d1", 10);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, "medium");
        assert_eq!(alerts[0].rule, "battery_low");

        let s2 = state(true);
        evaluate(&s2, &telemetry(5.0, 0.0));
        let alerts = s2.recent_alerts("d1", 10);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, "high");
        assert_eq!(alerts[0].rule, "battery_critical");
    }

    #[test]
    fn overspeed_is_medium() {
        let s = state(true);
        evaluate(&s, &telemetry(90.0, 50.0));
        let alerts = s.recent_alerts("d1", 10);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].rule, "overspeed");
        assert_eq!(alerts[0].level, "medium");
    }

    #[test]
    fn normal_telemetry_triggers_nothing() {
        let s = state(true);
        evaluate(&s, &telemetry(80.0, 5.0));
        assert_eq!(s.recent_alerts("d1", 10).len(), 0);
    }
}
