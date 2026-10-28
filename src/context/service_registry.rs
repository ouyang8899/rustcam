use std::sync::Arc;
use crate::config::AppConfig;
use crate::context::{InfraContext, MediaContext};
use crate::infrastructure::cluster::ClusterManager;
use crate::infrastructure::db_repository::DbRepository;
use crate::application::{
    DeviceService, StreamService, SessionService, RecordingService,
    MediaServerService,PlayerLayoutService, MapSettingsService,
    PlayService, WsBroadcaster, StreamRecoveryService,
    zlmediakit_hook::ZlMediaKitHookHandler, ChannelService,
    RecordingPolicyService, RecordingSchedulerService,
};
use crate::domain::traits::{DeviceLookup, StreamManager, EventPublisher, CacheStore};
use crate::domain::device::Protocol;
use crate::protocol::traits::ProtocolDeps;
use crate::protocol::adapter_manager::{AdapterEntry, set_adapter as am_set, remove_adapter as am_remove, register_adapter_factory, set_default_deps};
use crate::auth::AuthState;
pub use crate::auth::casbin::CasbinManager;
use crate::auth::auth_db::PostgresAuthRepository;
use crate::media::MediaSessionManager;

#[derive(Clone)]
pub struct ServiceRegistry {

    pub infra: InfraContext,

    pub media: MediaContext,

    pub device_service: Arc<DeviceService>,

    pub stream_service: Arc<StreamService>,

    pub stream_recovery_service: Arc<StreamRecoveryService>,

    pub session_service: Arc<SessionService>,

    pub recording_service: Arc<RecordingService>,

    pub recording_policy_service: Arc<RecordingPolicyService>,

    pub recording_scheduler_service: Arc<RecordingSchedulerService>,

    pub media_server_service: Arc<MediaServerService>,

    // pub hook_service: Arc<HookService>,

    pub zlmediakit_hook_handler: Arc<ZlMediaKitHookHandler>,

    pub player_layout_service: Arc<PlayerLayoutService>,

    pub map_settings_service: Arc<MapSettingsService>,

    pub channel_service: Arc<ChannelService>,

    pub play_service: Arc<PlayService>,

    pub ws_broadcaster: Arc<WsBroadcaster>,

    pub protocol_deps: ProtocolDeps,

    pub session_manager: Arc<MediaSessionManager>,
}

impl ServiceRegistry {

