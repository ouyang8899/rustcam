//! adapter 配置落库的集成测试。
//!
//! 需要本地 PostgreSQL；不可用时跳过（不视为失败）。
//! 连接地址取 `DRONE_DATABASE_URL`，默认与 `config.toml` 的本地库一致。

use rustcam_drone_service::infra::db::{AdapterRow, DroneDb};

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

fn row(adapter_id: &str) -> AdapterRow {
    AdapterRow {
        adapter_id: adapter_id.to_string(),
        name: "东场链路".into(),
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
    }
}

#[tokio::test]
async fn adapter_row_round_trips_config_fields() {
    let Some(db) = db().await else {
        return;
    };
    let id = "adapter-pg-round-trip";
    db.upsert_adapter(&row(id)).await.expect("adapter 应写入");

    let loaded = db.load_adapter(id).await.expect("应能读回").expect("行应存在");
    assert_eq!(loaded.name, "东场链路");
    assert_eq!(loaded.protocol, "mavlink");
    assert_eq!(loaded.bind_addr, "0.0.0.0");
    assert_eq!(loaded.port_start, 14540);
    assert_eq!(loaded.port_end, 14560);
    assert!(loaded.enabled);
}

#[tokio::test]
async fn disabled_adapter_is_hidden_from_default_list_but_kept_in_row() {
    let Some(db) = db().await else {
        return;
    };
    let id = "adapter-pg-soft-delete";
    db.upsert_adapter(&row(id)).await.expect("adapter 应写入");

    // 停用（软删除）：默认列表里不再出现，但行仍在，可恢复。
    let mut disabled = row(id);
    disabled.enabled = false;
    db.upsert_adapter(&disabled).await.expect("停用应写入");

    let active = db.load_adapters(false).await.expect("应能列启用中的 adapter");
    assert!(
        !active.iter().any(|a| a.adapter_id == id),
        "停用的 adapter 不应出现在默认列表"
    );

    let all = db.load_adapters(true).await.expect("应能列出全部 adapter");
    let found = all
        .iter()
        .find(|a| a.adapter_id == id)
        .expect("停用的 adapter 应仍在库里");
    assert!(!found.enabled);

    // 行保留意味着可以恢复：重新启用后重新出现在默认列表。
    let mut restored = row(id);
    restored.enabled = true;
    db.upsert_adapter(&restored).await.expect("恢复应写入");
    let active = db.load_adapters(false).await.expect("应能列启用中的 adapter");
    assert!(
        active.iter().any(|a| a.adapter_id == id),
        "恢复后的 adapter 应回到默认列表"
    );
}
