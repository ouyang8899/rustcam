use std::sync::Arc;
use uuid::Uuid;
use async_trait::async_trait;
use crate::context::{InfraContext, MediaContext};
use crate::domain::{Stream, StreamState};
use crate::domain::traits::{CacheStore, StreamManager};
use crate::adapter::media_server::{StreamInfo, Protocol as MediaProtocol};
use crate::error::{AppError, Result};
use crate::application::recording_service::RecordingService;

/// 流管理服务
pub struct StreamService {
    infra: InfraContext,
    media: MediaContext,
    recording_service: Arc<RecordingService>,
}

impl StreamService {
    pub fn new(
        infra: InfraContext,
        media: MediaContext,
        recording_service: Arc<RecordingService>,
    ) -> Self {
        Self { infra, media, recording_service }
    }

    /// 创建拉流
    pub async fn start_pull_stream(
        &self,
        device_id: i64,
        _session_id: i64,
        rtsp_url: &str,
    ) -> Result<StreamInfo> {
        self.stop_streams_by_device(device_id).await?;

        let server = self.media.cluster
            .select_any_server()
            .await
            .ok_or_else(|| AppError::MediaServerError("No available media server".to_string()))?;
        let server_tag = server.tag().to_string();

        let device = self.infra.db.devices_cache().get(&device_id)
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?;
        let app = device.app.clone().unwrap_or_else(|| "live".to_string());

        let stream_key = format!("{}_{}", device_id, Uuid::new_v4());
        let token = Uuid::new_v4().to_string();

        let stream = Stream::new(device_id, server_tag.clone(), stream_key.clone(), app.clone(), token);
        self.infra.db.create_stream(&stream).await?;

        let stream_info = server.add_stream_proxy(&stream_key, &app, rtsp_url).await
            .map_err(|e| {
                tracing::error!("[StreamService] add_stream_proxy failed for {}: {}", stream_key, e);
                AppError::MediaServerError(format!("Media server error: {}", e))
            })?;

        let stream_key_owned = stream_key.clone();
        let mut found = false;
        for entry in self.infra.db.streams_cache().iter() {
            if entry.stream_key == stream_key_owned {
                let mut s = entry.clone();
                s.start();
                self.infra.db.update_stream(&s).await?;
                found = true;
                break;
            }
        }
        if !found {
            tracing::warn!("[StreamService] Stream {} not found in cache after creation", stream_key);
        }

        self.start_recording_for_device(device_id, &stream_key, &server_tag).await;
        self.infra.redis.set_stream_info(&stream_key, &stream_info).await?;
        Ok(stream_info)
    }

    /// 创建 GB28181 推流
    pub async fn start_gb28181_stream(
        &self,
        device_id: i64,
        stream_key: &str,
        session_id: i64,
        media_server_name: Option<&str>,
    ) -> Result<StreamInfo> {
        self.stop_streams_by_device(device_id).await?;

        let device = self.infra.db.devices_cache().get(&device_id)
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?;
        let app = device.app.clone().unwrap_or_else(|| "live".to_string());

        let server_tag = if let Some(name) = media_server_name {
            if self.media.cluster.get_server(name).is_some() {
                name.to_string()
            } else {
                self.media.cluster
                    .select_any_server()
                    .await
                    .map(|s| s.tag().to_string())
                    .unwrap_or_else(|| "zlmediakit-1".to_string())
            }
        } else {
            self.media.cluster
                .select_any_server()
                .await
                .map(|s| s.tag().to_string())
                .unwrap_or_else(|| "zlmediakit-1".to_string())
        };

        let token = Uuid::new_v4().to_string();
        let stream = Stream::new(device_id, server_tag.clone(), stream_key.to_string(), app, token.clone());
        self.infra.db.create_stream(&stream).await?;

        self.start_recording_for_device(device_id, stream_key, &server_tag).await;

        let device = match self.infra.db.devices_cache().get(&device_id) {
            Some(d) => d.clone(),
            None => {
                return Err(AppError::NotFound(format!("Device {} not found", device_id)));
            }
        };
        let play_links = self.media.play_service.build_play_links_with_server(
            &device, &token, stream_key, Some(&server_tag),
        );

        let stream_info = StreamInfo {
            stream_key: stream_key.to_string(),
            play_url: String::new(),
            rtsp_url: play_links.rtsp_signaling.clone().unwrap_or_default(),
            rtmp_url: String::new(),
            hls_url: play_links.hls.clone().unwrap_or_default(),
            webrtc_url: play_links.webrtc.clone().unwrap_or_default(),
            flv_url: play_links.flv.clone(),
            web_flv_url: play_links.web_flv.clone(),
            media_server_id: server_tag.clone(),
            media_server_name: server_tag.clone(),
        };

        self.infra.redis.set_stream_info(stream_key, &stream_info).await?;
        Ok(stream_info)
    }

