use std::sync::Arc;
use std::time::Duration;

use rbatis::RBatis;
use rbdc::db::Driver;
use rbdc_pg::PgDriver;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sql_mappers::{
    self, AlertRow, FenceRow, RouteRow, TelemetryPointRow,
};
use crate::state::{AuditRecord, FenceRecord, RouteRecord, WaypointRecord};

/// drone-service 默认连接池大小。
const DEFAULT_MAX_CONNECTIONS: u32 = 32;

/// 一台无人机的机载链路参数：端点、签名密钥、唯一标识。
///
/// 数据来源是 `drone_device` 表：无人机与其链路同表，
/// 不另起 link 表，也不引入 adapter 侧表。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceLinkRow {
    pub drone_tag: String,
    pub unique_id: String,
    pub mavlink_addr: String,
    /// 签名密钥（`v1:<hex>` 密文）；未配置密钥的机按"不需要签名"处理
    pub signing_secret: String,
    pub signing_allow_unsigned: bool,
    /// MAVLink 协议版本：1 / 2（版本 1 没有签名字段）
    pub mavlink_version: i32,
    /// 飞控 system id；adapter 侧据此填 target_system
    pub mavlink_sysid: i32,
    /// 飞控 component id；adapter 侧据此填 target_component
    pub mavlink_compid: i32,
    pub adapter_id: String,
    /// 链路身份的传输族（tcp / udp / serial），由地址归一化而来
    pub link_family: String,
    /// 链路端点：端口号（tcp/udp）或串口设备名
    pub link_endpoint: String,
    /// 协议中立的飞控系统号（唯一性判定用）
    pub fc_sysid: i32,
}

/// 机载链路实体（adapter）的配置与运行状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterRow {
    pub adapter_id: String,
    pub name: String,
    /// 适配协议（mavlink / 厂商 SDK 等）
    pub protocol: String,
    /// 监听地址
    pub bind_addr: String,
    /// 可用端口段起点
    pub port_start: i32,
    /// 可用端口段终点
    pub port_end: i32,
    pub autopilot: String,
    pub signing_secret: String,
    pub signing_allow_unsigned: bool,
    pub reconnect_min_secs: i32,
    pub reconnect_max_secs: i32,
    pub online: bool,
    /// 启停状态：false = 已停用（软删除），保留配置与绑定关系以便回滚。
    pub enabled: bool,
}


/// drone-service 自有的 PG 连接池；只操作 drone_* 表。
#[derive(Clone)]
pub struct DroneDb {
    rb: RBatis,
}

impl DroneDb {
    pub async fn connect(url: &str) -> anyhow::Result<DroneDb> {
        let rb = RBatis::new();
        let driver = PgDriver {};
        let mut option = driver.default_option();
        option
            .set_uri(url)
            .map_err(|e| anyhow::anyhow!("DB option failed: {}", e))?;

        let pool = crate::infra::bb8_pool::Bb8Pool::new(
            Arc::new(Box::new(driver)),
            Arc::new(option),
            DEFAULT_MAX_CONNECTIONS,
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| anyhow::anyhow!("DB pool init failed: {}", e))?;

        // 连接失败必须暴露出来：静默吞掉会让服务在没有表的情况下继续跑。
        rb.init_pool(pool)
            .map_err(|e| anyhow::anyhow!("DB pool init failed: {}", e))?;
        Ok(DroneDb { rb })
    }

