use std::sync::atomic::AtomicI64;
use std::sync::Arc;

use rustcam_drone_core::keys;
use rustcam_drone_core::metrics::names;
use rustcam_drone_proto::link::{DeviceAssignment, DeviceIdentity, Register, RegisterAck};

use crate::infra::db::DeviceLinkRow;
use crate::infra::redis::DroneRedis;
use crate::presence;
use crate::state::{AdapterEntry, AdapterTx, DeviceEntry, DroneState};

/// 唯一标识判定规则（drone.md §12.4 L3，唯一定义处）。
///
/// - `registered` 为空（档案未登记）：不做判定，放行。
/// - `registered` 非空：要求 `observed` 非空且相等；`observed` 为空按不一致处理。
pub fn identity_matches(registered: &str, observed: &str) -> bool {
    let registered = registered.trim();
    if registered.is_empty() {
        return true;
    }
    !observed.trim().is_empty() && registered == observed.trim()
}

#[derive(Clone)]
pub struct Registry {
    pub state: DroneState,
    redis: Option<Arc<DroneRedis>>,
}

impl Registry {
    pub fn new(state: DroneState, redis: Option<Arc<DroneRedis>>) -> Registry {
        Registry { state, redis }
    }

    /// adapter 注册：分配设备、记录代次、返回 RegisterAck。
    /// 同一 adapter_id 重复注册时新连接胜出（代次自增，旧连接失效）。
    ///
    /// 设备来源：优先按 `adapter_id` 查 `drone_device`（drone.md §9.1），
    /// 查不到时回落到 adapter 自报的 `devices`（引导模式）。
    pub async fn register(&self, reg: &Register, tx: AdapterTx) -> RegisterAck {
        let generation = self.state.next_generation(&reg.adapter_id);
        let now = chrono::Utc::now().timestamp_millis();
        let mut assigned = Vec::new();
        let mut assignments: Vec<DeviceAssignment> = Vec::new();

        // 上报的唯一标识：按 device_id 建索引。
        let observed: std::collections::HashMap<&str, &str> = reg
            .identities
            .iter()
            .map(|i| (i.device_id.as_str(), i.unique_id.as_str()))
            .collect();

        let link_rows = self.load_link_rows(&reg.adapter_id).await;
        let candidates: Vec<String> = if link_rows.is_empty() {
            tracing::warn!(
                adapter = %reg.adapter_id,
                "[Registry] no device rows for adapter, falling back to reported devices"
            );
            reg.devices.clone()
        } else {
            link_rows
                .iter()
                .map(|r| r.device_id.clone())
                .collect::<Vec<String>>()
        };

        for device_id in &candidates {
            // 唯一标识校验（drone.md §12.4 L3）。
            // 只有 adapter **明确上报了该设备的**唯一标识时才在这里校验；
            // DB 驱动的流程里 adapter 注册时尚未连飞控（拿不到 uid），真正的校验
            // 发生在建链后的 `IdentityReport`。
            if let Some(&reported) = observed.get(device_id.as_str()) {
                if let Some(profile) = self.state.profiles.get(device_id.as_str()) {
                    if !identity_matches(&profile.unique_id, reported) {
                        tracing::warn!(
                            device = %device_id,
                            adapter = %reg.adapter_id,
                            registered = %profile.unique_id,
                            observed = %reported,
                            "[Registry] unique_id mismatch, device not assigned"
                        );
                        continue;
                    }
                }
            }

            let owner = self
                .state
                .device_to_adapter
                .get(device_id.as_str())
                .map(|v| v.clone());
            if let Some(owner) = owner {
                if owner != reg.adapter_id {
                    tracing::warn!(
                        device = %device_id,
                        owner = %owner,
                        adapter = %reg.adapter_id,
                        "[Registry] device is owned by another adapter, skipping"
                    );
                    continue;
                }
            }

            self.state
                .device_to_adapter
                .insert(device_id.clone(), reg.adapter_id.clone());
            assigned.push(device_id.clone());

            // 下发该机的端点与签名密钥。
            if let Some(row) = link_rows.iter().find(|r| r.device_id == *device_id) {
                assignments.push(self.build_assignment(row));
            }


            match self.state.devices.get_mut(device_id) {
                Some(mut entry) => {
                    entry.adapter_id = reg.adapter_id.clone();
                    entry.protocol = reg.protocol.clone();
                    entry.last_seen_ms = now;
                }
                None => {
                    let mut entry =
                        DeviceEntry::new(device_id, &reg.protocol, &reg.adapter_id, now);
                    entry.online = false;
                    self.state.devices.insert(device_id.clone(), entry);
                }
            }

            presence::set_device_link_state(
                &self.state,
                device_id,
                true,
                &format!("adapter `{}` connected", reg.adapter_id),
            )
            .await;

            if let Some(redis) = &self.redis {
                let key = keys::adapter_device_key(device_id);
                match redis.set_nx_ex(&key, &reg.adapter_id, 30).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!(
                            device = %device_id,
                            adapter = %reg.adapter_id,
                            "[Registry] redis claim already held (stale key?), continuing"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, device = %device_id, "[Registry] redis claim failed");
                    }
                }
            }
        }

        self.state.adapters.insert(
            reg.adapter_id.clone(),
            AdapterEntry {
                tx,
                generation,
                protocol: reg.protocol.clone(),
                connected_at_ms: now,
                last_heartbeat_ms: Arc::new(AtomicI64::new(now)),
            },
        );

        tracing::info!(
            adapter = %reg.adapter_id,
            generation,
            devices = assigned.len(),
            "[Registry] adapter registered"
        );

        self.state.metrics_inc_labeled(
            names::ADAPTER_CONNECTIONS_TOTAL,
            &[("adapter_id", reg.adapter_id.as_str())],
        );
        if generation > 1 {
            // 同 adapter_id 再次注册 = 链路重建（§7.5）。
            self.state.metrics_inc_labeled(
                names::ADAPTER_RECONNECT_TOTAL,
                &[("adapter_id", reg.adapter_id.as_str())],
            );
        }

        RegisterAck {
            accepted: true,
            reason: String::new(),
            assigned_devices: assigned,
            heartbeat_interval_sec: self.state.config.adapter_heartbeat_sec as i32,
            assignments,
        }
    }

    /// 按 adapter_id 取该 adapter 名下的链路参数；无 DB 或查询失败时返回空列表。
    async fn load_link_rows(&self, adapter_id: &str) -> Vec<DeviceLinkRow> {
        let Some(db) = self.state.db.clone() else {
            return Vec::new();
        };
        match db.load_device_links(adapter_id).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    adapter = %adapter_id,
                    "[Registry] load_device_links failed"
                );
                Vec::new()
            }
        }
    }

    /// 构造下发给 adapter 的分配信息（端点 + 密钥 + 版本 + 寻址地址）。
    fn build_assignment(&self, row: &DeviceLinkRow) -> DeviceAssignment {
        DeviceAssignment {
            device_id: row.device_id.clone(),
            mavlink_addr: row.mavlink_addr.clone(),
            signing_secret: self.decrypt_secret(&row.signing_secret),
            allow_unsigned: row.signing_allow_unsigned,
            mavlink_version: row.mavlink_version,
            mavlink_sysid: row.mavlink_sysid,
            mavlink_compid: row.mavlink_compid,
        }
    }

    /// 解密签名密钥；未配置主密钥时返回空串（该机按"不需要签名"处理）。
    fn decrypt_secret(&self, sealed: &str) -> String {
        if sealed.is_empty() {
            return String::new();
        }
        let Some(master) = self.state.config.signing_master_key.as_deref() else {
            tracing::warn!(
                "[Registry] DRONE_SIGNING_MASTER_KEY not configured, skipping signing secret"
            );
            return String::new();
        };
        match crate::secret::SecretBox::from_hex(master).and_then(|sb| sb.open(sealed)) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "[Registry] decrypt signing secret failed");
                String::new()
            }
        }
    }

    /// 处理 adapter 上报的实测唯一标识。
    ///
    /// 判定规则同 `identity_matches`；不一致的设备立即撤销分配（drone.md §12.4 L3）。
    /// 返回每个设备的处理结果，供调用方发事件与告警。
    pub async fn apply_identity_report(
        &self,
        adapter_id: &str,
        identities: &[DeviceIdentity],
    ) -> Vec<(String, bool, String)> {
        let mut out = Vec::new();
        for id in identities {
            let device_id = id.device_id.trim().to_string();
            let observed = id.unique_id.trim();
            let registered = self
                .state
                .profiles
                .get(device_id.as_str())
                .map(|p| p.unique_id.trim().to_string())
                .unwrap_or_default();

            if identity_matches(&registered, observed) {
                if registered.is_empty() {
                    tracing::warn!(
                        device = %device_id,
                        adapter = %adapter_id,
                        observed = %observed,
                        "[Registry] device has no registered unique_id, accepted with warning"
                    );
                }
                out.push((device_id, true, String::new()));
                continue;
            }

            tracing::warn!(
                device = %device_id,
                adapter = %adapter_id,
                registered = %registered,
                observed = %observed,
                "[Registry] unique_id mismatch, revoking assignment"
            );
            self.revoke_device(&device_id).await;
            out.push((
                device_id,
                false,
                format!("unique_id mismatch: registered `{registered}`, observed `{observed}`"),
            ));
        }
        out
    }

    /// 撤销设备分配：清掉归属、置离线、释放 Redis 独占键。
    async fn revoke_device(&self, device_id: &str) {
        self.state.device_to_adapter.remove(device_id);
        if let Some(mut entry) = self.state.devices.get_mut(device_id) {
            entry.online = false;
            entry.adapter_id = String::new();
        }
        if let Some(redis) = &self.redis {
            // 与 unregister 保持一致：把独占键 TTL 压到 1s，而不是直接删。
            let _ = redis
                .expire(&keys::adapter_device_key(device_id), 1)
                .await;
        }
        tracing::warn!(device = %device_id, "[Registry] device assignment revoked");
    }

    /// 断开清理：仅当仍是当前代次时才清理。
    pub async fn unregister(&self, adapter_id: &str, generation: u64) {
        if !self.is_current(adapter_id, generation) {
            return;
        }
        let devices = self.devices_of(adapter_id);
        for d in &devices {
            self.state.device_to_adapter.remove(d);
            presence::set_device_link_state(
                &self.state,
                d,
                false,
                &format!("adapter `{adapter_id}` disconnected"),
            )
            .await;
            if let Some(redis) = &self.redis {
                let _ = redis.expire(&keys::adapter_device_key(d), 1).await;
            }
        }
        self.state.adapters.remove(adapter_id);
        tracing::info!(adapter = %adapter_id, devices = devices.len(), "[Registry] adapter unregistered");
    }

    /// 代次校验：旧连接的上行消息必须被丢弃。
    pub fn is_current(&self, adapter_id: &str, generation: u64) -> bool {
        matches!(self.state.generation_of(adapter_id), Some(g) if g == generation)
    }

    pub fn devices_of(&self, adapter_id: &str) -> Vec<String> {
        self.state
            .device_to_adapter
            .iter()
            .filter(|e| e.value() == adapter_id)
            .map(|e| e.key().clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use rustcam_drone_proto::link::service_message::Payload;
    use tokio::sync::mpsc;

    fn registry() -> Registry {
        Registry::new(DroneState::new(DroneConfig::default()), None)
    }

    fn register_msg(adapter_id: &str, devices: Vec<&str>) -> Register {
        Register {
            adapter_id: adapter_id.into(),
            protocol: "mavlink".into(),
            version: "0.1.0".into(),
            devices: devices.into_iter().map(|s| s.to_string()).collect(),
            identities: Vec::new(),
        }
    }

    fn channel() -> (mpsc::Sender<Payload>, mpsc::Receiver<Payload>) {
        mpsc::channel(8)
    }

    #[tokio::test]
    async fn register_assigns_devices_and_creates_shadow() {
        let r = registry();
        let (tx, _rx) = channel();
        let ack = r
            .register(&register_msg("a-1", vec!["drone-001", "drone-002"]), tx)
            .await;
        assert!(ack.accepted);
        assert_eq!(ack.assigned_devices.len(), 2);
        assert_eq!(r.devices_of("a-1").len(), 2);
        assert!(ack.heartbeat_interval_sec > 0);
        let device = r.state.devices.get("drone-001").unwrap();
        assert!(device.online);
        assert_eq!(device.adapter_id, "a-1");
    }

    #[tokio::test]
    async fn re_register_invalidates_old_generation() {
        let r = registry();
        let reg = register_msg("a-1", vec!["drone-001"]);
        let (tx1, _rx1) = channel();
        r.register(&reg, tx1).await;
        let gen1 = r.state.generation_of("a-1").unwrap();
        let (tx2, _rx2) = channel();
        r.register(&reg, tx2).await;
        let gen2 = r.state.generation_of("a-1").unwrap();
        assert!(gen2 > gen1);
        assert!(r.is_current("a-1", gen2));
        assert!(!r.is_current("a-1", gen1));
    }

    #[tokio::test]
    async fn unregister_clears_mapping_and_marks_offline() {
        let r = registry();
        let (tx, _rx) = channel();
        r.register(&register_msg("a-1", vec!["drone-001"]), tx)
            .await;
        let gen = r.state.generation_of("a-1").unwrap();
        r.unregister("a-1", gen).await;
        assert!(r.devices_of("a-1").is_empty());
        assert!(!r.state.devices.get("drone-001").unwrap().online);
        assert!(!r.state.adapters.contains_key("a-1"));
    }

    #[tokio::test]
    async fn device_owned_by_other_adapter_is_not_assigned() {
        let r = registry();
        let (tx1, _rx1) = channel();
        r.register(&register_msg("a-1", vec!["drone-001"]), tx1)
            .await;
        let (tx2, _rx2) = channel();
        let ack = r
            .register(&register_msg("a-2", vec!["drone-001"]), tx2)
            .await;
        assert!(ack.assigned_devices.is_empty());
        assert_eq!(r.devices_of("a-2").len(), 0);
    }

    #[tokio::test]
    async fn same_adapter_reclaims_its_devices() {
        let r = registry();
        let (tx1, _rx1) = channel();
        r.register(&register_msg("a-1", vec!["drone-001"]), tx1)
            .await;
        let (tx2, _rx2) = channel();
        let ack = r
            .register(&register_msg("a-1", vec!["drone-001"]), tx2)
            .await;
        assert_eq!(ack.assigned_devices, vec!["drone-001".to_string()]);
    }

    #[tokio::test]
    async fn unregister_from_stale_generation_does_not_cleanup() {
        let r = registry();
        let reg = register_msg("a-1", vec!["drone-001"]);
        let (tx1, _rx1) = channel();
        r.register(&reg, tx1).await;
        let gen1 = r.state.generation_of("a-1").unwrap();
        let (tx2, _rx2) = channel();
        r.register(&reg, tx2).await;
        r.unregister("a-1", gen1).await;
        assert_eq!(r.devices_of("a-1").len(), 1);
    }

    #[test]
    fn identity_matches_requires_equal_non_empty_when_registered() {
        // 档案已登记：必须相等且上报非空。
        assert!(identity_matches("abc123", "abc123"));
        assert!(!identity_matches("abc123", "abc124"));
        assert!(!identity_matches("abc123", ""));
        // 档案未登记：不做判定，放行。
        assert!(identity_matches("", "abc123"));
        assert!(identity_matches("", ""));
    }

    #[tokio::test]
    async fn register_falls_back_to_reported_devices_without_db() {
        let r = registry();
        let (tx, _rx) = channel();
        let ack = r
            .register(&register_msg("a-1", vec!["drone-001"]), tx)
            .await;
        assert!(ack.accepted);
        assert_eq!(ack.assigned_devices, vec!["drone-001".to_string()]);
        // 无库时 assignments 为空，老 adapter 行为不变。
        assert!(ack.assignments.is_empty());
    }

    #[tokio::test]
    async fn register_rejects_device_when_identity_mismatch() {
        let r = registry();
        r.state.profiles.insert(
            "drone-001".into(),
            crate::state::DroneProfileRecord {
                device_id: "drone-001".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-001", 0)
            },
        );
        let mut reg = register_msg("a-1", vec!["drone-001"]);
        reg.identities = vec![DeviceIdentity {
            device_id: "drone-001".into(),
            unique_id: "observed".into(),
        }];
        let (tx, _rx) = channel();
        let ack = r.register(&reg, tx).await;
        assert!(ack.accepted);
        assert!(
            ack.assigned_devices.is_empty(),
            "唯一标识不一致的设备不应被分配"
        );
    }

    #[tokio::test]
    async fn identity_report_accepts_matching_unique_id() {
        let r = registry();
        r.state.profiles.insert(
            "drone-001".into(),
            crate::state::DroneProfileRecord {
                device_id: "drone-001".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-001", 0)
            },
        );
        let (tx, _rx) = channel();
        r.register(&register_msg("a-1", vec!["drone-001"]), tx)
            .await;
        let results = r
            .apply_identity_report(
                "a-1",
                &[DeviceIdentity {
                    device_id: "drone-001".into(),
                    unique_id: "registered".into(),
                }],
            )
            .await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1, "唯一标识一致应被接受");
        assert_eq!(r.devices_of("a-1").len(), 1);
    }

    #[tokio::test]
    async fn identity_report_revokes_device_on_mismatch() {
        let r = registry();
        r.state.profiles.insert(
            "drone-001".into(),
            crate::state::DroneProfileRecord {
                device_id: "drone-001".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-001", 0)
            },
        );
        let (tx, _rx) = channel();
        r.register(&register_msg("a-1", vec!["drone-001"]), tx)
            .await;
        let results = r
            .apply_identity_report(
                "a-1",
                &[DeviceIdentity {
                    device_id: "drone-001".into(),
                    unique_id: "observed".into(),
                }],
            )
            .await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].1, "唯一标识不一致应被拒绝");
        assert!(r.devices_of("a-1").is_empty(), "不一致的设备应被撤销分配");
        assert!(!r.state.devices.get("drone-001").unwrap().online);
    }
}
