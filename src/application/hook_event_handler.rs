use std::sync::Arc;
use crate::infrastructure::{DbRepository, EventBus};
use crate::infrastructure::RedisStore;
use crate::protocol::event::SignalEvent;
use crate::domain::{Session, SessionType, SessionState, Stream, StreamState};

use crate::application::recording_service::RecordingService;
use uuid::Uuid;

pub struct HookEventHandler {
    db: Arc<DbRepository>,
    redis: Arc<RedisStore>,
    event_bus: Arc<EventBus>,
    session_expiry_secs: i64,
    recording_service: Arc<RecordingService>,
}

impl HookEventHandler {
    pub fn new(
        db: Arc<DbRepository>,
        redis: Arc<RedisStore>,
        event_bus: Arc<EventBus>,
        session_expiry_secs: i64,
        recording_service: Arc<RecordingService>,
    ) -> Self {
        Self {
            db,
            redis,
            event_bus,
            session_expiry_secs,
            recording_service,
        }
    }

    pub async fn start(self) {
        tokio::spawn(async move {
            loop {
                match self.event_bus.subscribe().recv().await {
                    Ok(event) => {
                        if matches!(event, SignalEvent::HookPlay { .. } |
                            SignalEvent::HookPublish { .. } |
                            SignalEvent::HookUnpublish { .. } |
                            SignalEvent::HookStreamUpdate { .. } |
                            SignalEvent::HookStreamChanged { .. } |
                            SignalEvent::HookRecordDone { .. }) {
                            self.handle_event(&event).await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("[HookEventHandler] Event bus lagged {} events", n);
                    }
                }
            }
        });
    }

    async fn handle_event(&self, event: &SignalEvent) {
        match event {
            SignalEvent::HookPlay { media_server_tag, stream_id, client_ip, app, tcp_id } => {
                self.handle_hook_play(media_server_tag, stream_id, client_ip.clone(), app.as_deref(), tcp_id.as_deref()).await;
            }
            SignalEvent::HookPublish { media_server_tag, stream_id, client_ip, device_tag, channel_tag } => {
                self.handle_hook_publish(media_server_tag, stream_id, client_ip.clone(), device_tag.clone(), channel_tag.clone()).await;
            }
            SignalEvent::HookUnpublish { media_server_tag, stream_id, app, tcp_id } => {
                self.handle_hook_unpublish(media_server_tag, stream_id, app.as_deref(), tcp_id.as_deref()).await;
            }
            SignalEvent::HookStreamUpdate { media_server_tag, stream_id, regist, reader_count, total_reader_count, bytes_speed } => {
                self.handle_hook_stream_update(media_server_tag, stream_id, *regist, *reader_count, *total_reader_count, *bytes_speed).await;
            }
            SignalEvent::HookStreamChanged { media_server_tag, app: _, stream_id, regist } => {
                self.handle_hook_stream_changed(media_server_tag, stream_id, *regist).await;
            }
            SignalEvent::HookRecordDone { media_server_tag, stream_id, app, file_path, file_size, time_len, url, start_time, file_name } => {
                self.handle_hook_record_done(
                    media_server_tag,
                    stream_id,
                    app,
                    file_path.as_deref(),
                    *file_size,
                    *time_len,
                    url.as_deref(),
                    *start_time,
                    file_name.as_deref(),
                ).await;
            }
            _ => {}
        }
    }

    async fn handle_hook_play(&self, media_server_tag: &str, stream_id: &str, client_ip: Option<String>, app: Option<&str>, tcp_id: Option<&str>) {

        let stream_key = stream_id;
        let stream = self.find_stream_by_app_and_key(stream_key, app).await;

        // 同一条 TCP 连接只会有一条播放会话：RTSP 走专属鉴权(on_rtsp_auth)时
        // 不会触发 on_play，两处都要能建会话，但不能重复建
        if let Some(tcp_id) = tcp_id {
            let existing = self.db.sessions_cache()
                .iter()
                .find(|s| s.tcp_id.as_deref() == Some(tcp_id) && s.state != SessionState::Terminated)
                .map(|s| s.value().id);
            if let Some(existing_id) = existing {
                tracing::debug!(
                    "[HookEventHandler] HookPlay: session {} already tracks connection {}",
                    existing_id, tcp_id
                );
                return;
            }
        }

        if let Some(mut s) = stream.clone() {
            s.increment_viewers();
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookPlay: failed to update stream viewers: {}", e);
            }
        }

        let mut session = Session::new(SessionType::Play, 0);
        session.state = SessionState::Active;
        session.client_ip = client_ip;
        session.tcp_id = tcp_id.map(|s| s.to_string());
        session.media_server_tag = Some(media_server_tag.to_string());

        if let Some(ref stream) = stream {
            session.device_tag = stream.device_tag.clone();
            session.channel_tag = stream.channel_tag.clone();
        }
        session.refresh_expiry(self.session_expiry_secs);

        match self.db.create_session(&session).await {
            Ok(id) => {
                session.id = id;
            }
            Err(e) => {
                tracing::error!("[HookEventHandler] HookPlay: failed to create session: {}", e);
                return;
            }
        }

        self.db.sessions_cache().insert(session.id, session);

        tracing::debug!("[HookEventHandler] HookPlay: app={:?} stream={} tcp_id={:?}", app, stream_key, tcp_id);
    }

