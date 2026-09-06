use std::sync::Arc;
use crate::infrastructure::cluster::ClusterManager;
use crate::application::play_service::PlayService;
use crate::MediaServerService;

/// 媒体上下文
///
/// 包含流媒体相关的组件集合。
/// 管理媒体服务器集群和播放服务。
///
/// # 职责
/// - 媒体服务器负载均衡
/// - 播放 URL 生成
#[derive(Clone)]
pub struct MediaContext {
    /// 集群管理器 (媒体服务器负载均衡)
    pub cluster: Arc<ClusterManager>,

    /// 播放 URL 生成服务
    pub play_service: Arc<PlayService>,

    /// ZLMediaKit 服务器地址 (如果有)
    pub zlm_server_url: Arc<String>,
}

impl MediaContext {
    /// 从配置和基础设施上下文创建媒体上下文
    pub async fn new(
        infra: &crate::context::infra_context::InfraContext,
        cluster: Arc<ClusterManager>,
        media_server_service: Arc<MediaServerService>,
    ) -> anyhow::Result<Self> {
        let config = infra.config();

        // 创建播放服务
        let play_service = Arc::new(PlayService::new(
            infra.redis.clone(),
            config.session.expiration_secs,
            cluster.clone(),
            media_server_service.clone(),
        ));

        // 从数据库查找 ZLMediaKit 服务器地址
        let zlm_server_url = media_server_service.list().iter()
            .find(|s| s.server_type == crate::domain::server::ServerType::Zlmediakit)
            .map(|s| {
                s.url
                    .trim_start_matches("http://")
                    .trim_start_matches("https://")
                    .trim_end_matches('/')
                    .to_string()
            })
            .unwrap_or_else(|| "127.0.0.1".to_string());

        let zlm_server_url = Arc::new(zlm_server_url);

        tracing::info!(
            "[MediaContext] Initialized: cluster_servers={}, zlm_url={}",
            cluster.server_count(),
            zlm_server_url,
        );

        Ok(Self {
            cluster,
            play_service,
            zlm_server_url,
        })
    }

    /// 选择可用的媒体服务器
    pub async fn select_server(&self) -> Option<Arc<dyn crate::adapter::media_server::MediaServerAdapter>> {
        self.cluster.select_server().await
    }
}
