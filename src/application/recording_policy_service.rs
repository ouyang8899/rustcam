use std::sync::Arc;
use chrono::{ DateTime, Utc, Timelike, Datelike };

use crate::domain::recording_policy::{
    PolicyType, ScheduleType, RecordingPolicy, TimeWindow,
    CreateRecordingPolicyRequest, UpdateRecordingPolicyRequest,
    RecordingSegmentQuery, RecordingFileResponse, PaginatedRecordingFiles, PlayUrlResponse,
};
use crate::domain::recording::RecordingFormat;
use crate::error::{AppError, Result};

pub struct RecordingPolicyService {
    repo: Arc<crate::infrastructure::DbRepository>,
    cluster: Arc<crate::infrastructure::cluster::ClusterManager>,
}

impl RecordingPolicyService {
    pub fn new(
        repo: Arc<crate::infrastructure::DbRepository>,
        cluster: Arc<crate::infrastructure::cluster::ClusterManager>,
    ) -> Self {
        Self { repo, cluster }
    }

    fn build_play_url(&self, server_tag: &str, device_tag: &str, channel_tag: &str, start_time: &DateTime<Utc>) -> String {
        let base = self
            .cluster
            .get_server_http_base_url(server_tag)
            .unwrap_or_else(|| "http://localhost:8080".to_string());
        format!(
            "{}/recordings/{}/{}/{}/{}.mp4",
            base.trim_end_matches('/'),
            device_tag,
            channel_tag,
            start_time.format("%Y%m%d"),
            start_time.format("%H_%M_%S")
        )
    }

