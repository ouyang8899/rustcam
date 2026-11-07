use rbatis::RBatis;
use rbdc_pg::PgDriver;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::{AuditRecord, FenceRecord, RouteRecord, WaypointRecord};

#[derive(Debug, Clone)]
pub struct DroneDeviceRow {
    pub drone_tag: String,
    pub protocol: String,
    pub adapter_id: Option<String>,
    pub online: bool,
    pub device_tag: Option<String>,
    pub channel_tag: Option<String>,
}

/// 一台无人机的机载链路参数：端点、签名密钥、唯一标识。
///
/// 数据来源是 `drone_device` 表（drone.md §9.1）：无人机与其链路同表，
/// 不另起 link 表，也不引入 adapter 侧表。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceLinkRow {
    pub drone_tag: String,
    pub unique_id: String,
    pub mavlink_addr: String,
    /// 密文（`v1:<hex>`）；为空表示该机不需要签名。
    pub signing_secret: String,
    pub signing_allow_unsigned: bool,
    /// MAVLink 协议版本：1 / 2。版本 1 不能签名（MAVLink 1 帧无签名字段）。
    pub mavlink_version: i32,
    /// 该机的 system id；adapter 用作出站 target_system。
    pub mavlink_sysid: i32,
    /// 该机的 component id；adapter 用作出站 target_component。
    pub mavlink_compid: i32,
    pub adapter_id: String,
}

/// drone-service 自有的 PG 连接池；只操作 drone_* 表。
#[derive(Clone)]
pub struct DroneDb {
    rb: RBatis,
}

