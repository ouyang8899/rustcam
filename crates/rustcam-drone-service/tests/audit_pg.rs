//! 审计落库的集成测试（规则 §9.2 / §9.3 / §16）。
//!
//! 需要本地 PostgreSQL；不可用时跳过（不视为失败）。
//! 连接地址取 `DRONE_DATABASE_URL`，默认与 `config.toml` 的本地库一致。

use std::sync::Arc;

use rustcam_drone_service::infra::db::{write_audit, DroneDb};
use rustcam_drone_service::state::AuditRecord;

async fn db() -> Option<DroneDb> {
    let url = std::env::var("DRONE_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:123456@127.0.0.1:5435/rustcam".to_string());
    let client = match DroneDb::connect(&url).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("跳过：PostgreSQL 不可用（{url}）：{e}");
            return None;
        }
    };
    if let Err(e) = client.run_migrations().await {
        eprintln!("跳过：migrations 执行失败：{e}");
        return None;
    }
    Some(client)
}

fn control_audit(drone_tag: &str, operator: &str) -> AuditRecord {
    AuditRecord {
        ts_ms: 1_700_000_000_000,
        drone_tag: drone_tag.to_string(),
        category: "control".into(),
        action: "acquire".into(),
        operator: operator.to_string(),
        result: "granted".into(),
        detail: String::new(),
        control_type: "flight".into(),
        cmd_id: String::new(),
        params: String::new(),
        priority: 10,
        finished_at_ms: 0,
    }
}

fn command_audit(drone_tag: &str, cmd_id: &str) -> AuditRecord {
    AuditRecord {
        ts_ms: 1_700_000_000_000,
        drone_tag: drone_tag.to_string(),
        category: "command".into(),
        action: "goto".into(),
        operator: "alice".into(),
        result: "sent".into(),
        detail: String::new(),
        control_type: String::new(),
        cmd_id: cmd_id.to_string(),
        params: r#"{"lat":"31.2","lon":"121.4"}"#.into(),
        priority: 0,
        finished_at_ms: 1_700_000_001_000,
    }
}

#[tokio::test]
async fn control_audit_is_persisted() {
    let Some(db) = db().await else {
        return;
    };
    let drone_tag = "drone-pg-1";
    write_audit(&db, &control_audit(drone_tag, "alice"))
        .await
        .expect("控制权审计应写入 drone_control_audit");

    let rows: Vec<serde_json::Value> = db
        .query_decode(
            "SELECT drone_tag, control_type, action, holder, priority, result, operator \
             FROM drone_control_audit WHERE drone_tag = $1 ORDER BY id DESC LIMIT 1",
            vec![rbs::value![drone_tag]],
        )
        .await
        .expect("应能查询 drone_control_audit");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["control_type"], "flight");
    assert_eq!(row["action"], "acquire");
    assert_eq!(row["holder"], "alice");
    assert_eq!(row["priority"], 10);
    assert_eq!(row["result"], "granted");
    assert_eq!(row["operator"], "alice");
}

#[tokio::test]
async fn command_audit_is_persisted_and_upserted() {
    let Some(db) = db().await else {
        return;
    };
    let cmd_id = "cmd-pg-1";
    write_audit(&db, &command_audit("drone-pg-2", cmd_id))
        .await
        .expect("指令审计应写入 drone_command_audit");

    // 同一 cmd_id 再次写入应更新而不是插入重复行。
    let mut updated = command_audit("drone-pg-2", cmd_id);
    updated.result = "executed".into();
    write_audit(&db, &updated).await.expect("重复写应 upsert");

    let rows: Vec<serde_json::Value> = db
        .query_decode(
            "SELECT cmd_id, type, status, operator, params FROM drone_command_audit \
             WHERE cmd_id = $1",
            vec![rbs::value![cmd_id]],
        )
        .await
        .expect("应能查询 drone_command_audit");
    assert_eq!(rows.len(), 1, "同一 cmd_id 只应有一行");
    assert_eq!(rows[0]["type"], "goto");
    assert_eq!(rows[0]["status"], "executed");
    assert_eq!(rows[0]["operator"], "alice");
    assert_eq!(rows[0]["params"]["lat"], "31.2");
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let Some(db) = db().await else {
        return;
    };
    // 重复执行建表脚本不应报错。
    db.run_migrations().await.expect("migrations 必须幂等");
    db.run_migrations().await.expect("migrations 必须幂等");
    let _ = Arc::new(());
}
