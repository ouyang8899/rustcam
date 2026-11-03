use std::sync::Arc;
use std::time::Duration;
use crate::adapter::media_server::MediaServerAdapter;
use crate::application::media_server_service::MediaServerService;
use crate::application::StreamRecoveryService;
use crate::domain::traits::CacheStore;
use crate::infrastructure::RedisStore;
use crate::infrastructure::cluster::ClusterManager;

pub const SERVER_KEY_PREFIX: &str = "media_servers";
pub const ONLINE_SET_KEY: &str = "media_servers:online";

/// Number of consecutive probe failures a media server tolerates before it is
/// declared offline. A single timeout must not take a server out of the
/// cluster.
pub const DEFAULT_FAILURE_THRESHOLD: u32 = 3;

/// TTL of the heartbeat of a media server. It has to outlive the probe cycle
/// (interval + probe duration), otherwise a healthy server disappears from
/// Redis while the next probe is still running.
pub fn heartbeat_ttl_secs(interval_secs: u64) -> u64 {
    (interval_secs * 4).max(120)
}

/// Whether a server with the given number of consecutive failures has to be
/// declared offline.
pub fn should_mark_offline(consecutive_failures: u32, threshold: u32) -> bool {
    consecutive_failures >= threshold
}

/// Counts consecutive probe failures per media server.
#[derive(Default)]
pub struct FailureTracker {
    consecutive: std::collections::HashMap<String, u32>,
}

impl FailureTracker {
    pub fn record_success(&mut self, tag: &str) {
        self.reset(tag);
    }

    /// Forgets the failure streak of a server.
    pub fn reset(&mut self, tag: &str) {
        self.consecutive.remove(tag);
    }

    /// Records a failed probe and returns the number of consecutive failures.
    pub fn record_failure(&mut self, tag: &str) -> u32 {
        let counter = self.consecutive.entry(tag.to_string()).or_insert(0);
        *counter += 1;
        *counter
    }

    pub fn failures(&self, tag: &str) -> u32 {
        self.consecutive.get(tag).copied().unwrap_or(0)
    }

    /// Drops the counters of servers that no longer exist.
    pub fn retain_known(&mut self, known_tags: &std::collections::HashSet<String>) {
        self.consecutive.retain(|tag, _| known_tags.contains(tag));
    }
}

pub struct HealthMonitor {
    redis: Arc<RedisStore>,
    cluster: Arc<ClusterManager>,
    media_server_service: Arc<MediaServerService>,
    stream_recovery_service: Option<Arc<StreamRecoveryService>>,
    interval_secs: u64,
    heartbeat_ttl_secs: u64,
    failure_threshold: u32,
    failures: std::sync::Mutex<FailureTracker>,
}

impl HealthMonitor {
    pub fn new(
        redis: Arc<RedisStore>,
        cluster: Arc<ClusterManager>,
        media_server_service: Arc<MediaServerService>,
        stream_recovery_service: Option<Arc<StreamRecoveryService>>,
        interval_secs: u64,
    ) -> Self {
        Self {
            redis,
            cluster,
            media_server_service,
            stream_recovery_service,
            interval_secs,
            heartbeat_ttl_secs: heartbeat_ttl_secs(interval_secs),
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            failures: std::sync::Mutex::new(FailureTracker::default()),
        }
    }

    pub async fn start(self: Arc<Self>) {
        let interval = Duration::from_secs(self.interval_secs);

        tokio::spawn(async move {
            let cluster_tags = self.cluster.get_all_server_tags().clone();
            for  config_tag in &cluster_tags {
                if let Some(ref svc) = self.stream_recovery_service {
                    svc.mark_streams_recovering_for_media_server(&config_tag).await;
                    svc.restart_streams_for_media_server(&config_tag).await;
                }
            }
            loop {
                let start = std::time::Instant::now();
                self.sync_servers().await;
                let _elapsed = start.elapsed();
                tokio::time::sleep(interval).await;
            }
        });
    }

    async fn sync_servers(&self) {
        let servers = self.media_server_service.list();
        let cluster_tags = self.cluster.get_all_server_tags();
        let cluster_tags_set: std::collections::HashSet<String> =
            cluster_tags.iter().cloned().collect();

        let known_tags: std::collections::HashSet<String> =
            servers.iter().map(|s| s.server_tag.clone()).collect();
        if let Ok(mut tracker) = self.failures.lock() {
            tracker.retain_known(&known_tags);
        }

        // Probe every server in parallel: a single unreachable server must not
        // delay the probes of the healthy ones.
        let probes = servers.iter().map(|server| {
            let adapter = self
                .cluster
                .get_server(&server.server_tag)
                .map(|a| Arc::clone(&a))
                .or_else(|| self.create_adapter(server));

            async move {
                let tag = server.server_tag.clone();
                let enabled = server.enabled;
                let config = server.to_config();
                match adapter {
                    Some(adapter) => Some((tag, enabled, config, adapter.get_status().await)),
                    None => None,
                }
            }
        });

        for outcome in futures::future::join_all(probes).await {
            let Some((tag, enabled, config, result)) = outcome else {
                continue;
            };

            match result {
                Ok(status) => {
                    self.record_probe_success(&tag);
                    let in_cluster = cluster_tags_set.contains(&tag);

                    if enabled {
                        if !in_cluster {

                            self.cluster.register_server(config);

                            if let Some(ref svc) = self.stream_recovery_service {
                                svc.restart_streams_for_media_server(&tag).await;
                            }
                        }
                        self.update_server_status(&tag, &status).await;
                    } else {

                        if in_cluster {
                            self.cluster.unregister_server(&tag);
                            tracing::warn!("[HealthMonitor] Server {} is disabled, unregistered from cluster", tag);
                        }
                        self.mark_server_offline(&tag).await;
                    }
                }
                Err(e) => {
                    let failures = self.record_probe_failure(&tag);
                    tracing::warn!(
                        "[HealthMonitor] Server {} get_status failed ({} of {} consecutive failures): {}",
                        tag,
                        failures,
                        self.failure_threshold,
                        e
                    );

                    if !should_mark_offline(failures, self.failure_threshold) {
                        // Keep the server in the cluster until it failed often
                        // enough, a single timeout is not enough to judge.
                        continue;
                    }

                    if cluster_tags_set.contains(&tag) {
                        self.cluster.unregister_server(&tag);
                        tracing::warn!("[HealthMonitor] Server {} unregistered from cluster (offline)", tag);
                    }
                    self.mark_server_offline(&tag).await;
                    // The server is offline now, the next failures start a new
                    // streak.
                    self.reset_failures(&tag);
                }
            }
        }
    }