    /// 停止流：通知媒体服务器关闭 proxy、清理所有观看 Session、更新状态
    pub async fn stop_stream(&self, stream_key: &str) -> Result<()> {
        let (stream, server_tag) = {
            let cache = self.infra.db.streams_cache();
            let entry = cache.iter().find(|s| s.stream_key == stream_key).map(|s| s.clone());
            match entry {
                Some(s) => (s.clone(), s.media_server_tag.clone()),
                None => {
                    tracing::debug!("[StreamService] stop_stream: stream {} not in cache", stream_key);
                    return Ok(());
                }
            }
        };

        if let Some(server) = self.media.cluster.get_server(&server_tag) {
            if let Err(e) = server.remove_stream_proxy(stream_key, &stream.app).await {
                tracing::warn!("[StreamService] Failed to remove proxy on {}: {}", server_tag, e);
            }
            if let Err(e) = server.close_rtp_server(stream_key).await {
                tracing::debug!("[StreamService] close_rtp_server (optional): {}", e);
            }
        } else {
            tracing::warn!("[StreamService] Server {} not found for stream {}", server_tag, stream_key);
        }

        if let Err(e) = self.recording_service.stop_recording_by_stream_key(stream_key).await {
            tracing::warn!("[StreamService] Failed to stop recording: {}", e);
        }

        self.terminate_sessions_for_stream(stream_key).await;

        let mut stream = stream;
        stream.stopped();
        self.infra.db.update_stream(&stream).await?;
        self.infra.redis.delete_stream_info(stream_key).await?;

        tracing::info!("[StreamService] Stream stopped: {}", stream_key);
        Ok(())
    }

    /// 停止设备的所有流
    ///
    /// 包括：设备自身的流 + 推送流（device_id=0但stream_key属于该设备）
    pub async fn stop_streams_by_device(&self, device_id: i64) -> Result<()> {
        let device_tag = self.infra.db.devices_cache()
            .get(&device_id)
            .and_then(|d| d.device_tag.clone());

        let streams: Vec<Stream> = self.infra.db.list_streams()
            .await
            .into_iter()
            .filter(|s| {
                if s.device_id == device_id { return true; }
                if s.device_id == 0 && device_tag.is_some() {
                    let tag = device_tag.as_ref().unwrap();
                    return s.stream_key.contains(tag);
                }
                false
            })
            .collect();

        for stream in streams {
            if let Err(e) = self.stop_stream(&stream.stream_key).await {
                tracing::warn!("[StreamService] stop_streams_by_device: failed to stop {}: {}", stream.stream_key, e);
            }
        }
        Ok(())
    }

