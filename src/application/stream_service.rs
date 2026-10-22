use std::sync::Arc;
use uuid::Uuid;
use async_trait::async_trait;
use crate::context::{InfraContext, MediaContext};
use crate::domain::{Stream, StreamState, DeviceStatus};
use crate::domain::traits::{CacheStore, StreamManager};
use crate::adapter::media_server::{StreamInfo, Protocol as MediaProtocol};
use crate::error::{AppError, Result};
use crate::application::recording_service::RecordingService;

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

    pub async fn start_pull_stream(
        &self,
        device_tag: &str,
        channel_tag: &str,
        rtsp_url: &str,
    ) -> Result<StreamInfo> {
        let device = self.infra.db.devices_cache()
            .get(device_tag)
            .map(|r| r.value().clone())
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_tag)))?;

        let server_tag = if let Some(ref tag) = device.media_server_tag {
            if self.media.cluster.get_server(tag).is_none() {
                return Err(AppError::MediaServerError(
                    format!("Device bound media server '{}' is not online/available", tag)
                ));
            }
            tag.clone()
        } else {
            let server = self.media.cluster
                .select_any_server()
                .await
                .ok_or_else(|| AppError::MediaServerError("No available media server".to_string()))?;
            let new_tag = server.tag().to_string();

            let mut device = device.clone();
            device.media_server_tag = Some(new_tag.clone());
            self.infra.db.update_device(&device).await?;

            new_tag
        };

        let server = self.media.cluster.get_server(&server_tag)
            .ok_or_else(|| AppError::MediaServerError(
                format!("Media server '{}' not found", server_tag)
            ))?;

        let app = device.app.clone().unwrap_or_else(|| "live".to_string());

        let cache_key = format!("{}/{}", device_tag, channel_tag);
        let stream_key = crate::domain::make_stream_key(device_tag, channel_tag);

        let existing_in_cache = {
            let cache = self.infra.db.streams_cache();
            cache.iter().find(|r| r.key() == &cache_key).map(|r| r.value().clone())
        };

        let on_media_server = match server.is_stream_online(&app, &stream_key).await {
            Ok(true) => {
                true
            }
            Ok(false) => {
                false
            }
            Err(e) => {
                tracing::warn!("[StreamService] is_stream_online failed: {}, assuming not exists", e);
                false
            }
        };

        let proxy_added = if !on_media_server {
            let rtsp_url_with_auth = if let (Some(user), Some(pass)) = (&device.device_username, &device.device_password) {
                if rtsp_url.starts_with("rtsp://") && !rtsp_url.contains('@') {
                    if let Some(stripped) = rtsp_url.strip_prefix("rtsp://") {
                        format!("rtsp://{}:{}@{}", user, pass, stripped)
                    } else {
                        rtsp_url.to_string()
                    }
                } else {
                    rtsp_url.to_string()
                }
            } else {
                rtsp_url.to_string()
            };
            match server.add_stream_proxy(&app, &stream_key, &rtsp_url_with_auth).await {
                Ok(stream_info) => {
                    if let Err(e) = self.infra.redis.set_stream_info(&stream_key, &stream_info).await {
                        tracing::warn!("[StreamService] set_stream_info failed: {}", e);
                    }
                    true
                }
                Err(e) => {
                    tracing::error!("[StreamService] add_stream_proxy failed for {}: {}", stream_key, e);
                    false
                }
            }
        } else {
            false
        };

        if let Some(existing) = existing_in_cache {
            let mut updated_stream = existing.clone();
            if existing.app != app {
                updated_stream.app = app.clone();
            }
            if on_media_server {
                updated_stream.start();
            } else if proxy_added {
                updated_stream.state = crate::domain::StreamState::Starting;
            } else {
                updated_stream.state = crate::domain::StreamState::Error;
            }
            self.infra.db.update_stream(&updated_stream).await?;
            if updated_stream.state == crate::domain::StreamState::Active {
                self.start_recording_for_channel(device_tag, channel_tag, &server_tag, &app, &stream_key).await;
            }
        } else {
            let streams = self.infra.db.list_streams().await;
            if let Some(existing) = streams.iter().find(|s| s.device_tag.as_deref() == Some(device_tag) && s.channel_tag.as_deref() == Some(channel_tag)) {
                let mut updated = existing.clone();
                if on_media_server {
                    updated.start();
                } else if proxy_added {
                    updated.state = crate::domain::StreamState::Starting;
                } else {
                    updated.state = crate::domain::StreamState::Error;
                }
                updated.retry_count = 0;
                updated.last_error = None;
                updated.media_server_tag = server_tag.clone();
                let should_record = updated.state == crate::domain::StreamState::Active;
                if let Err(e) = self.infra.db.update_stream(&updated).await {
                    tracing::warn!("[StreamService] update_stream failed: {}", e);
                }
                if should_record {
                    self.start_recording_for_channel(device_tag, channel_tag, &server_tag, &app, &stream_key).await;
                }
            } else {
                let token = Uuid::new_v4().to_string();
                let mut stream = Stream::new(server_tag.clone(), device_tag.to_string(), channel_tag.to_string(), app.clone(), token);
                if on_media_server {
                    stream.start();
                } else if proxy_added {
                    stream.state = crate::domain::StreamState::Starting;
                } else {
                    stream.state = crate::domain::StreamState::Error;
                }
                if let Err(e) = self.infra.db.create_stream(&stream).await {
                    tracing::warn!("[StreamService] create_stream failed: {}", e);
                }
                if stream.state == crate::domain::StreamState::Active {
                    self.start_recording_for_channel(device_tag, channel_tag, &server_tag, &app, &stream_key).await;
                }
            }
        }

        let play_url = server.get_play_url(&app, &stream_key, crate::adapter::media_server::Protocol::Rtsp).await
            .unwrap_or_default();

        Ok(StreamInfo {
            stream_key,
            play_url,
            rtsp_url: rtsp_url.to_string(),
            rtmp_url: String::new(),
            hls_url: String::new(),
            webrtc_url: String::new(),
            flv_url: None,
            web_flv_url: None,
            media_server_id: server_tag.clone(),
            media_server_name: server_tag,
        })
    }

    async fn sync_stream_to_active(&self, device_tag: &str, channel_tag: &str, server_tag: &str, _stream_key: &str, app: &str) -> Result<()> {
        let cache_key = format!("{}/{}", device_tag, channel_tag);
        let stream_key = crate::domain::make_stream_key(device_tag, channel_tag);
        let cache = self.infra.db.streams_cache();

        let stream_to_update = {
            let existing = cache.iter()
                .find(|r| r.key() == &cache_key);
            match existing {
                Some(r) => {
                    if r.value().state != crate::domain::StreamState::Active {
                        let mut s = r.value().clone();
                        s.start();
                        Some(s)
                    } else {
                        None
                    }
                }
                None => None,
            }
        };

        if let Some(s) = stream_to_update {
            if let Err(e) = self.infra.db.update_stream(&s).await {
                tracing::warn!("[StreamService] sync_stream_to_active: update_stream failed: {}", e);
            }
        } else {
            let token = uuid::Uuid::new_v4().to_string();
            let stream = crate::domain::Stream::new(server_tag.to_string(), device_tag.to_string(), channel_tag.to_string(), app.to_string(), token);
            if let Err(e) = self.infra.db.create_stream(&stream).await {
                tracing::warn!("[StreamService] sync_stream_to_active: create_stream failed: {}", e);
            }
        }

        self.start_recording_for_channel(device_tag, channel_tag, server_tag, app, &stream_key).await;
        Ok(())
    }

    pub async fn start_gb28181_stream(
        &self,
        device_tag: &str,
        channel_tag: &str,
        stream_key: &str,
    ) -> Result<StreamInfo> {

        let device = self.infra.db.devices_cache()
            .get(device_tag)
            .map(|r| r.value().clone())
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_tag)))?;
        let app = device.app.clone().unwrap_or_else(|| "live".to_string());

        let server_tag = if let Some(ref tag) = device.media_server_tag {
            if self.media.cluster.get_server(tag).is_none() {
                return Err(AppError::MediaServerError(
                    format!("Device bound media server '{}' is not available", tag)
                ));
            }
            tag.clone()
        } else {
            let server = self.media.cluster
                .select_any_server()
                .await
                .ok_or_else(|| AppError::MediaServerError("No available media server".to_string()))?;
            let new_tag = server.tag().to_string();

            let mut device = device.clone();
            device.media_server_tag = Some(new_tag.clone());
            self.infra.db.update_device(&device).await?;

            new_tag
        };

        let token = Uuid::new_v4().to_string();
        let media_server_stream_key = crate::domain::make_stream_key(device_tag, channel_tag);
        let stream = Stream::new(server_tag.clone(), device_tag.to_string(), channel_tag.to_string(), app.clone(), token.clone());
        self.infra.db.create_stream(&stream).await?;

        self.start_recording_for_channel(device_tag, channel_tag, &server_tag, &app, &media_server_stream_key).await;

        let play_links = self.media.play_service.build_play_links_with_server(
            &device, &token, &media_server_stream_key, Some(&server_tag), Some("rtp"),
        ).await;

        let stream_info = StreamInfo {
            stream_key: media_server_stream_key,
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

    pub async fn stop_stream(&self, _app: &str, stream_key: &str) -> Result<()> {
        let (stream, server_tag) = {
            let cache_key = stream_key.replace('_', "/");
            let entry = self.infra.db.streams_cache().get(&cache_key).map(|s| s.clone());
            match entry {
                Some(s) => (s.clone(), s.media_server_tag.clone()),
                None => {
                    return Ok(());
                }
            }
        };

        if let Some(server) = self.media.cluster.get_server(&server_tag) {
            if let Err(e) = server.remove_stream_proxy(&stream.app, stream_key).await {
                tracing::warn!("[StreamService] Failed to remove proxy on {}: {}", server_tag, e);
            }
            if let Err(_e) = server.close_rtp_server(stream_key).await {
            }
        } else {
            tracing::warn!("[StreamService] Media server '{}' not found, skipping proxy removal", server_tag);
        }

        if let Err(e) = self.recording_service.stop_recording_by_stream_key(stream_key).await {
            tracing::warn!("[StreamService] Failed to stop recording: {}", e);
        }

        self.terminate_sessions_for_stream(stream_key).await;

        let mut stream = stream;
        stream.state = StreamState::Idle;
        self.infra.db.update_stream(&stream).await?;
        self.infra.redis.delete_stream_info(stream_key).await?;
        if let Some(ref dt) = stream.device_tag {
            self.sync_device_status(dt, StreamState::Idle).await;
        }

        Ok(())
    }

    pub async fn restart_stream_by_id(&self, stream_id: i64) -> Result<StreamInfo> {
        let stream = self.get_stream(stream_id).await?
            .ok_or_else(|| AppError::NotFound(format!("Stream {} not found", stream_id)))?;

        let device_tag = stream.device_tag.as_ref()
            .ok_or_else(|| AppError::NotFound(format!("Stream has no device_tag")))?;
        let device = self.infra.db.get_device_by_device_tag(device_tag)
            .ok_or_else(|| AppError::NotFound(format!("Device with tag {} not found", device_tag)))?
            .clone();

        let tag = stream.media_server_tag.clone();
        let app = stream.app.clone();
        let stream_key = crate::domain::stream::make_stream_key(stream.device_tag.as_deref().unwrap_or(""), stream.channel_tag.as_deref().unwrap_or(""));

        let adapter = self.media.cluster.get_server(&tag)
            .ok_or_else(|| AppError::MediaServerError(
                format!("Media server '{}' not found", tag)
            ))?;

        match adapter.is_stream_online(&app, &stream_key).await {
            Ok(true) => {
                let mut updated_stream = stream.clone();
                updated_stream.start();
                updated_stream.retry_count = 0;
                updated_stream.last_error = None;
                self.infra.db.update_stream(&updated_stream).await?;

                let stream_info = StreamInfo {
                    stream_key: stream_key.clone(),
                    play_url: String::new(),
                    rtsp_url: format!("rtsp://{}/{}", tag, stream_key),
                    rtmp_url: String::new(),
                    hls_url: String::new(),
                    webrtc_url: String::new(),
                    flv_url: None,
                    web_flv_url: None,
                    media_server_id: tag.clone(),
                    media_server_name: tag,
                };
                return Ok(stream_info);
            }
            Ok(false) | Err(_) => {
            }
        }

        let rtsp_url = device.select_source()
            .map(|(_, url)| {
                let (user, pass) = match (device.device_username.as_ref(), device.device_password.as_ref()) {
                    (Some(u), Some(p)) => (u.as_str(), p.as_str()),
                    _ => return url,
                };
                if let Some(stripped) = url.strip_prefix("rtsp://") {
                    format!("rtsp://{}:{}@{}", user, pass, stripped)
                } else {
                    url
                }
            })
            .unwrap_or_default();

        let stream_info = adapter.add_stream_proxy(&app, &stream_key, &rtsp_url).await?;

        let mut updated_stream = stream.clone();
        updated_stream.start();
        updated_stream.retry_count = 0;
        updated_stream.last_error = None;
        self.infra.db.update_stream(&updated_stream).await?;

        if let Some(ref dt) = stream.device_tag {
            self.sync_device_status(dt, StreamState::Active).await;
        }

        Ok(stream_info)
    }

    async fn sync_device_status(&self, device_tag: &str, stream_state: StreamState) {
        let mut device = match self.infra.db.devices_cache()
            .get(device_tag)
            .map(|r| r.value().clone()) {
            Some(d) => d,
            None => return,
        };

        if device.status == DeviceStatus::Maintaining {
            return;
        }

        let new_status = match stream_state {
            StreamState::Active | StreamState::Starting | StreamState::Recovering => DeviceStatus::Online,
            StreamState::Error => DeviceStatus::Error,
            StreamState::Idle | StreamState::Stopping | StreamState::Stopped => DeviceStatus::Offline,
        };

        if device.status != new_status {
            device.status = new_status;
            if let Err(e) = self.infra.db.update_device(&device).await {
                tracing::warn!("[StreamService] Failed to sync device status: {}", e);
            }
        }
    }

    pub async fn stop_streams_by_device(&self, device_tag: &str) -> Result<()> {
        let cache_key_prefix = format!("{}/", device_tag);
        let cache = self.infra.db.streams_cache();
        let streams: Vec<Stream> = cache
            .iter()
            .filter(|r| r.key().starts_with(&cache_key_prefix))
            .map(|r| r.value().clone())
            .collect();

        for stream in streams {
            let stream_key = crate::domain::stream::make_stream_key(stream.device_tag.as_deref().unwrap_or(""), stream.channel_tag.as_deref().unwrap_or(""));
            if let Err(e) = self.stop_stream(&stream.app, &stream_key).await {
                tracing::warn!("[StreamService] stop_streams_by_device: failed to stop {}: {}", stream_key, e);
            }
        }
        Ok(())
    }

    pub async fn stop_streams_by_channel(&self, device_tag: &str, channel_tag: &str) -> Result<()> {
        let cache_key = format!("{}/{}", device_tag, channel_tag);
        let cache = self.infra.db.streams_cache();
        let streams: Vec<Stream> = cache
            .iter()
            .filter(|r| r.key() == &cache_key)
            .map(|r| r.value().clone())
            .collect();

        for stream in streams {
            let stream_key = crate::domain::stream::make_stream_key(stream.device_tag.as_deref().unwrap_or(""), stream.channel_tag.as_deref().unwrap_or(""));
            if let Err(e) = self.stop_stream(&stream.app, &stream_key).await {
                tracing::warn!("[StreamService] stop_streams_by_channel: failed to stop {}: {}", stream_key, e);
            }
        }
        Ok(())
    }

    async fn terminate_sessions_for_stream(&self, stream_key: &str) {
        let session_ids: Vec<i64> = self.infra.db.sessions_cache()
            .iter()
            .filter(|s| s.stream_key() == stream_key)
            .filter(|s| s.state != crate::domain::SessionState::Terminated)
            .filter(|s| s.state != crate::domain::SessionState::Terminating)
            .map(|s| s.id)
            .collect();

        let count = session_ids.len();
        for session_id in session_ids {

            let updated = {
                let cache = self.infra.db.sessions_cache();
                cache.get(&session_id).map(|s| {
                    let mut s = s.clone();
                    s.terminate();
                    s
                })
            };
            if let Some(s) = updated {
                self.infra.db.sessions_cache().insert(session_id, s);
            }
        }
        if count > 0 {
        }
    }

    pub async fn get_stream(&self, id: i64) -> Result<Option<Stream>> {
        Ok(self.infra.db.list_streams().await
            .into_iter()
            .find(|s| s.id == id))
    }

    pub async fn list_all_streams(&self) -> Vec<Stream> {
        self.infra.db.list_streams().await
    }

    pub async fn list_streams_paginated(&self, search: Option<&str>, state: Option<&str>, limit: usize, offset: usize) -> Vec<Stream> {
        let streams = self.infra.db.list_streams_paginated(search, state, limit, offset).await;
        streams
    }

    pub async fn count_streams(&self, search: Option<&str>, state: Option<&str>) -> usize {
        self.infra.db.count_streams(search, state).await
    }

    pub async fn list_active_streams(&self) -> Vec<Stream> {
        self.infra.db.list_streams().await
            .into_iter()
            .filter(|s| s.state == StreamState::Active)
            .collect()
    }

    pub async fn get_streams_by_device(&self, device_tag: &str) -> Vec<Stream> {
        let cache_key_prefix = format!("{}/", device_tag);
        let cache = self.infra.db.streams_cache();
        cache.iter()
            .filter(|r| r.key().starts_with(&cache_key_prefix))
            .map(|r| r.value().clone())
            .collect()
    }

    pub async fn get_streams_by_channel(&self, device_tag: &str, channel_tag: &str) -> Vec<Stream> {
        let cache_key = format!("{}/{}", device_tag, channel_tag);
        let cache = self.infra.db.streams_cache();
        cache.iter()
            .filter(|r| r.key() == &cache_key)
            .map(|r| r.value().clone())
            .collect()
    }

    pub fn find_stream(&self, device_tag: &str, channel_tag: &str) -> Option<Stream> {
        let cache_key = format!("{}/{}", device_tag, channel_tag);
        self.infra.db.streams_cache().get(&cache_key).map(|s| s.value().clone())
    }

    pub async fn get_play_url(&self, stream_key: &str, protocol: MediaProtocol) -> Result<String> {
        let info = self.infra.redis.get_stream_info(stream_key)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Stream {} not found", stream_key)))?;

        match protocol {
            MediaProtocol::Rtsp => Ok(info.rtsp_url),
            MediaProtocol::Rtmp => Ok(info.rtmp_url),
            MediaProtocol::Hls => Ok(info.hls_url),
            MediaProtocol::Http => Err(AppError::BadRequest("Http is not a playable protocol".to_string())),
            MediaProtocol::WebRTC => Ok(info.webrtc_url),
            MediaProtocol::Flv => info.flv_url
                .ok_or_else(|| AppError::BadRequest("FLV URL not available".to_string())),
            MediaProtocol::WsFlv => info.web_flv_url
                .ok_or_else(|| AppError::BadRequest("WS-FLV URL not available".to_string())),
        }
    }

    pub async fn get_stats(&self) -> serde_json::Value {
        let streams = self.infra.db.list_streams().await;
        let total = streams.len();
        let active = streams.iter().filter(|s| s.state == StreamState::Active).count();

        serde_json::json!({
            "total": total,
            "active": active,
        })
    }

    pub async fn get_stream_by_token(&self, token: &str) -> Result<Option<Stream>> {
        self.infra.db.get_stream_by_token(token).await
    }

    pub async fn get_stream_by_stream_key(&self, _app: &str, stream_key: &str) -> Option<Stream> {
        let cache_key = stream_key.replace('_', "/");
        self.infra.db.streams_cache()
            .get(&cache_key)
            .map(|s| s.clone())
    }

    pub async fn update_stream_state(&self, stream: &Stream) -> Result<()> {
        self.infra.db.update_stream(stream).await
    }

    pub async fn restart_stream(
        &self,
        stream_key: &str,
        rtsp_url: &str,
    ) -> Result<StreamInfo> {
        let cache_key = match self.infra.db.get_stream_cache_key_by_stream_key(stream_key) {
            Some(key) => key,
            None => return Err(AppError::NotFound(format!("Stream {} not found", stream_key))),
        };

        let stream = match self.infra.db.streams_cache().get(&cache_key) {
            Some(s) => s.clone(),
            None => return Err(AppError::NotFound(format!("Stream {} not in cache", stream_key))),
        };

        let server_tag = &stream.media_server_tag;
        let server = self.media.cluster.get_server(server_tag)
            .ok_or_else(|| AppError::MediaServerError(
                format!("Media server '{}' not found for stream", server_tag)
            ))?;

        if let Some(old_server) = self.media.cluster.get_server(&stream.media_server_tag) {
            let _ = old_server.remove_stream_proxy(&stream.app, stream_key).await;
            let _ = old_server.close_rtp_server(stream_key).await;
        }

        let stream_info = server.add_stream_proxy(&stream.app, stream_key, rtsp_url).await
            .map_err(|e| AppError::MediaServerError(format!("Media server error: {}", e)))?;

        let mut updated_stream = stream.clone();
        updated_stream.start();
        self.infra.db.update_stream(&updated_stream).await?;

        if let Some(ref dt) = stream.device_tag {
            self.sync_device_status(dt, StreamState::Active).await;
        }
        if let (Some(ref dt), Some(ref ct)) = (&stream.device_tag, &stream.channel_tag) {
            self.start_recording_for_channel(dt, ct, &updated_stream.media_server_tag, &updated_stream.app, stream_key).await;
        }
        self.infra.redis.set_stream_info(stream_key, &stream_info).await?;

        Ok(stream_info)
    }

    pub async fn restart_gb28181_stream(
        &self,
        stream_key: &str,
    ) -> Result<StreamInfo> {
        let cache_key = match self.infra.db.get_stream_cache_key_by_stream_key(stream_key) {
            Some(key) => key,
            None => return Err(AppError::NotFound(format!("Stream {} not found", stream_key))),
        };

        let stream = match self.infra.db.streams_cache().get(&cache_key) {
            Some(s) => s.clone(),
            None => return Err(AppError::NotFound(format!("Stream {} not in cache", stream_key))),
        };

        let server_tag = &stream.media_server_tag;
        let _server = self.media.cluster.get_server(server_tag)
            .ok_or_else(|| AppError::MediaServerError(
                format!("Media server '{}' not found for stream", server_tag)
            ))?;

        if let Some(old_server) = self.media.cluster.get_server(&stream.media_server_tag) {
            let _ = old_server.remove_stream_proxy(&stream.app, stream_key).await;
            let _ = old_server.close_rtp_server(stream_key).await;
        }

        let device_tag = stream.device_tag.as_ref()
            .ok_or_else(|| AppError::NotFound(format!("Stream has no device_tag")))?;
        let device = match self.infra.db.get_device_by_device_tag(device_tag) {
            Some(d) => d,
            None => {
                return Err(AppError::NotFound(format!("Device with tag {} not found", device_tag)));
            }
        };
        let token = Uuid::new_v4().to_string();
        let play_links = self.media.play_service.build_play_links_with_server(
            &device, &token, stream_key, Some(server_tag.as_str()), Some("rtp"),
        ).await;

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
        updated_stream.media_server_tag = server_tag.clone();
        updated_stream.start();
        self.infra.db.update_stream(&updated_stream).await?;

        if let Some(ref dt) = stream.device_tag {
            self.sync_device_status(dt, StreamState::Active).await;
        }
        if let (Some(ref dt), Some(ref ct)) = (&stream.device_tag, &stream.channel_tag) {
            self.start_recording_for_channel(dt, ct, &updated_stream.media_server_tag, &updated_stream.app, stream_key).await;
        }
        self.infra.redis.set_stream_info(stream_key, &stream_info).await?;

        Ok(stream_info)
    }

    async fn start_recording_for_channel(&self, device_tag: &str, _channel_tag: &str, media_server: &str, _app: &str, stream_key: &str) {
        let device = match self.infra.db.devices_cache()
            .get(device_tag)
            .map(|r| r.value().clone()) {
            Some(d) => d,
            None => {
                return;
            }
        };

        let config = device.recording_config();
        if !config.enabled {
            return;
        }

        match self.recording_service.get_active_recording_by_stream_key(stream_key).await {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("[StreamService] Failed to check active recording: {}", e);
            }
        }

        match self.recording_service
            .start_recording_for_stream(device_tag.to_string(), stream_key, media_server, &config)
            .await
        {
            Ok(Some(_rec)) => {
            }
            Ok(None) => {}
            Err(e) => {
                tracing::error!("[StreamService] Failed to auto-start recording: {}", e);
            }
        }
    }
}

