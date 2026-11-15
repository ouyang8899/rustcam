use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use chrono::{DateTime, Utc};
use crate::context::InfraContext;
use crate::application::recording_policy_service::RecordingPolicyService;
use crate::application::recording_service::RecordingService;
use crate::domain::recording_policy::{PolicyType, RecordingPolicy};
use crate::domain::recording::{CreateRecordingRequest, Recording, RecordingState, RecordingType};
use crate::protocol::event::SignalEvent;

struct AlarmRecordingState {
    pub recording_id: i64,
    pub stream_key: String,
    pub alarm_type: String,
    pub latest_alarm_id: i64,
    pub last_alarm_at: DateTime<Utc>,
    pub recovery_secs: u32,
    pub post_record_secs: u32,
    pub stop_timer_handle: Option<tokio::task::JoinHandle<()>>,
}

/// 一个流上的同一类报警属于一个“报警会话”
fn alarm_session_key(stream_key: &str, alarm_type: &str) -> String {
    format!("{}#{}", stream_key, alarm_type)
}

/// 兜底恢复判定：连续 `recovery_secs` 秒没有新的报警上报即视为恢复（0=关闭）
fn alarm_session_should_end(
    now: DateTime<Utc>,
    last_alarm_at: DateTime<Utc>,
    recovery_secs: u32,
) -> bool {
    recovery_secs > 0
        && now.signed_duration_since(last_alarm_at).num_seconds() >= recovery_secs as i64
}

/// stream_key 形如 `device_tag/channel_tag`
fn session_matches(stream_key: &str, device_tag: &str, channel_tag: Option<&str>) -> bool {
    let mut parts = stream_key.splitn(2, '/');
    let device = parts.next().unwrap_or("");
    let channel = parts.next();
    device == device_tag
        && match channel_tag {
            Some(expected) => channel == Some(expected),
            None => true,
        }
}

pub struct RecordingSchedulerService {
    infra: InfraContext,
    cluster: Arc<crate::infrastructure::cluster::ClusterManager>,
    policy_service: Arc<RecordingPolicyService>,
    recording_service: Arc<RecordingService>,
    shutdown_tx: Arc<RwLock<Option<tokio::sync::watch::Sender<()>>>>,
    active_alarm_recordings: Arc<RwLock<HashMap<String, AlarmRecordingState>>>,
    check_interval_secs: u64,
}

