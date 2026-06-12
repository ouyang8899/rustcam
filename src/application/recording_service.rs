use std::sync::Arc;
use dashmap::DashMap;
use crate::context::{InfraContext, MediaContext};
use crate::domain::device::RecordingConfig;
use crate::domain::recording::{
    CreateRecordingRequest, Recording, RecordingFormat, RecordingState,
};
use crate::error::{AppError, Result};

/// 录制管理服务
///
/// 负责视频录制的创建、开始、停止、暂停、恢复和文件管理。
///
/// # 依赖
/// - InfraContext (仅 db)
/// - MediaContext (仅 cluster)
pub struct RecordingService {
    recordings: Arc<DashMap<i64, Recording>>,
    repo: Arc<crate::infrastructure::DbRepository>,
    cluster: Arc<crate::infrastructure::cluster::ClusterManager>,
}

impl RecordingService {
    pub fn new(
        repo: Arc<crate::infrastructure::DbRepository>,
        cluster: Arc<crate::infrastructure::cluster::ClusterManager>,
    ) -> Self {
        let recordings = repo.recordings_cache().clone();
        Self {
            recordings,
            repo,
            cluster,
        }
    }

    /// 创建录制记录
    pub async fn create_recording(
        &self,
        req: CreateRecordingRequest,
        stream_key: String,
        media_server: String,
    ) -> Recording {
        let format = req.format.unwrap_or(RecordingFormat::Mp4);
        let mut recording = Recording::new(req.device_id, stream_key, media_server, format);

        if let Some(labels) = req.labels {
            recording.labels = labels;
        }

        self.recordings.insert(recording.id, recording.clone());

        if let Err(e) = self.repo.create_recording(&recording).await {
            tracing::error!("[RecordingService] Failed to persist recording {}: {}", recording.id, e);
        }

        tracing::info!("[RecordingService] Created recording {} for device {}", recording.id, req.device_id);
        recording
    }

    /// 获取录制
    pub fn get_recording(&self, id: i64) -> Option<Recording> {
        self.recordings.get(&id).map(|r| r.clone())
    }