#[async_trait]
impl StreamManager for StreamService {
    async fn start_pull_stream(
        &self, device_tag: &str, channel_tag: &str, rtsp_url: &str,
    ) -> Result<StreamInfo> {
        StreamService::start_pull_stream(self, device_tag, channel_tag, rtsp_url).await
    }

    async fn start_gb28181_stream(
        &self, device_tag: &str, channel_tag: &str, stream_key: &str,
    ) -> Result<StreamInfo> {
        StreamService::start_gb28181_stream(self, device_tag, channel_tag, stream_key).await
    }

    async fn stop_stream(&self, app: &str, stream_key: &str) -> Result<()> {
        StreamService::stop_stream(self, app, stream_key).await
    }

    async fn stop_streams_by_device(&self, device_tag: &str) -> Result<()> {
        StreamService::stop_streams_by_device(self, device_tag).await
    }

    async fn stop_streams_by_channel(&self, device_tag: &str, channel_tag: &str) -> Result<()> {
        StreamService::stop_streams_by_channel(self, device_tag, channel_tag).await
    }

    async fn generate_token(&self, device_id: &str) -> Result<String> {
        self.media.play_service.generate_token(device_id).await
            .map_err(|e| AppError::Internal(e.to_string()))
    }

    async fn validate_token(&self, token: &str) -> Option<String> {
        self.media.play_service.validate_token(token).await
    }

    async fn build_play_links(
        &self, device: &crate::domain::Device, token: &str, stream_id: &str,
    ) -> crate::domain::device::PlayLinks {
        self.media.play_service.build_play_links(device, token, stream_id).await
    }

    async fn get_stats(&self) -> serde_json::Value {
        self.get_stats().await
    }

    async fn get_stream_by_stream_key(&self, app: &str, stream_key: &str) -> Option<Stream> {
        self.get_stream_by_stream_key(app, stream_key).await
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