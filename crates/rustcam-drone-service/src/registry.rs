use std::sync::atomic::AtomicI64;
use std::sync::Arc;

use rustcam_drone_core::keys;
use rustcam_drone_core::metrics::names;
use rustcam_drone_proto::link::{
    service_message, AssignmentsUpdated, DeviceAssignment, DeviceIdentity, Register, RegisterAck,
};

use crate::infra::db::DeviceLinkRow;
use crate::infra::redis::DroneRedis;
use crate::presence;
use crate::state::{AdapterEntry, AdapterTx, DeviceEntry, DroneState};

/// 唯一标识判定规则（唯一定义处）。
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
    /// 设备来源：**只**按 `adapter_id` 查 `drone_device`。
    /// 引导模式已取消——adapter 不再自报设备，平台未绑定则该 adapter 无 assignment。
    pub async fn register(&self, reg: &Register, tx: AdapterTx) -> RegisterAck {
        let link_rows = self.load_link_rows(&reg.adapter_id).await;
        self.cache_adapter_bind_addr(&reg.adapter_id).await;
        self.register_with_rows(reg, tx, link_rows).await
    }

    /// 从 `drone_adapter` 表读监听地址并缓存，供拼装端点时使用。
    async fn cache_adapter_bind_addr(&self, adapter_id: &str) {
        let Some(db) = self.state.db.clone() else {
            return;
        };
        if let Ok(Some(row)) = db.load_adapter(adapter_id).await {
            self.state.set_adapter_bind_addr(adapter_id, &row.bind_addr);
        }
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

        self.touch_adapter_row(&reg.adapter_id, true).await;

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

    /// 由链路行组装分配（纯函数，便于单测）。
    ///
    /// 归属以 DB 的 `drone_device.adapter_id` 为准，内存映射同步更新，
    /// 避免内存与库不一致导致归属漂移。
    fn assignments_from_rows(&self, rows: Vec<DeviceLinkRow>) -> Vec<DeviceAssignment> {
        let mut assignments = Vec::with_capacity(rows.len());
        for row in rows {
            let adapter_id = row.adapter_id.trim().to_string();
            if !adapter_id.is_empty() {
                self.state
                    .device_to_adapter
                    .insert(row.drone_tag.clone(), adapter_id);
            }
            assignments.push(self.build_assignment(&row));
        }
        assignments
    }

    /// 重算某 adapter 名下的全部分配（保存后下发用）。
    pub async fn resync_assignments(&self, adapter_id: &str) -> Vec<DeviceAssignment> {
        self.assignments_from_rows(self.load_link_rows(adapter_id).await)
    }

    /// 把最新分配下发给指定 adapter；adapter 未连接或通道满时返回 false。
    ///
    /// 消息是**全量快照**：adapter 与本地生效配置做差量，漏收一条后
    /// 下一次全量即可自愈，服务端不必维护"发了什么"的状态。
    pub fn push_assignments(
        &self,
        adapter_id: &str,
        assignments: Vec<DeviceAssignment>,
    ) -> bool {
        let Some(tx) = self.state.adapter_tx(adapter_id) else {
            tracing::warn!(
                adapter = %adapter_id,
                "[Registry] adapter not connected; assignments not pushed"
            );
            return false;
        };
        let revision = self.state.next_adapter_revision(adapter_id);
        let payload = service_message::Payload::AssignmentsUpdated(AssignmentsUpdated {
            assignments,
            revision,
        });
        match tx.try_send(payload) {
            Ok(()) => {
                tracing::info!(
                    adapter = %adapter_id,
                    revision,
                    "[Registry] assignments pushed"
                );
                true
            }
            Err(e) => {
                tracing::warn!(
                    adapter = %adapter_id,
                    error = %e,
                    "[Registry] push assignments failed"
                );
                false
            }
        }
    }

    /// 重算并下发某 adapter 的最新分配。
    pub async fn resync_and_push(&self, adapter_id: &str) -> bool {
        let assignments = self.resync_assignments(adapter_id).await;
        self.push_assignments(adapter_id, assignments)
    }

    /// 记录 adapter 的在线状态（机载链路实体的运行状态）。
    async fn touch_adapter_row(&self, adapter_id: &str, online: bool) {
        let Some(db) = self.state.db.clone() else {
            return;
        };
        let existing = match db.load_adapter(adapter_id).await {
            Ok(Some(row)) => row,
            Ok(None) => crate::infra::db::AdapterRow {
                adapter_id: adapter_id.to_string(),
                name: String::new(),
                protocol: "mavlink".into(),
                bind_addr: "0.0.0.0".into(),
                port_start: 14540,
                port_end: 14560,
                autopilot: "auto".into(),
                signing_secret: String::new(),
                signing_allow_unsigned: true,
                reconnect_min_secs: 1,
                reconnect_max_secs: 30,
                online: false,
                enabled: true,
            },
            Err(e) => {
                tracing::warn!(error = %e, adapter = %adapter_id, "[Registry] load adapter row failed");
                return;
            }
        };
        self.state
            .set_adapter_bind_addr(adapter_id, &existing.bind_addr);
        let row = crate::infra::db::AdapterRow {
            online,
            ..existing
        };
        if let Err(e) = db.upsert_adapter(&row).await {
            tracing::warn!(error = %e, adapter = %adapter_id, "[Registry] persist adapter row failed");
        }
    }

    /// 构造下发给 adapter 的分配信息（端点 + 密钥 + 版本 + 寻址地址）。
    ///
    /// 端点优先用无人机自己登记的地址；没登记时用 adapter 的监听地址 + 该机
    /// 的端点（端口/串口）拼出来——端口写在 adapter 上，每机只登记身份。
    fn build_assignment(&self, row: &DeviceLinkRow) -> DeviceAssignment {
        let mavlink_addr = if row.mavlink_addr.trim().is_empty() {
            self.compose_endpoint(row)
        } else {
            row.mavlink_addr.clone()
        };
        DeviceAssignment {
            drone_tag: row.drone_tag.clone(),
            mavlink_addr,
            signing_secret: self.decrypt_secret(&row.signing_secret),
            allow_unsigned: row.signing_allow_unsigned,
            mavlink_version: row.mavlink_version,
            mavlink_sysid: if row.fc_sysid > 0 {
                row.fc_sysid
            } else {
                row.mavlink_sysid
            },
            mavlink_compid: row.mavlink_compid,
        }
    }

    /// 用 adapter 的监听地址与该机的端点拼出完整链路地址。
    fn compose_endpoint(&self, row: &DeviceLinkRow) -> String {
        let endpoint = if !row.link_endpoint.is_empty() {
            row.link_endpoint.clone()
        } else {
            match crate::link_identity::parse_endpoint(&row.mavlink_addr) {
                Some(ep) => ep.endpoint,
                None => String::new(),
            }
        };
        if endpoint.is_empty() {
            return String::new();
        }
        let bind_addr = self
            .state
            .adapter_bind_addr(&row.adapter_id)
            .unwrap_or_else(|| "0.0.0.0".to_string());
        match crate::link_identity::parse_endpoint(&format!("udpin:{bind_addr}:{endpoint}")) {
            Some(_) => format!("udpin:{bind_addr}:{endpoint}"),
            None => String::new(),
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
    /// 判定规则同 `identity_matches`；不一致的设备立即撤销分配。
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
        // 同步数据库里的在线标记，避免列表把已断开的 adapter 长期显示为“已接入”。
        self.touch_adapter_row(adapter_id, false).await;
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
            link_family: "udp".into(),
            link_endpoint: "14540".into(),
            fc_sysid: 1,
        }
    }


    fn channel() -> (mpsc::Sender<Payload>, mpsc::Receiver<Payload>) {
        mpsc::channel(8)
    }

    /// 由链路行组装分配：归属映射同步 + 端点/sysid 透传。
    #[tokio::test]
    async fn assignments_from_rows_builds_assignments_and_syncs_owner() {
        let r = registry();
        let rows = vec![
            link_row("drone-001", "a-1"),
            link_row("drone-002", "a-1"),
        ];

        let assignments = r.assignments_from_rows(rows);

        assert_eq!(assignments.len(), 2);
        assert_eq!(assignments[0].drone_tag, "drone-001");
        assert_eq!(assignments[0].mavlink_sysid, 1);
        assert_eq!(assignments[1].drone_tag, "drone-002");
        // 归属以 DB 的 adapter_id 为准，内存映射同步更新
        assert_eq!(
            r.state
                .device_to_adapter
                .get("drone-001")
                .map(|v| v.to_string()),
            Some("a-1".to_string())
        );
    }

    /// 修订号按 adapter 单调自增。
    #[test]
    fn adapter_revision_increases_per_adapter() {
        let state = DroneState::new(DroneConfig::default());
        assert_eq!(state.next_adapter_revision("a-1"), 1);
        assert_eq!(state.next_adapter_revision("a-1"), 2);
        assert_eq!(state.next_adapter_revision("a-2"), 1);
    }

    /// 下发分配：adapter 在线时能收到 AssignmentsUpdated。
    #[tokio::test]
    async fn push_assignments_reaches_connected_adapter() {
        let r = registry();
        let (tx, mut rx) = channel();
        r.state.adapters.insert(
            "a-1".to_string(),
            crate::state::AdapterEntry {
                tx,
                generation: 1,
                protocol: "mavlink".into(),
                connected_at_ms: 0,
                last_heartbeat_ms: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0)),
            },
        );

        let pushed = r.push_assignments(
            "a-1",
            vec![DeviceAssignment {
                drone_tag: "drone-001".into(),
                mavlink_addr: "udpin:127.0.0.1:14540".into(),
                mavlink_sysid: 1,
                mavlink_compid: 1,
                ..Default::default()
            }],
        );

        assert!(pushed, "adapter 在线时应下发成功");
        match rx.try_recv().expect("应收到 AssignmentsUpdated") {
            Payload::AssignmentsUpdated(update) => {
                assert_eq!(update.assignments.len(), 1);
                assert_eq!(update.assignments[0].drone_tag, "drone-001");
                assert_eq!(update.revision, 1);
            }
            other => panic!("expected AssignmentsUpdated, got {other:?}"),
        }
    }

    /// adapter 未连接时下发失败但不报错（等它重连时再注册拿最新分配）。
    #[tokio::test]
    async fn push_assignments_skips_disconnected_adapter() {
        let r = registry();
        assert!(!r.push_assignments("a-missing", Vec::new()));
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