    pub async fn new(config: AppConfig) -> anyhow::Result<Self> {

        let db = Arc::new(
            DbRepository::new_without_load_with_pool(
                &config.database.url,
                config.database.debug_sql.unwrap_or(false),
                config.database.max_connections as u64,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize database: {}", e))?
        );

        for cfg in &config.media_servers.servers {
            let tag = if cfg.server_tag.is_empty() {
                cfg.name.clone()
            } else {
                cfg.server_tag.clone()
            };

            let exists_in_db = db.server_exists_by_tag(&tag).await?;
            if !exists_in_db {
                let new_server = crate::domain::server::Server {
                    id: 0,
                    name: cfg.name.clone(),
                    url: cfg.url.clone(),
                    api_key: cfg.api_key.clone(),
                    server_type: crate::domain::server::ServerType::from_str(&cfg.server_type).unwrap_or(crate::domain::server::ServerType::Zlmediakit),
                    weight: cfg.weight,
                    enabled: cfg.enabled,
                    server_tag: tag.clone(),
                    protocol_ports: cfg.protocol_ports.clone(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                if let Err(e) = db.create_server(&new_server).await {
                    tracing::warn!("[ServiceRegistry] Failed to sync config server to DB: {}", e);
                }
            } else if !cfg.protocol_ports.is_empty() {

                if let Ok(Some(mut existing)) = db.get_server_by_tag(&tag).await {
                    if existing.protocol_ports.is_empty() {
                        existing.protocol_ports = cfg.protocol_ports.clone();
                        existing.updated_at = chrono::Utc::now();
                        if let Err(e) = db.update_server(&existing).await {
                            tracing::warn!("[ServiceRegistry] Failed to fill protocol ports for '{}': {}", tag, e);
                        } else {
                        }
                    }
                }
            }
        }

        db.load_caches().await?;

        let configured_servers = config.media_servers.servers.clone();

        let infra = InfraContext::from_parts(db.clone(), Arc::new(config)).await?;

        let cluster = Arc::new(ClusterManager::new_from_configs(vec![]));

        let media_server_service = Arc::new(MediaServerService::new(
            infra.db.clone(),
            cluster.clone(),
            configured_servers,
        ));

        let media = MediaContext::new(&infra, cluster,media_server_service.clone()).await?;

        let recording_service = Arc::new(RecordingService::new(
            infra.db.clone(),
            media.cluster.clone(),
        ));

        let recording_policy_service = Arc::new(RecordingPolicyService::new(
            infra.db.clone(),
            media.cluster.clone(),
        ));

        let recording_scheduler_service = Arc::new(RecordingSchedulerService::new(
            infra.clone(),
            media.cluster.clone(),
            recording_policy_service.clone(),
            recording_service.clone(),
        ));

        let stream_service = Arc::new(StreamService::new(
            infra.clone(),
            media.clone(),
            recording_service.clone(),
        ));

        let stream_recovery_service = Arc::new(StreamRecoveryService::new(
            infra.clone(),
            media.clone(),
            stream_service.clone(),
        ));

        let device_service = Arc::new(DeviceService::new(
            infra.clone(),
            media.clone(),
            stream_service.clone(),
        ));

        let session_service = Arc::new(SessionService::new(
            infra.clone(),
            media.clone(),
        ));

        // let hook_service = Arc::new(HookService::new(
        //     infra.db.clone(),
        //     infra.redis.clone(),
        //     media.cluster.clone(),
        //     infra.event_bus.clone(),
        //     infra.config().session.expiration_secs,
        //     recording_service.clone(),
        //     stream_service.clone(),
        // ));

        let zlmediakit_hook_handler = Arc::new(
            ZlMediaKitHookHandler::from_context(
                &infra,
                &media,
            ).with_stream_recovery_service(stream_recovery_service.clone())
        );

        let player_layout_service = Arc::new(PlayerLayoutService::new(
            infra.db.clone(),
        ));

        let map_settings_service = Arc::new(MapSettingsService::new(
            infra.db.clone(),
        ));

        let channel_service = Arc::new(ChannelService::new(
            infra.db.clone(),
        ));

        let register_fn: Arc<dyn Fn(String, AdapterEntry) + Send + Sync> =
            Arc::new(|key, entry| am_set(key, entry));
        let unregister_fn: Arc<dyn Fn(String) + Send + Sync> =
            Arc::new(|key| am_remove(&key));
        let protocol_deps = ProtocolDeps::new(
            device_service.clone() as Arc<dyn DeviceLookup>,
            stream_service.clone() as Arc<dyn StreamManager>,
            infra.event_bus.clone() as Arc<dyn EventPublisher>,
            media.cluster.clone(),
            infra.redis.clone() as Arc<dyn CacheStore>,
            infra.config.clone(),
        ).with_registration(register_fn, unregister_fn);

        set_default_deps(protocol_deps.clone());
        register_adapter_factory(Protocol::Onvif, Arc::new(|deps, _key| Box::new(crate::protocol::onvif::OnvifAdapter::new(deps))));

        let ws_broadcaster = infra.ws_broadcaster.clone();
        let play_service = media.play_service.clone();
        let session_manager = Arc::new(MediaSessionManager::new());

        Ok(Self {
            infra,
            media,
            device_service,
            stream_service,
            stream_recovery_service,
            session_service,
            recording_service,
            recording_policy_service,
            recording_scheduler_service,
            media_server_service,
            // hook_service,
            zlmediakit_hook_handler,
            player_layout_service,
            map_settings_service,
            channel_service,
            play_service,
            ws_broadcaster,
            protocol_deps,
            session_manager,
        })
    }

    pub async fn get_auth_state(&self) -> anyhow::Result<AuthState> {
        let jwt_config = self.infra.config().jwt.clone();
        let default_username = self.infra.config().server.default_username.as_deref().unwrap_or("admin");
        let default_password = self.infra.config().server.default_password.as_deref().unwrap_or("admin");

        if self.infra.config().auth.auto_init_schema {
            let url = self.infra.config().database.url.clone();
            let user = default_username.to_string();
            let pass = default_password.to_string();
            tokio::spawn(async move {
                match PostgresAuthRepository::new(&url, &user, &pass).await {
                    Ok(_) => {},
                    Err(e) => tracing::warn!("[AuthState] auth schema init failed: {}", e),
                }
            });
        } else {
        }

        let auth_repo = Arc::new(PostgresAuthRepository::with_rb(self.infra.db.rb().clone()));

        let casbin = Arc::new(
            CasbinManager::new(auth_repo.clone()).await
                .map_err(|e| anyhow::anyhow!("Casbin init failed: {}", e))?
        );

        Ok(AuthState::new(jwt_config, casbin, auth_repo))
    }

    pub async fn start_all(&self) {
        self.device_service.start().await;
        self.session_service.start(self.infra.config().session.expiration_secs).await;
        self.stream_recovery_service.clone().start().await;
        self.recording_scheduler_service.clone().start().await;
        self.start_health_monitor().await;
        self.start_ws_service().await;
    }

    async fn start_health_monitor(&self) {
        let monitor = crate::infrastructure::health_monitor::HealthMonitor::new(
            self.infra.redis.clone(),
            self.media.cluster.clone(),
            self.media_server_service.clone(),
            Some(self.stream_recovery_service.clone()),
            self.infra.config().cluster.health_check_interval_secs,
        );
        let monitor = std::sync::Arc::new(monitor);
        monitor.start().await;
    }

    async fn start_ws_service(&self) {
        let ws_service = crate::application::ws_service::WsService::new(
            self.infra.ws_broadcaster.clone(),
            self.device_service.clone(),
            self.session_service.clone(),
            self.media.cluster.clone(),
        );
        let ws_service = std::sync::Arc::new(ws_service);
        ws_service.start();
    }

    pub async fn graceful_shutdown(&self) {
        self.device_service.stop().await;
        self.session_service.stop().await;
    }

    pub async fn get_dashboard_data(&self) -> serde_json::Value {
        let device_stats = self.device_service.get_stats();
        let stream_stats = self.stream_service.get_stats().await;
        let servers = self.media_server_service.list();

        let mut server_statuses = Vec::new();
        let mut online_count = 0usize;
        let mut statuses: Vec<crate::adapter::media_server::ServerStatus> = Vec::new();

        for server in &servers {

            let status = self
                .infra
                .redis
                .get_server_status(&server.server_tag)
                .await
                .ok()
                .flatten();
            let online = status.as_ref().map(|s| s.online).unwrap_or(false);
            let sessions = status.as_ref().map(|s| s.session_count).unwrap_or(0);

            if online { online_count += 1; }

            let cpu_usage = status.as_ref().map(|s| s.cpu_usage).unwrap_or(0.0);
            let memory_usage = status.as_ref().map(|s| s.memory_usage).unwrap_or(0.0);

            server_statuses.push(serde_json::json!({
                "id": server.id,
                "name": server.name,
                "server_tag": server.server_tag,
                "server_type": server.server_type,
                "online": online,
                "enabled": server.enabled,
                "session_count": sessions,
                "cpu_usage": cpu_usage,
                "memory_usage": memory_usage,
            }));

            statuses.push(crate::adapter::media_server::ServerStatus {
                name: server.name.clone(),
                server_type: server.server_type.to_string(),
                online,
                session_count: sessions,
                cpu_usage,
                memory_usage,
                bandwidth_in: 0,
                bandwidth_out: 0,
                last_heartbeat: None,
            });
        }

        let server_count = servers.len();
        let health_score = if server_count == 0 { 0.0 } else { (online_count as f64 / server_count as f64) * 100.0 };

        let aggregate = ServerAggregate::build(
            &statuses,
            self.session_service.get_active_count(),
            self.session_service.get_live_count(),
        );

        serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "devices": device_stats,
            "streams": stream_stats,
            "servers": {
                "total": server_count,
                "online": online_count,
                "offline": server_count.saturating_sub(online_count),
                "servers": server_statuses,
                "aggregate": {
                    "active_sessions": aggregate.active_sessions,
                    "total_sessions": aggregate.total_sessions,
                    "avg_cpu": aggregate.avg_cpu,
                    "avg_memory": aggregate.avg_memory,
                },
            },
            "health": {
                "score": health_score,
                "level": if health_score >= 80.0 { "healthy" } else if health_score >= 50.0 { "degraded" } else { "critical" },
            }
        })
    }
}

/// Aggregated metrics of all media servers, shown in the dashboard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerAggregate {
    pub active_sessions: usize,
    pub total_sessions: usize,
    pub avg_cpu: f64,
    pub avg_memory: f64,
}

