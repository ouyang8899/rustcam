use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use chrono::{DateTime, Utc};
use crate::context::InfraContext;
use crate::application::recording_policy_service::RecordingPolicyService;
use crate::application::recording_service::RecordingService;
use crate::domain::recording_policy::{PolicyType, RecordingPolicy};
use crate::domain::recording::CreateRecordingRequest;
use crate::protocol::event::SignalEvent;

struct AlarmRecordingState {
    pub policy_id: i64,
    pub recording_id: i64,
    pub alarm_id: i64,
    pub triggered_at: DateTime<Utc>,
    pub post_record_secs: u32,
    pub stop_timer_handle: Option<tokio::task::JoinHandle<()>>,
}

pub struct RecordingSchedulerService {
    infra: InfraContext,
    policy_service: Arc<RecordingPolicyService>,
    recording_service: Arc<RecordingService>,
    shutdown_tx: Arc<RwLock<Option<tokio::sync::watch::Sender<()>>>>,
    active_alarm_recordings: Arc<RwLock<HashMap<String, AlarmRecordingState>>>,
    check_interval_secs: u64,
}

impl RecordingSchedulerService {
    pub fn new(
        infra: InfraContext,
        policy_service: Arc<RecordingPolicyService>,
        recording_service: Arc<RecordingService>,
    ) -> Self {
        Self {
            infra,
            policy_service,
            recording_service,
            shutdown_tx: Arc::new(RwLock::new(None)),
            active_alarm_recordings: Arc::new(RwLock::new(HashMap::new())),
            check_interval_secs: 60,
        }
    }

