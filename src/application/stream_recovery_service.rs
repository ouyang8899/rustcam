use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use crate::context::{InfraContext, MediaContext};
use crate::domain::{Stream, StreamState, Device, DeviceStatus, Protocol};
use crate::domain::stream::make_stream_key;
use crate::protocol::event::SignalEvent;
use crate::application::StreamService;

fn compute_stream_key(stream: &Stream) -> String {
    make_stream_key(stream.device_tag.as_deref().unwrap_or(""), stream.channel_tag.as_deref().unwrap_or(""))
}

fn embed_rtsp_auth(rtsp_url: &str, username: &str, password: &str) -> String {
    if username.is_empty() || password.is_empty() {
        return rtsp_url.to_string();
    }
    if rtsp_url.starts_with("rtsp://") && !rtsp_url.contains('@') {
        if let Some(stripped) = rtsp_url.strip_prefix("rtsp://") {
            return format!("rtsp://{}:{}@{}", username, password, stripped);
        }
    }
    rtsp_url.to_string()
}

pub struct StreamRecoveryService {
    infra: InfraContext,
    media: MediaContext,
    stream_service: Arc<StreamService>,
    shutdown_tx: Arc<RwLock<Option<tokio::sync::watch::Sender<()>>>>,
}

impl StreamRecoveryService {
    pub fn new(infra: InfraContext, media: MediaContext, stream_service: Arc<StreamService>) -> Self {
        Self {
            infra,
            media,
            stream_service,
            shutdown_tx: Arc::new(RwLock::new(None)),
        }
    }

    fn recovery_config(&self) -> &crate::config::RecoveryConfig {
        &self.infra.config().recovery
    }