    /// 终止流对应的所有播放 Session
    async fn terminate_sessions_for_stream(&self, stream_key: &str) {
        let session_ids: Vec<i64> = self.infra.db.sessions_cache()
            .iter()
            .filter(|s| s.stream_id == stream_key)
            .filter(|s| s.state != crate::domain::SessionState::Terminated)
            .filter(|s| s.state != crate::domain::SessionState::Terminating)
            .map(|s| s.id)
            .collect();

        let count = session_ids.len();
        for session_id in session_ids {
            if let Some(session) = self.infra.db.sessions_cache().get(&session_id) {
                let mut s = session.clone();
                s.terminate();
                self.infra.db.sessions_cache().insert(session_id, s);
            }
        }
        if count > 0 {
            tracing::debug!("[StreamService] Terminated {} sessions for stream {}", count, stream_key);
        }
    }

    /// 获取流
    pub async fn get_stream(&self, id: i64) -> Result<Option<Stream>> {
        Ok(self.infra.db.list_streams().await
            .into_iter()
            .find(|s| s.id == id))
    }

    /// 获取所有流
    pub async fn list_all_streams(&self) -> Vec<Stream> {
        self.infra.db.list_streams().await
    }

    /// 分页获取流列表
    pub async fn list_streams_paginated(&self, limit: usize, offset: usize) -> Vec<Stream> {
        self.infra.db.list_streams_paginated(limit, offset).await
    }

    /// 获取流总数
    pub async fn count_streams(&self) -> usize {
        self.infra.db.count_streams().await
    }

    /// 获取活跃流
    pub async fn list_active_streams(&self) -> Vec<Stream> {
        self.infra.db.list_streams().await
            .into_iter()
            .filter(|s| s.state == StreamState::Active)
            .collect()
    }

    /// 获取播放 URL
    pub async fn get_play_url(&self, stream_key: &str, protocol: MediaProtocol) -> Result<String> {
        let info = self.infra.redis.get_stream_info(stream_key)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Stream {} not found", stream_key)))?;

        match protocol {
            MediaProtocol::Rtsp => Ok(info.rtsp_url),
            MediaProtocol::Rtmp => Ok(info.rtmp_url),
            MediaProtocol::Hls => Ok(info.hls_url),
            MediaProtocol::WebRTC => Ok(info.webrtc_url),
            MediaProtocol::Flv => info.flv_url
                .ok_or_else(|| AppError::BadRequest("FLV URL not available".to_string())),
            MediaProtocol::WsFlv => info.web_flv_url
                .ok_or_else(|| AppError::BadRequest("WS-FLV URL not available".to_string())),
        }
    }

    /// 获取流统计
    pub async fn get_stats(&self) -> serde_json::Value {
        let streams = self.infra.db.list_streams().await;
        let total = streams.len();
        let active = streams.iter().filter(|s| s.state == StreamState::Active).count();

        serde_json::json!({
            "total": total,
            "active": active,
        })
    }

    /// 根据 Token 获取流
    pub async fn get_stream_by_token(&self, token: &str) -> Result<Option<Stream>> {
        self.infra.db.get_stream_by_token(token).await
    }

    /// 根据 stream_key 获取流
    pub async fn get_stream_by_stream_key(&self, stream_key: &str) -> Option<Stream> {
        self.infra.db.streams_cache()
            .iter()
            .find(|s| s.stream_key == stream_key)
            .map(|s| s.clone())
    }

    /// 更新流状态
    pub async fn update_stream_state(&self, stream: &Stream) -> Result<()> {
        self.infra.db.update_stream(stream).await
    }

