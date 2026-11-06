//! 设备在线状态：由 adapter 心跳 / 飞控事件驱动，并负责半开连接的超时回收。

use rustcam_drone_core::event::DroneEvent;

use crate::commands;
use crate::state::{AlertRecord, AuditRecord, DroneState};

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 更新设备在线状态；只在状态变化时广播告警，避免心跳刷屏。
pub async fn set_device_link_state(
    state: &DroneState,
    device_id: &str,
    online: bool,
    message: &str,
) -> bool {
    let changed = if online {
        state.mark_device_online(device_id, now_ms())
    } else {
        state.mark_device_offline(device_id)
    };
    if !changed {
        return false;
    }
    if !online {
        commands::fail_inflight_for_device(state, device_id, message).await;
    }
    let alert = AlertRecord {
        device_id: device_id.to_string(),
        rule: if online {
            "device_online"
        } else {
            "device_offline"
        }
        .to_string(),
        // 失联（超过 3 个心跳周期无遥测）按规则 §6.11 为 high。
        level: if online { "info" } else { "high" }.to_string(),
        message: message.to_string(),
        ts_ms: now_ms(),
    };
    state.push_alert(alert.clone());
    state.push_audit(AuditRecord {
        ts_ms: now_ms(),
        device_id: device_id.to_string(),
        category: "device".into(),
        action: if online { "online" } else { "offline" }.to_string(),
        operator: "system".into(),
        result: "ok".into(),
        detail: message.to_string(),
        ..Default::default()
    });
    let _ = state.event_tx.send(DroneEvent::Alert {
        device_id: alert.device_id,
        rule: alert.rule,
        level: alert.level,
        message: alert.message,
    });
    state.persist();
    true
}

/// 超过 3 个心跳周期没有 adapter 心跳，判定其设备离线（半开连接 / adapter 卡死）。
pub async fn reap_stale_adapters(state: &DroneState) -> usize {
    let timeout_ms = (state.config.adapter_heartbeat_sec.max(1) * 3 * 1000) as i64;
    let now = now_ms();
    let stale: Vec<String> = state
        .adapters
        .iter()
        .filter(|a| {
            now - a
                .last_heartbeat_ms
                .load(std::sync::atomic::Ordering::Relaxed)
                > timeout_ms
        })
        .map(|a| a.key().clone())
        .collect();

    let mut count = 0;
    for adapter_id in stale {
        let devices: Vec<String> = state
            .device_to_adapter
            .iter()
            .filter(|e| e.value() == &adapter_id)
            .map(|e| e.key().clone())
            .collect();
        for device_id in devices {
            if set_device_link_state(
                state,
                &device_id,
                false,
                &format!("adapter `{adapter_id}` heartbeat timeout"),
            )
            .await
            {
                count += 1;
            }
        }
    }
    count
}

pub async fn run_presence_reaper(state: DroneState) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        reap_stale_adapters(&state).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use crate::state::{AdapterEntry, DeviceEntry};
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn state_with_stale_adapter() -> DroneState {
        let state = DroneState::new(DroneConfig::default());
        let (tx, _rx) = mpsc::channel(8);
        state.adapters.insert(
            "a-1".into(),
            AdapterEntry {
                tx,
                generation: 1,
                protocol: "mavlink".into(),
                connected_at_ms: 1,
                last_heartbeat_ms: Arc::new(AtomicI64::new(0)),
            },
        );
        state
            .device_to_adapter
            .insert("drone-001".into(), "a-1".into());
        state.devices.insert(
            "drone-001".into(),
            DeviceEntry::new("drone-001", "mavlink", "a-1", 1),
        );
        state
    }

    #[tokio::test]
    async fn stale_adapter_marks_devices_offline() {
        let state = state_with_stale_adapter();
        assert_eq!(reap_stale_adapters(&state).await, 1);
        assert!(!state.devices.get("drone-001").unwrap().online);
        assert_eq!(reap_stale_adapters(&state).await, 0, "重复回收不应重复告警");
    }

    #[tokio::test]
    async fn fresh_heartbeat_keeps_device_online() {
        let state = state_with_stale_adapter();
        state
            .adapters
            .get("a-1")
            .unwrap()
            .last_heartbeat_ms
            .store(now_ms(), Ordering::Relaxed);
        assert_eq!(reap_stale_adapters(&state).await, 0);
        assert!(state.devices.get("drone-001").unwrap().online);
    }
}
