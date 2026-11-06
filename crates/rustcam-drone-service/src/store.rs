//! 轻量状态快照：把档案 / 航线 / 围栏 / 控制租约 / 审计 / 告警写入 JSON 文件。
//!
//! 产品级持久化最终应落到 PostgreSQL，但在内测与单机演示环境中，文件快照可以在
//! 不引入外部依赖的前提下解决“服务重启后档案和航线全丢”的问题。

use std::io::Write;
use std::path::Path;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

use crate::state::{
    AlertRecord, AuditRecord, DroneProfileRecord, DroneState, FenceRecord, LeaseRecord, RouteRecord,
};

const FILE_VERSION: u32 = 1;
const MAX_SNAPSHOT_AUDITS: usize = 2_000;
const MAX_SNAPSHOT_ALERTS: usize = 1_000;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub profiles: Vec<DroneProfileRecord>,
    #[serde(default)]
    pub routes: Vec<RouteRecord>,
    #[serde(default)]
    pub fences: Vec<FenceRecord>,
    #[serde(default)]
    pub leases: Vec<LeaseRecord>,
    #[serde(default)]
    pub audits: Vec<AuditRecord>,
    #[serde(default)]
    pub alerts: Vec<AlertRecord>,
}

pub fn load(state: &DroneState) {
    let Some(path) = state.config.state_file.as_deref() else {
        return;
    };
    let path = Path::new(path);
    if !path.exists() {
        return;
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "[Store] read state file failed");
            return;
        }
    };
    let snapshot: Snapshot = match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "[Store] parse state file failed");
            return;
        }
    };

    let mut backfilled = false;
    for mut p in snapshot.profiles {
        if p.drone_tag.trim().is_empty() {
            p.drone_tag = format!("drone-{}", uuid::Uuid::new_v4());
            backfilled = true;
        }
        state.profiles.insert(p.device_id.clone(), p);
    }
    if backfilled {
        state.persist();
    }
    for r in snapshot.routes {
        state.routes.insert(r.id.clone(), r);
    }
    for f in snapshot.fences {
        state.fences.insert(f.id.clone(), f);
    }
    for l in snapshot.leases {
        if l.expire_at_ms > chrono::Utc::now().timestamp_millis() {
            state
                .leases
                .insert(format!("{}:{}", l.device_id, l.control_type), l);
        }
    }
    if let Ok(mut audits) = state.audits.lock() {
        audits.extend(snapshot.audits);
    }
    if let Ok(mut alerts) = state.alerts.lock() {
        alerts.extend(snapshot.alerts);
    }

    tracing::info!(
        path = %path.display(),
        profiles = state.profiles.len(),
        routes = state.routes.len(),
        fences = state.fences.len(),
        "[Store] state restored"
    );
}

/// 采集当前状态并序列化；不做任何磁盘 IO。
pub fn build_snapshot(state: &DroneState) -> Option<(std::path::PathBuf, Vec<u8>)> {
    let path = state.config.state_file.as_deref()?;
    let _guard = match state.persist_lock.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let mut profiles: Vec<_> = state.profiles.iter().map(|e| e.value().clone()).collect();
    profiles.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    let mut routes: Vec<_> = state.routes.iter().map(|e| e.value().clone()).collect();
    routes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut fences: Vec<_> = state.fences.iter().map(|e| e.value().clone()).collect();
    fences.sort_by(|a, b| a.id.cmp(&b.id));
    let mut leases: Vec<_> = state.leases.iter().map(|e| e.value().clone()).collect();
    leases.sort_by(|a, b| a.device_id.cmp(&b.device_id));

    let audits = state
        .recent_audits("", MAX_SNAPSHOT_AUDITS)
        .into_iter()
        .rev()
        .collect();
    let alerts = state
        .recent_alerts("", MAX_SNAPSHOT_ALERTS)
        .into_iter()
        .rev()
        .collect();

    let snapshot = Snapshot {
        version: FILE_VERSION,
        profiles,
        routes,
        fences,
        leases,
        audits,
        alerts,
    };
    let json = match serde_json::to_vec_pretty(&snapshot) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!(error = %e, "[Store] serialize state failed");
            return None;
        }
    };
    Some((Path::new(path).to_path_buf(), json))
}

/// 原子写盘：先写临时文件再改名；调用方需放在阻塞线程池里执行。
pub fn write_atomic(path: &Path, json: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::File::create(&tmp)?.write_all(json)?;
    std::fs::rename(&tmp, path)
}