    /// 重启现有流（复用记录，不创建新记录）
    /// 用于流恢复场景：设备重新上线后，重新拉流到媒体服务器
    pub async fn restart_stream(
        &self,
        stream_key: &str,
        rtsp_url: &str,
    ) -> Result<StreamInfo> {
        let stream_id = match self.infra.db.get_stream_id_by_key(stream_key) {
            Some(id) => id,
            None => return Err(AppError::NotFound(format!("Stream {} not found", stream_key))),
        };

        let stream = match self.infra.db.streams_cache().get(&stream_id) {
            Some(s) => s.clone(),
            None => return Err(AppError::NotFound(format!("Stream {} not in cache", stream_key))),
        };

        let server = self.media.cluster
            .select_any_server()
            .await
            .ok_or_else(|| AppError::MediaServerError("No available media server".to_string()))?;
        let server_tag = server.tag().to_string();

        if let Some(old_server) = self.media.cluster.get_server(&stream.media_server_tag) {
            let _ = old_server.remove_stream_proxy(stream_key, &stream.app).await;
            let _ = old_server.close_rtp_server(stream_key).await;
        }

        let stream_info = server.add_stream_proxy(stream_key, &stream.app, rtsp_url).await
            .map_err(|e| AppError::MediaServerError(format!("Media server error: {}", e)))?;

        let mut updated_stream = stream.clone();
        updated_stream.media_server_tag = server_tag;
        updated_stream.start();
        self.infra.db.update_stream(&updated_stream).await?;

        self.start_recording_for_device(stream.device_id, stream_key, &updated_stream.media_server_tag).await;
        self.infra.redis.set_stream_info(stream_key, &stream_info).await?;

        tracing::info!("[StreamService] Stream {} restarted on {}", stream_key, updated_stream.media_server_tag);
        Ok(stream_info)
    }

    /// 重启 GB28181 推送流（复用记录）
    pub async fn restart_gb28181_stream(
        &self,
        stream_key: &str,
        media_server_name: Option<&str>,
    ) -> Result<StreamInfo> {
        let stream_id = match self.infra.db.get_stream_id_by_key(stream_key) {
            Some(id) => id,
            None => return Err(AppError::NotFound(format!("Stream {} not found", stream_key))),
        };

        let stream = match self.infra.db.streams_cache().get(&stream_id) {
            Some(s) => s.clone(),
            None => return Err(AppError::NotFound(format!("Stream {} not in cache", stream_key))),
        };

        let server_tag = if let Some(name) = media_server_name {
            if self.media.cluster.get_server(name).is_some() {
                name.to_string()
            } else {
                self.media.cluster.select_any_server().await
                    .map(|s| s.tag().to_string())
                    .unwrap_or_else(|| "zlmediakit-1".to_string())
            }
        } else {
            self.media.cluster.select_any_server().await
                .map(|s| s.tag().to_string())
                .unwrap_or_else(|| "zlmediakit-1".to_string())
        };

        if let Some(old_server) = self.media.cluster.get_server(&stream.media_server_tag) {
            let _ = old_server.remove_stream_proxy(stream_key, &stream.app).await;
            let _ = old_server.close_rtp_server(stream_key).await;
        }

        let device = match self.infra.db.devices_cache().get(&stream.device_id) {
            Some(d) => d.clone(),
            None => {
                return Err(AppError::NotFound(format!("Device {} not found", stream.device_id)));
            }
        };
        let token = Uuid::new_v4().to_string();
        let play_links = self.media.play_service.build_play_links_with_server(
            &device, &token, stream_key, Some(&server_tag),
        );

        let stream_info = StreamInfo {
            stream_key: stream_key.to_string(),
            play_url: String::new(),
            rtsp_url: play_links.rtsp_signaling.clone().unwrap_or_default(),
            rtmp_url: String::new(),
            hls_url: play_links.hls.clone().unwrap_or_default(),
            webrtc_url: play_links.webrtc.clone().unwrap_or_default(),
            flv_url: play_links.flv.clone(),
            web_flv_url: play_links.web_flv.clone(),
            media_server_id: server_tag.clone(),
            media_server_name: server_tag.clone(),
        };

        let mut updated_stream = stream.clone();
        updated_stream.media_server_tag = server_tag;
        updated_stream.start();
        self.infra.db.update_stream(&updated_stream).await?;

        self.start_recording_for_device(stream.device_id, stream_key, &updated_stream.media_server_tag).await;
        self.infra.redis.set_stream_info(stream_key, &stream_info).await?;

        tracing::info!("[StreamService] GB28181 stream {} restarted on {}", stream_key, updated_stream.media_server_tag);
        Ok(stream_info)
    }

