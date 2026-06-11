use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use uuid::Uuid;
use crate::context::{InfraContext, MediaContext};
use crate::infrastructure::DbRepository;
use crate::infrastructure::RedisStore;
use crate::infrastructure::cluster::ClusterManager;
use crate::error::{AppError, Result};
use crate::application::RecordingService;

/// Hook 处理结果
#[derive(Debug, Clone)]
pub enum HookAction {
    /// 允许访问
    Allow,
    /// 拒绝访问（带原因）
    Deny(String),
}

/// Hook 事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum HookEvent {
    /// 播放事件
    #[serde(rename = "on_play")]
    OnPlay {
        media_server_id: String,
        stream_id: String,
        ip: Option<String>,
        port: Option<u16>,
        schema: Option<String>,
        #[serde(default)]
        params: Option<HashMap<String, String>>,
    },

    /// 心跳事件
    #[serde(rename = "on_keepalive")]
    OnKeepalive {
        media_server_id: String,
        stream_id: String,
        ip: Option<String>,
    },

    /// 停止播放事件
    #[serde(rename = "on_stop")]
    OnStop {
        media_server_id: String,
        stream_id: String,
        ip: Option<String>,
        #[serde(default)]
        params: Option<HashMap<String, String>>,
    },

    /// 推流事件
    #[serde(rename = "on_publish")]
    OnPublish {
        media_server_id: String,
        stream_id: String,
        ip: Option<String>,
    },

    /// RTCP 统计事件
    #[serde(rename = "on_rtcp_stats")]
    OnRtcpStats {
        media_server_id: String,
        stream_id: String,
        bytes_sent: Option<u64>,
        bytes_received: Option<u64>,
    },

    /// 录制完成事件（MP4/TS/HLS 等）
    #[serde(rename = "on_record_mp4")]
    #[serde(alias = "on_record_ts")]
    #[serde(alias = "on_record_flv")]
    #[serde(alias = "on_record_hls")]
    OnRecordDone {
        media_server_id: String,
        stream_id: String,
        app: String,
        file_path: Option<String>,
        file_size: Option<u64>,
        time_len: Option<f64>,
    },
}

/// Hook 服务
///
/// 处理来自媒体服务器的 WebHook 回调。
/// 负责 Token 验证、会话创建、流量统计等。
///
/// # 依赖
/// - DbRepository: 数据库操作
/// - RedisStore: Redis 缓存
/// - ClusterManager: 集群管理
/// - session_expiry_secs: 会话过期时间
/// - recording_service: 录制服务
pub struct HookService {
    /// 数据库仓库
    repo: Arc<DbRepository>,
    /// Redis 存储
    redis: Arc<RedisStore>,
    /// 集群管理器
    cluster: Arc<ClusterManager>,
    /// 会话过期时间（秒）
    session_expiry_secs: i64,
    /// 录制服务
    recording_service: Arc<RecordingService>,
}

impl HookService {
    /// 创建 Hook 服务
    ///
    /// # 参数
    /// * `repo` - 数据库仓库
    /// * `redis` - Redis 存储
    /// * `cluster` - 集群管理器
    /// * `session_expiry_secs` - 会话过期时间
    pub fn new(
        repo: Arc<DbRepository>,
        redis: Arc<RedisStore>,
        cluster: Arc<ClusterManager>,
        session_expiry_secs: i64,
        recording_service: Arc<RecordingService>,
    ) -> Self {
        Self {
            repo,
            redis,
            cluster,
            session_expiry_secs,
            recording_service,
        }
    }

    /// 从 InfraContext 和 MediaContext 创建 Hook 服务
    ///
    /// 便捷构造函数，自动从上下文中提取所需组件。
    pub fn from_context(
        infra: &InfraContext,
        media: &MediaContext,
        recording_service: Arc<RecordingService>,
    ) -> Self {
        Self {
            repo: infra.db.clone(),
            redis: infra.redis.clone(),
            cluster: media.cluster.clone(),
            session_expiry_secs: infra.config().session.expiration_secs,
            recording_service,
        }
    }