/// 同步落盘（无 runtime 上下文时使用）。
pub fn save(state: &DroneState) {
    let Some((path, json)) = build_snapshot(state) else {
        return;
    };
    if let Err(e) = write_atomic(&path, &json) {
        tracing::warn!(error = %e, path = %path.display(), "[Store] write state file failed");
    }
}

/// 快照写入任务：合并短时间内的多次写入，磁盘 IO 放在阻塞线程池。
///
/// 规则 §11.6/§16：写盘不能阻塞 runtime 线程，也不能让高频事件把磁盘打满。
pub async fn run_writer(state: DroneState, mut rx: tokio::sync::mpsc::Receiver<()>) {
    state.persist_writer_started.store(true, Ordering::Relaxed);
    while rx.recv().await.is_some() {
        // 合并 200ms 内的连续写入请求。
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        while rx.try_recv().is_ok() {}

        let Some((path, json)) = build_snapshot(&state) else {
            continue;
        };
        let result = tokio::task::spawn_blocking(move || write_atomic(&path, &json)).await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "[Store] write state file failed"),
            Err(e) => tracing::warn!(error = %e, "[Store] snapshot writer task failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use crate::state::{DroneProfileRecord, FenceRecord, RouteRecord, WaypointRecord};

    fn temp_state_file(name: &str) -> String {
        let dir = std::env::temp_dir().join("rustcam-drone-store-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name).to_string_lossy().to_string()
    }

    #[test]
    fn snapshot_round_trip_restores_profiles_routes_and_fences() {
        let path = temp_state_file("round-trip.json");
        let _ = std::fs::remove_file(&path);

        let cfg = DroneConfig {
            state_file: Some(path.clone()),
            ..Default::default()
        };
        let state = DroneState::new(cfg.clone());
        state
            .profiles
            .insert("drone-001".into(), DroneProfileRecord::new("drone-001", 1));
        state.routes.insert(
            "route-1".into(),
            RouteRecord {
                id: "route-1".into(),
                name: "巡检".into(),
                device_id: "drone-001".into(),
                device_ids: vec!["drone-001".into()],
                waypoints: vec![WaypointRecord {
                    lat: 31.2,
                    lon: 121.4,
                    alt: 30.0,
                    speed: 8.0,
                    loiter_sec: 5,
                    action: "waypoint".into(),
                }],
                enabled: true,
                updated_ms: 2,
                end_action: "hold".into(),
            },
        );
        state.fences.insert(
            "fence-1".into(),
            FenceRecord {
                id: "fence-1".into(),
                name: "禁飞区".into(),
                rule_type: "no_fly".into(),
                shape: "polygon".into(),
                geojson: r#"{"type":"Polygon","coordinates":[[[121.4,31.2],[121.5,31.2],[121.5,31.3],[121.4,31.3],[121.4,31.2]]]}"#.into(),
                center_lat: 0.0,
                center_lon: 0.0,
                radius_m: 0.0,
                min_alt: 0.0,
                max_alt: 120.0,
                device_id: String::new(),
                device_ids: Vec::new(),
                enabled: true,
                version: 1,
                updated_ms: 3,
            },
        );
        save(&state);

        let restored = DroneState::new(cfg);
        assert_eq!(restored.profiles.len(), 1);
        assert_eq!(restored.routes.len(), 1);
        assert_eq!(restored.fences.len(), 1);
        assert_eq!(
            restored.routes.get("route-1").unwrap().waypoints[0].lat,
            31.2
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn legacy_snapshot_gets_a_drone_tag() {
        let path = temp_state_file("legacy-no-drone-tag.json");
        let _ = std::fs::remove_file(&path);

        let mut profile = DroneProfileRecord::new("drone-legacy", 1);
        profile.drone_tag.clear();
        let snapshot = Snapshot {
            profiles: vec![profile],
            ..Default::default()
        };
        std::fs::write(&path, serde_json::to_string(&snapshot).unwrap()).unwrap();

        let cfg = DroneConfig {
            state_file: Some(path.clone()),
            ..Default::default()
        };
        let state = DroneState::new(cfg);
        let restored = state.profiles.get("drone-legacy").unwrap();
        assert!(restored.drone_tag.starts_with("drone-"));

        let reread: Snapshot =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(reread.profiles[0].drone_tag, restored.drone_tag);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let cfg = DroneConfig {
            state_file: Some(temp_state_file("does-not-exist.json")),
            ..Default::default()
        };
        let state = DroneState::new(cfg);
        assert!(state.profiles.is_empty());
    }

    #[test]
    fn state_file_none_disables_persistence() {
        let cfg = DroneConfig::default();
        assert!(cfg.state_file.is_none());
        let state = DroneState::new(cfg);
        state.persist();
        assert!(state.profiles.is_empty());
    }
}
