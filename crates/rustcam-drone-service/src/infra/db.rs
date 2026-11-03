use rbatis::RBatis;
use rbdc_pg::PgDriver;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct DroneDeviceRow {
    pub device_id: String,
    pub protocol: String,
    pub adapter_id: Option<String>,
    pub online: bool,
    pub device_tag: Option<String>,
    pub channel_tag: Option<String>,
}

/// drone-service 自有的 PG 连接池；只操作 drone_* 表。
#[derive(Clone)]
pub struct DroneDb {
    rb: RBatis,
}

impl DroneDb {
    pub async fn connect(url: &str) -> anyhow::Result<DroneDb> {
        let rb = RBatis::new();
        let _init_res = rb.init(PgDriver {}, url);
        Ok(DroneDb { rb })
    }

    /// 执行内置 migrations.sql（幂等）。
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        let sql = include_str!("../../migrations.sql");
        for stmt in split_statements(sql) {
            if let Err(e) = self.rb.exec(&stmt, vec![]).await {
                tracing::warn!(error = %e, sql = %stmt, "[DroneDb] migration statement failed");
            }
        }
        Ok(())
    }

    pub async fn exec(&self, sql: &str) -> anyhow::Result<()> {
        self.rb.exec(sql, vec![]).await?;
        Ok(())
    }

    pub async fn query_one_device(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Option<DroneDeviceRow>> {
        let rows: Vec<Value> = self
            .rb
            .exec_decode(
                "SELECT device_id, protocol, adapter_id, online, device_tag, channel_tag \
                 FROM drone_device WHERE device_id = $1 LIMIT 1",
                vec![rbs::value![device_id]],
            )
            .await?;
        Ok(rows.into_iter().next().map(|r| DroneDeviceRow {
            device_id: r["device_id"].as_str().unwrap_or_default().to_string(),
            protocol: r["protocol"].as_str().unwrap_or_default().to_string(),
            adapter_id: r["adapter_id"].as_str().map(|s| s.to_string()),
            online: r["online"].as_bool().unwrap_or(false),
            device_tag: r["device_tag"].as_str().map(|s| s.to_string()),
            channel_tag: r["channel_tag"].as_str().map(|s| s.to_string()),
        }))
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
        let stmts = split_statements("-- comment\nCREATE TABLE a (x int);\nCREATE INDEX i ON a (x);");
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].starts_with("CREATE TABLE a"));
        assert!(stmts[1].starts_with("CREATE INDEX i"));
        assert!(!stmts.iter().any(|s| s.contains("--")));
    }
}