    /// 处理 Hook 事件
    ///
    /// # 参数
    /// * `event` - Hook 事件
    ///
    /// # 返回
    /// HookAction::Allow 或 HookAction::Deny
    pub async fn handle(&self, event: HookEvent) -> HookAction {
        match event {
            HookEvent::OnPlay {
                media_server_id,
                stream_id,
                ip,
                params,
                ..
            } => {
                self.on_play(&media_server_id, &stream_id, ip.as_deref(), params.as_ref())
                    .await
            }

            HookEvent::OnKeepalive {
                media_server_id,
                stream_id,
                ip,
            } => {
                self.on_keepalive(&media_server_id, &stream_id, ip.as_deref())
                    .await;
                HookAction::Allow
            }

            HookEvent::OnStop {
                media_server_id,
                stream_id,
                ip,
                params,
                ..
            } => {
                self.on_stop(&media_server_id, &stream_id, ip.as_deref(), params.as_ref())
                    .await;
                HookAction::Allow
            }

            HookEvent::OnPublish {
                media_server_id,
                stream_id,
                ip,
            } => {
                self.on_publish(&media_server_id, &stream_id, ip.as_deref())
                    .await;
                HookAction::Allow
            }

            HookEvent::OnRtcpStats {
                media_server_id,
                stream_id,
                bytes_sent,
                bytes_received,
            } => {
                self.on_rtcp_stats(&media_server_id, &stream_id, bytes_sent, bytes_received)
                    .await;
                HookAction::Allow
            }

            HookEvent::OnRecordDone {
                stream_id,
                file_path,
                file_size,
                time_len,
                ..
            } => {
                self.on_record_done(&stream_id, file_path.as_deref(), file_size, time_len)
                    .await;
                HookAction::Allow
            }
        }
    }

