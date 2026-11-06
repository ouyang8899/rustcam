//! 控制权 Redis Lua 仲裁的集成测试（规则 §6.7 / §14.5）。
//!
//! 需要本地 Redis；不可用时跳过（不视为失败）。
//! 连接地址取 `DRONE_REDIS_URL`，默认 `redis://127.0.0.1:6379`。

use std::sync::Arc;

use rustcam_drone_service::config::DroneConfig;
use rustcam_drone_service::control;
use rustcam_drone_service::infra::redis::DroneRedis;
use rustcam_drone_service::lease::LeaseScripts;
use rustcam_drone_service::state::DroneState;

async fn state_with_redis() -> Option<DroneState> {
    let url =
        std::env::var("DRONE_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let client = match DroneRedis::connect(&url).await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("跳过：Redis 不可用（{url}）：{e}");
            return None;
        }
    };
    if let Err(e) = LeaseScripts::new().preload(&client).await {
        eprintln!("跳过：Lua 脚本预热失败：{e}");
        return None;
    }
    Some(
        DroneState::new(DroneConfig {
            require_control: true,
            ..Default::default()
        })
        .with_redis(Some(client)),
    )
}

async fn cleanup(client: &Arc<DroneRedis>, device_id: &str) {
    let mut conn = client.connection();
    let _: redis::RedisResult<i64> = redis::cmd("DEL")
        .arg(format!("control:{device_id}:flight"))
        .arg(format!("control:{device_id}:gimbal"))
        .query_async(&mut conn)
        .await;
}

#[tokio::test]
async fn redis_lua_arbitration_follows_rule_semantics() {
    let Some(state) = state_with_redis().await else {
        return;
    };
    let redis = state.redis.clone().expect("redis 已连接");
    cleanup(&redis, "drone-it-1").await;

    // 1) 无人持有时 acquire 成功
    let a = control::acquire(&state, "drone-it-1", "flight", "alice", 10, 30)
        .await
        .unwrap();
    assert!(a.granted, "无人持有时 acquire 应成功");

    // 2) 有人持有且未过期时 acquire 失败（不抢占），即使优先级更高
    let b = control::acquire(&state, "drone-it-1", "flight", "bob", 99, 30)
        .await
        .unwrap();
    assert!(!b.granted, "acquire 不得抢占");
    assert_eq!(b.lease.holder, "alice");

    // 3) 同优先级 preempt 失败
    let same = control::preempt(&state, "drone-it-1", "flight", "bob", 10, 30)
        .await
        .unwrap();
    assert!(!same.granted, "同优先级不得抢占");

    // 4) 更高优先级 preempt 成功，并返回被抢占者
    let higher = control::preempt(&state, "drone-it-1", "flight", "bob", 20, 30)
        .await
        .unwrap();
    assert!(higher.granted, "更高优先级应抢占成功");
    assert_eq!(higher.preempted, "alice", "必须返回被抢占者");
    assert_eq!(higher.lease.holder, "bob");

    // 5) 非持有人续约失败
    assert!(
        control::renew(&state, "drone-it-1", "flight", "alice", 30)
            .await
            .is_err(),
        "非持有人不得续约"
    );
    assert!(control::renew(&state, "drone-it-1", "flight", "bob", 30)
        .await
        .is_ok());

    // 6) 非持有人释放失败；持有人释放成功
    assert!(
        control::release(&state, "drone-it-1", "flight", "alice", false)
            .await
            .is_err(),
        "非持有人不得释放"
    );
    assert!(
        control::release(&state, "drone-it-1", "flight", "bob", false)
            .await
            .is_ok()
    );

    // 7) Redis 里的键名遵循规则 §8.2
    let mut conn = redis.connection();
    let exists: i64 = redis::cmd("EXISTS")
        .arg("control:drone-it-1:flight")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(exists, 0, "释放后控制权键应被删除");

    cleanup(&redis, "drone-it-1").await;
}

#[tokio::test]
async fn redis_lease_expires_and_can_be_reacquired() {
    let Some(state) = state_with_redis().await else {
        return;
    };
    let redis = state.redis.clone().expect("redis 已连接");
    cleanup(&redis, "drone-it-2").await;

    let a = control::acquire(&state, "drone-it-2", "flight", "alice", 10, 5)
        .await
        .unwrap();
    assert!(a.granted);

    // 租约 5s 最短，TTL 到期后原持有人不再持有
    let mut conn = redis.connection();
    let ttl: i64 = redis::cmd("PTTL")
        .arg("control:drone-it-2:flight")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(ttl > 0, "控制权键必须带 TTL");
    assert!(ttl <= 5_000, "TTL 与租约一致，实际 {ttl}ms");

    cleanup(&redis, "drone-it-2").await;
}