    async fn handle_hook_publish(&self, media_server_tag: &str, stream_id: &str, _client_ip: Option<String>, device_tag_from_params: Option<String>, channel_tag_from_params: Option<String>) {

        let (device_tag, channel_tag) = if let (Some(dt), Some(ct)) = (&device_tag_from_params, &channel_tag_from_params) {
            (dt.clone(), ct.clone())
        } else {
            crate::common::device_utils::parse_stream_key(stream_id)
                .unwrap_or_else(|| (stream_id.to_string(), stream_id.to_string()))
        };

        let cache_key = format!("{}/{}", device_tag, channel_tag);
        if let Some(s) = self.db.streams_cache().get(&cache_key).map(|s| s.clone()) {
            let mut s = s;
            s.update_keepalive();
            if s.state == StreamState::Idle || s.state == StreamState::Error {
                s.start();
            }
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookPublish: failed to update stream: {}", e);
            }
            return;
        }

        let app = self.db.devices_cache().get(&device_tag)
            .map(|d| d.app.clone().unwrap_or_else(|| "live".to_string()))
            .unwrap_or_else(|| "live".to_string());

        let stream_key = crate::common::device_utils::format_stream_key(&device_tag, &channel_tag);
        self.start_recording_if_enabled(&device_tag, &channel_tag, media_server_tag, &app, &stream_key).await;

        let mut stream = Stream::new(
            media_server_tag.to_string(),
            device_tag.clone(),
            channel_tag.clone(),
            app,
            Uuid::new_v4().to_string(),
        );
        stream.start();