    /// 处理播放事件
    ///
    /// 验证 Token，创建会话记录，并更新流观看人数。
    async fn on_play(
        &self,
        media_server_id: &str,
        stream_id: &str,
        client_ip: Option<&str>,
        params: Option<&HashMap<String, String>>,
    ) -> HookAction {
        tracing::info!(
            "[Hook] on_play: server={}, stream={}, ip={:?}",
            media_server_id,
            stream_id,
            client_ip
        );

        // 提取 Token
        let token = params
            .and_then(|p| p.get("token"))
            .map(|s| s.as_str());

        let token = match token {
            Some(t) => t,
            None => {
                tracing::warn!("[Hook] on_play: no token provided for stream={}", stream_id);
                return HookAction::Deny("Missing token".to_string());
            }
        };

        // 验证 Token
        let stream = match self.get_stream_by_token(token).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                tracing::warn!("[Hook] on_play: invalid token for stream={}", stream_id);
                return HookAction::Deny("Invalid token".to_string());
            }
            Err(e) => {
                tracing::error!("[Hook] on_play: failed to get stream by token: {}", e);
                return HookAction::Deny("Internal error".to_string());
            }
        };

        // 更新流观看人数
        if let Err(e) = self.increment_stream_viewers(&stream.stream_key).await {
            tracing::warn!("[Hook] on_play: failed to increment viewers: {}", e);
        }

        // 创建会话
        let mut session = crate::domain::Session::new(
            crate::domain::SessionType::Play,
            stream.device_id,
            0,
        );
        session.state = crate::domain::SessionState::Active;
        session.client_ip = client_ip.map(String::from);
        session.media_server_id = Some(media_server_id.to_string());
        session.stream_id = stream.stream_key.clone();
        session.refresh_expiry(self.session_expiry_secs);

        if let Err(e) = self.repo.create_session(&session).await {
            tracing::error!("[Hook] on_play: failed to create session: {}", e);
            return HookAction::Deny("Failed to create session".to_string());
        }

        self.repo.sessions_cache().insert(session.id, session);
        tracing::info!("[Hook] on_play: session created, stream={}, viewers now={}",
            stream_id,
            stream.viewer_count + 1
        );

        HookAction::Allow
    }

    /// 处理停止事件
    ///
    /// 终止会话，更新流观看人数（最后一个观看者离开时触发流清理）。
    async fn on_stop(
        &self,
        media_server_id: &str,
        stream_id: &str,
        client_ip: Option<&str>,
        _params: Option<&HashMap<String, String>>,
    ) {
        tracing::info!(
            "[Hook] on_stop: server={}, stream={}, ip={:?}",
            media_server_id,
            stream_id,
            client_ip
        );

        let sessions = self.find_sessions_by_stream(stream_id).await;
        let count = sessions.len();
        for session in sessions {
            let mut s = session;
            s.terminate();
            self.repo.sessions_cache().insert(s.id, s);
        }

        if count > 0 {
            tracing::info!(
                "[Hook] on_stop: {} session(s) terminated, stream={}",
                count,
                stream_id
            );
        }

        if let Err(e) = self.decrement_stream_viewers(stream_id).await {
            tracing::warn!("[Hook] on_stop: failed to decrement viewers: {}", e);
        }
    }

    /// 处理心跳事件
    ///
    /// 更新会话活跃时间，同时更新流的最后心跳时间。
    async fn on_keepalive(
        &self,
        media_server_id: &str,
        stream_id: &str,
        _client_ip: Option<&str>,
    ) {
        tracing::debug!(
            "[Hook] on_keepalive: server={}, stream={}",
            media_server_id,
            stream_id
        );

        let sessions = self.find_sessions_by_stream(stream_id).await;
        for session in sessions {
            let mut s = session;
            s.touch();
            s.refresh_expiry(self.session_expiry_secs);
            self.repo.sessions_cache().insert(s.id, s);
        }

        if let Err(e) = self.touch_stream_keepalive(stream_id).await {
            tracing::debug!("[Hook] on_keepalive: failed to update stream keepalive: {}", e);
        }

        const STREAM_TTL_SECS: u64 = 86400;
        if let Err(e) = self.redis.expire_key(&format!("stream:{}", stream_id), STREAM_TTL_SECS).await {
            tracing::debug!("[Hook] on_keepalive: failed to refresh stream TTL: {}", e);
        }
    }

    /// 处理推流事件
    ///
    /// 当 RTMP/RTSP 推送流到达媒体服务器时，在本地记录流状态。
    async fn on_publish(
        &self,
        media_server_id: &str,
        stream_id: &str,
        client_ip: Option<&str>,
    ) {
        tracing::info!(
            "[Hook] on_publish: server={}, stream={}, ip={:?}",
            media_server_id,
            stream_id,
            client_ip
        );

        if let Err(e) = self.register_push_stream(media_server_id, stream_id).await {
            tracing::warn!("[Hook] on_publish: failed to register push stream: {}", e);
        }
    }

    /// 处理 RTCP 统计事件
    ///
    /// 更新会话流量统计，同时更新流带宽数据。
    async fn on_rtcp_stats(
        &self,
        media_server_id: &str,
        stream_id: &str,
        bytes_sent: Option<u64>,
        bytes_received: Option<u64>,
    ) {
        tracing::debug!(
            "[Hook] on_rtcp_stats: server={}, stream={}, sent={:?}, recv={:?}",
            media_server_id,
            stream_id,
            bytes_sent,
            bytes_received
        );

        let sessions = self.find_sessions_by_stream(stream_id).await;
        for session in sessions {
            let mut s = session;
            s.update_stats(
                bytes_sent.unwrap_or(0),
                bytes_received.unwrap_or(0),
            );
            self.repo.sessions_cache().insert(s.id, s);
        }

        if let Err(e) = self.update_stream_bandwidth(
            stream_id,
            bytes_received.unwrap_or(0),
            bytes_sent.unwrap_or(0),
        ).await {
            tracing::debug!("[Hook] on_rtcp_stats: failed to update stream bandwidth: {}", e);
        }
    }

    async fn on_record_done(
        &self,
        stream_id: &str,
        file_path: Option<&str>,
        file_size: Option<u64>,
        time_len: Option<f64>,
    ) {
        tracing::info!(
            "[Hook] on_record_done: stream={}, file_path={:?}, file_size={:?}, time_len={:?}",
            stream_id, file_path, file_size, time_len
        );

        let filename = file_path
            .and_then(|p| std::path::Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .map(String::from)
            .unwrap_or_else(|| stream_id.to_string());

        let size = file_size.unwrap_or(0);
        let duration = time_len.map(|t| t as u64).unwrap_or(0);

        if let Err(e) = self.recording_service.finalize_recording(stream_id, filename, size, duration).await {
            tracing::error!("[Hook] on_record_done: failed to finalize recording: {}", e);
        }
    }

    /// 根据 Token 获取流信息
    async fn get_stream_by_token(
        &self,
        token: &str,
    ) -> std::result::Result<Option<crate::domain::Stream>, AppError> {
        self.repo.get_stream_by_token(token).await
    }

    /// 根据 stream_key 查找流
    fn find_stream_by_stream_key(&self, stream_key: &str) -> Option<crate::domain::Stream> {
        self.repo.streams_cache()
            .iter()
            .find(|s| s.stream_key == stream_key)
            .map(|s| s.clone())
    }

    /// 增加流观看人数
    async fn increment_stream_viewers(&self, stream_key: &str) -> Result<()> {
        if let Some(s) = self.find_stream_by_stream_key(stream_key) {
            let mut s = s;
            s.increment_viewers();
            self.repo.update_stream(&s).await?;
        }
        Ok(())
    }

    /// 减少流观看人数
    async fn decrement_stream_viewers(&self, stream_key: &str) -> Result<()> {
        if let Some(s) = self.find_stream_by_stream_key(stream_key) {
            let mut s = s;
            s.decrement_viewers();
            self.repo.update_stream(&s).await?;
        }
        Ok(())
    }

    /// 更新流心跳时间
    async fn touch_stream_keepalive(&self, stream_key: &str) -> Result<()> {
        if let Some(s) = self.find_stream_by_stream_key(stream_key) {
            let mut s = s;
            s.update_keepalive();
            self.repo.update_stream(&s).await?;
        }
        Ok(())
    }

    /// 更新流带宽数据
    async fn update_stream_bandwidth(&self, stream_key: &str, bytes_in: u64, bytes_out: u64) -> Result<()> {
        if let Some(s) = self.find_stream_by_stream_key(stream_key) {
            let mut s = s;
            s.update_bandwidth(bytes_in, bytes_out);
            self.repo.update_stream(&s).await?;
        }
        Ok(())
    }

    /// 注册推送流（RTMP/RTSP push）
    ///
    /// 尝试从 stream_id 匹配设备（stream_key 或 push_urls），若匹配则关联 device_id。
    async fn register_push_stream(&self, media_server_id: &str, stream_id: &str) -> Result<()> {
        let device_id = self.find_device_by_push_stream(stream_id);

        if let Some(s) = self.find_stream_by_stream_key(stream_id) {
            let mut s = s;
            s.update_keepalive();
            if s.state == crate::domain::StreamState::Idle || s.state == crate::domain::StreamState::Error {
                s.start();
            }
            self.repo.update_stream(&s).await?;
            tracing::info!("[Hook] on_publish: updated push stream {} (device_id={})", stream_id, s.device_id);
        } else {
            let token = uuid::Uuid::new_v4().to_string();
            let app = device_id
                .and_then(|id| self.repo.devices_cache().get(&id))
                .map(|d| d.app.clone().unwrap_or_else(|| "live".to_string()))
                .unwrap_or_else(|| "live".to_string());
            let mut stream = crate::domain::Stream::new(
                device_id.unwrap_or(0),
                media_server_id.to_string(),
                stream_id.to_string(),
                app,
                token,
            );
            stream.start();
            self.repo.create_stream(&stream).await?;
            tracing::info!("[Hook] on_publish: registered push stream {} on server {} (device_id={})",
                stream_id, media_server_id, stream.device_id);
        }
        Ok(())
    }

    /// 根据推送流的 stream_id 查找关联的设备 ID
    fn find_device_by_push_stream(&self, stream_id: &str) -> Option<i64> {
        for entry in self.repo.devices_cache().iter() {
            let device = entry.value();
            if let Some(ref sk) = device.stream_key {
                if !sk.is_empty() && stream_id.contains(sk) {
                    return Some(device.id);
                }
            }
            for pu in &device.push_urls {
                if !pu.url.is_empty() && (pu.url.contains(stream_id) || stream_id.contains(&device.device_tag.clone().unwrap_or_default())) {
                    return Some(device.id);
                }
            }
        }
        None
    }

    /// 根据流标识查找活跃会话
    async fn find_sessions_by_stream(
        &self,
        stream_id: &str,
    ) -> Vec<crate::domain::Session> {
        use crate::domain::SessionState;

        self.repo.sessions_cache()
            .iter()
            .filter(|s| {
                s.stream_id == stream_id
                    && s.state != SessionState::Terminated
                    && s.state != SessionState::Terminating
            })
            .map(|s| s.clone())
            .collect()
    }
}