impl ServerAggregate {
    /// Builds the aggregate from the status of every server and the session
    /// counters. The averages are calculated over the online servers only.
    pub fn build(
        statuses: &[crate::adapter::media_server::ServerStatus],
        active_sessions: usize,
        total_sessions: usize,
    ) -> Self {
        let online: Vec<&crate::adapter::media_server::ServerStatus> =
            statuses.iter().filter(|s| s.online).collect();

        let average = |value: fn(&crate::adapter::media_server::ServerStatus) -> f64| -> f64 {
            if online.is_empty() {
                return 0.0;
            }
            online.iter().map(|s| value(s)).sum::<f64>() / online.len() as f64
        };

        Self {
            active_sessions,
            total_sessions,
            avg_cpu: average(|s| s.cpu_usage),
            avg_memory: average(|s| s.memory_usage),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::media_server::ServerStatus;

    fn status(name: &str, online: bool, cpu: f64, memory: f64) -> ServerStatus {
        ServerStatus {
            name: name.to_string(),
            server_type: "zlmediakit".to_string(),
            online,
            session_count: 0,
            cpu_usage: cpu,
            memory_usage: memory,
            bandwidth_in: 0,
            bandwidth_out: 0,
            last_heartbeat: None,
        }
    }

    #[test]
    fn test_aggregate_counts_sessions_and_averages_the_online_servers() {
        let statuses = vec![
            status("a", true, 10.0, 100.0),
            status("b", true, 30.0, 300.0),
            status("c", false, 90.0, 900.0),
        ];

        let aggregate = ServerAggregate::build(&statuses, 3, 7);

        assert_eq!(aggregate.active_sessions, 3);
        assert_eq!(aggregate.total_sessions, 7);
        assert_eq!(aggregate.avg_cpu, 20.0, "only the online servers count");
        assert_eq!(aggregate.avg_memory, 200.0, "only the online servers count");
    }

    #[test]
    fn test_aggregate_without_online_server_has_no_averages() {
        let statuses = vec![status("a", false, 50.0, 500.0)];

        let aggregate = ServerAggregate::build(&statuses, 0, 0);

        assert_eq!(aggregate.avg_cpu, 0.0);
        assert_eq!(aggregate.avg_memory, 0.0);
    }
}