    pub async fn start(self: Arc<Self>) {
        let (tx, _rx) = tokio::sync::watch::channel(());
        *self.shutdown_tx.write().await = Some(tx);

        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.run_scheduler().await;
        });

        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.run_event_listener().await;
        });

    }

    async fn run_scheduler(self: Arc<Self>) {
        let mut ticker = interval(Duration::from_secs(self.check_interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.check_timed_policies().await {
                        tracing::error!("[RecordingScheduler] Timed policy check failed: {}", e);
                    }
                    if let Err(e) = self.cleanup_expired_alarm_recordings().await {
                        tracing::error!("[RecordingScheduler] Alarm recording cleanup failed: {}", e);
                    }
                }
            }
        }
    }

    async fn run_event_listener(self: Arc<Self>) {
        let mut event_rx = self.infra.subscribe_events();
        let shutdown_rx = {
            let tx_opt = self.shutdown_tx.read().await;
            tx_opt.as_ref().map(|tx| tx.subscribe())
        };

        let mut shutdown_rx = match shutdown_rx {
            Some(rx) => rx,
            None => {
                tracing::error!("[RecordingScheduler] No shutdown channel available");
                return;
            }
        };

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    break;
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Err(e) = self.handle_event(&event).await {
                                tracing::error!("[RecordingScheduler] Event handler error: {}", e);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("[RecordingScheduler] Lagged {} events", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn handle_event(&self, event: &SignalEvent) -> anyhow::Result<()> {
        match event {
            SignalEvent::Alarm { device_id, alarm_type, message: _, timestamp } => {
                self.handle_alarm_event(device_id, alarm_type, *timestamp).await?;
            }
            SignalEvent::AlarmRecovered { device_id, alarm_type, timestamp } => {
                self.handle_alarm_recovered(device_id, alarm_type, *timestamp).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_alarm_event(
        &self,
        device_id: &str,
        alarm_type: &str,
        _timestamp: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let _device_tag = device_id.to_string();

        let policies = self.policy_service.find_alarm_triggered_policies(alarm_type).await?;

        for policy in policies {
            if !policy.enabled {
                continue;
            }

            let stream_key =
                crate::domain::stream::make_stream_key(&policy.device_tag, &policy.channel_tag);

            if self.policy_service.should_start_alarm_recording(&policy, alarm_type) {
                let existing = self.active_alarm_recordings.read().await;
                if existing.contains_key(&stream_key) {
                    continue;
                }
                drop(existing);

                if let Err(e) = self.start_alarm_recording(&policy, &stream_key, 0).await {
                    tracing::error!(
                        "[RecordingScheduler] Failed to start alarm recording for policy {}: {}",
                        policy.id, e
                    );
                }
            }
        }

        Ok(())
    }

    async fn handle_alarm_recovered(
        &self,
        device_id: &str,
        alarm_type: &str,
        _timestamp: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let _device_tag = device_id.to_string();

        let policies = self.policy_service.find_alarm_triggered_policies(alarm_type).await?;

        for policy in policies {
            if !policy.enabled {
                continue;
            }

            let stream_key =
                crate::domain::stream::make_stream_key(&policy.device_tag, &policy.channel_tag);

            let mut active_recordings = self.active_alarm_recordings.write().await;
            if let Some(state) = active_recordings.get_mut(&stream_key) {
                if state.policy_id == policy.id && state.stop_timer_handle.is_none() {
                    if policy.post_record_secs > 0 {
                        drop(active_recordings);
                        let self_clone = self.clone();
                        let stream_key_clone = stream_key.to_string();
                        let post_secs = policy.post_record_secs;

                        let handle = tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(post_secs as u64)).await;
                            if let Err(e) = self_clone.stop_alarm_recording(&stream_key_clone).await {
                                tracing::warn!(
                                    "[RecordingScheduler] Auto-stop alarm recording failed: stream_key={} err={}",
                                    stream_key_clone, e
                                );
                            }
                        });

                        let mut active_recordings = self.active_alarm_recordings.write().await;
                        if let Some(state) = active_recordings.get_mut(&stream_key) {
                            state.stop_timer_handle = Some(handle);
                        }
                    }
                    break;
                }
            }
        }

        Ok(())
    }

    async fn start_alarm_recording(
        &self,
        policy: &RecordingPolicy,
        stream_key: &str,
        alarm_id: i64,
    ) -> anyhow::Result<()> {
        let (media_server, _app) = self.get_media_server_for_stream(stream_key).await?;

        let req = CreateRecordingRequest {
            device_tag: policy.device_tag.clone(),
            channel_tag: policy.channel_tag.clone(),
            format: Some(policy.format.clone()),
            duration_secs: Some(policy.max_duration_secs),
            max_file_size_mb: Some(policy.max_file_size_mb),
            output_path: policy.output_path.clone(),
            labels: Some(vec![format!("alarm:{}", alarm_id)]),
            pre_record_secs: Some(policy.pre_record_secs),
        };

        let recording = self.recording_service.create_recording(
            req,
            policy.channel_tag.clone(),
            media_server.clone(),
        ).await;

        let recording = self.recording_service.start_recording(recording.id).await?;

        let state = AlarmRecordingState {
            policy_id: policy.id,
            recording_id: recording.id,
            alarm_id,
            triggered_at: Utc::now(),
            post_record_secs: policy.post_record_secs,
            stop_timer_handle: None,
        };

        if policy.post_record_secs > 0 {
            let self_clone = self.clone();
            let stream_key_clone = stream_key.to_string();
            let post_secs = policy.post_record_secs;

            let handle = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(post_secs as u64)).await;
                if let Err(e) = self_clone.stop_alarm_recording(&stream_key_clone).await {
                    tracing::warn!(
                        "[RecordingScheduler] Auto-stop alarm recording failed: stream_key={} err={}",
                        stream_key_clone, e
                    );
                }
            });

            let mut state = state;
            state.stop_timer_handle = Some(handle);
            self.active_alarm_recordings.write().await.insert(stream_key.to_string(), state);
        } else {
            self.active_alarm_recordings.write().await.insert(stream_key.to_string(), state);
        }

        Ok(())
    }

    async fn stop_alarm_recording(&self, stream_key: &str) -> anyhow::Result<()> {
        let state = self.active_alarm_recordings.write().await.remove(stream_key);

        if let Some(state) = state {
            if let Some(handle) = state.stop_timer_handle {
                handle.abort();
            }

            self.recording_service.stop_recording(state.recording_id).await?;

        }

        Ok(())
    }

    async fn check_timed_policies(&self) -> anyhow::Result<()> {
        let policies = self.policy_service.list_policies().await?;
        let now = Utc::now();

        for policy in policies.iter().filter(|p| p.policy_type == PolicyType::Timed && p.enabled) {

            let stream_key = crate::domain::stream::make_stream_key(&policy.device_tag, &policy.channel_tag);

            let within_window = self.policy_service.is_within_time_window(policy, now);

            let active_recordings = self.active_alarm_recordings.read().await;
            let is_alarm_recording = active_recordings.contains_key(&stream_key);
            drop(active_recordings);

            let has_active = self
                .recording_service
                .get_active_recording_by_stream_key(&stream_key)
                .await
                .ok()
                .flatten()
                .is_some();

            if within_window && !is_alarm_recording {
                if has_active {
                    continue;
                }
                if let Err(e) = self.start_timed_recording(policy, &stream_key).await {
                    tracing::error!(
                        "[RecordingScheduler] Failed to start timed recording: policy_id={} err={}",
                        policy.id, e
                    );
                }
            } else if !within_window && !is_alarm_recording {
                if let Err(_e) = self.stop_timed_recording(&stream_key).await {
                }
            }
        }

        Ok(())
    }

    async fn start_timed_recording(
        &self,
        policy: &RecordingPolicy,
        stream_key: &str,
    ) -> anyhow::Result<()> {
        let (media_server, _app) = self.get_media_server_for_stream(stream_key).await?;

        let req = CreateRecordingRequest {
            device_tag: policy.device_tag.clone(),
            channel_tag: policy.channel_tag.clone(),
            format: Some(policy.format.clone()),
            duration_secs: Some(policy.max_duration_secs),
            max_file_size_mb: Some(policy.max_file_size_mb),
            output_path: policy.output_path.clone(),
            labels: Some(vec!["timed".to_string()]),
            pre_record_secs: Some(policy.pre_record_secs),
        };

        let recording = self.recording_service.create_recording(
            req,
            policy.channel_tag.clone(),
            media_server.clone(),
        ).await;

        self.recording_service.start_recording(recording.id).await?;

        Ok(())
    }

    async fn stop_timed_recording(&self, stream_key: &str) -> anyhow::Result<()> {
        if let Some(recording) = self.recording_service.get_active_recording_by_stream_key(stream_key).await? {
            self.recording_service.stop_recording(recording.id).await?;
        }
        Ok(())
    }

    async fn get_media_server_for_stream(&self, stream_key: &str) -> anyhow::Result<(String, String)> {
        let parts: Vec<&str> = stream_key.split('/').collect();
        let device_tag = parts.first().unwrap_or(&stream_key);
        let channel_id = parts.get(1).unwrap_or(&"0");

        if let Some(stream) = self.infra.db.streams_cache().iter()
            .find(|s| s.device_tag.as_deref() == Some(device_tag) && s.channel_tag.as_deref() == Some(channel_id))
            .map(|s| s.value().clone())
        {
            let app = stream.app.clone();
            let server_tag = if stream.media_server_tag.is_empty() {
                "default".to_string()
            } else {
                stream.media_server_tag.clone()
            };
            return Ok((server_tag, app));
        }

        Ok(("default".to_string(), "live".to_string()))
    }

    async fn cleanup_expired_alarm_recordings(&self) -> anyhow::Result<()> {
        let mut to_stop = Vec::new();
        let now = Utc::now();

        let guard = self.active_alarm_recordings.read().await;
        for (stream_key, state) in guard.iter() {
            let elapsed = now.signed_duration_since(state.triggered_at).num_seconds();
            if elapsed > (state.post_record_secs as i64 * 2) && state.post_record_secs > 0 {
                to_stop.push(stream_key.clone());
            }
        }
        drop(guard);

        for stream_key in to_stop {
            if let Err(e) = self.stop_alarm_recording(&stream_key).await {
                tracing::warn!(
                    "[RecordingScheduler] Cleanup stop failed: stream_key={} err={}",
                    stream_key, e
                );
            }
        }

        Ok(())
    }

    pub async fn stop(&self) {
        if let Some(tx) = self.shutdown_tx.read().await.as_ref() {
            let _ = tx.send(());
        }

        let mut guard = self.active_alarm_recordings.write().await;
        for (_, state) in guard.drain() {
            if let Some(handle) = state.stop_timer_handle {
                handle.abort();
            }
            let _ = self.recording_service.stop_recording(state.recording_id).await;
        }
    }
}

impl Clone for RecordingSchedulerService {
    fn clone(&self) -> Self {
        Self {
            infra: self.infra.clone(),
            policy_service: self.policy_service.clone(),
            recording_service: self.recording_service.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
            active_alarm_recordings: self.active_alarm_recordings.clone(),
            check_interval_secs: self.check_interval_secs,
        }
    }
}