    async fn start_recording_for_device(&self, device_id: i64, stream_key: &str, media_server: &str) {
        let device = match self.infra.db.devices_cache().get(&device_id) {
            Some(d) => d.clone(),
            None => {
                tracing::debug!(
                    device_id = %device_id,
                    stream_key = %stream_key,
                    "[StreamService] Device not found in cache, skipping recording"
                );
                return;
            }
        };

        let config = device.recording_config();
        if !config.enabled {
            tracing::debug!(
                device_id = %device_id,
                stream_key = %stream_key,
                "[StreamService] Recording not enabled for device"
            );
            return;
        }

        if self.recording_service.get_active_recording_by_stream_key(stream_key).is_some() {
            tracing::info!(
                device_id = %device_id,
                stream_key = %stream_key,
                "[StreamService] Active recording already exists for stream, skipping"
            );
            return;
        }

        match self.recording_service
            .start_recording_for_stream(device_id, stream_key, media_server, &config)
            .await
        {
            Ok(Some(rec)) => {
                tracing::info!(
                    device_id = %device_id,
                    stream_key = %stream_key,
                    recording_id = %rec.id,
                    format = ?rec.format,
                    "[StreamService] Recording task auto-started"
                );
            }
            Ok(None) => {
                tracing::debug!(
                    device_id = %device_id,
                    stream_key = %stream_key,
                    "[StreamService] No recording started (config disabled)"
                );
            }
            Err(e) => {
                tracing::error!(
                    device_id = %device_id,
                    stream_key = %stream_key,
                    error = %e,
                    "[StreamService] Failed to auto-start recording task"
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// StreamManager Trait 实现
// ═══════════════════════════════════════════════════════════

#[async_trait]
impl StreamManager for StreamService {
    async fn start_pull_stream(
        &self, device_id: i64, session_id: i64, rtsp_url: &str,
    ) -> Result<StreamInfo> {
        self.start_pull_stream(device_id, session_id, rtsp_url).await
    }

    async fn start_gb28181_stream(
        &self, device_id: i64, stream_key: &str, session_id: i64, media_server_name: Option<&str>,
    ) -> Result<StreamInfo> {
        self.start_gb28181_stream(device_id, stream_key, session_id, media_server_name).await
    }

    async fn stop_stream(&self, stream_key: &str) -> Result<()> {
        self.stop_stream(stream_key).await
    }

    async fn stop_streams_by_device(&self, device_id: i64) -> Result<()> {
        self.stop_streams_by_device(device_id).await
    }

    async fn generate_token(&self, device_id: &str) -> Result<String> {
        self.media.play_service.generate_token(device_id).await
            .map_err(|e| AppError::Internal(e.to_string()))
    }

    async fn validate_token(&self, token: &str) -> Option<String> {
        self.media.play_service.validate_token(token).await
    }

    fn build_play_links(
        &self, device: &crate::domain::Device, token: &str, stream_id: &str,
    ) -> crate::domain::device::PlayLinks {
        self.media.play_service.build_play_links(device, token, stream_id)
    }

    async fn get_stats(&self) -> serde_json::Value {
        self.get_stats().await
    }

    async fn get_stream_by_stream_key(&self, stream_key: &str) -> Option<Stream> {
        self.get_stream_by_stream_key(stream_key).await
    }

    async fn update_stream_state(&self, stream: &Stream) -> Result<()> {
        self.update_stream_state(stream).await
    }
}

impl Clone for StreamService {
    fn clone(&self) -> Self {
        Self {
            infra: self.infra.clone(),
            media: self.media.clone(),
            recording_service: self.recording_service.clone(),
        }
    }
}