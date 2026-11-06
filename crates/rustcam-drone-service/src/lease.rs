//! 控制权租约的 Redis Lua 仲裁（规则 §6.7）。
//!
//! 仲裁必须「读-判断-写一次完成」，因此全部走 Lua 脚本：
//! 运行时用 `EVALSHA`；Redis 重启导致 SHA 失效（`NOSCRIPT`）时，
//! 由 `redis::Script` 自动重新 `SCRIPT LOAD` 并重试一次。
//!
//! 脚本内不取服务器时间：时间戳一律由 Rust 传入，避免多机时钟偏差。

use redis::Script;

use crate::infra::redis::DroneRedis;

pub const ACQUIRE_LUA: &str = include_str!("lua/acquire.lua");
pub const RELEASE_LUA: &str = include_str!("lua/release.lua");
pub const RENEW_LUA: &str = include_str!("lua/renew.lua");
pub const PREEMPT_LUA: &str = include_str!("lua/preempt.lua");

/// 控制权仲裁结果：`granted` 是否成功，`other` 为对方持有人（失败或被抢占者）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arbitration {
    pub granted: bool,
    pub other: String,
}

/// 四个控制权脚本。
#[derive(Clone)]
pub struct LeaseScripts {
    pub acquire: Script,
    pub release: Script,
    pub renew: Script,
    pub preempt: Script,
}

impl Default for LeaseScripts {
    fn default() -> Self {
        Self::new()
    }
}

impl LeaseScripts {
    pub fn new() -> LeaseScripts {
        LeaseScripts {
            acquire: Script::new(ACQUIRE_LUA),
            release: Script::new(RELEASE_LUA),
            renew: Script::new(RENEW_LUA),
            preempt: Script::new(PREEMPT_LUA),
        }
    }

    /// 启动时预热：把脚本装进 Redis，避免首次调用出现 NOSCRIPT。
    pub async fn preload(&self, redis: &DroneRedis) -> anyhow::Result<()> {
        for source in [ACQUIRE_LUA, RELEASE_LUA, RENEW_LUA, PREEMPT_LUA] {
            redis.script_load(source).await?;
        }
        Ok(())
    }

    /// 申请控制权：只在无人持有或已过期时成功，**不抢占**。
    pub async fn acquire(
        &self,
        redis: &DroneRedis,
        key: &str,
        holder: &str,
        priority: i32,
        now_ms: i64,
        lease_ms: i64,
    ) -> anyhow::Result<Arbitration> {
        let mut conn = redis.connection();
        let (granted, other): (i64, String) = self
            .acquire
            .key(key)
            .arg(holder)
            .arg(priority)
            .arg(now_ms)
            .arg(lease_ms)
            .invoke_async(&mut conn)
            .await?;
        Ok(Arbitration {
            granted: granted == 1,
            other,
        })
    }

    /// 抢占控制权：只有优先级严格更高才成功。
    pub async fn preempt(
        &self,
        redis: &DroneRedis,
        key: &str,
        holder: &str,
        priority: i32,
        now_ms: i64,
        lease_ms: i64,
    ) -> anyhow::Result<Arbitration> {
        let mut conn = redis.connection();
        let (granted, other): (i64, String) = self
            .preempt
            .key(key)
            .arg(holder)
            .arg(priority)
            .arg(now_ms)
            .arg(lease_ms)
            .invoke_async(&mut conn)
            .await?;
        Ok(Arbitration {
            granted: granted == 1,
            other,
        })
    }

    /// 续约：只允许当前持有人。
    pub async fn renew(
        &self,
        redis: &DroneRedis,
        key: &str,
        holder: &str,
        now_ms: i64,
        lease_ms: i64,
    ) -> anyhow::Result<bool> {
        let mut conn = redis.connection();
        let renewed: i64 = self
            .renew
            .key(key)
            .arg(holder)
            .arg(now_ms)
            .arg(lease_ms)
            .invoke_async(&mut conn)
            .await?;
        Ok(renewed == 1)
    }

    /// 释放：只允许当前持有人；`force` 时直接删除键。
    pub async fn release(
        &self,
        redis: &DroneRedis,
        key: &str,
        holder: &str,
        force: bool,
    ) -> anyhow::Result<bool> {
        let mut conn = redis.connection();
        if force {
            let _: i64 = redis::cmd("DEL").arg(key).query_async(&mut conn).await?;
            return Ok(true);
        }
        let released: i64 = self
            .release
            .key(key)
            .arg(holder)
            .invoke_async(&mut conn)
            .await?;
        Ok(released == 1)
    }

    /// 读取当前租约字段。
    pub async fn read(&self, redis: &DroneRedis, key: &str) -> anyhow::Result<Option<RedisLease>> {
        let mut conn = redis.connection();
        let fields: Vec<(String, String)> = redis::cmd("HGETALL")
            .arg(key)
            .query_async(&mut conn)
            .await?;
        if fields.is_empty() {
            return Ok(None);
        }
        let map: std::collections::HashMap<String, String> = fields.into_iter().collect();
        Ok(Some(RedisLease {
            holder: map.get("holder").cloned().unwrap_or_default(),
            priority: map
                .get("priority")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0),
            expire_at_ms: map
                .get("expire_at")
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0),
        }))
    }
}

/// Redis 中的租约字段。
#[derive(Debug, Clone)]
pub struct RedisLease {
    pub holder: String,
    pub priority: i32,
    pub expire_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_loaded_from_lua_files() {
        assert!(ACQUIRE_LUA.contains("HMGET"));
        assert!(ACQUIRE_LUA.contains("PEXPIRE"));
        assert!(RELEASE_LUA.contains("HGET"));
        assert!(RENEW_LUA.contains("expire_at"));
        assert!(PREEMPT_LUA.contains("prio > old_prio"));
    }

    #[test]
    fn scripts_do_not_use_server_time() {
        for src in [ACQUIRE_LUA, RELEASE_LUA, RENEW_LUA, PREEMPT_LUA] {
            assert!(
                !src.contains("redis.call('TIME')") && !src.contains("redis.call(\"TIME\")"),
                "脚本内不得使用服务器时间"
            );
        }
    }
}
