//! 电子围栏判定：支持多边形/多边形组（GeoJSON）与圆形，带高度区间。
//!
//! 围栏类型（规则 §5.4 / §6.10）：
//! - `no_fly`：进入区域（且高度落在区间内）即违规 → **high**；
//! - `work`：离开工作区或高度越界即违规 → **medium**；
//! - `height`：只做高度区间判定 → **medium**。
//!
//! 几何判定使用 `geo` crate（`MultiPolygon::contains`），先做 bbox 粗筛再精判。
//! 告警去抖：连续 2 个采样或持续 1s 才触发，离开后 3s 恢复（规则 §6.10）。

use geo::algorithm::{BoundingRect, Contains};
use geo::{Coord, MultiPolygon, Point, Polygon, Rect};
use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_core::metrics::names;
use rustcam_drone_core::telemetry::Telemetry;

use crate::state::{AlertRecord, AuditRecord, DroneState, FenceDebounce, FenceRecord};

const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// 触发告警所需的连续违规采样数（规则 §6.10）。
pub const TRIGGER_SAMPLES: u32 = 2;
/// 触发告警所需的最短持续时间（规则 §6.10）。
pub const TRIGGER_DURATION_MS: i64 = 1_000;
/// 离开违规状态后的恢复等待（规则 §6.10）。
pub const RECOVER_DURATION_MS: i64 = 3_000;

/// 判断一个点是否落在围栏的水平投影内。
pub fn contains(fence: &FenceRecord, lat: f64, lon: f64) -> bool {
    if fence.shape == "circle" {
        return haversine_m(lat, lon, fence.center_lat, fence.center_lon) <= fence.radius_m;
    }
    match parse_multipolygon(fence) {
        Some((mp, bbox)) => {
            let pt = Point::new(lon, lat);
            // 粗筛：bbox 不含则一定不在多边形内。
            bbox.contains(&pt) && mp.contains(&pt)
        }
        None => false,
    }
}

/// 解析围栏 GeoJSON 为 `geo::MultiPolygon`，并附带 bbox 粗筛框。
fn parse_multipolygon(fence: &FenceRecord) -> Option<(MultiPolygon<f64>, Rect<f64>)> {
    let value = fence.geojson.parse::<geojson::GeoJson>().ok()?;
    let polygons = collect_polygons(&value);
    if polygons.is_empty() {
        return None;
    }
    let mp = MultiPolygon::new(polygons);
    let bbox = mp.bounding_rect().unwrap_or_else(|| {
        Rect::new(
            Coord {
                x: f64::MIN,
                y: f64::MIN,
            },
            Coord {
                x: f64::MAX,
                y: f64::MAX,
            },
        )
    });
    Some((mp, bbox))
}

fn collect_polygons(value: &geojson::GeoJson) -> Vec<Polygon<f64>> {
    match value {
        geojson::GeoJson::Geometry(g) => polygons_from_geometry(g),
        geojson::GeoJson::Feature(f) => f
            .geometry
            .as_ref()
            .map(polygons_from_geometry)
            .unwrap_or_default(),
        geojson::GeoJson::FeatureCollection(fc) => fc
            .features
            .iter()
            .flat_map(|f| {
                f.geometry
                    .as_ref()
                    .map(polygons_from_geometry)
                    .unwrap_or_default()
            })
            .collect(),
    }
}

