use std::sync::Arc;
use uuid::Uuid;
use crate::domain::device::{Device, PlayLinks};
use crate::infrastructure::RedisStore;
use crate::application::media_server_service::MediaServerService;
use crate::domain::traits::{CacheStore, CacheStoreExt};
use crate::infrastructure::cluster::ClusterManager;
use crate::error::{AppError, Result};

/// 播放服务
pub struct PlayService {
    redis: Arc<RedisStore>,
    token_ttl_secs: i64,
    cluster: Arc<ClusterManager>,
    media_server_service: Arc<MediaServerService>,
}

impl PlayService {
    pub fn new(
        redis: Arc<RedisStore>,
        token_ttl_secs: i64,
        cluster: Arc<ClusterManager>,
        media_server_service: Arc<MediaServerService>,
    ) -> Self {
        Self {
            redis,
            token_ttl_secs,
            cluster,
            media_server_service,
        }
    }

    /// 生成播放 Token
    pub async fn generate_token(&self, device_id: &str) -> Result<String> {
        let token = Uuid::new_v4().to_string();
        let key = format!("play_token:{}", token);

        #[derive(serde::Serialize)]
        struct TokenData<'a> {
            device_id: &'a str,
            created_at: i64,
        }

        let data = TokenData {
            device_id,
            created_at: chrono::Utc::now().timestamp(),
        };

        self.redis
            .set(&key, &data, Some(self.token_ttl_secs as u64))
            .await?;

        let device_sessions_key = format!("device_sessions:{}", device_id);
        self.redis.sadd(&device_sessions_key, &token).await?;
        self.redis.expire(&device_sessions_key, self.token_ttl_secs as u64).await?;

        tracing::debug!("[PlayService] Generated token for device: {}", device_id);
        Ok(token)
    }

    /// 验证播放 Token
    pub async fn validate_token(&self, token: &str) -> Option<String> {
        let key = format!("play_token:{}", token);

        #[derive(serde::Deserialize)]
        struct TokenData {
            device_id: String,
        }

        match self.redis.get::<TokenData>(&key).await {
            Ok(Some(data)) => Some(data.device_id),
            _ => None,
        }
    }

    /// 撤销播放 Token
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        let key = format!("play_token:{}", token);

        if let Some(device_id) = self.validate_token(token).await {
            let device_sessions_key = format!("device_sessions:{}", device_id);
            self.redis.srem(&device_sessions_key, token).await?;
        }

        self.redis.del(&key).await?;
        tracing::debug!("[PlayService] Revoked token");
        Ok(())
    }

    /// 构建播放链接
    pub fn build_play_links(
        &self,
        device: &Device,
        token: &str,
        stream_id: &str,
    ) -> PlayLinks {
        self.build_play_links_with_server(device, token, stream_id, device.media_server_tag.as_deref())
    }

    /// 构建播放链接，指定媒体服务器
    pub fn build_play_links_with_server(
        &self,
        device: &Device,
        token: &str,
        stream_id: &str,
        media_server_name: Option<&str>,
    ) -> PlayLinks {
        let expires_at = chrono::Utc::now().timestamp() + self.token_ttl_secs;

        let server_name = media_server_name.or(device.media_server_tag.as_deref()).unwrap_or("default");
        let ports = self.media_server_service
            .get(server_name)
            .or_else(|| self.media_server_service.get_enabled_servers().first().cloned())
            .map(|s| s.protocol_ports)
            .unwrap_or_default();
        let c_ports = ports.clone();
        let host = if device.host.is_empty() {
            self.media_server_service
                .get_enabled_servers()
                .first()
                .map(|s| s.url.clone())
                .unwrap_or_else(|| "localhost".to_string())
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .split(':')
                .next()
                .unwrap_or("localhost")
                .to_string()
        } else {
            device.host.clone()
        };

        let rtsp_auth = match (
            device.playback_username.as_deref(),
            device.playback_password.as_deref(),
        ) {
            (Some(u), Some(p)) => format!("{}:{}@", u, p),
            _ => String::new(),
        };

        let app = device.app.as_deref().filter(|s| !s.is_empty()).unwrap_or("live");

        PlayLinks {
            token: token.to_string(),
            stream_id: stream_id.to_string(),
            expires_at,
            ports,
            rtsp_signaling: Some(format!(
                "rtsp://{}{}:{}/{}/{}?token={}",
                rtsp_auth, host, c_ports.rtsp.unwrap_or(554), app, stream_id, token
            )),
            rtsp_media: Some(format!(
                "rtsp://{}{}:{}/{}/{}?token={}",
                rtsp_auth, host, c_ports.rtp_tcp.unwrap_or(8555), app, stream_id, token
            )),
            flv: Some(format!(
                "http://{}:{}/{}/{}.flv?token={}",
                host, c_ports.http_flv.unwrap_or(8080), app, stream_id, token
            )),
            hls: Some(format!(
                "http://{}:{}/{}/{}.m3u8?token={}",
                host, c_ports.http.unwrap_or(80), app, stream_id, token
            )),
            webrtc: Some(format!(
                "http://{}:{}/webrtc/play/{}?token={}",
                host, c_ports.http.unwrap_or(80), stream_id, token
            )),
            web_flv: Some(format!(
                "ws://{}:{}/{}/{}.flv?token={}",
                host, c_ports.ws_flv.unwrap_or(8080), app, stream_id, token
            )),
        }
    }
}

impl Clone for PlayService {
    fn clone(&self) -> Self {
        Self {
            redis: self.redis.clone(),
            token_ttl_secs: self.token_ttl_secs,
            cluster: self.cluster.clone(),
            media_server_service: self.media_server_service.clone(),
        }
    }
}