    pub async fn start(self: Arc<Self>) {
        let (tx, mut rx) = tokio::sync::watch::channel(());
        *self.shutdown_tx.write().await = Some(tx);

        let self_clone = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            self_clone.start_idle_streams().await;
            self_clone.event_loop(&mut rx).await;
        });

    }

    async fn start_idle_streams(&self) {
        let streams: Vec<Stream> = self.infra.db.streams_cache()
            .iter()
            .map(|r| r.value().clone())
            .collect();
        let devices: Vec<(String, Device)> = self.infra.db.devices_cache()
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect();

        for stream in streams {
            if matches!(stream.state, StreamState::Active | StreamState::Starting | StreamState::Recovering) {
                continue;
            }
            let device_tag = match stream.device_tag.as_deref() {
                Some(t) => t,
                None => continue,
            };
            let channel_tag = match stream.channel_tag.as_deref() {
                Some(t) => t,
                None => continue,
            };
            let device = match devices.iter().find(|(k, _)| k == device_tag) {
                Some((_, d)) => d.clone(),
                None => continue,
            };
            if device.status == crate::domain::DeviceStatus::Maintaining {
                continue;
            }
            if device.is_push_mode() {
                continue;
            }
            match device.protocol {
                Protocol::Onvif | Protocol::Rtsp => {
                    let raw_url = device.pull_urls.iter()
                        .min_by_key(|p| p.priority)
                        .map(|p| p.url.clone());
                    if let Some(raw_url) = raw_url {
                        let uname = device.device_username.as_deref().unwrap_or("");
                        let pwd = device.device_password.as_deref().unwrap_or("");
                        let full_url = embed_rtsp_auth(&raw_url, uname, pwd);
                        if let Err(e) = self.stream_service.start_pull_stream(device_tag, channel_tag, &full_url).await {
                            tracing::error!("[StreamRecoveryService] auto-pull failed for {}/{}: {}", device_tag, channel_tag, e);
                        }
                    }
                }
                Protocol::Gb28181 => {
                    let adapter_key = device.parent_device_tag.clone().unwrap_or_else(|| device_tag.to_string());
                    if let Some(adapter) = crate::protocol::adapter_manager::get_adapter(&adapter_key) {
                        if let Ok(_) = adapter.lock().await.start(device_tag, channel_tag).await {
                        }
                    }
                }
                _ => {}
            }
        }
    }

    async fn event_loop(self: &Arc<Self>, rx: &mut tokio::sync::watch::Receiver<()>) {
        let mut event_rx = self.infra.subscribe_events();
        let interval_secs = self.recovery_config().check_interval_secs;
        let mut check_interval = tokio::time::interval(Duration::from_secs(interval_secs));

        self.check_recovering_streams().await;

        loop {
            tokio::select! {
                _ = rx.changed() => {
                    break;
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            self.handle_event(&event).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("[StreamRecoveryService] Lagged {} events", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                _ = check_interval.tick() => {
                    self.check_recovering_streams().await;
                }
            }
        }
    }

    async fn handle_event(&self, event: &SignalEvent) {
        match event {
            SignalEvent::DeviceOffline { device_id: _, device_tag, reason: _ } => {
                if let Some(tag) = device_tag {
                    self.mark_streams_recovering(&tag).await;
                }
            }
            SignalEvent::DeviceOnline { device_id: _, device_tag } => {
                if let Some(ref tag) = device_tag {
                    self.recover_streams_for_device(tag).await;
                }
            }
            SignalEvent::StreamCreated { device_id: _, device_tag: _, channel_tag: _ } => {
                self.trigger_start_idle().await;
            }
            _ => {}
        }
    }

    async fn trigger_start_idle(&self) {
        let streams: Vec<Stream> = self.infra.db.streams_cache()
            .iter()
            .map(|r| r.value().clone())
            .collect();
        let devices: Vec<(String, Device)> = self.infra.db.devices_cache()
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect();

        for stream in streams {
            if !matches!(stream.state, StreamState::Idle) {
                continue;
            }
            let device_tag = match stream.device_tag.as_deref() {
                Some(t) => t,
                None => continue,
            };
            let channel_tag = match stream.channel_tag.as_deref() {
                Some(t) => t,
                None => continue,
            };
            let device = match devices.iter().find(|(k, _)| k == device_tag) {
                Some((_, d)) => d.clone(),
                None => continue,
            };
            if device.status == DeviceStatus::Maintaining {
                continue;
            }
            if device.is_push_mode() {
                continue;
            }
            match device.protocol {
                Protocol::Onvif | Protocol::Rtsp => {
                    let raw_url = device.pull_urls.iter()
                        .min_by_key(|p| p.priority)
                        .map(|p| p.url.clone());
                    if let Some(raw_url) = raw_url {
                        let uname = device.device_username.as_deref().unwrap_or("");
                        let pwd = device.device_password.as_deref().unwrap_or("");
                        let full_url = embed_rtsp_auth(&raw_url, uname, pwd);
                        if let Err(e) = self.stream_service.start_pull_stream(device_tag, channel_tag, &full_url).await {
                            tracing::error!("[StreamRecoveryService] auto-pull failed for {}/{}: {}", device_tag, channel_tag, e);
                        }
                    }
                }
                Protocol::Gb28181 => {
                    let adapter_key = device.parent_device_tag.clone().unwrap_or_else(|| device_tag.to_string());
                    if let Some(adapter) = crate::protocol::adapter_manager::get_adapter(&adapter_key) {
                        if let Err(e) = adapter.lock().await.start(device_tag, "").await {
                            tracing::error!("[StreamRecoveryService] GB28181 auto-start failed for {}: {}", device_tag, e);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    async fn mark_streams_recovering(&self, device_tag: &str) {
        let streams: Vec<Stream> = self.infra.db.list_streams()
            .await
            .into_iter()
            .filter(|s| {
                s.device_tag.as_deref() == Some(device_tag)
                    && matches!(s.state, StreamState::Active | StreamState::Starting | StreamState::Recovering | StreamState::Idle)
            })
            .collect();

        for stream in streams {
            let mut s = stream.clone();
            s.start_recovering();
            let s_stream_key = compute_stream_key(&s);
            if let Err(e) = self.infra.db.update_stream(&s).await {
                tracing::warn!(
                    stream_key = %s_stream_key,
                    device_tag = %device_tag,
                    error = %e,
                    "[StreamRecoveryService] Failed to mark stream as recovering"
                );
            } else {
                self.metrics_inc("stream_recovery_marked");
            }
        }
    }

    pub async fn mark_streams_recovering_for_media_server(&self, media_server_tag: &str) {
        let streams: Vec<Stream> = self.infra.db.streams_cache()
            .iter()
            .filter(|s| {
                s.media_server_tag == media_server_tag
                    && matches!(s.state, StreamState::Active | StreamState::Starting)
            })
            .map(|r| r.clone())
            .collect();

        for stream in streams {
            let device_tag = match stream.device_tag.as_deref() {
                Some(tag) => tag,
                None => {
                    tracing::warn!(
                        stream_key = %compute_stream_key(&stream),
                        "[StreamRecoveryService] Stream has no device_tag for mark recovering (media server restart)"
                    );
                    continue;
                }
            };
            let device = match self.infra.db.devices_cache().get(device_tag) {
                Some(d) => d.clone(),
                None => {
                    tracing::warn!(
                        stream_key = %compute_stream_key(&stream),
                        device_tag = %device_tag,
                        "[StreamRecoveryService] Device not found for stream mark recovering (media server restart)"
                    );
                    continue;
                }
            };

            if !device.push_urls.is_empty() {
                continue;
            }

            let mut s = stream.clone();
            s.start_recovering();
            let s_stream_key = compute_stream_key(&s);
            if let Err(e) = self.infra.db.update_stream(&s).await {
                tracing::warn!(
                    stream_key = %s_stream_key,
                    media_server_tag = %media_server_tag,
                    error = %e,
                    "[StreamRecoveryService] Failed to mark stream as recovering (media server restart)"
                );
            } else {
                self.metrics_inc("stream_recovery_marked");
            }
        }
    }

    pub async fn restart_streams_for_media_server(&self, media_server_tag: &str) {
        let streams: Vec<Stream> = self.infra.db.streams_cache()
            .iter()
            .filter(|s| {
                s.media_server_tag == media_server_tag
                    && matches!(s.state, StreamState::Active | StreamState::Starting | StreamState::Recovering | StreamState::Idle)
            })
            .map(|r| r.clone())
            .collect();

        for stream in streams {
            let stream_device_tag = match stream.device_tag.as_deref() {
                Some(tag) => tag,
                None => {
                    tracing::warn!(
                        stream_key = %compute_stream_key(&stream),
                        "[StreamRecoveryService] Stream has no device_tag for restart"
                    );
                    continue;
                }
            };
            let device = match self.infra.db.devices_cache().get(stream_device_tag) {
                Some(d) => d.clone(),
                None => {
                    tracing::warn!(
                        stream_key = %compute_stream_key(&stream),
                        device_tag = %stream_device_tag,
                        "[StreamRecoveryService] Device not found for stream restart"
                    );
                    continue;
                }
            };

            let _device_tag = device.device_tag.as_deref().unwrap_or("unknown");
            if device.status == crate::domain::DeviceStatus::Maintaining {
                continue;
            }

            if stream.state == StreamState::Idle {
                self.handle_idle_stream(media_server_tag, &stream).await;
                continue;
            }

            match device.protocol {
                Protocol::Gb28181 => {
                    if device.status != crate::domain::DeviceStatus::Online {
                        continue;
                    }
                    match self.stream_service.restart_gb28181_stream(&compute_stream_key(&stream)).await {
                        Ok(_info) => {
                        }
                        Err(e) => {
                            tracing::error!(
                                stream_key = %compute_stream_key(&stream),
                                error = %e,
                                "[StreamRecoveryService] GB28181 stream restart failed"
                            );
                        }
                    }
                }
                _ => {
                    if device.is_push_mode() {
                        continue;
                    }

                    self.direct_restart_stream(&device, &stream).await;
                }
            }
        }
    }

    async fn handle_idle_stream(&self, media_server_tag: &str, stream: &Stream) {
        let adapter = match self.media.cluster.get_server(media_server_tag) {
            Some(a) => a,
            None => {
                tracing::warn!("[StreamRecoveryService] Media server {} not found for Idle stream", media_server_tag);
                return;
            }
        };

        let stream_device_tag = match stream.device_tag.as_deref() {
            Some(tag) => tag,
            None => {
                tracing::warn!("[StreamRecoveryService] Stream {} has no device_tag", compute_stream_key(&stream));
                return;
            }
        };

        match adapter.is_stream_online(&stream.app, &compute_stream_key(&stream)).await {
            Ok(true) => {
                let mut s = stream.clone();
                s.start();
                s.last_error = None;
                if let Err(e) = self.infra.db.update_stream(&s).await {
                    tracing::warn!("[StreamRecoveryService] Failed to sync Idle stream to Active: {}", e);
                } else {
                }
                self.sync_device_status(stream_device_tag, StreamState::Active).await;
            }
            Ok(false) => {

                let cached_device = self.infra.db.device_cached(stream_device_tag);
                if let Some(device) = cached_device {
                    self.direct_restart_stream(&device, stream).await;
                } else {
                    tracing::warn!("[StreamRecoveryService] Device {} not found for Idle stream restart", stream_device_tag);
                }
            }
            Err(e) => {
                tracing::warn!("[StreamRecoveryService] is_stream_online failed for {}, assuming offline: {}", compute_stream_key(&stream), e);

                let cached_device = self.infra.db.device_cached(stream_device_tag);
                if let Some(device) = cached_device {
                    self.direct_restart_stream(&device, stream).await;
                }
            }
        }
    }

    async fn direct_restart_stream(&self, device: &Device, stream: &Stream) {
        let stream_key = &compute_stream_key(&stream);
        let tag = &stream.media_server_tag;
        let _device_tag = device.device_tag.as_deref().unwrap_or("unknown");

        if device.protocol == Protocol::Gb28181 {
            return;
        }

        if device.is_push_mode() {
            return;
        }

        let adapter = match self.media.cluster.get_server(tag) {
            Some(a) => a,
            None => {
                tracing::warn!(
                    stream_key = %stream_key,
                    media_server_tag = %tag,
                    "[StreamRecoveryService] Media server not found for direct restart"
                );
                self.record_failure(stream, "Media server not found").await;
                return;
            }
        };

        let rtsp_url = device.select_source()
            .map(|(_, url)| Self::embed_auth(&url, &device.device_username, &device.device_password))
            .unwrap_or_default();

        match adapter.add_stream_proxy(&stream.app, stream_key, &rtsp_url).await {
            Ok(_info) => {
                let mut s = stream.clone();
                s.state = StreamState::Starting;
                s.retry_count = 0;
                s.last_error = None;
                if let Err(e) = self.infra.db.update_stream(&s).await {
                    tracing::warn!("[StreamRecoveryService] Failed to update stream state: {}", e);
                }
            }
            Err(e) => {
                tracing::error!(
                    stream_key = %stream_key,
                    error = %e,
                    "[StreamRecoveryService] Direct restart failed"
                );
                self.record_failure(stream, &e.to_string()).await;
            }
        }
    }

    async fn sync_device_status(&self, device_tag: &str, stream_state: StreamState) {
        use crate::domain::DeviceStatus;

        let mut device = match self.infra.db.devices_cache().get(device_tag) {
            Some(d) => d.clone(),
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
                tracing::warn!("[StreamRecoveryService] Failed to sync device status: {}", e);
            }
        }
    }

    pub async fn recover_streams_for_media_server(&self, media_server_tag: &str) {
        let streams: Vec<Stream> = self.infra.db.streams_cache()
            .iter()
            .filter(|s| {
                s.media_server_tag == media_server_tag
                    && s.should_retry()
                    && matches!(s.state, StreamState::Recovering | StreamState::Error)
            })
            .map(|r| r.clone())
            .collect();

        for stream in streams {
            let stream_device_tag = match stream.device_tag.as_deref() {
                Some(tag) => tag,
                None => continue,
            };
            let device = {
                let guard = match self.infra.db.devices_cache().get(stream_device_tag) {
                    Some(d) => d,
                    None => {
                        tracing::warn!(
                            stream_key = %compute_stream_key(&stream),
                            device_tag = %stream_device_tag,
                            "[StreamRecoveryService] Device not found for stream recovery"
                        );
                        continue;
                    }
                };
                guard.value().clone()
            };

            self.recover_stream(&device, &stream).await;
        }
    }

    async fn recover_streams_for_device(&self, device_tag: &str) {
        let device = match self.infra.db.devices_cache().get(device_tag) {
            Some(d) => d.clone(),
            None => {
                tracing::warn!(
                    device_tag = %device_tag,
                    "[StreamRecoveryService] Device not in cache"
                );
                return;
            }
        };

        if device.status == crate::domain::DeviceStatus::Maintaining {
            return;
        }

        let streams: Vec<Stream> = self.infra.db.list_streams()
            .await
            .into_iter()
            .filter(|s| {
                s.device_tag.as_deref() == Some(device_tag)
                    && s.should_retry()
                    && matches!(s.state, StreamState::Recovering | StreamState::Error)
            })
            .collect();

        for stream in streams {
            self.recover_stream(&device, &stream).await;
        }
    }

    async fn check_recovering_streams(&self) {
        let streams: Vec<Stream> = self.infra.db.list_streams()
            .await
            .into_iter()
            .filter(|s| {
                s.should_retry()
                    && matches!(s.state, StreamState::Recovering | StreamState::Error)
            })
            .collect();

        for stream in streams {
            let stream_device_tag = match stream.device_tag.as_deref() {
                Some(tag) => tag,
                None => continue,
            };
            let device = match self.infra.db.devices_cache().get(stream_device_tag) {
                Some(d) => d.clone(),
                None => continue,
            };

            if device.status == crate::domain::DeviceStatus::Maintaining {
                continue;
            }

            let backoff = self.backoff_duration(stream.retry_count);
            let elapsed = chrono::Utc::now() - stream.last_keepalive_at;
            if elapsed < backoff {
                continue;
            }

            self.recover_stream(&device, &stream).await;
        }
    }

    async fn recover_stream(&self, device: &Device, stream: &Stream) {
        let stream_key = &compute_stream_key(&stream);
        let device_tag = device.device_tag.as_deref().unwrap_or("unknown");

        self.metrics_inc("stream_recovery_attempts");

        let rtsp_url = match device.select_source() {
            Some((_, url)) => Self::embed_auth(&url, &device.device_username, &device.device_password),
            None => {
                tracing::warn!(
                    stream_key = %stream_key,
                    device_tag = %device_tag,
                    "[StreamRecoveryService] No pull URL available"
                );
                self.record_failure(stream, "No pull URL available").await;
                return;
            }
        };

        let result = match device.protocol {
            Protocol::Gb28181 => {
                if device.status != crate::domain::DeviceStatus::Online {
                    return;
                }
                self.stream_service.restart_gb28181_stream(stream_key).await
            }
            _ => {
                self.stream_service.restart_stream(stream_key, &rtsp_url).await
            }
        };

        match result {
            Ok(info) => {
                self.metrics_inc("stream_recovery_success");
                self.sync_device_status(device_tag, StreamState::Active).await;
                let _ = self.infra.publish_event(SignalEvent::StreamRecover {
                    device_id: device.id,
                    stream_key: info.stream_key,
                }).await;
            }
            Err(e) => {
                tracing::error!(
                    stream_key = %stream_key,
                    device_tag = %device_tag,
                    error = %e,
                    "[StreamRecoveryService] Recovery failed"
                );
                self.metrics_inc("stream_recovery_failures");
                self.record_failure(stream, &e.to_string()).await;
                self.sync_device_status(device_tag, StreamState::Error).await;
            }
        }
    }

    async fn record_failure(&self, stream: &Stream, reason: &str) {
        let cache_key = match self.infra.db.get_stream_cache_key_by_stream_key(&compute_stream_key(&stream)) {
            Some(key) => key,
            None => {
                tracing::warn!(
                    stream_key = %compute_stream_key(&stream),
                    "[StreamRecoveryService] Stream not in cache during failure record"
                );
                return;
            }
        };

        let mut s = match self.infra.db.streams_cache().get(&cache_key) {
            Some(entry) => entry.clone(),
            None => {
                tracing::warn!(
                    stream_key = %compute_stream_key(&stream),
                    "[StreamRecoveryService] Stream not in cache during failure record"
                );
                return;
            }
        };

        let was_retryable = s.should_retry();
        s.error(reason);
        s.increment_retry();
        s.update_keepalive();
        let s_stream_key = compute_stream_key(&s);

        if let Err(e) = self.infra.db.update_stream(&s).await {
            tracing::warn!(
                stream_key = %s_stream_key,
                error = %e,
                "[StreamRecoveryService] Failed to update stream after failure"
            );
        }

        if was_retryable && !s.should_retry() {
            let device_tag = s.device_tag.as_deref().unwrap_or("unknown");
            tracing::warn!(
                stream_key = %s_stream_key,
                device_tag = %device_tag,
                retry_count = %s.retry_count,
                max_retries = %s.max_retries,
                last_error = ?s.last_error,
                "[StreamRecoveryService] Stream retries exhausted — manual intervention required"
            );
            self.metrics_inc("stream_retries_exhausted");
            let _ = self.infra.publish_event(SignalEvent::StreamRetriesExhausted {
                device_id: 0,
                stream_key: s_stream_key.clone(),
                retry_count: s.retry_count,
                last_error: s.last_error.clone(),
            }).await;
        }
    }

    fn embed_auth(url: &str, username: &Option<String>, password: &Option<String>) -> String {
        let user = username.as_ref().map(|u| u.as_str()).unwrap_or("");
        let pass = password.as_ref().map(|p| p.as_str()).unwrap_or("");

        if user.is_empty() || pass.is_empty() {
            return url.to_string();
        }

        if url.starts_with("rtsp://") && !url.contains('@') {
            if let Some(stripped) = url.strip_prefix("rtsp://") {
                return format!("rtsp://{}:{}@{}", user, pass, stripped);
            }
        }
        url.to_string()
    }

    pub async fn handle_stream_not_found(&self, stream_key: &str, media_server_tag: &str) -> bool {
        let cache_key = stream_key.replace('_', "/");

        let cached_stream = self.infra.db.stream_cached(&cache_key);
        if let Some(stream) = cached_stream {
            if stream.media_server_tag != media_server_tag {
                return false;
            }

            let device_tag = match stream.device_tag.as_deref() {
                Some(tag) => tag,
                None => {
                    tracing::warn!("[StreamRecovery] Stream {} has no device_tag", stream_key);
                    return false;
                }
            };

            let cached_device = self.infra.db.device_cached(device_tag);
            let device = match cached_device {
                Some(d) => d,
                None => {
                    tracing::warn!("[StreamRecovery] Device {} not found for stream {}", device_tag, stream_key);
                    return false;
                }
            };

            let rtsp_url = device.extended
                .as_ref()
                .and_then(|e| e.get("rtsp_full_url"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if rtsp_url.is_empty() {
                tracing::warn!("[StreamRecovery] Device {} has no rtsp_url", device_tag);
                return false;
            }

            match self.stream_service.start_pull_stream(device_tag, device_tag, rtsp_url).await {
                Ok(_) => {
                    true
                }
                Err(e) => {
                    tracing::error!("[StreamRecovery] Failed to pull stream {}: {}", stream_key, e);
                    false
                }
            }
        } else {
            false
        }
    }

    fn backoff_duration(&self, retry_count: u8) -> chrono::Duration {
        let cfg = self.recovery_config();
        let secs = cfg.base_backoff_secs * 2u64.saturating_pow(retry_count as u32);
        chrono::Duration::seconds(secs.min(cfg.max_backoff_secs) as i64)
    }

    fn metrics_inc(&self, name: &'static str) {
        self.infra.metrics.record_recovery_event(name);
    }
}

impl Clone for StreamRecoveryService {
    fn clone(&self) -> Self {
        Self {
            infra: self.infra.clone(),
            media: self.media.clone(),
            stream_service: self.stream_service.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
        }
    }
}