    /// 执行内置 migrations.sql（幂等）。
    ///
    /// DDL 失败会返回错误：表不存在时功能会直接报错（规则 §16）。
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        let sql = include_str!("../../migrations.sql");
        for stmt in split_statements(sql) {
            if let Err(e) = self.rb.exec(&stmt, vec![]).await {
                tracing::error!(error = %e, sql = %stmt, "[DroneDb] migration statement failed");
                return Err(e.into());
            }
        }
        self.check_legacy_schema().await
    }

    /// 检查旧结构残留并告警。
    ///
    /// 旧库的航线/围栏关联存在 JSONB 数组列里，媒体域遗留列还在 `drone_device` 上；
    /// 这些数据需要 `sql/migrate_drone_tag_normalize.py` 搬运，服务自身只读关联表。
    pub async fn check_legacy_schema(&self) -> anyhow::Result<()> {
        const LEGACY: &[(&str, &str)] = &[
            ("drone_route", "device_ids"),
            ("drone_route", "drone_tags"),
            ("drone_fence", "device_ids"),
            ("drone_fence", "drone_tags"),
            ("drone_device", "device_tag"),
            ("drone_device", "channel_tag"),
        ];
        for (table, column) in LEGACY {
            let rows = sql_mappers::drone_device_legacy_column_exists(&self.rb, table, column).await?;
            if !rows.is_empty() {
                tracing::warn!(
                    table = *table,
                    column = *column,
                    "[DroneDb] 检测到旧结构残留：关联数据需先执行 sql/migrate_drone_tag_normalize.py"
                );
            }
        }
        Ok(())
    }

    pub async fn exec(&self, sql: &str) -> anyhow::Result<()> {
        self.rb.exec(sql, vec![]).await?;
        Ok(())
    }

    /// 带参数执行 SQL（`$1` 占位符）。
    pub async fn exec_with_args(&self, sql: &str, args: Vec<rbs::Value>) -> anyhow::Result<()> {
        self.rb.exec(sql, args).await?;
        Ok(())
    }

    /// 带参数查询，返回行数组。
    pub async fn query_decode(
        &self,
        sql: &str,
        args: Vec<rbs::Value>,
    ) -> anyhow::Result<Vec<Value>> {
        let rows = self.rb.exec_decode(sql, args).await?;
        Ok(rows)
    }

    /// 按 adapter_id 取出该 adapter 名下的无人机链路参数。
    pub async fn load_device_links(&self, adapter_id: &str) -> anyhow::Result<Vec<DeviceLinkRow>> {
        Ok(sql_mappers::drone_device_link_select_by_adapter(&self.rb, adapter_id).await?)
    }

    /// 按 drone_tag 取单台无人机的链路参数。
    pub async fn load_device_link(&self, drone_tag: &str) -> anyhow::Result<Option<DeviceLinkRow>> {
        let rows = sql_mappers::drone_device_link_select_by_tag(&self.rb, drone_tag).await?;
        Ok(rows.into_iter().next())
    }

    /// 写入（或更新）一台无人机的链路参数。
    ///
    /// 只写链路相关列；`unique_id` / `mavlink_addr` / `signing_secret` 为空时清空对应列。
    pub async fn upsert_device_link(&self, row: &DeviceLinkRow) -> anyhow::Result<()> {
        sql_mappers::drone_device_link_upsert(
            &self.rb,
            &row.drone_tag,
            &row.adapter_id,
            &row.unique_id,
            &row.mavlink_addr,
            &row.signing_secret,
            row.signing_allow_unsigned,
            row.mavlink_version,
            row.mavlink_sysid,
            row.mavlink_compid,
            &row.link_family,
            &row.link_endpoint,
            row.fc_sysid,
        )
        .await?;
        Ok(())
    }

    /// 同一 adapter 下「协议族 + 端点 + 飞控系统号」是否已被别的机占用。
    ///
    /// 返回占用者的 drone_tag（None = 未占用）。
    pub async fn link_identity_owner(
        &self,
        adapter_id: &str,
        link_family: &str,
        link_endpoint: &str,
        fc_sysid: i32,
        drone_tag: &str,
    ) -> anyhow::Result<Option<String>> {
        let rows = sql_mappers::drone_device_link_identity_taken(
            &self.rb,
            adapter_id,
            link_family,
            link_endpoint,
            fc_sysid,
            drone_tag,
        )
        .await?;
        Ok(rows.into_iter().next().map(|r| r.drone_tag))
    }

    /// 读取一个机载链路实体（adapter）的配置。
    pub async fn load_adapter(&self, adapter_id: &str) -> anyhow::Result<Option<AdapterRow>> {
        let rows = sql_mappers::drone_adapter_select_by_id(&self.rb, adapter_id).await?;
        Ok(rows.into_iter().next())
    }

    /// 列出全部机载链路实体。
    ///
    /// `include_disabled = false` 时只返回启用中的（软删除的默认不出现）。
    pub async fn load_adapters(&self, include_disabled: bool) -> anyhow::Result<Vec<AdapterRow>> {
        Ok(sql_mappers::drone_adapter_select_all(&self.rb, include_disabled).await?)
    }

    /// 写入（或更新）一个机载链路实体。
    pub async fn upsert_adapter(&self, row: &AdapterRow) -> anyhow::Result<()> {
        sql_mappers::drone_adapter_upsert(
            &self.rb,
            &row.adapter_id,
            &row.name,
            &row.protocol,
            &row.bind_addr,
            row.port_start,
            row.port_end,
            &row.autopilot,
            &row.signing_secret,
            row.signing_allow_unsigned,
            row.reconnect_min_secs,
            row.reconnect_max_secs,
            row.online,
            row.enabled,
        )
        .await?;
        Ok(())
    }

    /// 载入全部航线（含适用无人机与航点）。
    pub async fn load_routes(&self) -> anyhow::Result<Vec<RouteRecord>> {
        let rows: Vec<RouteRow> = sql_mappers::drone_route_select_all(&self.rb).await?;
        let mut routes = Vec::with_capacity(rows.len());
        for row in rows {
            if row.id.is_empty() {
                continue;
            }
            let drone_tags: Vec<String> =
                serde_json::from_str(&row.drone_tags).unwrap_or_default();
            let waypoints: Vec<WaypointRecord> =
                serde_json::from_str(&row.waypoints).unwrap_or_default();
            routes.push(RouteRecord {
                id: row.id,
                name: row.name,
                drone_tags,
                waypoints,
                enabled: row.enabled,
                updated_ms: row.updated_ms,
                end_action: row.end_action,
            });
        }
        Ok(routes)
    }

    /// 航线入库（按 id 幂等 upsert）；适用无人机写 `drone_route_device` 关联表。
    pub async fn upsert_route(&self, route: &RouteRecord) -> anyhow::Result<()> {
        let waypoints = serde_json::to_string(&route.waypoints)?;
        sql_mappers::drone_route_upsert(
            &self.rb,
            &route.id,
            &route.name,
            &waypoints,
            route.enabled,
            &route.end_action,
            route.updated_ms,
        )
        .await?;
        self.replace_route_devices(route.id.as_str(), &route.drone_tags)
            .await
    }

    /// 重写某条航线的适用无人机关联。
    ///
    /// 单语句完成：只删掉不再需要的关联，其余关联靠 `ON CONFLICT` 保留。
    /// （同一语句里"先全删再全插"会丢数据——DELETE 的效果对 INSERT 不可见，
    /// 被删掉的行仍会触发唯一键冲突而插不回来。）
    pub async fn replace_route_devices(
        &self,
        route_id: &str,
        drone_tags: &[String],
    ) -> anyhow::Result<()> {
        let tags = serde_json::to_string(drone_tags)?;
        sql_mappers::drone_route_devices_replace(&self.rb, route_id, &tags).await?;
        Ok(())
    }

    pub async fn delete_route(&self, id: &str) -> anyhow::Result<()> {
        sql_mappers::drone_route_delete(&self.rb, id).await?;
        Ok(())
    }

    /// 载入全部围栏（含适用无人机）。
    pub async fn load_fences(&self) -> anyhow::Result<Vec<FenceRecord>> {
        let rows: Vec<FenceRow> = sql_mappers::drone_fence_select_all(&self.rb).await?;
        let mut fences = Vec::with_capacity(rows.len());
        for row in rows {
            if row.id.is_empty() {
                continue;
            }
            let drone_tags: Vec<String> =
                serde_json::from_str(&row.drone_tags).unwrap_or_default();
            fences.push(FenceRecord {
                id: row.id,
                name: row.name,
                rule_type: row.rule_type,
                shape: row.shape,
                geojson: row.geojson,
                center_lat: row.center_lat,
                center_lon: row.center_lon,
                radius_m: row.radius_m,
                min_alt: row.min_alt,
                max_alt: row.max_alt,
                drone_tags,
                enabled: row.enabled,
                version: row.version,
                updated_ms: row.updated_ms,
            });
        }
        Ok(fences)
    }

    /// 围栏入库（按 id 幂等 upsert）；适用无人机写 `drone_fence_device` 关联表。
    pub async fn upsert_fence(&self, fence: &FenceRecord) -> anyhow::Result<()> {
        sql_mappers::drone_fence_upsert(
            &self.rb,
            &fence.id,
            &fence.name,
            &fence.rule_type,
            &fence.shape,
            &fence.geojson,
            fence.center_lat,
            fence.center_lon,
            fence.radius_m,
            fence.min_alt,
            fence.max_alt,
            fence.enabled,
            fence.version,
            fence.updated_ms,
        )
        .await?;
        self.replace_fence_devices(fence.id.as_str(), &fence.drone_tags)
            .await
    }

    /// 重写某个围栏的适用无人机关联。
    ///
    /// 单语句完成：只删掉不再需要的关联，其余关联靠 `ON CONFLICT` 保留
    /// （原因同 `replace_route_devices`）。
    pub async fn replace_fence_devices(
        &self,
        fence_id: &str,
        drone_tags: &[String],
    ) -> anyhow::Result<()> {
        let tags = serde_json::to_string(drone_tags)?;
        sql_mappers::drone_fence_devices_replace(&self.rb, fence_id, &tags).await?;
        Ok(())
    }

    pub async fn delete_fence(&self, id: &str) -> anyhow::Result<()> {
        sql_mappers::drone_fence_delete(&self.rb, id).await?;
        Ok(())
    }

    /// 批量写入遥测（TimescaleDB 超表）。
    ///
    /// 一条语句 + 一个 jsonb 参数：避免逐行往返，也避免绑定几十个数组参数。
    pub async fn insert_telemetry_batch(
        &self,
        rows: &[crate::archive::TelemetryRow],
    ) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let payload = serde_json::to_string(rows)?;
        sql_mappers::drone_telemetry_insert_batch(&self.rb, &payload).await?;
        Ok(())
    }

    /// 写入一条告警。
    pub async fn insert_alert(&self, alert: &crate::state::AlertRecord) -> anyhow::Result<()> {
        sql_mappers::drone_alert_insert(
            &self.rb,
            alert.ts_ms,
            &alert.drone_tag,
            &alert.rule,
            &alert.level,
            &alert.message,
            alert.lat,
            alert.lon,
            alert.alt,
        )
        .await?;
        Ok(())
    }

    /// 读取时间窗内的遥测（等距降采样，保留首尾）。
    pub async fn load_telemetry(
        &self,
        drone_tag: &str,
        start_ms: i64,
        end_ms: i64,
        limit: usize,
    ) -> anyhow::Result<Vec<rustcam_drone_core::telemetry::Telemetry>> {
        let limit = limit.clamp(1, 100_000) as i64;
        let rows: Vec<TelemetryPointRow> =
            sql_mappers::drone_telemetry_select_window(&self.rb, drone_tag, start_ms, end_ms, limit)
                .await?;
        Ok(rows.into_iter().map(telemetry_from_row).collect())
    }

    /// 读取时间窗内的告警。
    pub async fn load_alerts(
        &self,
        drone_tag: &str,
        start_ms: i64,
        end_ms: i64,
        limit: usize,
    ) -> anyhow::Result<Vec<crate::state::AlertRecord>> {
        let limit = limit.clamp(1, 1000) as i64;
        let rows: Vec<AlertRow> =
            sql_mappers::drone_alert_select_window(&self.rb, drone_tag, start_ms, end_ms, limit)
                .await?;
        Ok(rows
            .into_iter()
            .map(|r| crate::state::AlertRecord {
                drone_tag: r.drone_tag,
                rule: r.rule,
                level: r.level,
                message: r.message,
                ts_ms: r.ts_ms,
                lat: r.lat,
                lon: r.lon,
                alt: r.alt,
            })
            .collect())
    }
}