impl RecordingSchedulerService {
    pub fn new(
        infra: InfraContext,
        cluster: Arc<crate::infrastructure::cluster::ClusterManager>,
        policy_service: Arc<RecordingPolicyService>,
        recording_service: Arc<RecordingService>,
    ) -> Self {
        Self {
            infra,
            cluster,
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

      // 开机回收僵尸录制任务：否则残留的 starting/recording 记录会让本流
      // 的定时录制被静默跳过
      let self_clone = self.clone();
      tokio::spawn(async move {
          if let Err(e) = self_clone.recording_service.recover_stale_recordings().await {
              tracing::error!("[RecordingScheduler] Stale recording recovery failed: {}", e);
          }
      });

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
            SignalEvent::AlarmRaised { alarm_id, device_tag, channel_tag, alarm_type, .. } => {
                let device_tag = device_tag.as_deref().filter(|s| !s.is_empty());
                self.handle_alarm_event(*alarm_id, device_tag, channel_tag.as_deref(), alarm_type).await?;
            }
            SignalEvent::AlarmRecovered { device_tag, channel_tag, alarm_type, .. } => {
                self.handle_alarm_recovered(device_tag, channel_tag.as_deref(), alarm_type).await?;
            }
            SignalEvent::StreamStart { stream_key, .. } => {
                // 流就绪了：立刻尝试启动这个流上“等待中”的录像任务
                if let Some((device_tag, channel_tag)) =
                    crate::common::device_utils::parse_stream_key(stream_key)
                {
                    let is_alarm = {
                        let guard = self.active_alarm_recordings.read().await;
                        guard.values().any(|state| state.stream_key == *stream_key)
                    };
                    // 只有策略仍然生效（启用中且在时间窗口内）才启动，
                    // 避免把已经过期的等待任务重新拉起来。
                    let policy_active = self
                        .active_timed_policy_for(&device_tag, &channel_tag)
                        .await
                        .is_some();
                    if is_alarm || policy_active {
                        if let Some(pending) = self
                            .pending_recording_for(&device_tag, &channel_tag, RecordingType::Timed)
                            .await
                        {
                            self.try_start_pending(&pending).await;
                        }
                        if let Some(pending) = self
                            .pending_recording_for(&device_tag, &channel_tag, RecordingType::Alarm)
                            .await
                        {
                            self.try_start_pending(&pending).await;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_alarm_event(
        &self,
        alarm_id: i64,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        alarm_type: &str,
    ) -> anyhow::Result<()> {
        let policies = self.policy_service.find_alarm_triggered_policies(alarm_type).await?;
        let now = Utc::now();

        for policy in policies {
            if !policy.enabled {
                continue;
            }

            // 报警来自具体设备时，只对匹配该设备/通道的策略生效
            if let Some(dev) = device_tag {
                if policy.device_tag != dev {
                    continue;
                }
            }
            if let Some(ch) = channel_tag {
                if policy.channel_tag != ch {
                    continue;
                }
            }

            if !self.policy_service.should_start_alarm_recording(&policy, alarm_type) {
                continue;
            }

            let stream_key =
                crate::common::device_utils::format_stream_key(&policy.device_tag, &policy.channel_tag);
            let session_key = alarm_session_key(&stream_key, alarm_type);

            // 持续报警只刷新会话时间，不重复创建录像任务；
            // 会话结束（显式恢复或超时）后才允许下一次报警重新开始。
            {
                let mut active = self.active_alarm_recordings.write().await;
                if let Some(state) = active.get_mut(&session_key) {
                    state.last_alarm_at = now;
                    state.latest_alarm_id = alarm_id;
                    tracing::debug!(
                        "[RecordingScheduler] Alarm session refreshed: {} (alarm {})",
                        session_key, alarm_id
                    );
                    continue;
                }
            }

            if let Err(e) = self
                .start_alarm_session(&policy, &stream_key, alarm_type, alarm_id)
                .await
            {
                tracing::error!(
                    "[RecordingScheduler] Failed to start alarm recording for policy {}: {}",
                    policy.id, e
                );
            }
        }

        Ok(())
    }

    /// 显式恢复（例如告警被标记为已处理）：结束匹配设备/通道/类型的报警会话。
    async fn handle_alarm_recovered(
        &self,
        device_tag: &str,
        channel_tag: Option<&str>,
        alarm_type: &str,
    ) -> anyhow::Result<()> {
        let keys: Vec<String> = {
            let active = self.active_alarm_recordings.read().await;
            active
                .iter()
                .filter(|(_, state)| {
                    state.alarm_type == alarm_type
                        && session_matches(&state.stream_key, device_tag, channel_tag)
                })
                .map(|(key, _)| key.clone())
                .collect()
        };

        for key in keys {
            if let Err(e) = self.end_alarm_session(&key).await {
                tracing::warn!(
                    "[RecordingScheduler] Stop alarm session failed: session={} err={}",
                    key, e
                );
            }
        }

        Ok(())
    }

    async fn start_alarm_session(
        &self,
        policy: &RecordingPolicy,
        stream_key: &str,
        alarm_type: &str,
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
            recording_type: Some(RecordingType::Alarm),
            alarm_id: Some(alarm_id),
        };

        let recording = self.recording_service.create_recording(
            req,
            policy.channel_tag.clone(),
            media_server.clone(),
        ).await;

        let recording = match self.recording_service.start_recording(recording.id).await {
            Ok(recording) => recording,
            Err(e) => {
                // 流还没就绪等情况已经在 start_recording 里转成 pending，
                // 这里不再直接丢弃报警录像，而是挂起等待流就绪。
                let pending = self
                    .pending_recording_for(&policy.device_tag, &policy.channel_tag, RecordingType::Alarm)
                    .await;
                match pending {
                    Some(pending) if pending.is_pending() => pending,
                    _ => return Err(anyhow::anyhow!("{}", e)),
                }
            }
        };

        let now = Utc::now();
        let state = AlarmRecordingState {
            recording_id: recording.id,
            stream_key: stream_key.to_string(),
            alarm_type: alarm_type.to_string(),
            latest_alarm_id: alarm_id,
            last_alarm_at: now,
            recovery_secs: policy.alarm_recovery_secs,
            post_record_secs: policy.post_record_secs,
            stop_timer_handle: None,
        };

        let session_key = alarm_session_key(stream_key, alarm_type);
        self.active_alarm_recordings
            .write()
            .await
            .insert(session_key.clone(), state);

        tracing::info!(
            "[RecordingScheduler] Alarm session started: {} (recording {}, recovery {}s, post {}s)",
            session_key, recording.id, policy.alarm_recovery_secs, policy.post_record_secs
        );

        Ok(())
    }

    /// 结束报警会话：停止该会话的录像；`post_record_secs` 作为结束后的尾留时长。
    async fn end_alarm_session(&self, session_key: &str) -> anyhow::Result<()> {
        let state = self.active_alarm_recordings.write().await.remove(session_key);
        let Some(state) = state else {
            return Ok(());
        };

        if let Some(handle) = state.stop_timer_handle {
            handle.abort();
        }

        let recording_id = state.recording_id;
        let post_secs = state.post_record_secs;

        tracing::info!(
            "[RecordingScheduler] Alarm session ended: {} (recording {}, tail {}s)",
            session_key, recording_id, post_secs
        );

        if post_secs > 0 {
            let recording_service = self.recording_service.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(post_secs as u64)).await;
                if let Err(e) = recording_service.stop_recording(recording_id).await {
                    tracing::warn!(
                        "[RecordingScheduler] Alarm tail stop failed: recording={} err={}",
                        recording_id, e
                    );
                }
            });
        } else {
            self.recording_service.stop_recording(recording_id).await?;
        }

        Ok(())
    }

    async fn check_timed_policies(&self) -> anyhow::Result<()> {
        let policies = self.policy_service.list_policies().await?;
        let now = Utc::now();

        for policy in policies.iter().filter(|p| p.policy_type == PolicyType::Timed && p.enabled) {

            let stream_key = crate::common::device_utils::format_stream_key(&policy.device_tag, &policy.channel_tag);

            let within_window = self.policy_service.is_within_time_window(policy, now);

            // 定时录像和报警录像可以并存：它们共享媒体服务器上的同一份录制，
            // 每个任务各记一份文件
            let has_timed_task = self
                .recording_service
                .get_active_recordings_by_stream_key(&stream_key)
                .await
                .map(|list| {
                    list.iter().any(|r| {
                        r.recording_type == RecordingType::Timed && r.state != RecordingState::Pending
                    })
                })
                .unwrap_or(false);

            if within_window {
                if has_timed_task {
                    tracing::debug!(
                        "[RecordingScheduler] Skip: an active timed recording already exists for {}",
                        stream_key
                    );
                    continue;
                }

                // 已经有一个“等待设备/流就绪”的任务：复用它，不要每次都新建一行
                if let Some(pending) = self
                    .pending_recording_for(&policy.device_tag, &policy.channel_tag, RecordingType::Timed)
                    .await
                {
                    self.try_start_pending(&pending).await;
                    continue;
                }

                if let Err(e) = self.start_timed_recording(policy, &stream_key).await {
                    tracing::error!(
                        "[RecordingScheduler] Failed to start timed recording: policy_id={} err={}",
                        policy.id, e
                    );
                }
            } else {
                if let Err(_e) = self.stop_timed_recording(&stream_key).await {
                }
                // 窗口结束：放弃还在等待的任务，避免它被流就绪事件误触发
                if let Some(pending) = self
                    .pending_recording_for(&policy.device_tag, &policy.channel_tag, RecordingType::Timed)
                    .await
                {
                    self.abandon_pending_recording(&pending).await;
                }
            }
        }

        Ok(())
    }

    /// 该流上“等待设备/流就绪”的录像任务
    async fn pending_recording_for(
        &self,
        device_tag: &str,
        channel_tag: &str,
        recording_type: RecordingType,
    ) -> Option<Recording> {
        self.infra
            .db
            .find_pending_recording_by_stream(device_tag, channel_tag, &recording_type.to_string())
            .await
            .ok()
            .flatten()
    }

    /// 流是否已经在媒体服务器上就绪（代理存在，可以开始录像）
    async fn is_stream_ready(&self, server_tag: &str, app: &str, stream_key: &str) -> bool {
        self.cluster.is_stream_online(app, stream_key, server_tag).await
    }

    /// 尝试启动一个“等待中”的录像任务。
    /// 流还没就绪或者退避时间没到就继续等待，避免反复打扰媒体服务器。
    async fn try_start_pending(&self, pending: &Recording) {
        let now = Utc::now();
        if !pending.can_retry_at(now) {
            return;
        }

        let stream_key = pending.stream_key();

        // 报警会话可能已经结束（超时/显式恢复）：不要再把等待中的任务拉起来
        if pending.recording_type == RecordingType::Alarm {
            let has_session = {
                let active = self.active_alarm_recordings.read().await;
                active.values().any(|state| state.stream_key == stream_key)
            };
            if !has_session {
                return;
            }
        }

        let (server_tag, app) = self
            .get_media_server_for_stream(&stream_key)
            .await
            .unwrap_or_else(|_| ("default".to_string(), "live".to_string()));

        if !self.is_stream_ready(&server_tag, &app, &stream_key).await {
            tracing::debug!(
                "[RecordingScheduler] Recording {} still waiting for the stream {}",
                pending.id, stream_key
            );
            return;
        }

        match self.recording_service.start_recording(pending.id).await {
            Ok(recording) => {
                tracing::info!(
                    "[RecordingScheduler] Pending recording {} started on {}",
                    recording.id, stream_key
                );
            }
            Err(e) => {
                tracing::debug!(
                    "[RecordingScheduler] Pending recording {} could not be started yet: {}",
                    pending.id, e
                );
            }
        }
    }

    async fn start_timed_recording(
        &self,
        policy: &RecordingPolicy,
        stream_key: &str,
    ) -> anyhow::Result<()> {
        let (media_server, app) = self.get_media_server_for_stream(stream_key).await?;

        // 前置检查：流还没在媒体服务器上就绪时，先挂一个“等待中”的任务，
        // 不要反复创建任务并报错。
        if !self.is_stream_ready(&media_server, &app, stream_key).await {
            self.create_pending_recording(
                policy,
                stream_key,
                &media_server,
                vec!["timed".to_string()],
                "等待设备/流就绪",
            )
            .await?;
            return Ok(());
        }

        let req = CreateRecordingRequest {
            device_tag: policy.device_tag.clone(),
            channel_tag: policy.channel_tag.clone(),
            format: Some(policy.format.clone()),
            duration_secs: Some(policy.max_duration_secs),
            max_file_size_mb: Some(policy.max_file_size_mb),
            output_path: policy.output_path.clone(),
            labels: Some(vec!["timed".to_string()]),
            pre_record_secs: Some(policy.pre_record_secs),
            recording_type: Some(RecordingType::Timed),
            alarm_id: None,
        };

        let recording = self.recording_service.create_recording(
            req,
            policy.channel_tag.clone(),
            media_server.clone(),
        ).await;

        self.recording_service.start_recording(recording.id).await?;

        Ok(())
    }

    /// 创建一个“等待设备/流就绪”的录像任务。
    async fn create_pending_recording(
        &self,
        policy: &RecordingPolicy,
        stream_key: &str,
        media_server: &str,
        labels: Vec<String>,
        reason: &str,
    ) -> anyhow::Result<Recording> {
        let req = CreateRecordingRequest {
            device_tag: policy.device_tag.clone(),
            channel_tag: policy.channel_tag.clone(),
            format: Some(policy.format.clone()),
            duration_secs: Some(policy.max_duration_secs),
            max_file_size_mb: Some(policy.max_file_size_mb),
            output_path: policy.output_path.clone(),
            labels: Some(labels),
            pre_record_secs: Some(policy.pre_record_secs),
            recording_type: Some(RecordingType::Timed),
            alarm_id: None,
        };

        let mut recording = self.recording_service.create_recording(
            req,
            policy.channel_tag.clone(),
            media_server.to_string(),
        ).await;

        recording.state = RecordingState::Pending;
        recording.error_message = Some(reason.to_string());
        recording.retry_count = 0;
        recording.next_retry_at = Some(Utc::now());

        if let Err(e) = self.infra.db.update_recording_start_result(&recording).await {
            tracing::error!("[RecordingScheduler] Failed to persist pending recording: {}", e);
        }

        tracing::info!(
            "[RecordingScheduler] Recording task {} for {} is waiting for the device/stream: {}",
            recording.id, stream_key, reason
        );
        Ok(recording)
    }

    /// 放弃一个“等待中”的录像任务（例如时间窗口已经结束）。
    async fn abandon_pending_recording(&self, pending: &Recording) {
        let mut recording = pending.clone();
        recording.set_error("等待期间录像窗口已结束，任务已放弃".to_string());
        recording.next_retry_at = None;
        if let Err(e) = self.infra.db.update_recording_start_result(&recording).await {
            tracing::error!("[RecordingScheduler] Failed to abandon pending recording: {}", e);
        }
        tracing::info!(
            "[RecordingScheduler] Pending recording {} abandoned (window ended)",
            recording.id
        );
    }

    /// 该设备/通道上当前生效（启用中且在时间窗口内）的定时策略
    async fn active_timed_policy_for(
        &self,
        device_tag: &str,
        channel_tag: &str,
    ) -> Option<RecordingPolicy> {
        let policies = self.policy_service.find_policies_by_device(device_tag).await.ok()?;
        let now = Utc::now();
        policies.into_iter().find(|p| {
            p.policy_type == PolicyType::Timed
                && p.enabled
                && p.channel_tag == channel_tag
                && self.policy_service.is_within_time_window(p, now)
        })
    }

    /// 只结束定时录像任务，报警录像任务不受影响
    async fn stop_timed_recording(&self, stream_key: &str) -> anyhow::Result<()> {
        for recording in self.recording_service.get_active_recordings_by_stream_key(stream_key).await? {
            if recording.recording_type == RecordingType::Timed {
                self.recording_service.stop_recording(recording.id).await?;
            }
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
        let now = Utc::now();
        let expired: Vec<String> = {
            let guard = self.active_alarm_recordings.read().await;
            guard
                .iter()
                .filter(|(_, state)| {
                    alarm_session_should_end(now, state.last_alarm_at, state.recovery_secs)
                })
                .map(|(key, _)| key.clone())
                .collect()
        };

        for key in expired {
            tracing::info!(
                "[RecordingScheduler] Alarm session timed out (no new alarm): {}",
                key
            );
            if let Err(e) = self.end_alarm_session(&key).await {
                tracing::warn!(
                    "[RecordingScheduler] Alarm session timeout stop failed: session={} err={}",
                    key, e
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
            cluster: self.cluster.clone(),
            policy_service: self.policy_service.clone(),
            recording_service: self.recording_service.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
            active_alarm_recordings: self.active_alarm_recordings.clone(),
            check_interval_secs: self.check_interval_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn session_key_includes_stream_and_alarm_type() {
        assert_eq!(alarm_session_key("dev/ch", "motion"), "dev/ch#motion");
    }

    #[test]
    fn session_ends_only_after_recovery_window() {
        let last = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(!alarm_session_should_end(
            last + chrono::Duration::seconds(299),
            last,
            300
        ));
        assert!(alarm_session_should_end(
            last + chrono::Duration::seconds(300),
            last,
            300
        ));
    }

    #[test]
    fn session_with_zero_recovery_never_times_out() {
        let last = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(!alarm_session_should_end(
            last + chrono::Duration::seconds(10_000),
            last,
            0
        ));
    }
}