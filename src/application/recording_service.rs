use std::sync::Arc;

use chrono::Utc;

use crate::domain::device::RecordingConfig;
use crate::domain::recording::{
    CreateRecordingRequest, Recording, RecordingFormat, RecordingState,
};
use crate::error::{AppError, Result};

pub struct RecordingService {
    repo: Arc<crate::infrastructure::DbRepository>,
    cluster: Arc<crate::infrastructure::cluster::ClusterManager>,
}

impl RecordingService {
    pub fn new(
        repo: Arc<crate::infrastructure::DbRepository>,
        cluster: Arc<crate::infrastructure::cluster::ClusterManager>,
    ) -> Self {
        Self {
            repo,
            cluster,
        }
    }

    /// 回收僵尸录制任务
    ///
    /// 上一轮进程异常退出时，recording 任务会残留在 starting/recording 状态。
    /// 调度器把"存在活动任务"视为"正在录制"从而**静默跳过**该流，
    /// 导致该设备的定时/报警录制永久无法启动（无任何错误日志）。
    /// 这里在服务启动时核对媒体服务器：仍在录制的保留，其余标记为 error。
    pub async fn recover_stale_recordings(&self) -> Result<usize> {
        let active = self.repo.list_active_recordings().await?;
        if active.is_empty() {
            return Ok(0);
        }

        let mut recovered = 0usize;
        for mut recording in active {
            let format_str = Self::format_to_str(recording.format.clone());
            let cache_key = format!(
                "{}/{}",
                recording.device_tag.as_deref().unwrap_or(""),
                recording.channel_tag.as_deref().unwrap_or("")
            );
            let app = self
                .repo
                .streams_cache()
                .get(&cache_key)
                .map(|s| s.app.clone())
                .unwrap_or_else(|| "live".to_string());

            let still_recording = match self.cluster.get_server(&recording.server_tag) {
                Some(adapter) => adapter
                    .is_recording(&recording.stream_key(), format_str)
                    .await
                    .unwrap_or(false),
                None => false,
            };

            if still_recording {
                continue;
            }

            recording.set_error("服务重启后录制未在媒体服务器上恢复，已自动回收".to_string());
            if let Err(e) = self.repo.update_recording(&recording.clone()).await {
                tracing::error!(
                    "[RecordingService] Failed to persist stale recording cleanup: {}",
                    e
                );
                continue;
            }
            recovered += 1;
        }

        if recovered > 0 {
            tracing::warn!(
                "[RecordingService] Recovered {} stale recording(s) that blocked the scheduler",
                recovered
            );
        }
        Ok(recovered)
    }

    pub async fn create_recording(
        &self,
        req: CreateRecordingRequest,
        channel_tag: String,
        media_server: String,
    ) -> Recording {
        let format = req.format.unwrap_or(RecordingFormat::Mp4);
        let mut recording = Recording::new(req.device_tag.clone(), channel_tag, media_server, format);

        if let Some(labels) = req.labels {
            recording.labels = labels;
        }

        recording.pre_record_secs = req.pre_record_secs;

        recording.max_duration_secs = req.duration_secs;

        recording.output_path = req.output_path;

        match self.repo.create_recording(&recording).await {
            Ok(id) => {
                recording.id = id;
            }
            Err(e) => {
                tracing::error!("[RecordingService] Failed to persist recording: {}", e);
            }
        }

        recording
    }

    pub async fn get_recording(&self, id: i64) -> Result<Option<Recording>> {
        self.repo.get_recording(id).await
    }