fn polygons_from_geometry(geometry: &geojson::Geometry) -> Vec<Polygon<f64>> {
    match &geometry.value {
        geojson::GeometryValue::Polygon { .. } => {
            geo_types::Polygon::<f64>::try_from(&geometry.value)
                .ok()
                .map(|p| vec![p])
                .unwrap_or_default()
        }
        geojson::GeometryValue::MultiPolygon { .. } => {
            geo_types::MultiPolygon::<f64>::try_from(&geometry.value)
                .ok()
                .map(|mp| mp.0)
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().asin()
}

fn altitude_ok(fence: &FenceRecord, alt: f64) -> bool {
    let min = if fence.min_alt > 0.0 {
        fence.min_alt
    } else {
        f64::MIN
    };
    let max = if fence.max_alt > 0.0 {
        fence.max_alt
    } else {
        f64::MAX
    };
    alt >= min && alt <= max
}

fn applies(fence: &FenceRecord, drone_tag: &str) -> bool {
    if !fence.enabled {
        return false;
    }
    // 关联表（drone_fence_device）为空表示模板围栏：对所有无人机生效。
    fence.drone_tags.is_empty() || fence.drone_tags.iter().any(|id| id == drone_tag)
}

/// 围栏类型归一化（规则 §5.4：`no_fly` / `work` / `height`；`allowed` 视作 `work`）。
pub fn normalize_rule(rule_type: &str) -> &str {
    match rule_type.trim().to_ascii_lowercase().as_str() {
        "no_fly" | "nofly" | "no-fly" => "no_fly",
        "height" | "limited" => "height",
        "work" | "allowed" => "work",
        _ => "no_fly",
    }
}

/// 违规级别（规则 §6.10 / §17.3）。
pub fn breach_level(rule_type: &str) -> &'static str {
    match normalize_rule(rule_type) {
        "no_fly" => "high",
        _ => "medium",
    }
}

/// 判定是否违规。
pub fn is_breach(fence: &FenceRecord, lat: f64, lon: f64, alt: f64) -> bool {
    let rule = normalize_rule(&fence.rule_type);
    let inside = contains(fence, lat, lon);
    let alt_ok = altitude_ok(fence, alt);
    match rule {
        "work" => !inside || !alt_ok,
        "height" => !alt_ok,
        // 默认按禁飞区处理
        _ => inside && alt_ok,
    }
}

/// 遥测越界判定；命中时写入告警/审计并广播。
pub fn evaluate(state: &DroneState, telemetry: &Telemetry) {
    if !telemetry.has_position() {
        return;
    }
    let eval_started = std::time::Instant::now();
    let now = chrono::Utc::now().timestamp_millis();
    for entry in state.fences.iter() {
        let fence = entry.value();
        if !applies(fence, &telemetry.drone_tag) {
            continue;
        }
        let breach = is_breach(fence, telemetry.lat, telemetry.lon, telemetry.alt_amsl);
        let key = format!("{}:{}", telemetry.drone_tag, fence.id);

        if !breach {
            // 离开后 3s 恢复（规则 §6.10）。
            let mut guard = state
                .fence_debounce
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let Some(entry) = guard.get_mut(&key) else {
                continue;
            };
            if entry.alerting && now - entry.last_violation_ms >= RECOVER_DURATION_MS {
                entry.alerting = false;
                entry.consecutive = 0;
                entry.first_violation_ms = 0;
            }
            continue;
        }

        // 连续 2 个采样或持续 1s 才触发。
        let should_alert = {
            let mut guard = state
                .fence_debounce
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let entry = guard.entry(key.clone()).or_insert(FenceDebounce {
                consecutive: 0,
                first_violation_ms: now,
                last_violation_ms: now,
                alerting: false,
            });
            entry.consecutive += 1;
            entry.last_violation_ms = now;
            if entry.alerting {
                false
            } else if entry.consecutive >= TRIGGER_SAMPLES
                || now - entry.first_violation_ms >= TRIGGER_DURATION_MS
            {
                entry.alerting = true;
                true
            } else {
                false
            }
        };
        if !should_alert {
            continue;
        }

        let level = breach_level(&fence.rule_type);
        let message = format!(
            "{} `{}` 越界：({:.6}, {:.6}, {:.1}m)",
            if normalize_rule(&fence.rule_type) == "work" {
                "离开允许区"
            } else {
                "进入禁飞区"
            },
            fence.name,
            telemetry.lat,
            telemetry.lon,
            telemetry.alt_amsl
        );
        let alert = AlertRecord {
            drone_tag: telemetry.drone_tag.clone(),
            rule: "geofence_breach".into(),
            level: level.into(),
            message: message.clone(),
            ts_ms: now,
            // 越界位置：事后追溯要知道在哪越的界。
            lat: Some(telemetry.lat),
            lon: Some(telemetry.lon),
            alt: Some(telemetry.alt_amsl),
        };
        state.push_alert(alert.clone());
        let _ = state.event_tx.send(DroneEvent::Alert {
            drone_tag: alert.drone_tag.clone(),
            rule: alert.rule,
            level: alert.level,
            message: alert.message,
        });
        state.push_audit(AuditRecord {
            ts_ms: now,
            drone_tag: telemetry.drone_tag.clone(),
            category: "fence".into(),
            action: "breach".into(),
            operator: "system".into(),
            result: "alerted".into(),
            detail: message,
            ..Default::default()
        });
        state.metrics_inc_labeled(names::GEOFENCE_ALERT_TOTAL, &[("level", level)]);
        state.persist();
    }
    state.metrics_set(
        names::GEOFENCE_EVAL_DURATION_SECONDS,
        eval_started.elapsed().as_secs_f64(),
    );
}

/// `goto` 预检：目标点是否被围栏规则允许。
pub fn check_target(
    state: &DroneState,
    drone_tag: &str,
    lat: f64,
    lon: f64,
    alt: f64,
) -> Result<(), String> {
    for entry in state.fences.iter() {
        let fence = entry.value();
        if !applies(fence, drone_tag) {
            continue;
        }
        if is_breach(fence, lat, lon, alt) {
            return Err(format!(
                "目标点 ({lat:.6}, {lon:.6}, {alt:.1}m) 落在围栏 `{}` 内",
                fence.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use crate::state::DroneState;

    fn fence(rule_type: &str, geojson: &str) -> FenceRecord {
        FenceRecord {
            id: "f1".into(),
            name: "禁飞区".into(),
            rule_type: rule_type.into(),
            shape: "polygon".into(),
            geojson: geojson.into(),
            center_lat: 0.0,
            center_lon: 0.0,
            radius_m: 0.0,
            min_alt: 0.0,
            max_alt: 120.0,
            drone_tags: Vec::new(),
            enabled: true,
            version: 1,
            updated_ms: 0,
        }
    }

    const SQUARE: &str = r#"{"type":"Polygon","coordinates":[[[121.40,31.20],[121.50,31.20],[121.50,31.30],[121.40,31.30],[121.40,31.20]]]}"#;
    const MULTI: &str = r#"{"type":"MultiPolygon","coordinates":[[[[121.40,31.20],[121.50,31.20],[121.50,31.30],[121.40,31.30],[121.40,31.20]]]]}"#;

    #[test]
    fn polygon_contains_uses_geo_crate() {
        let f = fence("no_fly", SQUARE);
        assert!(contains(&f, 31.25, 121.45), "多边形内应判定为命中");
        assert!(!contains(&f, 31.40, 121.45), "多边形外应判定为未命中");
        assert!(!contains(&f, 31.25, 121.60), "bbox 外应直接排除");
    }

    #[test]
    fn multipolygon_and_feature_are_supported() {
        let f = fence("no_fly", MULTI);
        assert!(contains(&f, 31.25, 121.45));
        let feature = fence(
            "no_fly",
            r#"{"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[121.40,31.20],[121.50,31.20],[121.50,31.30],[121.40,31.30],[121.40,31.20]]]}}"#,
        );
        assert!(contains(&feature, 31.25, 121.45));
    }

    #[test]
    fn rule_types_map_to_levels() {
        assert_eq!(normalize_rule("no_fly"), "no_fly");
        assert_eq!(normalize_rule("allowed"), "work");
        assert_eq!(normalize_rule("limited"), "height");
        assert_eq!(breach_level("no_fly"), "high");
        assert_eq!(breach_level("work"), "medium");
        assert_eq!(breach_level("height"), "medium");
    }

    #[test]
    fn work_fence_breaches_when_leaving() {
        let f = fence("work", SQUARE);
        assert!(!is_breach(&f, 31.25, 121.45, 30.0), "在工作区内不应违规");
        assert!(is_breach(&f, 31.40, 121.45, 30.0), "离开工作区应违规");
    }

    #[test]
    fn height_fence_only_checks_altitude() {
        let f = fence("height", SQUARE);
        assert!(!is_breach(&f, 31.25, 121.45, 30.0));
        assert!(is_breach(&f, 31.25, 121.45, 500.0), "超出限高应违规");
    }

    #[test]
    fn alert_needs_two_samples_or_one_second() {
        let state = DroneState::new(DroneConfig::default());
        state.fences.insert("f1".into(), fence("no_fly", SQUARE));
        let telemetry = Telemetry {
            drone_tag: "d1".into(),
            ts_ms: 1,
            lat: 31.25,
            lon: 121.45,
            alt_amsl: 30.0,
            ..Default::default()
        };

        let mut rx = state.event_tx.subscribe();
        evaluate(&state, &telemetry);
        assert!(
            rx.try_recv().is_err(),
            "第一个采样不应立即告警（需要连续 2 个采样或持续 1s）"
        );

        evaluate(&state, &telemetry);
        let evt = rx.try_recv().expect("第二个采样应触发告警");
        assert_eq!(evt.msg_type(), "alert");
    }

    #[test]
    fn circle_fence_uses_radius() {
        let mut f = fence("no_fly", "");
        f.shape = "circle".into();
        f.center_lat = 31.25;
        f.center_lon = 121.45;
        f.radius_m = 1000.0;
        assert!(contains(&f, 31.25, 121.45));
        assert!(!contains(&f, 31.40, 121.45));
    }

    #[test]
    fn no_fly_only_breaches_inside_altitude_band() {
        let mut f = fence("no_fly", SQUARE);
        f.min_alt = 0.0;
        f.max_alt = 120.0;
        assert!(is_breach(&f, 31.25, 121.45, 50.0), "禁飞区高度带内应违规");
        assert!(
            !is_breach(&f, 31.25, 121.45, 500.0),
            "超出高度带不应按平面禁飞区判定"
        );
    }

    #[test]
    fn check_target_rejects_no_fly_and_allows_outside() {
        let state = DroneState::new(DroneConfig::default());
        state.fences.insert("f1".into(), fence("no_fly", SQUARE));
        assert!(check_target(&state, "d1", 31.25, 121.45, 30.0).is_err());
        assert!(check_target(&state, "d1", 31.40, 121.45, 30.0).is_ok());
    }

    #[test]
    fn work_fence_allows_inside_and_blocks_outside() {
        let state = DroneState::new(DroneConfig::default());
        state.fences.insert("f1".into(), fence("work", SQUARE));
        assert!(check_target(&state, "d1", 31.25, 121.45, 30.0).is_ok());
        assert!(check_target(&state, "d1", 31.40, 121.45, 30.0).is_err());
    }
}