    pub async fn create_policy(&self, req: CreateRecordingPolicyRequest) -> Result<RecordingPolicy> {
        let format = match req.format.as_deref() {
            Some("hls") | Some("HLS") => RecordingFormat::Hls,
            Some("flv") | Some("FLV") => RecordingFormat::Flv,
            Some("ts") | Some("TS") => RecordingFormat::Ts,
            _ => RecordingFormat::Mp4,
        };

        let policy = RecordingPolicy {
            id: 0,
            device_tag: req.device_tag,
            channel_tag: req.channel_tag,
            policy_type: req.policy_type,
            enabled: req.enabled.unwrap_or(true),
            schedule_type: req.schedule_type,
            cron_expr: req.cron_expr,
            alarm_type: req.alarm_type,
            pre_record_secs: req.pre_record_secs.unwrap_or(0),
            post_record_secs: req.post_record_secs.unwrap_or(30),
            format,
            max_duration_secs: req.max_duration_secs.unwrap_or(3600),
            max_file_size_mb: req.max_file_size_mb.unwrap_or(512),
            output_path: req.output_path,
            time_windows: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let policy_id = self.repo.create_recording_policy(&policy).await?;

        if let Some(windows) = req.time_windows {
            for window in windows {
                let tw = TimeWindow {
                    id: 0,
                    policy_id,
                    start_time: window.start_time,
                    end_time: window.end_time,
                    days: window.days,
                };
                self.repo.create_recording_time_window(&tw).await?;
            }
        }

        let mut policy = policy;
        policy.id = policy_id;
        policy.time_windows = self.repo.list_recording_time_windows(policy_id).await?;

        Ok(policy)
    }

    pub async fn update_policy(&self, id: i64, req: UpdateRecordingPolicyRequest) -> Result<RecordingPolicy> {
        let mut policy = self.repo.get_recording_policy(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Policy {} not found", id)))?;

        if let Some(enabled) = req.enabled {
            policy.enabled = enabled;
        }
        if let Some(schedule_type) = req.schedule_type {
            policy.schedule_type = Some(schedule_type);
        }
        if let Some(cron_expr) = req.cron_expr {
            policy.cron_expr = Some(cron_expr);
        }
        if let Some(alarm_type) = req.alarm_type {
            policy.alarm_type = Some(alarm_type);
        }
        if let Some(pre_record_secs) = req.pre_record_secs {
            policy.pre_record_secs = pre_record_secs;
        }
        if let Some(post_record_secs) = req.post_record_secs {
            policy.post_record_secs = post_record_secs;
        }
        if let Some(format_str) = req.format {
            policy.format = match format_str.to_lowercase().as_str() {
                "hls" => RecordingFormat::Hls,
                "flv" => RecordingFormat::Flv,
                "ts" => RecordingFormat::Ts,
                _ => RecordingFormat::Mp4,
            };
        }
        if let Some(max_duration_secs) = req.max_duration_secs {
            policy.max_duration_secs = max_duration_secs;
        }
        if let Some(max_file_size_mb) = req.max_file_size_mb {
            policy.max_file_size_mb = max_file_size_mb;
        }
        if let Some(output_path) = req.output_path {
            policy.output_path = Some(output_path);
        }

        policy.updated_at = Utc::now();

        if let Some(windows) = req.time_windows {
            self.repo.delete_recording_time_windows(id).await?;
            for window in windows {
                let tw = TimeWindow {
                    id: 0,
                    policy_id: id,
                    start_time: window.start_time,
                    end_time: window.end_time,
                    days: window.days,
                };
                self.repo.create_recording_time_window(&tw).await?;
            }
        }

        self.repo.update_recording_policy(&policy).await?;
        policy.time_windows = self.repo.list_recording_time_windows(id).await?;

        Ok(policy)
    }

    pub async fn delete_policy(&self, id: i64) -> Result<()> {
        self.repo.delete_recording_policy(id).await?;
        Ok(())
    }

    pub async fn get_policy(&self, id: i64) -> Result<Option<RecordingPolicy>> {
        let mut policy = self.repo.get_recording_policy(id).await?;
        if let Some(ref mut p) = policy {
            p.time_windows = self.repo.list_recording_time_windows(p.id).await?;
        }
        Ok(policy)
    }

    pub async fn list_policies(&self) -> Result<Vec<RecordingPolicy>> {
        let mut policies = self.repo.list_recording_policies().await?;
        for policy in &mut policies {
            policy.time_windows = self.repo.list_recording_time_windows(policy.id).await?;
        }
        Ok(policies)
    }

    /// 分页查询录制策略，过滤条件在 SQL 中执行。
    pub async fn list_policies_paginated(
        &self,
        device_tag: Option<&str>,
        policy_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<RecordingPolicy>> {
        let mut policies = self
            .repo
            .list_recording_policies_paginated(device_tag, policy_type, limit, offset)
            .await?;
        for policy in &mut policies {
            policy.time_windows = self.repo.list_recording_time_windows(policy.id).await?;
        }
        Ok(policies)
    }

    /// 录制策略总数（与分页查询使用相同的过滤条件）。
    pub async fn count_policies(
        &self,
        device_tag: Option<&str>,
        policy_type: Option<&str>,
    ) -> Result<i64> {
        self.repo.count_recording_policies(device_tag, policy_type).await
    }

    pub async fn find_policies_by_device(&self, device_tag: &str) -> Result<Vec<RecordingPolicy>> {
        let mut policies = self.repo.list_recording_policies_by_device_tag(device_tag).await?;
        for policy in &mut policies {
            policy.time_windows = self.repo.list_recording_time_windows(policy.id).await?;
        }
        Ok(policies)
    }

    pub async fn find_alarm_triggered_policies(&self, alarm_type: &str) -> Result<Vec<RecordingPolicy>> {
        let mut policies = self.repo.list_alarm_triggered_policies(alarm_type).await?;
        for policy in &mut policies {
            policy.time_windows = self.repo.list_recording_time_windows(policy.id).await?;
        }
        Ok(policies)
    }

    pub async fn list_recording_segments(&self, query: RecordingSegmentQuery) -> Result<PaginatedRecordingFiles> {
        let segment_type_str = query.segment_type.map(|st| st.to_string());
        let device_tag = query.device_tag.clone();
        let channel_tag = query.channel_tag.clone();

        let total = self.repo.count_recording_segments_filtered(
            device_tag.as_deref(),
            channel_tag.as_deref(),
            segment_type_str.as_deref(),
            query.start_time,
            query.end_time,
        ).await?;

        let segments = self.repo.list_recording_segments_filtered_paginated(
            device_tag.as_deref(),
            channel_tag.as_deref(),
            segment_type_str.as_deref(),
            query.start_time,
            query.end_time,
            query.page_limit(),
            query.page_offset(),
        ).await?;

        // 每个文件属于哪个录像任务：外部应用按任务类型分别处理
        let task_ids: Vec<i64> = segments.iter().filter_map(|s| s.recording_id).collect();
        let task_infos = self.repo.recording_task_infos(&task_ids).await?;

        let files: Vec<RecordingFileResponse> = segments.into_iter().map(|seg| {
            // play_url 多数情况下没有落库，这里按媒体服务器上的实际路径补齐，
            // 客户端就不需要为每个分段再请求一次 play-url。
            let play_url = match seg.play_url.clone() {
                Some(url) if !url.is_empty() => Some(url),
                _ => {
                    let server_tag = seg.server_tag.clone().unwrap_or_else(|| "default".to_string());
                    Some(self.build_play_url(
                        &server_tag,
                        &seg.device_tag,
                        &seg.channel_tag,
                        &seg.start_time,
                    ))
                }
            };

            RecordingFileResponse {
                id: format!("seg_{}", seg.id),
                device_tag: seg.device_tag.clone(),
                channel_tag: seg.channel_tag.clone(),
                start_time: seg.start_time,
                end_time: seg.end_time,
                duration_secs: seg.duration_secs,
                file_path: seg.file_path.clone(),
                server_tag: seg.server_tag.clone(),
                play_url,
                file_size: seg.file_size,
                segment_type: seg.segment_type,
                alarm_id: seg.alarm_id,
                task: seg.recording_id.and_then(|id| task_infos.get(&id).cloned()),
            }
        }).collect();

        let page = query.page.unwrap_or(1);
        let limit = query.page_limit();
        let has_more = (page * limit) < (total as usize);

        Ok(PaginatedRecordingFiles {
            files,
            total: total as i64,
            page,
            limit,
            has_more,
        })
    }

    pub async fn get_play_url(&self, segment_id: &str) -> Result<PlayUrlResponse> {
        let id = segment_id.strip_prefix("seg_")
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| AppError::BadRequest("Invalid segment ID format".to_string()))?;

        let segment = self.repo.get_recording_segment(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Segment {} not found", id)))?;

        let server_tag = segment.server_tag.clone().unwrap_or_else(|| "default".to_string());
        let play_url = self.build_play_url(
            &server_tag,
            &segment.device_tag,
            &segment.channel_tag,
            &segment.start_time,
        );

        let expires_at = Utc::now() + chrono::Duration::hours(1);

        let token = generate_token();

        Ok(PlayUrlResponse {
            id: segment_id.to_string(),
            play_url,
            expires_at: Some(expires_at),
            token: Some(token),
        })
    }

    pub fn is_within_time_window(&self, policy: &RecordingPolicy, now: DateTime<Utc>) -> bool {
        if policy.policy_type != PolicyType::Timed {
            return true;
        }

        if let Some(ScheduleType::TimeWindows) = policy.schedule_type {
            let current_time = format!("{:02}:{:02}", now.hour(), now.minute());
            let current_day = now.weekday().num_days_from_monday() as i32;

            for window in &policy.time_windows {
                if window.days.contains(&current_day) {
                    if Self::is_time_in_window(&current_time, &window.start_time, &window.end_time) {
                        return true;
                    }
                }
            }
            return false;
        }

        true
    }

    fn parse_time_to_minutes(time_str: &str) -> Option<u32> {
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 2 {
            return None;
        }
        let hour: u32 = parts[0].parse().ok()?;
        let minute: u32 = parts[1].parse().ok()?;
        Some(hour * 60 + minute)
    }

    fn is_time_in_window(current: &str, start: &str, end: &str) -> bool {
        let Some(current_mins) = Self::parse_time_to_minutes(current) else {
            return false;
        };
        let Some(start_mins) = Self::parse_time_to_minutes(start) else {
            return false;
        };
        let Some(end_mins) = Self::parse_time_to_minutes(end) else {
            return false;
        };

        if end_mins < start_mins {
            current_mins >= start_mins || current_mins < end_mins
        } else {
            current_mins >= start_mins && current_mins <= end_mins
        }
    }

    pub fn should_start_alarm_recording(&self, policy: &RecordingPolicy, alarm_type: &str) -> bool {
        if policy.policy_type != PolicyType::AlarmTriggered {
            return false;
        }

        if !policy.enabled {
            return false;
        }

        if let Some(ref policy_alarm_type) = policy.alarm_type {
            policy_alarm_type == alarm_type || policy_alarm_type == "*"
        } else {
            false
        }
    }

    pub async fn list_timed_policies(&self) -> Result<Vec<RecordingPolicy>> {
        let policies = self.repo.list_recording_policies().await?;
        let timed: Vec<RecordingPolicy> = policies
            .into_iter()
            .filter(|p| p.policy_type == PolicyType::Timed)
            .collect();
        Ok(timed)
    }
}

impl Clone for RecordingPolicyService {
    fn clone(&self) -> Self {
        Self {
            repo: self.repo.clone(),
            cluster: self.cluster.clone(),
        }
    }
}

fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{:x}", timestamp)
}