        if let Err(e) = self.db.create_stream(&stream).await {
            tracing::error!("[HookEventHandler] HookPublish: failed to create stream: {}", e);
            return;
        }

    }

    async fn handle_hook_unpublish(&self, _media_server_tag: &str, stream_id: &str, app: Option<&str>, tcp_id: Option<&str>) {

        tracing::info!("[HookEventHandler] HookUnpublish: app={:?} stream={} tcp_id={:?}", app, stream_id, tcp_id);

        if let Some(mut s) = self.find_stream_by_stream_key(stream_id).await {
            s.stopped();
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookUnpublish: failed to update stream: {}", e);
            }
        }

        self.terminate_sessions_of_stream(stream_id, app).await;

        self.decrement_stream_viewers(stream_id).await;
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_hook_record_done(
        &self,
        media_server_tag: &str,
        stream_id: &str,
        app: &str,
        file_path: Option<&str>,
        file_size: Option<u64>,
        time_len: Option<f64>,
        url: Option<&str>,
        start_time: Option<u64>,
        file_name: Option<&str>,
    ) {

        let size = file_size.unwrap_or(0);
        let duration = time_len.map(|t| t as u64).unwrap_or(0);

        let source_path = file_path
            .or(url)
            .map(|s| s.to_string())
            .or_else(|| Some(media_server_tag.to_string()));

        tracing::debug!(
            "[HookEventHandler] HookRecordDone: app={} stream={} file={:?} size={} duration={} start_time={:?} file_name={:?}",
            app, stream_id, source_path, size, duration, start_time, file_name
        );

        if let Err(e) = self
            .recording_service
            .finalize_recording(stream_id, source_path, size, duration, start_time)
            .await
        {
            tracing::error!("[HookEventHandler] HookRecordDone: failed to record segment: {}", e);
        }
    }

    async fn terminate_sessions_of_stream(&self, stream_id: &str, app: Option<&str>) {
        let (device_tag, channel_tag) = self.resolve_stream_identity(stream_id, app).await;

        let cached: Vec<Session> = self.db.sessions_cache()
            .iter()
            .filter(|s| s.stream_key() == stream_id && s.state != SessionState::Terminated)
            .map(|s| s.value().clone())
            .collect();

        for mut s in cached {
            s.mark_terminated();
            if let Err(e) = self.db.update_session_state(&s).await {
                tracing::warn!("[HookEventHandler] terminate_sessions_of_stream: failed to persist session {}: {}", s.id, e);
            }
            self.db.sessions_cache().insert(s.id, s);
        }

        if let (Some(dt), Some(ct)) = (&device_tag, &channel_tag) {
            if let Err(e) = self.db.update_session_states_by_stream(dt, ct, SessionState::Terminated).await {
                tracing::warn!("[HookEventHandler] terminate_sessions_of_stream: failed to update sessions of {}/{}: {}", dt, ct, e);
            }
        }
    }

    async fn resolve_stream_identity(&self, stream_id: &str, app: Option<&str>) -> (Option<String>, Option<String>) {
        if let Some(s) = self.find_stream_by_app_and_key(stream_id, app).await {
            return (s.device_tag.clone(), s.channel_tag.clone());
        }

        crate::common::device_utils::parse_stream_key(stream_id)
            .map(|(dt, ct)| (Some(dt), Some(ct)))
            .unwrap_or((None, None))
    }

    async fn find_stream_by_app_and_key(&self, stream_key: &str, app: Option<&str>) -> Option<Stream> {
        if let Some(s) = self.find_stream_by_stream_key(stream_key).await {
            let app_matches = app.map(|a| a == s.app).unwrap_or(true);
            if app_matches {
                return Some(s);
            }
        }

        self.db.streams_cache()
            .iter()
            .find_map(|entry| {
                let s = entry.value();
                if let Some(app) = app {
                    if s.app != app {
                        return None;
                    }
                }
                let key = crate::common::device_utils::format_stream_key(
                    s.device_tag.as_deref().unwrap_or(""),
                    s.channel_tag.as_deref().unwrap_or(""),
                );
                if key == stream_key {
                    Some(s.clone())
                } else {
                    None
                }
            })
    }

    async fn decrement_stream_viewers(&self, stream_key: &str) {
        if let Some(mut s) = self.find_stream_by_stream_key(stream_key).await {
            s.decrement_viewers();
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] decrement_stream_viewers: failed to update stream: {}", e);
            }
        }
    }

    async fn handle_hook_stream_update(&self, media_server_tag: &str, stream_id: &str, regist: bool, _reader_count: Option<u32>, _total_reader_count: Option<u32>, _bytes_speed: Option<u64>) {

        if !regist {
            self.handle_hook_unpublish(media_server_tag, stream_id, None, None).await;
            return;
        }

        let mut became_active = false;
        if let Some(mut s) = self.find_stream_by_stream_key(stream_id).await {
            s.update_keepalive();
            if s.state != StreamState::Active {
                s.start();
                became_active = true;
            }
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookStreamUpdate: failed to update stream: {}", e);
            }
        }

        if became_active {
            let device_tag = crate::common::device_utils::parse_stream_key(stream_id)
                .map(|(device_tag, _)| device_tag)
                .unwrap_or_else(|| stream_id.to_string());
            let device_id = self
                .db
                .device_cached(&device_tag)
                .map(|d| d.id)
                .unwrap_or(0);
            let _ = self.event_bus.publish(SignalEvent::StreamStart {
                device_id,
                stream_key: stream_id.to_string(),
            }).await;
        }

        if let Some(device_id) = self.find_device_id_by_push_stream(stream_id).await {

            let cached_device = self.db.device_cached(&device_id);
            if let Some(mut device) = cached_device {
                device.set_online();
                if let Err(e) = self.db.update_device(&device).await {
                    tracing::warn!("[HookEventHandler] HookStreamUpdate: failed to update device online status: {}", e);
                }
            }
        }
    }

    async fn find_stream_by_stream_key(&self, stream_key: &str) -> Option<Stream> {
        if let Some((device_tag, channel_tag)) = crate::common::device_utils::parse_stream_key(stream_key) {
            let cache_key = format!("{}/{}", device_tag, channel_tag);
            if let Some(s) = self.db.streams_cache().get(&cache_key) {
                return Some(s.clone());
            }
        }
        if let Some(s) = self.db.streams_cache().get(stream_key) {
            return Some(s.clone());
        }
        let cache_key = stream_key.replace('_', "/");
        if let Some(s) = self.db.streams_cache().get(&cache_key) {
            return Some(s.clone());
        }
        None
    }

    async fn find_device_id_by_push_stream(&self, stream_id: &str) -> Option<String> {
        let (device_tag, _) = crate::common::device_utils::parse_stream_key(stream_id)
            .unwrap_or_else(|| (stream_id.to_string(), stream_id.to_string()));
        Some(device_tag)
    }

    async fn start_recording_if_enabled(&self, device_tag: &str, _channel_tag: &str, media_server: &str, _app: &str, stream_key: &str) {
        let device = match self.db.devices_cache().get(device_tag).map(|r| r.value().clone()) {
            Some(d) => d,
            None => {
                return;
            }
        };

        let config = device.recording_config();
        if !config.enabled {
            return;
        }

        // 已经有定时录像任务时不再重复创建：报警录像任务可以并存
        match self.recording_service.get_active_recordings_by_stream_key(stream_key).await {
            Ok(list) => {
                if list.iter().any(|r| r.recording_type == crate::domain::recording::RecordingType::Timed) {
                    return;
                }
            }
            Err(e) => {
                tracing::warn!("[HookEventHandler] Failed to check active recording: {}", e);
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
                tracing::error!("[HookEventHandler] Failed to auto-start recording: {}", e);
            }
        }
    }

    async fn handle_hook_stream_changed(&self, _media_server_tag: &str, stream_id: &str, regist: bool) {

        if !regist {
            if let Some(s) = self.find_stream_by_stream_key(stream_id).await {
                let device_tag = s.device_tag.clone();
                let channel_tag = s.channel_tag.clone();
                let mut s = s;
                if let Some(ref dt) = device_tag {
                    if let Some(device) = self.db.devices_cache().get(dt) {
                        if device.status == crate::domain::DeviceStatus::Maintaining {
                            s.stopped();
                        } else {
                            s.error("stream closed by media server");
                        }
                    } else {
                        s.error("stream closed by media server");
                    }
                } else {
                    s.error("stream closed by media server");
                }
                if let Err(e) = self.db.update_stream(&s).await {
                    tracing::warn!("[HookEventHandler] HookStreamChanged: failed to stop stream: {}", e);
                }
                if let Some(ref dt) = device_tag {
                    if let Ok(Some(mut channel)) = self.db.get_channel(dt, channel_tag.as_deref().unwrap_or(dt)).await {
                        channel.status = crate::domain::DeviceStatus::Offline;
                        if let Err(e) = self.db.update_channel(&channel).await {
                            tracing::warn!("[HookEventHandler] HookStreamChanged: failed to update channel status: {}", e);
                        }
                    }
                    self.sync_device_status_by_channels(dt).await;
                }
            }
            return;
        }

        if let Some(s) = self.find_stream_by_stream_key(stream_id).await {
            let device_tag = s.device_tag.clone();
            let channel_tag = s.channel_tag.clone();
            let mut s = s;
            s.update_keepalive();
            if s.state != crate::domain::StreamState::Active {
                s.start();
            }
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookStreamChanged: failed to update stream: {}", e);
            }
            if let Some(ref dt) = device_tag {
                if let Ok(Some(mut channel)) = self.db.get_channel(dt, channel_tag.as_deref().unwrap_or(dt)).await {
                    channel.status = crate::domain::DeviceStatus::Online;
                    if let Err(e) = self.db.update_channel(&channel).await {
                        tracing::warn!("[HookEventHandler] HookStreamChanged: failed to update channel status: {}", e);
                    }
                } else {
                    tracing::warn!("[HookEventHandler] HookStreamChanged: channel not found device_tag={}, channel_tag={}", dt, channel_tag.as_deref().unwrap_or(dt));
                }
                self.sync_device_status_by_channels(dt).await;
            }
        }
    }

    async fn sync_device_status_by_channels(&self, device_tag: &str) {
        let channels = self.db.channels_cache()
            .iter()
            .filter(|c| c.key().starts_with(&format!("{}/", device_tag)))
            .map(|c| c.value().clone())
            .collect::<Vec<_>>();

        let all_offline = channels.iter().all(|c| c.status == crate::domain::DeviceStatus::Offline);
        let has_online = channels.iter().any(|c| c.status == crate::domain::DeviceStatus::Online);

        let cached_device = self.db.device_cached(device_tag);
        if let Some(mut device) = cached_device {
            if all_offline && !has_online {
                if device.status != crate::domain::DeviceStatus::Maintaining {
                    device.set_offline();
                    if let Err(e) = self.db.update_device(&device).await {
                        tracing::warn!("[HookEventHandler] sync_device_status: failed to update device status: {}", e);
                    }
                }
            } else if has_online {
                device.set_online();
                if let Err(e) = self.db.update_device(&device).await {
                    tracing::warn!("[HookEventHandler] sync_device_status: failed to update device status: {}", e);
                }
            }
        }
    }
}