    pub async fn start_recording(&self, id: i64) -> Result<Recording> {
        let mut recording = self.repo.get_recording(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        let format_str = Self::format_to_str(recording.format.clone());

        let cache_key = format!("{}/{}", recording.device_tag.as_deref().unwrap_or(""), recording.channel_tag.as_deref().unwrap_or(""));
        let app = self.repo.streams_cache().get(&cache_key)
            .map(|s| s.app.clone())
            .unwrap_or_else(|| "live".to_string());

        let rec_info = if let Some(adapter) = self.cluster.get_server(&recording.server_tag) {
            match adapter.start_recording(
                &app,
                &recording.stream_key(),
                format_str,
                recording.output_path.as_deref(),
                recording.max_duration_secs,
                recording.pre_record_secs,
            ).await {
                Ok(info) => {
                    Some(info)
                }
                Err(e) => {
                    tracing::error!(
                        "[RecordingService] Failed to start recording on media server {}: {}",
                        recording.server_tag, e
                    );
                    recording.set_error(e.to_string());
                    if let Err(e) = self.repo.update_recording(&recording.clone()).await {
                        tracing::error!("[RecordingService] Failed to persist error state: {}", e);
                    }
                    return Err(AppError::Internal(format!("Media server error: {}", e)));
                }
            }
        } else {
            tracing::error!(
                "[RecordingService] Media server '{}' not found, cannot start recording",
                recording.server_tag
            );
            recording.set_error(format!("Media server '{}' not found", recording.server_tag));
            if let Err(e) = self.repo.update_recording(&recording.clone()).await {
                tracing::error!("[RecordingService] Failed to persist error state: {}", e);
            }
            return Err(AppError::Internal(format!(
                "Media server '{}' not found",
                recording.server_tag
            )));
        };

        recording.start();
        if let Some(info) = &rec_info {
            if !info.output_path.is_empty() {
                recording.set_output(info.output_path.clone(), 0);
            }
        }
        if let Err(e) = self.repo.update_recording(&recording.clone()).await {
            tracing::error!("[RecordingService] Failed to persist: {}", e);
        }

        Ok(recording)
    }

    pub async fn stop_recording(&self, id: i64) -> Result<Recording> {
        let mut recording = self.repo.get_recording(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        let format_str = Self::format_to_str(recording.format.clone());

        let cache_key = format!("{}/{}", recording.device_tag.as_deref().unwrap_or(""), recording.channel_tag.as_deref().unwrap_or(""));
        let app = self.repo.streams_cache().get(&cache_key)
            .map(|s| s.app.clone())
            .unwrap_or_else(|| "live".to_string());

        if let Some(adapter) = self.cluster.get_server(&recording.server_tag) {
            if let Err(e) = adapter.stop_recording(&app, &recording.stream_key(), format_str).await {
                tracing::warn!("[RecordingService] Failed to stop recording on media server: {}", e);
            }
        }

        recording.stop();
        if let Err(e) = self.repo.update_recording(&recording.clone()).await {
            tracing::error!("[RecordingService] Failed to persist: {}", e);
        }

        Ok(recording)
    }

    pub async fn pause_recording(&self, id: i64) -> Result<Recording> {
        let mut recording = self.repo.get_recording(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        recording.pause();
        if let Err(e) = self.repo.update_recording(&recording.clone()).await {
            tracing::error!("[RecordingService] Failed to persist: {}", e);
        }

        Ok(recording)
    }

    pub async fn resume_recording(&self, id: i64) -> Result<Recording> {
        let mut recording = self.repo.get_recording(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        recording.resume();
        if let Err(e) = self.repo.update_recording(&recording.clone()).await {
            tracing::error!("[RecordingService] Failed to persist: {}", e);
        }

        Ok(recording)
    }

    pub async fn delete_recording(&self, id: i64) -> Result<()> {
        if self.repo.get_recording(id).await?.is_none() {
            return Err(AppError::NotFound(format!("Recording {} not found", id)));
        }

        if let Err(e) = self.repo.delete_recording(id).await {
            tracing::error!("[RecordingService] Failed to delete from DB: {}", e);
        }

        Ok(())
    }

    pub async fn stop_recording_by_stream_key(&self, stream_key: &str) -> Result<()> {
        if let Some(recording) = self.get_active_recording_by_stream_key(stream_key).await? {
            self.stop_recording(recording.id).await?;
        }
        Ok(())
    }

    pub async fn start_recording_for_stream(
        &self,
        device_tag: String,
        stream_key: &str,
        media_server: &str,
        config: &RecordingConfig,
    ) -> Result<Option<Recording>> {
        if !config.enabled {
            return Ok(None);
        }

        let channel_tag = crate::domain::stream::parse_stream_key(stream_key)
            .map(|(_, ch)| ch)
            .unwrap_or_else(|| "recording".to_string());

        let format = config.format.clone().unwrap_or(RecordingFormat::Mp4);
        let req = CreateRecordingRequest {
            device_tag,
            channel_tag: channel_tag.clone(),
            format: Some(format),
            duration_secs: config.max_duration_secs,
            max_file_size_mb: config.max_file_size_mb,
            output_path: None,
            labels: config.labels.clone(),
            pre_record_secs: None,
        };

        let recording = self.create_recording(req, channel_tag, media_server.to_string()).await;
        let recording = self.start_recording(recording.id).await?;

        Ok(Some(recording))
    }

    pub async fn get_active_recording_by_stream_key(&self, stream_key: &str) -> Result<Option<Recording>> {
        self.repo.find_active_recording_by_stream_key(stream_key).await
    }

    pub async fn list_device_recordings(&self, device_tag: &str) -> Result<Vec<Recording>> {
        self.repo.list_recordings_by_device(device_tag).await
    }

    pub async fn list_device_recordings_by_device_tag(&self, device_tag: &str) -> Result<Vec<Recording>> {
        self.repo.list_recordings_by_device_tag(device_tag).await
    }

    pub async fn list_recordings_paginated(&self, limit: usize, offset: usize) -> Result<Vec<Recording>> {
        self.repo.list_recordings_paginated(limit, offset).await
    }

    pub async fn count_recordings(&self) -> Result<usize> {
        self.repo.count_recordings().await
    }

    pub async fn list_recorded_files(&self, id: i64) -> Result<Vec<crate::adapter::media_server::RecordingFile>> {
        let recording = self.repo.get_recording(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        let cache_key = format!("{}/{}", recording.device_tag.as_deref().unwrap_or(""), recording.channel_tag.as_deref().unwrap_or(""));
        let app = self.repo.streams_cache().get(&cache_key)
            .map(|s| s.app.clone())
            .unwrap_or_else(|| "live".to_string());

        if let Some(adapter) = self.cluster.get_server(&recording.server_tag) {
            match adapter.list_recordings(&app, &recording.stream_key()).await {
                Ok(files) => return Ok(files),
                Err(e) => tracing::warn!("[RecordingService] Failed to list recordings: {}", e),
            }
        }

        Ok(vec![])
    }

    pub async fn get_stats(&self) -> Result<serde_json::Value> {
        let recordings = self.repo.list_recordings_paginated(10000, 0).await?;
        let total = recordings.len();
        let recording_count = recordings.iter()
            .filter(|r| r.state == RecordingState::Recording)
            .count();
        let completed = recordings.iter()
            .filter(|r| r.state == RecordingState::Completed)
            .count();
        let total_size: u64 = recordings.iter().map(|r| r.file_size).sum();

        Ok(serde_json::json!({
            "total": total,
            "recording": recording_count,
            "completed": completed,
            "total_size_bytes": total_size,
            "total_size_mb": total_size as f64 / (1024.0 * 1024.0),
        }))
    }

    pub async fn finalize_recording(
        &self,
        stream_key: &str,
        source_path: Option<String>,
        file_size: u64,
        duration_secs: u64,
    ) -> Result<()> {
        if let Some(mut recording) = self.repo.find_active_recording_by_stream_key(stream_key).await? {

            recording.filename = source_path
                .as_ref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .and_then(|n| n.to_str())
                .map(String::from);
            recording.file_size = file_size;
            recording.duration_secs = duration_secs;
            recording.complete();
            if let Err(e) = self.repo.update_recording(&recording.clone()).await {
                tracing::error!("[RecordingService] Failed to finalize recording {}: {}", recording.id, e);
            } else {
            }

            let indexed_path = source_path.unwrap_or_else(|| recording.stream_key());
            let play_url = self.build_segment_play_url(&recording, &indexed_path);
            if let Err(e) = self
                .index_recording_segment(&recording, &indexed_path, play_url.as_deref(), file_size, duration_secs)
                .await
            {
                tracing::error!(
                    "[RecordingService] Failed to index recording segment for recording {}: {}",
                    recording.id,
                    e
                );
            }
        } else {
            tracing::warn!("[RecordingService] No active recording found for stream={}", stream_key);
        }
        Ok(())
    }

    fn build_segment_play_url(&self, recording: &Recording, file_path: &str) -> Option<String> {
        let base = self
            .cluster
            .get_server_http_base_url(&recording.server_tag)?;

        let path = std::path::Path::new(file_path);
        let comps: Vec<String> = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();

        let idx = comps.iter().rposition(|c| c == "www")?;
        let relative = comps[idx + 1..].join("/");
        if relative.is_empty() {
            return None;
        }

        Some(format!("{}/{}", base.trim_end_matches('/'), relative))
    }

    async fn index_recording_segment(
        &self,
        recording: &Recording,
        file_path: &str,
        play_url: Option<&str>,
        file_size: u64,
        duration_secs: u64,
    ) -> Result<()> {
        let device_tag = recording.device_tag.clone().unwrap_or_default();
        let channel_tag = recording.channel_tag.clone().unwrap_or_default();

        let start_time = recording.started_at.unwrap_or_else(Utc::now);
        let end_time = recording.stopped_at.unwrap_or_else(Utc::now);

        let mut segment_type = "normal".to_string();
        let mut alarm_id: Option<i64> = None;
        for label in &recording.labels {
            if label == "alarm" {
                segment_type = "alarm".to_string();
            } else if label == "timed" {
                segment_type = "normal".to_string();
            } else if let Some(id) = label.strip_prefix("alarm:") {
                segment_type = "alarm".to_string();
                alarm_id = id.parse().ok();
            }
        }

        self.repo
            .insert_recording_segment(
                Some(recording.id),
                &device_tag,
                &channel_tag,
                start_time,
                end_time,
                duration_secs,
                file_size,
                file_path,
                Some(recording.server_tag.as_str()),
                &segment_type,
                alarm_id,
                play_url,
            )
            .await?;

        Ok(())
    }

    fn format_to_str(format: RecordingFormat) -> &'static str {
        match format {
            RecordingFormat::Mp4 => "mp4",
            RecordingFormat::Hls => "hls",
            RecordingFormat::Flv => "flv",
            RecordingFormat::Ts => "ts",
        }
    }
}

impl Clone for RecordingService {
    fn clone(&self) -> Self {
        Self {
            repo: self.repo.clone(),
            cluster: self.cluster.clone(),
        }
    }
}
