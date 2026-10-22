use std::sync::Arc;
use crate::infrastructure::cluster::ClusterManager;
use crate::application::play_service::PlayService;
use crate::MediaServerService;

#[derive(Clone)]
pub struct MediaContext {

    pub cluster: Arc<ClusterManager>,

    pub play_service: Arc<PlayService>,

    pub zlm_server_url: Arc<String>,
}

impl MediaContext {

    pub async fn new(
        infra: &crate::context::infra_context::InfraContext,
        cluster: Arc<ClusterManager>,
        media_server_service: Arc<MediaServerService>,
    ) -> anyhow::Result<Self> {
        let config = infra.config();

        let play_service = Arc::new(PlayService::new(
            infra.redis.clone(),
            config.session.expiration_secs,
            cluster.clone(),
            media_server_service.clone(),
        ));

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

        Ok(Self {
            cluster,
            play_service,
            zlm_server_url,
        })
    }

    pub async fn select_server(&self) -> Option<Arc<dyn crate::adapter::media_server::MediaServerAdapter>> {
        self.cluster.select_server().await
    }
}
