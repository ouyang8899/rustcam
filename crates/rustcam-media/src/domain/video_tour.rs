use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TourChannel {
    pub channel_tag: String,
    pub device_tag: String,
    pub name: String,
    #[serde(default)]
    pub region_id: String,
    #[serde(default)]
    pub group_ids: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub rtsp_main: String,
    #[serde(default)]
    pub rtsp_sub: String,
    #[serde(default)]
    pub sort: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LockedSlot {
    pub layout_item_id: String,
    pub device_tag: String,
    pub channel_tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunWindow {
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub cross_day: bool,
    #[serde(default)]
    pub weekdays: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TourScheme {
    pub id: i64,
    pub name: String,
    pub version: i32,
    pub layout_id: i32,
    pub device_channels: Vec<TourChannel>,
    pub locked_set: Vec<LockedSlot>,
    pub dwell_ms: i32,
    pub batch_timeout_ms: i32,
    pub switch_gap_ms: i32,
    pub cycle_gap_ms: i32,
    pub max_cycles: Option<i32>,
    pub run_window: Option<RunWindow>,
    pub safety_max_duration_ms: Option<i64>,
    pub backfill_limit: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTourSchemeRequest {
    pub name: String,
    pub layout_id: i32,
    pub device_channels: Vec<TourChannel>,
    pub locked_set: Vec<LockedSlot>,
    pub dwell_ms: i32,
    pub batch_timeout_ms: i32,
    pub switch_gap_ms: i32,
    pub cycle_gap_ms: i32,
    pub max_cycles: Option<i32>,
    pub run_window: Option<RunWindow>,
    pub safety_max_duration_ms: Option<i64>,
    pub backfill_limit: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTourSchemeRequest {
    pub expected_version: i32,
    #[serde(flatten)]
    pub scheme: CreateTourSchemeRequest,
}

pub fn validate_tour_scheme(
    req: &CreateTourSchemeRequest,
    layout_w: usize,
    layout_item_ids: &[String],
) -> Result<(), String> {
    if req.name.trim().is_empty() {
        return Err("方案名称不能为空".into());
    }
    if req.device_channels.is_empty() {
        return Err("方案至少需要一个通道".into());
    }
    if req.locked_set.len() > layout_w {
        return Err("锁定数量超过布局窗口数".into());
    }
    if req.dwell_ms < 1000 || req.dwell_ms > 86_400_000 {
        return Err("dwellMs 必须在 1000..86400000 之间".into());
    }
    if req.batch_timeout_ms < 500 || req.batch_timeout_ms > req.dwell_ms {
        return Err("batchTimeoutMs 必须在 500..dwellMs 之间".into());
    }
    if req.switch_gap_ms < 0 || req.cycle_gap_ms < 0 {
        return Err("切换/轮次间隔不能为负".into());
    }
    if req.backfill_limit < 0 || req.backfill_limit > 10 {
        return Err("backfillLimit 必须在 0..10 之间".into());
    }
    if let Some(max) = req.max_cycles {
        if max < 1 {
            return Err("maxCycles 必须大于 0 或为空".into());
        }
    } else if req.run_window.is_none() && req.safety_max_duration_ms.is_none() {
        return Err("无限次数必须配置运行时段或安全兜底".into());
    }
    let channel_keys: std::collections::HashSet<(String, String)> = req
        .device_channels
        .iter()
        .map(|c| (c.device_tag.clone(), c.channel_tag.clone()))
        .collect();
    let mut seen_items = std::collections::HashSet::new();
    for lock in &req.locked_set {
        if !layout_item_ids.iter().any(|id| id == &lock.layout_item_id) {
            return Err(format!("锁定引用了不存在的布局窗口 {}", lock.layout_item_id));
        }
        if !seen_items.insert(lock.layout_item_id.clone()) {
            return Err("同一布局窗口不能重复锁定".into());
        }
        if !channel_keys.contains(&(lock.device_tag.clone(), lock.channel_tag.clone())) {
            return Err(format!("锁定通道 {} 不在通道队列中", lock.channel_tag));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(tag: &str) -> TourChannel {
        TourChannel {
            channel_tag: tag.to_string(),
            device_tag: "dev".to_string(),
            name: tag.to_string(),
            region_id: String::new(),
            group_ids: vec![],
            tags: vec![],
            rtsp_main: String::new(),
            rtsp_sub: String::new(),
            sort: 0,
        }
    }

    fn base() -> CreateTourSchemeRequest {
        CreateTourSchemeRequest {
            name: "s".into(),
            layout_id: 1,
            device_channels: vec![channel("c1"), channel("c2")],
            locked_set: vec![],
            dwell_ms: 10_000,
            batch_timeout_ms: 5_000,
            switch_gap_ms: 1_000,
            cycle_gap_ms: 0,
            max_cycles: Some(1),
            run_window: None,
            safety_max_duration_ms: None,
            backfill_limit: 2,
        }
    }

    fn ids() -> Vec<String> { vec!["cell-0".into(), "cell-1".into()] }

    #[test]
    fn accepts_valid_scheme() {
        assert!(validate_tour_scheme(&base(), 2, &ids()).is_ok());
    }

    #[test]
    fn rejects_too_many_locks() {
        let mut req = base();
        req.locked_set = vec![
            LockedSlot { layout_item_id: "cell-0".into(), device_tag: "dev".into(), channel_tag: "c1".into() },
            LockedSlot { layout_item_id: "cell-1".into(), device_tag: "dev".into(), channel_tag: "c2".into() },
        ];
        assert_eq!(validate_tour_scheme(&req, 1, &ids()), Err("锁定数量超过布局窗口数".into()));
    }

    #[test]
    fn rejects_lock_for_unknown_layout_item() {
        let mut req = base();
        req.locked_set = vec![LockedSlot { layout_item_id: "nope".into(), device_tag: "dev".into(), channel_tag: "c1".into() }];
        assert!(validate_tour_scheme(&req, 2, &ids()).is_err());
    }

    #[test]
    fn rejects_lock_for_missing_channel() {
        let mut req = base();
        req.locked_set = vec![LockedSlot { layout_item_id: "cell-0".into(), device_tag: "dev".into(), channel_tag: "c9".into() }];
        assert!(validate_tour_scheme(&req, 2, &ids()).is_err());
    }

    #[test]
    fn rejects_timeout_over_dwell() {
        let mut req = base();
        req.batch_timeout_ms = 20_000;
        assert!(validate_tour_scheme(&req, 2, &ids()).is_err());
    }

    #[test]
    fn infinite_requires_window_or_safety() {
        let mut req = base();
        req.max_cycles = None;
        assert!(validate_tour_scheme(&req, 2, &ids()).is_err());
        req.safety_max_duration_ms = Some(3_600_000);
        assert!(validate_tour_scheme(&req, 2, &ids()).is_ok());
    }

    #[test]
    fn rejects_empty_channels_and_bad_numbers() {
        let mut req = base();
        req.device_channels.clear();
        assert!(validate_tour_scheme(&req, 2, &ids()).is_err());
        let mut req = base();
        req.dwell_ms = 0;
        assert!(validate_tour_scheme(&req, 2, &ids()).is_err());
        let mut req = base();
        req.backfill_limit = -1;
        assert!(validate_tour_scheme(&req, 2, &ids()).is_err());
    }
}
