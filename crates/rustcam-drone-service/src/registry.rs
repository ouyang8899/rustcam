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
    /// 设备来源：**只**按 `adapter_id` 查 `drone_device`（drone.md §9.1）。
    /// 引导模式已取消——adapter 不再自报设备，平台未绑定则该 adapter 无 assignment。
    pub async fn register(&self, reg: &Register, tx: AdapterTx) -> RegisterAck {
        let link_rows = self.load_link_rows(&reg.adapter_id).await;
        self.register_with_rows(reg, tx, link_rows).await
    }

    /// 按给定的链路行做分配（生产走 `register` 查库；测试直接注入行）。
    async fn register_with_rows(
        &self,
        reg: &Register,
        tx: AdapterTx,
        link_rows: Vec<DeviceLinkRow>,
    ) -> RegisterAck {
        let generation = self.state.next_generation(&reg.adapter_id);
        let now = chrono::Utc::now().timestamp_millis();
        let mut assigned = Vec::new();
        let mut assignments: Vec<DeviceAssignment> = Vec::new();

        let candidates: Vec<String> = if link_rows.is_empty() {
            tracing::warn!(
                adapter = %reg.adapter_id,
                "[Registry] adapter has no device rows in platform, nothing assigned"
            );
            Vec::new()
        } else {
            link_rows
                .iter()
                .map(|r| r.drone_tag.clone())
                .collect::<Vec<String>>()
        };

        for drone_tag in &candidates {
            let owner = self
                .state
                .device_to_adapter
                .get(drone_tag.as_str())
                .map(|v| v.clone());
            if let Some(owner) = owner {
                if owner != reg.adapter_id {
                    tracing::warn!(
                        device = %drone_tag,
                        owner = %owner,
                        adapter = %reg.adapter_id,
                        "[Registry] device is owned by another adapter, skipping"
                    );
                    continue;
                }
            }

            self.state
                .device_to_adapter
                .insert(drone_tag.clone(), reg.adapter_id.clone());
            assigned.push(drone_tag.clone());

            // 下发该机的端点与签名密钥。
            if let Some(row) = link_rows.iter().find(|r| r.drone_tag == *drone_tag) {
                assignments.push(self.build_assignment(row));
            }


            match self.state.devices.get_mut(drone_tag) {
                Some(mut entry) => {
                    entry.adapter_id = reg.adapter_id.clone();
                    entry.protocol = reg.protocol.clone();
                    entry.last_seen_ms = now;
                }
                None => {
                    let mut entry =
                        DeviceEntry::new(drone_tag, &reg.protocol, &reg.adapter_id, now);
                    entry.online = false;
                    self.state.devices.insert(drone_tag.clone(), entry);
                }
            }

            presence::set_device_link_state(
                &self.state,
                drone_tag,
                true,
                &format!("adapter `{}` connected", reg.adapter_id),
            )
            .await;

            if let Some(redis) = &self.redis {
                let key = keys::adapter_device_key(drone_tag);
                match redis.set_nx_ex(&key, &reg.adapter_id, 30).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!(
                            device = %drone_tag,
                            adapter = %reg.adapter_id,
                            "[Registry] redis claim already held (stale key?), continuing"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, device = %drone_tag, "[Registry] redis claim failed");
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
            assigned_drone_tags: assigned,
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
            drone_tag: row.drone_tag.clone(),
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
            let drone_tag = id.drone_tag.trim().to_string();
            let observed = id.unique_id.trim();
            let registered = self
                .state
                .profiles
                .get(drone_tag.as_str())
                .map(|p| p.unique_id.trim().to_string())
                .unwrap_or_default();

            if identity_matches(&registered, observed) {
                if registered.is_empty() {
                    tracing::warn!(
                        device = %drone_tag,
                        adapter = %adapter_id,
                        observed = %observed,
                        "[Registry] device has no registered unique_id, accepted with warning"
                    );
                }
                out.push((drone_tag, true, String::new()));
                continue;
            }

            tracing::warn!(
                device = %drone_tag,
                adapter = %adapter_id,
                registered = %registered,
                observed = %observed,
                "[Registry] unique_id mismatch, revoking assignment"
            );
            self.revoke_device(&drone_tag).await;
            out.push((
                drone_tag,
                false,
                format!("unique_id mismatch: registered `{registered}`, observed `{observed}`"),
            ));
        }
        out
    }

    /// 撤销设备分配：清掉归属、置离线、释放 Redis 独占键。
    async fn revoke_device(&self, drone_tag: &str) {
        self.state.device_to_adapter.remove(drone_tag);
        if let Some(mut entry) = self.state.devices.get_mut(drone_tag) {
            entry.online = false;
            entry.adapter_id = String::new();
        }
        if let Some(redis) = &self.redis {
            // 与 unregister 保持一致：把独占键 TTL 压到 1s，而不是直接删。
            let _ = redis
                .expire(&keys::adapter_device_key(drone_tag), 1)
                .await;
        }
        tracing::warn!(device = %drone_tag, "[Registry] device assignment revoked");
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

    fn register_msg(adapter_id: &str) -> Register {
        Register {
            adapter_id: adapter_id.into(),
            protocol: "mavlink".into(),
            version: "0.1.0".into(),
        }
    }

    /// 模拟平台侧 `drone_device` 行：该 adapter 名下的机体及其链路参数。
    fn link_row(drone_tag: &str, adapter_id: &str) -> DeviceLinkRow {
        DeviceLinkRow {
            drone_tag: drone_tag.into(),
            unique_id: String::new(),
            mavlink_addr: format!("udpin:127.0.0.1:14540:{drone_tag}"),
            signing_secret: String::new(),
            signing_allow_unsigned: true,
            mavlink_version: 2,
            mavlink_sysid: 1,
            mavlink_compid: 1,
            adapter_id: adapter_id.into(),
        }
    }

    fn channel() -> (mpsc::Sender<Payload>, mpsc::Receiver<Payload>) {
        mpsc::channel(8)
    }

    #[tokio::test]
    async fn register_assigns_drone_tags_from_platform_rows() {
        let r = registry();
        let (tx, _rx) = channel();
        let rows = vec![link_row("drone-001", "a-1"), link_row("drone-002", "a-1")];
        let ack = r.register_with_rows(&register_msg("a-1"), tx, rows).await;
        assert!(ack.accepted);
        assert_eq!(ack.assigned_drone_tags.len(), 2);
        assert_eq!(r.devices_of("a-1").len(), 2);
        assert!(ack.heartbeat_interval_sec > 0);
        let device = r.state.devices.get("drone-001").unwrap();
        assert!(device.online);
        assert_eq!(device.adapter_id, "a-1");
        assert_eq!(ack.assignments.len(), 2);
        assert_eq!(ack.assignments[0].drone_tag, "drone-001");
    }

    #[tokio::test]
    async fn register_without_platform_rows_assigns_nothing() {
        // 引导模式已取消：adapter 不再自报设备，平台未绑定则无 assignment。
        let r = registry();
        let (tx, _rx) = channel();
        let ack = r
            .register_with_rows(&register_msg("a-1"), tx, Vec::new())
            .await;
        assert!(ack.accepted);
        assert!(ack.assigned_drone_tags.is_empty());
        assert!(ack.assignments.is_empty());
    }

    #[tokio::test]
    async fn re_register_invalidates_old_generation() {
        let r = registry();
        let reg = register_msg("a-1");
        let rows = vec![link_row("drone-001", "a-1")];
        let (tx1, _rx1) = channel();
        r.register_with_rows(&reg, tx1, rows.clone()).await;
        let gen1 = r.state.generation_of("a-1").unwrap();
        let (tx2, _rx2) = channel();
        r.register_with_rows(&reg, tx2, rows).await;
        let gen2 = r.state.generation_of("a-1").unwrap();
        assert!(gen2 > gen1);
        assert!(r.is_current("a-1", gen2));
        assert!(!r.is_current("a-1", gen1));
    }

    #[tokio::test]
    async fn unregister_clears_mapping_and_marks_offline() {
        let r = registry();
        let (tx, _rx) = channel();
        let rows = vec![link_row("drone-001", "a-1")];
        r.register_with_rows(&register_msg("a-1"), tx, rows).await;
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
        r.register_with_rows(
            &register_msg("a-1"),
            tx1,
            vec![link_row("drone-001", "a-1")],
        )
        .await;
        let (tx2, _rx2) = channel();
        let ack = r
            .register_with_rows(
                &register_msg("a-2"),
                tx2,
                vec![link_row("drone-001", "a-2")],
            )
            .await;
        assert!(ack.assigned_drone_tags.is_empty());
        assert_eq!(r.devices_of("a-2").len(), 0);
    }

    #[tokio::test]
    async fn same_adapter_reclaims_its_devices() {
        let r = registry();
        let rows = vec![link_row("drone-001", "a-1")];
        let (tx1, _rx1) = channel();
        r.register_with_rows(&register_msg("a-1"), tx1, rows.clone())
            .await;
        let (tx2, _rx2) = channel();
        let ack = r
            .register_with_rows(&register_msg("a-1"), tx2, rows)
            .await;
        assert_eq!(ack.assigned_drone_tags, vec!["drone-001".to_string()]);
    }

    #[tokio::test]
    async fn unregister_from_stale_generation_does_not_cleanup() {
        let r = registry();
        let reg = register_msg("a-1");
        let rows = vec![link_row("drone-001", "a-1")];
        let (tx1, _rx1) = channel();
        r.register_with_rows(&reg, tx1, rows.clone()).await;
        let gen1 = r.state.generation_of("a-1").unwrap();
        let (tx2, _rx2) = channel();
        r.register_with_rows(&reg, tx2, rows).await;
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
    async fn identity_report_accepts_matching_unique_id() {
        let r = registry();
        r.state.profiles.insert(
            "drone-001".into(),
            crate::state::DroneProfileRecord {
                drone_tag: "drone-001".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-001", 0)
            },
        );
        let (tx, _rx) = channel();
        r.register_with_rows(
            &register_msg("a-1"),
            tx,
            vec![link_row("drone-001", "a-1")],
        )
        .await;
        let results = r
            .apply_identity_report(
                "a-1",
                &[DeviceIdentity {
                    drone_tag: "drone-001".into(),
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
                drone_tag: "drone-001".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-001", 0)
            },
        );
        let (tx, _rx) = channel();
        r.register_with_rows(
            &register_msg("a-1"),
            tx,
            vec![link_row("drone-001", "a-1")],
        )
        .await;
        let results = r
            .apply_identity_report(
                "a-1",
                &[DeviceIdentity {
                    drone_tag: "drone-001".into(),
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