/// 从查询结果行构造遥测。
fn telemetry_from_row(r: TelemetryPointRow) -> rustcam_drone_core::telemetry::Telemetry {
    rustcam_drone_core::telemetry::Telemetry {
        drone_tag: r.drone_tag,
        ts_ms: r.ts_ms,
        lat: r.lat,
        lon: r.lon,
        alt_amsl: r.alt_amsl,
        rel_alt: r.rel_alt,
        roll: r.roll,
        pitch: r.pitch,
        yaw: r.yaw,
        vx: r.vx,
        vy: r.vy,
        vz: r.vz,
        qx: r.qx,
        qy: r.qy,
        qz: r.qz,
        qw: r.qw,
        battery_pct: r.battery_pct,
        voltage: r.voltage,
        link_quality: r.link_quality as u32,
        seq: r.seq as u32,
        satellites: r.satellites as u32,
        gps_fix: r.gps_fix as u32,
        hdop: r.hdop,
        armed: r.armed,
        mode: r.mode,
        ..Default::default()
    }
}

/// 按类别把审计写入 drone_* 表（规则 §9.2 / §9.3）。
///
/// - `control` → `drone_control_audit`
/// - `command` → `drone_command_audit`
/// - 其它类别 → 不落库（避免污染权威表）
pub async fn write_audit(db: &DroneDb, audit: &AuditRecord) -> anyhow::Result<()> {
    match audit.category.as_str() {
        "control" => {
            sql_mappers::drone_control_audit_insert(
                &db.rb,
                audit.ts_ms,
                &audit.drone_tag,
                &audit.control_type,
                &audit.action,
                &audit.operator,
                audit.priority,
                &audit.result,
                &audit.operator,
            )
            .await?;
            Ok(())
        }
        "command" => {
            let params = if audit.params.is_empty() {
                "{}"
            } else {
                audit.params.as_str()
            };
            sql_mappers::drone_command_audit_upsert(
                &db.rb,
                audit.ts_ms,
                &audit.drone_tag,
                &audit.cmd_id,
                &audit.action,
                params,
                &audit.result,
                &audit.operator,
                audit.finished_at_ms,
            )
            .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 去掉注释行后按 `;` 拆分语句。
fn split_statements(sql: &str) -> Vec<String> {
    let body: String = sql
        .lines()
        .filter(|l| !l.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    body.split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent_sql() {
        let sql = include_str!("../../migrations.sql");
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_device"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_control_audit"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_command_audit"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_route_device"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_fence_device"));
        assert!(
            !sql.contains("CREATE TABLE drone_device "),
            "建表必须带 IF NOT EXISTS"
        );
    }

    #[test]
    fn schema_drops_media_domain_columns_and_jsonb_lists() {
        let sql = include_str!("../../migrations.sql");
        // 媒体域命名（device_tag / channel_tag）不得再出现在无人机表里。
        assert!(!sql.contains("device_tag    text"));
        assert!(!sql.contains("channel_tag   text"));
        assert!(!sql.contains("idx_drone_device_device_tag"));
        assert!(!sql.contains("idx_drone_device_channel_tag"));
        // 关联不再塞在 JSONB 数组列里。
        assert!(!sql.contains("drone_tags  jsonb"));
        assert!(!sql.contains("device_ids"));
        // 废弃表 drone_geofence 被清理。
        assert!(sql.contains("DROP TABLE IF EXISTS drone_geofence"));
        assert!(!sql.contains("CREATE TABLE IF NOT EXISTS drone_geofence"));
    }

    #[test]
    fn link_tables_use_drone_tag() {
        let sql = include_str!("../../migrations.sql");
        assert!(sql.contains("route_id   text NOT NULL REFERENCES drone_route (id) ON DELETE CASCADE"));
        assert!(sql.contains("fence_id   text NOT NULL REFERENCES drone_fence (id) ON DELETE CASCADE"));
        assert!(sql.contains("PRIMARY KEY (route_id, drone_tag)"));
        assert!(sql.contains("PRIMARY KEY (fence_id, drone_tag)"));
    }

    #[test]
    fn statements_are_split_and_comments_removed() {
        let stmts =
            split_statements("-- comment\nCREATE TABLE a (x int);\nCREATE INDEX i ON a (x);");
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].starts_with("CREATE TABLE a"));
        assert!(stmts[1].starts_with("CREATE INDEX i"));
        assert!(!stmts.iter().any(|s| s.contains("--")));
    }

    #[test]
    fn migration_adds_link_identity_columns() {
        let sql = include_str!("../../migrations.sql");
        assert!(sql.contains("unique_id"));
        assert!(sql.contains("mavlink_addr"));
        assert!(sql.contains("signing_secret"));
        assert!(sql.contains("signing_allow_unsigned"));
        assert!(sql.contains("mavlink_version"));
        assert!(sql.contains("mavlink_sysid"));
        assert!(sql.contains("mavlink_compid"));
        assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_drone_device_adapter_id"));
        // 机载链路实体表与协议中立的链路身份列。
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_adapter"));
        assert!(sql.contains("port_start"));
        assert!(sql.contains("port_end"));
        assert!(sql.contains("uq_drone_device_link_identity"));
    }

    #[test]
    fn schema_uses_drone_tag_and_no_device_id() {
        let sql = include_str!("../../migrations.sql");
        assert!(sql.contains("drone_tag"));
        assert!(!sql.contains("device_id"));
        // 链路绑定唯一性按「协议族 + 端点 + 飞控系统号」判定。
        assert!(sql.contains("uq_drone_device_link_identity"));
        assert!(sql.contains("link_family"));
        assert!(sql.contains("link_endpoint"));
        assert!(sql.contains("fc_sysid"));
    }

    #[test]
    fn device_link_row_serializes_identity_fields() {
        let row = DeviceLinkRow {
            drone_tag: "drone-001".into(),
            unique_id: "0123456789abcdef".into(),
            mavlink_addr: "udpin:10.88.0.2:14540".into(),
            signing_secret: String::new(),
            signing_allow_unsigned: true,
            mavlink_version: 2,
            mavlink_sysid: 1,
            mavlink_compid: 1,
            adapter_id: "adapter-1".into(),
            link_family: "udp".into(),
            link_endpoint: "14540".into(),
            fc_sysid: 1,
        };
        let json = serde_json::to_value(&row).expect("serialize");
        assert_eq!(json["drone_tag"], "drone-001");
        assert_eq!(json["unique_id"], "0123456789abcdef");
        assert_eq!(json["signing_allow_unsigned"], true);
        assert_eq!(json["mavlink_version"], 2);
        assert_eq!(json["mavlink_sysid"], 1);
        assert_eq!(json["mavlink_compid"], 1);
        assert_eq!(json["link_family"], "udp");
        assert_eq!(json["link_endpoint"], "14540");
        assert_eq!(json["fc_sysid"], 1);
    }
}