impl DroneDb {
    pub async fn connect(url: &str) -> anyhow::Result<DroneDb> {
        let rb = RBatis::new();
        // 连接失败必须暴露出来：静默吞掉会让服务在没有表的情况下继续跑。
        rb.init(PgDriver {}, url)?;
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

    pub async fn query_one_device(
        &self,
        drone_tag: &str,
    ) -> anyhow::Result<Option<DroneDeviceRow>> {
        let rows: Vec<Value> = self
            .rb
            .exec_decode(
                "SELECT drone_tag, protocol, adapter_id, online, device_tag, channel_tag \
                 FROM drone_device WHERE drone_tag = $1 LIMIT 1",
                vec![rbs::value![drone_tag]],
            )
            .await?;
        Ok(rows.into_iter().next().map(|r| DroneDeviceRow {
            drone_tag: r["drone_tag"].as_str().unwrap_or_default().to_string(),
            protocol: r["protocol"].as_str().unwrap_or_default().to_string(),
            adapter_id: r["adapter_id"].as_str().map(|s| s.to_string()),
            online: r["online"].as_bool().unwrap_or(false),
            device_tag: r["device_tag"].as_str().map(|s| s.to_string()),
            channel_tag: r["channel_tag"].as_str().map(|s| s.to_string()),
        }))
    }

    /// 按 adapter_id 取出该 adapter 名下的无人机链路参数。
    pub async fn load_device_links(&self, adapter_id: &str) -> anyhow::Result<Vec<DeviceLinkRow>> {
        let rows = self
            .query_decode(
                "SELECT drone_tag, \
                        COALESCE(unique_id, '') AS unique_id, \
                        COALESCE(mavlink_addr, '') AS mavlink_addr, \
                        COALESCE(signing_secret, '') AS signing_secret, \
                        signing_allow_unsigned, \
                        mavlink_version, \
                        mavlink_sysid, \
                        mavlink_compid, \
                        COALESCE(adapter_id, '') AS adapter_id, \
                        COALESCE(drone_tag, '') AS drone_tag \
                 FROM drone_device WHERE adapter_id = $1 ORDER BY drone_tag",
                vec![rbs::value![adapter_id]],
            )
            .await?;
        Ok(rows.into_iter().map(|r| device_link_from_row(&r)).collect())
    }

    /// 按 drone_tag 取单台无人机的链路参数。
    pub async fn load_device_link(&self, drone_tag: &str) -> anyhow::Result<Option<DeviceLinkRow>> {
        let rows = self
            .query_decode(
                "SELECT drone_tag, \
                        COALESCE(unique_id, '') AS unique_id, \
                        COALESCE(mavlink_addr, '') AS mavlink_addr, \
                        COALESCE(signing_secret, '') AS signing_secret, \
                        signing_allow_unsigned, \
                        mavlink_version, \
                        mavlink_sysid, \
                        mavlink_compid, \
                        COALESCE(adapter_id, '') AS adapter_id, \
                        COALESCE(drone_tag, '') AS drone_tag \
                 FROM drone_device WHERE drone_tag = $1 LIMIT 1",
                vec![rbs::value![drone_tag]],
            )
            .await?;
        Ok(rows.into_iter().next().map(|r| device_link_from_row(&r)))
    }

    /// 写入（或更新）一台无人机的链路参数。
    ///
    /// 只写链路相关列；`unique_id` / `mavlink_addr` / `signing_secret` 为空时清空对应列。
    pub async fn upsert_device_link(&self, row: &DeviceLinkRow) -> anyhow::Result<()> {
        self.exec_with_args(
            "INSERT INTO drone_device \
                 (drone_tag, protocol, adapter_id, unique_id, mavlink_addr, \
                  signing_secret, signing_allow_unsigned, mavlink_version, \
                  mavlink_sysid, mavlink_compid) \
             VALUES ($1, 'mavlink', NULLIF($2, ''), NULLIF($3, ''), NULLIF($4, ''), \
                     NULLIF($5, ''), $6, $7, $8, $9) \
             ON CONFLICT (drone_tag) DO UPDATE SET \
                 adapter_id = EXCLUDED.adapter_id, \
                 unique_id = EXCLUDED.unique_id, \
                 mavlink_addr = EXCLUDED.mavlink_addr, \
                 signing_secret = EXCLUDED.signing_secret, \
                 signing_allow_unsigned = EXCLUDED.signing_allow_unsigned, \
                 mavlink_version = EXCLUDED.mavlink_version, \
                 mavlink_sysid = EXCLUDED.mavlink_sysid, \
                 mavlink_compid = EXCLUDED.mavlink_compid",
            vec![
                rbs::value![row.drone_tag.clone()],
                rbs::value![row.adapter_id.clone()],
                rbs::value![row.unique_id.clone()],
                rbs::value![row.mavlink_addr.clone()],
                rbs::value![row.signing_secret.clone()],
                rbs::value![row.signing_allow_unsigned],
                rbs::value![row.mavlink_version],
                rbs::value![row.mavlink_sysid],
                rbs::value![row.mavlink_compid],
            ],
        )
        .await?;
        Ok(())
    }

    /// 载入全部航线（含适用无人机与航点）。
    pub async fn load_routes(&self) -> anyhow::Result<Vec<RouteRecord>> {
        let rows: Vec<Value> = self
            .query_decode(
                "SELECT id, name, drone_tags::text AS drone_tags, waypoints::text AS waypoints, \
                 enabled, end_action, updated_ms FROM drone_route ORDER BY updated_ms DESC",
                vec![],
            )
            .await?;
        let mut routes = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row["id"].as_str().unwrap_or_default().to_string();
            if id.is_empty() {
                continue;
            }
            let drone_tags: Vec<String> =
                serde_json::from_str(row["drone_tags"].as_str().unwrap_or("[]"))
                    .unwrap_or_default();
            let waypoints: Vec<WaypointRecord> =
                serde_json::from_str(row["waypoints"].as_str().unwrap_or("[]"))
                    .unwrap_or_default();
            routes.push(RouteRecord {
                id,
                name: row["name"].as_str().unwrap_or_default().to_string(),
                drone_tag: if drone_tags.len() == 1 {
                    drone_tags[0].clone()
                } else {
                    String::new()
                },
                drone_tags,
                waypoints,
                enabled: row["enabled"].as_bool().unwrap_or(true),
                updated_ms: row["updated_ms"].as_i64().unwrap_or(0),
                end_action: row["end_action"].as_str().unwrap_or("hold").to_string(),
            });
        }
        Ok(routes)
    }

    /// 航线入库（按 id 幂等 upsert）。
    pub async fn upsert_route(&self, route: &RouteRecord) -> anyhow::Result<()> {
        // 兼容旧数据：只有 drone_tag 的航线，入库时转成 drone_tags。
        let effective_ids: Vec<String> = if route.drone_tags.is_empty()
            && !route.drone_tag.trim().is_empty()
        {
            vec![route.drone_tag.clone()]
        } else {
            route.drone_tags.clone()
        };
        let drone_tags = serde_json::to_string(&effective_ids)?;
        let waypoints = serde_json::to_string(&route.waypoints)?;
        self.exec_with_args(
            "INSERT INTO drone_route \
             (id, name, drone_tags, waypoints, enabled, end_action, updated_ms, updated_at) \
             VALUES ($1, $2, $3::jsonb, $4::jsonb, $5, $6, $7, now()) \
             ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, \
             drone_tags = EXCLUDED.drone_tags, waypoints = EXCLUDED.waypoints, \
             enabled = EXCLUDED.enabled, end_action = EXCLUDED.end_action, \
             updated_ms = EXCLUDED.updated_ms, updated_at = now()",
            vec![
                rbs::value![route.id.as_str()],
                rbs::value![route.name.as_str()],
                rbs::value![drone_tags.as_str()],
                rbs::value![waypoints.as_str()],
                rbs::value![route.enabled],
                rbs::value![route.end_action.as_str()],
                rbs::value![route.updated_ms],
            ],
        )
        .await
    }

