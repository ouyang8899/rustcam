use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::domain::device::RecordingConfig;
use crate::domain::recording::{
    classify_start_failure, retry_backoff_secs, CreateRecordingRequest, Recording, RecordingFormat,
    RecordingState, RecordingType, StartFailureKind,
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

        recording.recording_type = req.recording_type.unwrap_or(RecordingType::Timed);

        recording.alarm_id = req.alarm_id;

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

        let stream_key = recording.stream_key();

        // 同一个流上可能已经有别的任务在录：媒体服务器只能录一份，
        // 新任务直接共享那份录制，不能再调 startRecord
        let shared_with = self.repo.find_active_recordings_by_stream_key(&stream_key).await?
            .into_iter()
            .find(|r| r.id != recording.id && r.state != RecordingState::Pending);

        if let Some(existing) = shared_with {
            tracing::info!(
                "[RecordingService] Recording task {} shares the recording of task {} on {}",
                recording.id, existing.id, stream_key
            );
            recording.start();
            recording.retry_count = 0;
            recording.next_retry_at = None;
            if let Err(e) = self.repo.update_recording_start_result(&recording).await {
                tracing::error!("[RecordingService] Failed to persist: {}", e);
            }
            return Ok(recording);
        }

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
                    let message = e.to_string();
                    let kind = classify_start_failure(&message);

                    match kind {
                        StartFailureKind::Permanent => {
                            tracing::error!(
                                "[RecordingService] Failed to start recording on media server {}: {}",
                                recording.server_tag, message
                            );
                            recording.set_error(message.clone());
                        }
                        _ => {
                            // 设备或流还没就绪：任务保持“等待”状态，稍后重试
                            let backoff = retry_backoff_secs(kind, recording.retry_count);
                            let next_retry_at = Utc::now() + chrono::TimeDelta::seconds(backoff);
                            tracing::info!(
                                "[RecordingService] Recording {} is waiting for the stream (retry in {}s): {}",
                                recording.id, backoff, message
                            );
                            recording.set_pending(message.clone(), next_retry_at);
                        }
                    }

                    if let Err(e) = self.repo.update_recording_start_result(&recording).await {
                        tracing::error!("[RecordingService] Failed to persist start result: {}", e);
                    }
                    return Err(AppError::Internal(format!("Media server error: {}", message)));
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
        // 启动成功：清掉重试信息
        recording.retry_count = 0;
        recording.next_retry_at = None;
        if let Err(e) = self.repo.update_recording_start_result(&recording).await {
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

        let stream_key = recording.stream_key();

        // 还有别的任务在用这份录制时不能停媒体服务器，否则把别人的录像也停了
        let others = self.repo.find_active_recordings_by_stream_key(&stream_key).await?
            .into_iter()
            .filter(|r| r.id != recording.id && r.state != RecordingState::Pending)
            .count();

        if others == 0 {
            if let Some(adapter) = self.cluster.get_server(&recording.server_tag) {
                if let Err(e) = adapter.stop_recording(&app, &stream_key, format_str).await {
                    tracing::warn!("[RecordingService] Failed to stop recording on media server: {}", e);
                }
            }
        } else {
            tracing::info!(
                "[RecordingService] Recording task {} stopped, {} other task(s) still use the recording of {}",
                recording.id, others, stream_key
            );
        }

        recording.complete();
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
        for recording in self.get_active_recordings_by_stream_key(stream_key).await? {
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

        let channel_tag = crate::common::device_utils::parse_stream_key(stream_key)
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
            recording_type: Some(RecordingType::Timed),
            alarm_id: None,
        };

        let recording = self.create_recording(req, channel_tag, media_server.to_string()).await;
        let recording = self.start_recording(recording.id).await?;

        Ok(Some(recording))
    }

    pub async fn get_active_recording_by_stream_key(&self, stream_key: &str) -> Result<Option<Recording>> {
        self.repo.find_active_recording_by_stream_key(stream_key).await
    }

    /// 同一个流上并行的所有录像任务
    pub async fn get_active_recordings_by_stream_key(&self, stream_key: &str) -> Result<Vec<Recording>> {
        self.repo.find_active_recordings_by_stream_key(stream_key).await
    }

    /// 该流上“等待设备/流就绪”的录像任务
    pub async fn get_pending_recording_by_stream(
        &self,
        device_tag: &str,
        channel_tag: &str,
        recording_type: RecordingType,
    ) -> Result<Option<Recording>> {
        self.repo
            .find_pending_recording_by_stream(device_tag, channel_tag, &recording_type.to_string())
            .await
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

    /// 分页查询录像任务，过滤条件全部下推到 SQL。
    pub async fn list_recordings_filtered(
        &self,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        state: Option<&str>,
        search: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Recording>> {
        self.repo
            .list_recordings_filtered(device_tag, channel_tag, state, search, limit, offset)
            .await
    }

    pub async fn count_recordings(&self) -> Result<usize> {
        self.repo.count_recordings().await
    }

    /// 录像任务总数（与分页查询使用相同的过滤条件）。
    pub async fn count_recordings_filtered(
        &self,
        device_tag: Option<&str>,
        channel_tag: Option<&str>,
        state: Option<&str>,
        search: Option<&str>,
    ) -> Result<usize> {
        self.repo
            .count_recordings_filtered(device_tag, channel_tag, state, search)
            .await
    }

    /// 一个录像任务下的文件列表：录像文件以数据库里的分段为准，
    /// 媒体服务器上的目录只是文件的存放位置。
    /// 每个文件都带上它所属的任务，外部应用可以按任务类型分别处理。
    pub async fn list_recorded_files(&self, id: i64) -> Result<Vec<crate::domain::recording_policy::RecordingFileWithTask>> {
        let recording = self.repo.get_recording(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        let segments = self.repo.list_recording_segments_by_recording_id(id).await?;

        let mut task_info = recording.task_info();
        if let Some(alarm_id) = recording.alarm_id {
            if let Ok(Some(alarm)) = self.repo.get_alarm(alarm_id).await {
                task_info.alarm_type = Some(alarm.alarm_type);
                task_info.alarm_time = Some(alarm.alarm_time);
            }
        }

        Ok(segments.into_iter().map(|s| {
            let filename = std::path::Path::new(&s.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&s.file_path)
                .to_string();
            crate::domain::recording_policy::RecordingFileWithTask {
                file: crate::adapter::media_server::RecordingFile {
                    filename,
                    path: s.file_path,
                    size: s.file_size,
                    duration_secs: s.duration_secs,
                    created_at: s.start_time.timestamp(),
                    stream_key: Some(recording.stream_key()),
                    media_server_name: s.server_tag.clone(),
                },
                task: Some(task_info.clone()),
            }
        }).collect())
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

    /// 媒体服务器每录完一个文件就通知一次。
    /// 这个文件属于该流上的所有录像任务：每个任务各记一条分段，任务类型不同。
    pub async fn finalize_recording(
        &self,
        stream_key: &str,
        source_path: Option<String>,
        file_size: u64,
        duration_secs: u64,
        file_start_time: Option<u64>,
    ) -> Result<()> {
        let recordings = self.repo.find_active_recordings_by_stream_key(stream_key).await?;

        if recordings.is_empty() {
            tracing::warn!("[RecordingService] No active recording found for stream={}", stream_key);
            return Ok(());
        }

        for mut recording in recordings {
            recording.filename = source_path
                .as_ref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .and_then(|n| n.to_str())
                .map(String::from);
            recording.file_size = file_size;
            recording.duration_secs = duration_secs;
            if let Err(e) = self.repo.update_recording(&recording.clone()).await {
                tracing::error!("[RecordingService] Failed to update recording {}: {}", recording.id, e);
            }

            let indexed_path = source_path.clone().unwrap_or_else(|| recording.stream_key());
            let play_url = self.build_segment_play_url(&recording, &indexed_path);
            if let Err(e) = self
                .index_recording_segment(
                    &recording,
                    &indexed_path,
                    play_url.as_deref(),
                    file_size,
                    duration_secs,
                    file_start_time,
                )
                .await
            {
                tracing::error!(
                    "[RecordingService] Failed to index recording segment for recording {}: {}",
                    recording.id,
                    e
                );
            }
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
        file_start_time: Option<u64>,
    ) -> Result<()> {
        let device_tag = recording.device_tag.clone().unwrap_or_default();
        let channel_tag = recording.channel_tag.clone().unwrap_or_default();

        // 文件自己的时间范围：媒体服务器给的文件开始时间，没有就用任务开始时间
        let start_time = file_start_time
            .and_then(|ts| DateTime::from_timestamp(ts as i64, 0))
            .or(recording.started_at)
            .unwrap_or_else(Utc::now);
        let end_time = start_time + chrono::TimeDelta::seconds(duration_secs as i64);

        let segment_type = match recording.recording_type {
            RecordingType::Alarm => "alarm".to_string(),
            _ => "normal".to_string(),
        };
        let alarm_id = recording.alarm_id;

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