    /// 开始录制
    pub async fn start_recording(&self, id: i64) -> Result<Recording> {
        let mut recording = self.recordings.get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        let format_str = Self::format_to_str(recording.format.clone());

        let app = self.repo.streams_cache().iter()
            .find(|s| s.stream_key == recording.stream_key)
            .map(|s| s.app.clone())
            .unwrap_or_else(|| "live".to_string());

        let rec_info = if let Some(adapter) = self.cluster.get_server(&recording.media_server_name) {
            match adapter.start_recording(&app, &recording.stream_key, format_str, recording.output_path.as_deref()).await {
                Ok(info) => {
                    tracing::info!(
                        "[RecordingService] Media server started recording: {} -> {}",
                        recording.stream_key, info.output_path
                    );
                    Some(info)
                }
                Err(e) => {
                    tracing::error!(
                        "[RecordingService] Failed to start recording on media server {}: {}",
                        recording.media_server_name, e
                    );
                    recording.set_error(e.to_string());
                    if let Err(e) = self.repo.update_recording(&recording.clone()).await {
                        tracing::error!("[RecordingService] Failed to persist error state: {}", e);
                    }
                    return Err(AppError::Internal(format!("Media server error: {}", e)));
                }
            }
        } else {
            tracing::warn!(
                "[RecordingService] Media server {} not found",
                recording.media_server_name
            );
            None
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

        tracing::info!("[RecordingService] Started recording {} (id={})", recording.stream_key, id);
        Ok(recording.clone())
    }

    /// 停止录制
    pub async fn stop_recording(&self, id: i64) -> Result<Recording> {
        let mut recording = self.recordings.get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        let format_str = Self::format_to_str(recording.format.clone());

        let app = self.repo.streams_cache().iter()
            .find(|s| s.stream_key == recording.stream_key)
            .map(|s| s.app.clone())
            .unwrap_or_else(|| "live".to_string());

        if let Some(adapter) = self.cluster.get_server(&recording.media_server_name) {
            if let Err(e) = adapter.stop_recording(&app, &recording.stream_key, format_str).await {
                tracing::warn!("[RecordingService] Failed to stop recording on media server: {}", e);
            }
        }

        recording.stop();
        if let Err(e) = self.repo.update_recording(&recording.clone()).await {
            tracing::error!("[RecordingService] Failed to persist: {}", e);
        }

        tracing::info!("[RecordingService] Stopped recording {}", id);
        Ok(recording.clone())
    }

    /// 暂停录制
    pub async fn pause_recording(&self, id: i64) -> Result<Recording> {
        let mut recording = self.recordings.get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        recording.pause();
        if let Err(e) = self.repo.update_recording(&recording.clone()).await {
            tracing::error!("[RecordingService] Failed to persist: {}", e);
        }

        Ok(recording.clone())
    }

    /// 恢复录制
    pub async fn resume_recording(&self, id: i64) -> Result<Recording> {
        let mut recording = self.recordings.get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        recording.resume();
        if let Err(e) = self.repo.update_recording(&recording.clone()).await {
            tracing::error!("[RecordingService] Failed to persist: {}", e);
        }

        Ok(recording.clone())
    }

    /// 删除录制
    pub async fn delete_recording(&self, id: i64) -> Result<()> {
        if self.recordings.remove(&id).is_none() {
            return Err(AppError::NotFound(format!("Recording {} not found", id)));
        }

        if let Err(e) = self.repo.delete_recording(id).await {
            tracing::error!("[RecordingService] Failed to delete from DB: {}", e);
        }

        tracing::info!("[RecordingService] Deleted recording {}", id);
        Ok(())
    }

    /// 根据流标识停止录制
    pub async fn stop_recording_by_stream_key(&self, stream_key: &str) -> Result<()> {
        if let Some(recording) = self.get_active_recording_by_stream_key(stream_key) {
            self.stop_recording(recording.id).await?;
            tracing::info!("[RecordingService] Auto-stopped recording for stream={}", stream_key);
        }
        Ok(())
    }

    /// 为流自动开始录制
    pub async fn start_recording_for_stream(
        &self,
        device_id: i64,
        stream_key: &str,
        media_server: &str,
        config: &RecordingConfig,
    ) -> Result<Option<Recording>> {
        if !config.enabled {
            return Ok(None);
        }

        let format = config.format.clone().unwrap_or(RecordingFormat::Mp4);
        let req = CreateRecordingRequest {
            device_id,
            format: Some(format),
            duration_secs: config.max_duration_secs,
            max_file_size_mb: config.max_file_size_mb,
            output_path: None,
            labels: config.labels.clone(),
        };

        let recording = self.create_recording(req, stream_key.to_string(), media_server.to_string()).await;
        let recording = self.start_recording(recording.id).await?;

        tracing::info!("[RecordingService] Auto-started recording {} for stream={}", recording.id, stream_key);
        Ok(Some(recording))
    }

    /// 获取活跃录制
    pub fn get_active_recording_by_stream_key(&self, stream_key: &str) -> Option<Recording> {
        self.recordings.iter()
            .find(|r| {
                r.stream_key == stream_key
                    && (r.state == RecordingState::Recording || r.state == RecordingState::Paused)
            })
            .map(|r| r.clone())
    }

    /// 获取设备的所有录制
    pub fn list_device_recordings(&self, device_id: i64) -> Vec<Recording> {
        self.recordings.iter()
            .filter(|r| r.device_id == device_id)
            .map(|r| r.value().clone())
            .collect()
    }

    /// 分页获取录制
    pub async fn list_recordings_paginated(&self, limit: usize, offset: usize) -> Vec<Recording> {
        self.repo.list_recordings_paginated(limit, offset).await
    }

    /// 获取录制总数
    pub async fn count_recordings(&self) -> usize {
        self.repo.count_recordings().await
    }

    /// 获取录制文件列表
    pub async fn list_recorded_files(&self, id: i64) -> Result<Vec<crate::adapter::media_server::RecordingFile>> {
        let recording = self.recordings.get(&id)
            .ok_or_else(|| AppError::NotFound(format!("Recording {} not found", id)))?;

        let app = self.repo.streams_cache().iter()
            .find(|s| s.stream_key == recording.stream_key)
            .map(|s| s.app.clone())
            .unwrap_or_else(|| "live".to_string());

        if let Some(adapter) = self.cluster.get_server(&recording.media_server_name) {
            match adapter.list_recordings(&app, &recording.stream_key).await {
                Ok(files) => return Ok(files),
                Err(e) => tracing::warn!("[RecordingService] Failed to list recordings: {}", e),
            }
        }

        Ok(vec![])
    }

    /// 获取录制统计
    pub fn get_stats(&self) -> serde_json::Value {
        let total = self.recordings.len();
        let recording = self.recordings.iter()
            .filter(|r| r.state == RecordingState::Recording)
            .count();
        let completed = self.recordings.iter()
            .filter(|r| r.state == RecordingState::Completed)
            .count();
        let total_size: u64 = self.recordings.iter().map(|r| r.file_size).sum();

        serde_json::json!({
            "total": total,
            "recording": recording,
            "completed": completed,
            "total_size_bytes": total_size,
            "total_size_mb": total_size as f64 / (1024.0 * 1024.0),
        })
    }

    pub async fn finalize_recording(
        &self,
        stream_key: &str,
        filename: String,
        file_size: u64,
        duration_secs: u64,
    ) -> Result<()> {
        if let Some(mut recording) = self.repo.find_active_recording_by_stream_key(stream_key) {
            recording.filename = Some(filename);
            recording.file_size = file_size;
            recording.duration_secs = duration_secs;
            recording.complete();
            if let Err(e) = self.repo.update_recording(&recording).await {
                tracing::error!("[RecordingService] Failed to finalize recording {}: {}", recording.id, e);
            } else {
                self.recordings.insert(recording.id, recording);
                tracing::info!("[RecordingService] Finalized recording for stream={}", stream_key);
            }
        } else {
            tracing::warn!("[RecordingService] No active recording found for stream={}", stream_key);
        }
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
            recordings: self.recordings.clone(),
            repo: self.repo.clone(),
            cluster: self.cluster.clone(),
        }
    }
}