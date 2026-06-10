use std::sync::Arc;
use std::time::Duration;
use crate::adapter::media_server::ServerStatus;
use crate::domain::traits::CacheStore;
use crate::infrastructure::RedisStore;

pub const SERVER_KEY_PREFIX: &str = "media_servers";
pub const ONLINE_SET_KEY: &str = "media_servers:online";
pub const HEARTBEAT_TTL_SECS: u64 = 30;

pub struct HealthMonitor {
    redis: Arc<RedisStore>,
    interval_secs: u64,
}

impl HealthMonitor {
    pub fn new(redis: Arc<RedisStore>, interval_secs: u64) -> Self {
        Self { redis, interval_secs }
    }

    pub async fn start(self: Arc<Self>, server_names: Vec<String>) {
        let interval = Duration::from_secs(self.interval_secs);

        tokio::spawn(async move {
            tracing::info!("[HealthMonitor] Started (interval: {}s)", self.interval_secs);

            loop {
                tokio::time::sleep(interval).await;
                self.sync_servers_to_redis(&server_names).await;
            }
        });
    }

    async fn sync_servers_to_redis(&self, server_names: &[String]) {
        for name in server_names {
            if let Ok(Some(status)) = self.redis.get_server_status(name).await {
                self.update_server_status(&status).await;
            } else {
                self.mark_server_offline(name).await;
            }
        }
    }

    pub async fn update_server_status(&self, status: &ServerStatus) {
        let key = format!("{}:{}", SERVER_KEY_PREFIX, status.name);
        let fields = [
            ("name", status.name.as_str()),
            ("type", status.server_type.as_str()),
            ("online", if status.online { "true" } else { "false" }),
            ("session_count", &status.session_count.to_string()),
            ("cpu_usage", &format!("{:.1}", status.cpu_usage)),
            ("memory_usage", &format!("{:.1}", status.memory_usage)),
            ("bandwidth_in", &status.bandwidth_in.to_string()),
            ("bandwidth_out", &status.bandwidth_out.to_string()),
            ("last_heartbeat", &chrono::Utc::now().timestamp().to_string()),
        ];

        if let Err(e) = self.redis.hmset(&key, &fields).await {
            tracing::error!("[HealthMonitor] Failed to update {}: {}", status.name, e);
            return;
        }

        let _ = self.redis.expire(&key, HEARTBEAT_TTL_SECS).await;

        if status.online {
            let _ = self.redis.sadd(ONLINE_SET_KEY, &status.name).await;
        }

        tracing::debug!("[HealthMonitor] Updated server {} (online: {})", status.name, status.online);
    }

    pub async fn mark_server_offline(&self, name: &str) {
        let key = format!("{}:{}", SERVER_KEY_PREFIX, name);
        let _ = self.redis.hset(&key, "online", "false").await;
        let _ = self.redis.srem(ONLINE_SET_KEY, name).await;
        tracing::warn!("[HealthMonitor] Server {} marked offline", name);
    }
}