    fn record_probe_success(&self, tag: &str) {
        if let Ok(mut tracker) = self.failures.lock() {
            tracker.record_success(tag);
        }
    }

    fn reset_failures(&self, tag: &str) {
        if let Ok(mut tracker) = self.failures.lock() {
            tracker.reset(tag);
        }
    }

    fn record_probe_failure(&self, tag: &str) -> u32 {
        match self.failures.lock() {
            Ok(mut tracker) => tracker.record_failure(tag),
            Err(_) => self.failure_threshold,
        }
    }

    fn create_adapter(&self, server: &crate::application::media_server_service::ManagedServer) -> Option<Arc<dyn MediaServerAdapter>> {
        let config = server.to_config();
        match config.server_type.as_str() {
            "zlmediakit" => Some(Arc::new(crate::adapter::media_server::zlmediakit::ZlMediaKitAdapter::new(config))),
            "srs" => Some(Arc::new(crate::adapter::media_server::srs::SrsAdapter::new(config))),
            "xiu" => Some(Arc::new(crate::adapter::media_server::xiu::XiuAdapter::new(config))),
            _ => None,
        }
    }

    async fn update_server_status(&self, tag: &str, status: &crate::adapter::media_server::ServerStatus) {
        let key = format!("{}:{}", SERVER_KEY_PREFIX, tag);
        let fields = [
            ("name", status.name.as_str()),
            ("type", status.server_type.as_str()),
            ("online", "true"),
            ("session_count", &status.session_count.to_string()),
            ("cpu_usage", &format!("{:.1}", status.cpu_usage)),
            ("memory_usage", &format!("{:.1}", status.memory_usage)),
            ("bandwidth_in", &status.bandwidth_in.to_string()),
            ("bandwidth_out", &status.bandwidth_out.to_string()),
            ("last_heartbeat", &chrono::Utc::now().timestamp().to_string()),
        ];

        if let Err(e) = self.redis.hmset(&key, &fields).await {
            tracing::error!("[HealthMonitor] Failed to update {}: {}", tag, e);
            return;
        }
        let _ = self.redis.expire(&key, self.heartbeat_ttl_secs).await;
        let _ = self.redis.sadd(ONLINE_SET_KEY, tag).await;
    }

    async fn mark_server_offline(&self, name: &str) {
        let key = format!("{}:{}", SERVER_KEY_PREFIX, name);
        let _ = self.redis.hset(&key, "online", "false").await;
        let _ = self.redis.srem(ONLINE_SET_KEY, name).await;
        tracing::warn!("[HealthMonitor] Server {} marked offline", name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_ttl_outlives_the_probe_cycle() {
        // The probe cycle is interval + probe duration; the heartbeat of a
        // healthy server must not expire in between.
        assert!(heartbeat_ttl_secs(30) > 30);
        assert_eq!(heartbeat_ttl_secs(30), 120);
        assert_eq!(heartbeat_ttl_secs(60), 240);
    }

    #[test]
    fn test_server_is_marked_offline_only_after_consecutive_failures() {
        let mut tracker = FailureTracker::default();

        assert_eq!(tracker.record_failure("server-a"), 1);
        assert!(!should_mark_offline(
            tracker.failures("server-a"),
            DEFAULT_FAILURE_THRESHOLD
        ));

        assert_eq!(tracker.record_failure("server-a"), 2);
        assert!(!should_mark_offline(
            tracker.failures("server-a"),
            DEFAULT_FAILURE_THRESHOLD
        ));

        assert_eq!(tracker.record_failure("server-a"), 3);
        assert!(should_mark_offline(
            tracker.failures("server-a"),
            DEFAULT_FAILURE_THRESHOLD
        ));

        tracker.record_success("server-a");
        assert_eq!(tracker.failures("server-a"), 0);
        assert!(!should_mark_offline(
            tracker.failures("server-a"),
            DEFAULT_FAILURE_THRESHOLD
        ));
    }

    #[test]
    fn test_failures_are_counted_per_server() {
        let mut tracker = FailureTracker::default();
        tracker.record_failure("server-a");
        tracker.record_failure("server-b");
        tracker.record_failure("server-b");

        assert_eq!(tracker.failures("server-a"), 1);
        assert_eq!(tracker.failures("server-b"), 2);
        assert_eq!(tracker.failures("unknown"), 0);
    }

    #[test]
    fn test_failure_streak_is_reset_when_the_server_went_offline() {
        let mut tracker = FailureTracker::default();
        tracker.record_failure("server-a");
        tracker.record_failure("server-a");

        tracker.reset("server-a");

        assert_eq!(
            tracker.failures("server-a"),
            0,
            "a new streak must start after the server was declared offline"
        );
    }
}