    pub async fn delete_route(&self, id: &str) -> anyhow::Result<()> {
        self.exec_with_args(
            "DELETE FROM drone_route WHERE id = $1",
            vec![rbs::value![id]],
        )
        .await
    }

    /// 载入全部围栏（含适用无人机）。
    pub async fn load_fences(&self) -> anyhow::Result<Vec<FenceRecord>> {
        let rows: Vec<Value> = self
            .query_decode(
                "SELECT id, name, type, shape, geojson, center_lat, center_lon, radius_m, \
                 min_alt, max_alt, drone_tags::text AS drone_tags, enabled, version, updated_ms \
                 FROM drone_fence ORDER BY updated_ms DESC",
                vec![],
            )
            .await?;
        let mut fences = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row["id"].as_str().unwrap_or_default().to_string();
            if id.is_empty() {
                continue;
            }
            let drone_tags: Vec<String> =
                serde_json::from_str(row["drone_tags"].as_str().unwrap_or("[]"))
                    .unwrap_or_default();
            fences.push(FenceRecord {
                id,
                name: row["name"].as_str().unwrap_or_default().to_string(),
                rule_type: row["type"].as_str().unwrap_or("no_fly").to_string(),
                shape: row["shape"].as_str().unwrap_or("polygon").to_string(),
                geojson: row["geojson"].as_str().unwrap_or_default().to_string(),
                center_lat: row["center_lat"].as_f64().unwrap_or(0.0),
                center_lon: row["center_lon"].as_f64().unwrap_or(0.0),
                radius_m: row["radius_m"].as_f64().unwrap_or(0.0),
                min_alt: row["min_alt"].as_f64().unwrap_or(0.0),
                max_alt: row["max_alt"].as_f64().unwrap_or(120.0),
                drone_tag: if drone_tags.len() == 1 {
                    drone_tags[0].clone()
                } else {
                    String::new()
                },
                drone_tags,
                enabled: row["enabled"].as_bool().unwrap_or(true),
                version: row["version"].as_i64().unwrap_or(1) as i32,
                updated_ms: row["updated_ms"].as_i64().unwrap_or(0),
            });
        }
        Ok(fences)
    }

    /// 围栏入库（按 id 幂等 upsert）。
    pub async fn upsert_fence(&self, fence: &FenceRecord) -> anyhow::Result<()> {
        // 兼容旧数据：只有 drone_tag 的围栏，入库时转成 drone_tags。
        let effective_ids: Vec<String> = if fence.drone_tags.is_empty()
            && !fence.drone_tag.trim().is_empty()
        {
            vec![fence.drone_tag.clone()]
        } else {
            fence.drone_tags.clone()
        };
        let drone_tags = serde_json::to_string(&effective_ids)?;
        self.exec_with_args(
            "INSERT INTO drone_fence \
             (id, name, type, shape, geojson, center_lat, center_lon, radius_m, min_alt, max_alt, \
              drone_tags, enabled, version, updated_ms, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12, $13, $14, now()) \
             ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, type = EXCLUDED.type, \
             shape = EXCLUDED.shape, geojson = EXCLUDED.geojson, center_lat = EXCLUDED.center_lat, \
             center_lon = EXCLUDED.center_lon, radius_m = EXCLUDED.radius_m, \
             min_alt = EXCLUDED.min_alt, max_alt = EXCLUDED.max_alt, \
             drone_tags = EXCLUDED.drone_tags, enabled = EXCLUDED.enabled, \
             version = EXCLUDED.version, updated_ms = EXCLUDED.updated_ms, updated_at = now()",
            vec![
                rbs::value![fence.id.as_str()],
                rbs::value![fence.name.as_str()],
                rbs::value![fence.rule_type.as_str()],
                rbs::value![fence.shape.as_str()],
                rbs::value![fence.geojson.as_str()],
                rbs::value![fence.center_lat],
                rbs::value![fence.center_lon],
                rbs::value![fence.radius_m],
                rbs::value![fence.min_alt],
                rbs::value![fence.max_alt],
                rbs::value![drone_tags.as_str()],
                rbs::value![fence.enabled],
                rbs::value![fence.version],
                rbs::value![fence.updated_ms],
            ],
        )
        .await
    }

    pub async fn delete_fence(&self, id: &str) -> anyhow::Result<()> {
        self.exec_with_args(
            "DELETE FROM drone_fence WHERE id = $1",
            vec![rbs::value![id]],
        )
        .await
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
            db.exec_with_args(
                "INSERT INTO drone_control_audit \
                 (ts, drone_tag, control_type, action, holder, priority, result, operator) \
                 VALUES (to_timestamp($1::double precision / 1000.0), $2, $3, $4, $5, $6, $7, $8)",
                vec![
                    rbs::value![audit.ts_ms],
                    rbs::value![audit.drone_tag.as_str()],
                    rbs::value![audit.control_type.as_str()],
                    rbs::value![audit.action.as_str()],
                    rbs::value![audit.operator.as_str()],
                    rbs::value![audit.priority],
                    rbs::value![audit.result.as_str()],
                ],
            )
            .await?;
            Ok(())
        }
        "command" => {
            db.exec_with_args(
                "INSERT INTO drone_command_audit \
                 (ts, drone_tag, cmd_id, type, params, status, operator, finished_at) \
                 VALUES (to_timestamp($1::double precision / 1000.0), $2, $3, $4, $5::jsonb, $6, $7, \
                 CASE WHEN $8::bigint > 0 THEN to_timestamp($8::double precision / 1000.0) ELSE NULL END) \
                 ON CONFLICT (cmd_id) DO UPDATE SET status = EXCLUDED.status, \
                 finished_at = EXCLUDED.finished_at",
                vec![
                    rbs::value![audit.ts_ms],
                    rbs::value![audit.drone_tag.as_str()],
                    rbs::value![audit.cmd_id.as_str()],
                    rbs::value![audit.action.as_str()],
                    rbs::value![if audit.params.is_empty() { "{}" } else { audit.params.as_str() }],
                    rbs::value![audit.result.as_str()],
                    rbs::value![audit.operator.as_str()],
                    rbs::value![audit.finished_at_ms],
                ],
            )
            .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 从查询结果行构造链路参数。
fn device_link_from_row(r: &Value) -> DeviceLinkRow {
    DeviceLinkRow {
        drone_tag: r["drone_tag"].as_str().unwrap_or_default().to_string(),
        unique_id: r["unique_id"].as_str().unwrap_or_default().to_string(),
        mavlink_addr: r["mavlink_addr"].as_str().unwrap_or_default().to_string(),
        signing_secret: r["signing_secret"].as_str().unwrap_or_default().to_string(),
        signing_allow_unsigned: r["signing_allow_unsigned"].as_bool().unwrap_or(true),
        mavlink_version: r["mavlink_version"].as_i64().unwrap_or(2) as i32,
        mavlink_sysid: r["mavlink_sysid"].as_i64().unwrap_or(1) as i32,
        mavlink_compid: r["mavlink_compid"].as_i64().unwrap_or(1) as i32,
        adapter_id: r["adapter_id"].as_str().unwrap_or_default().to_string(),
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
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_geofence"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_control_audit"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS drone_command_audit"));
        assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_drone_device_device_tag"));
        assert!(
            !sql.contains("CREATE TABLE drone_device "),
            "建表必须带 IF NOT EXISTS"
        );
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
        assert!(sql.contains("uq_drone_device_adapter_sysid"));
    }

    #[test]
    fn schema_uses_drone_tag_and_no_device_id() {
        let sql = include_str!("../../migrations.sql");
        assert!(sql.contains("drone_tag"));
        assert!(!sql.contains("device_id"));
        assert!(sql.contains("uq_drone_device_adapter_sysid"));
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
        };
        let json = serde_json::to_value(&row).expect("serialize");
        assert_eq!(json["drone_tag"], "drone-001");
        assert_eq!(json["unique_id"], "0123456789abcdef");
        assert_eq!(json["signing_allow_unsigned"], true);
        assert_eq!(json["mavlink_version"], 2);
        assert_eq!(json["mavlink_sysid"], 1);
        assert_eq!(json["mavlink_compid"], 1);
    }
}
