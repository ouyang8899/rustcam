use std::sync::Arc;
use crate::infrastructure::{DbRepository, EventBus};
use crate::infrastructure::RedisStore;
use crate::protocol::event::SignalEvent;
use crate::domain::{Session, SessionType, SessionState, Stream, StreamState};
use crate::domain::traits::EventPublisher;
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

    pub async fn start(mut self) {
        tokio::spawn(async move {
            loop {
                match self.event_bus.subscribe().recv().await {
                    Ok(event) => {
                        if matches!(event, SignalEvent::HookPlay { .. } |
                            SignalEvent::HookPublish { .. } |
                            SignalEvent::HookUnpublish { .. } |
                            SignalEvent::HookStreamUpdate { .. }) {
                            self.handle_event(&event).await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("[HookEventHandler] Event bus closed, stopping");
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
            SignalEvent::HookPlay { media_server_tag, stream_id, client_ip } => {
                self.handle_hook_play(media_server_tag, stream_id, client_ip.clone()).await;
            }
            SignalEvent::HookPublish { media_server_tag, stream_id, client_ip, device_tag, channel_tag } => {
                self.handle_hook_publish(media_server_tag, stream_id, client_ip.clone(), device_tag.clone(), channel_tag.clone()).await;
            }
            SignalEvent::HookUnpublish { media_server_tag, stream_id } => {
                self.handle_hook_unpublish(media_server_tag, stream_id).await;
            }
            SignalEvent::HookStreamUpdate { media_server_tag, stream_id, regist, reader_count, total_reader_count, bytes_speed } => {
                self.handle_hook_stream_update(media_server_tag, stream_id, *regist, *reader_count, *total_reader_count, *bytes_speed).await;
            }
            _ => {}
        }
    }

    async fn handle_hook_play(&self, media_server_tag: &str, stream_id: &str, client_ip: Option<String>) {
        tracing::info!("[HookEventHandler] HookPlay: server={}, stream={}, ip={:?}", 
            media_server_tag, stream_id, client_ip);

        let stream_key = stream_id;
        if let Some(mut s) = self.find_stream_by_stream_key(stream_key).await {
            s.increment_viewers();
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookPlay: failed to update stream viewers: {}", e);
            }
        }

        let mut session = Session::new(SessionType::Play, 0);
        session.state = SessionState::Active;
        session.client_ip = client_ip;
        session.media_server_tag = Some(media_server_tag.to_string());
        
        if let Some(stream) = self.find_stream_by_stream_key(stream_key).await {
            session.device_tag = stream.device_tag.clone();
            session.channel_tag = stream.channel_tag.clone();
        }
        session.refresh_expiry(self.session_expiry_secs);

        if let Err(e) = self.db.create_session(&session).await {
            tracing::error!("[HookEventHandler] HookPlay: failed to create session: {}", e);
            return;
        }

        self.db.sessions_cache().insert(session.id, session);
        tracing::info!("[HookEventHandler] HookPlay: session created, stream={}", stream_id);
    }

    async fn handle_hook_publish(&self, media_server_tag: &str, stream_id: &str, client_ip: Option<String>, device_tag_from_params: Option<String>, channel_tag_from_params: Option<String>) {
        tracing::info!("[HookEventHandler] HookPublish: server={}, stream={}, ip={:?}", 
            media_server_tag, stream_id, client_ip);

        let (device_tag, channel_tag) = if let (Some(dt), Some(ct)) = (&device_tag_from_params, &channel_tag_from_params) {
            (dt.clone(), ct.clone())
        } else {
            crate::domain::parse_stream_key(stream_id)
                .unwrap_or_else(|| (stream_id.to_string(), stream_id.to_string()))
        };

        let cache_key = format!("{}/{}", device_tag, channel_tag);
        if let Some(mut s) = self.db.streams_cache().get(&cache_key).map(|s| s.clone()) {
            let mut s = s;
            s.update_keepalive();
            if s.state == StreamState::Idle || s.state == StreamState::Error {
                s.start();
            }
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookPublish: failed to update stream: {}", e);
            }
            tracing::info!("[HookEventHandler] HookPublish: updated push stream {} (device_tag={})", stream_id, device_tag);
            return;
        }

        let app = self.db.devices_cache().get(&device_tag)
            .map(|d| d.app.clone().unwrap_or_else(|| "live".to_string()))
            .unwrap_or_else(|| "live".to_string());

        let stream_key = format!("{}/{}", device_tag, channel_tag);
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

        tracing::info!("[HookEventHandler] HookPublish: registered push stream {} on server {}", stream_id, media_server_tag);
    }

    async fn handle_hook_unpublish(&self, media_server_tag: &str, stream_id: &str) {
        tracing::info!("[HookEventHandler] HookUnpublish: server={}, stream={}", media_server_tag, stream_id);

        if let Some(mut s) = self.find_stream_by_stream_key(stream_id).await {
            s.stopped();
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookUnpublish: failed to update stream: {}", e);
            }
            tracing::info!("[HookEventHandler] HookUnpublish: stream {} stopped", stream_id);
        }
    }

    async fn handle_hook_stream_update(&self, media_server_tag: &str, stream_id: &str, regist: bool, reader_count: Option<u32>, total_reader_count: Option<u32>, bytes_speed: Option<u64>) {
        tracing::info!("[HookEventHandler] HookStreamUpdate: server={}, stream={}, regist={}, readers={:?}/{:?}", 
            media_server_tag, stream_id, regist, reader_count, total_reader_count);

        if !regist {
            self.handle_hook_unpublish(media_server_tag, stream_id).await;
            return;
        }

        if let Some(mut s) = self.find_stream_by_stream_key(stream_id).await {
            s.update_keepalive();
            if s.state != StreamState::Active {
                s.start();
            }
            if let Err(e) = self.db.update_stream(&s).await {
                tracing::warn!("[HookEventHandler] HookStreamUpdate: failed to update stream: {}", e);
            }
        }

        if let Some(device_id) = self.find_device_id_by_push_stream(stream_id).await {
            if let Some(device) = self.db.devices_cache().get(&device_id) {
                let mut device = (*device).clone();
                device.set_online();
                if let Err(e) = self.db.update_device(&device).await {
                    tracing::warn!("[HookEventHandler] HookStreamUpdate: failed to update device online status: {}", e);
                }
            }
        }
    }

    async fn find_stream_by_stream_key(&self, stream_key: &str) -> Option<Stream> {
        if let Some((device_tag, channel_tag)) = crate::domain::parse_stream_key(stream_key) {
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
        let (device_tag, _) = crate::domain::parse_stream_key(stream_id)
            .unwrap_or_else(|| (stream_id.to_string(), stream_id.to_string()));
        Some(device_tag)
    }

    async fn start_recording_if_enabled(&self, device_tag: &str, channel_tag: &str, media_server: &str, app: &str, stream_key: &str) {
        let device = match self.db.devices_cache().get(device_tag).map(|r| r.value().clone()) {
            Some(d) => d,
            None => {
                tracing::debug!("[HookEventHandler] Device {} not found in cache, skipping recording", device_tag);
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
                tracing::warn!("[HookEventHandler] Failed to check active recording: {}", e);
            }
        }

        match self.recording_service
            .start_recording_for_stream(device_tag.to_string(), stream_key, media_server, &config)
            .await
        {
            Ok(Some(rec)) => {
                tracing::info!("[HookEventHandler] Recording {} auto-started for stream {}", rec.id, stream_key);
            }
            Ok(None) => {}
            Err(e) => {
                tracing::error!("[HookEventHandler] Failed to auto-start recording: {}", e);
            }
        }
    }
}
