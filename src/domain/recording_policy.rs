use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PolicyType {
    Timed,
    AlarmTriggered,
}

impl fmt::Display for PolicyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyType::Timed => write!(f, "Timed"),
            PolicyType::AlarmTriggered => write!(f, "AlarmTriggered"),
        }
    }
}

impl PolicyType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "timed" => Some(PolicyType::Timed),
            "alarmtriggered" => Some(PolicyType::AlarmTriggered),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ScheduleType {
    Cron,
    TimeWindows,
}

impl fmt::Display for ScheduleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScheduleType::Cron => write!(f, "Cron"),
            ScheduleType::TimeWindows => write!(f, "TimeWindows"),
        }
    }
}

impl ScheduleType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cron" => Some(ScheduleType::Cron),
            "timewindows" => Some(ScheduleType::TimeWindows),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentType {
    Normal,
    Alarm,
    PreBuffer,
}

impl fmt::Display for SegmentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SegmentType::Normal => write!(f, "normal"),
            SegmentType::Alarm => write!(f, "alarm"),
            SegmentType::PreBuffer => write!(f, "prebuffer"),
        }
    }
}

impl SegmentType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "normal" => Some(SegmentType::Normal),
            "alarm" => Some(SegmentType::Alarm),
            "prebuffer" | "pre-buffer" => Some(SegmentType::PreBuffer),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TimeWindow {
    pub id: i64,
    pub policy_id: i64,
    pub start_time: String,
    pub end_time: String,
    pub days: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RecordingPolicy {
    pub id: i64,
    pub device_tag: String,
    pub channel_tag: String,
    pub policy_type: PolicyType,
    pub enabled: bool,
    pub schedule_type: Option<ScheduleType>,
    pub cron_expr: Option<String>,
    pub alarm_type: Option<String>,
    pub pre_record_secs: u32,
    pub post_record_secs: u32,
    pub format: crate::domain::recording::RecordingFormat,
    pub max_duration_secs: u32,
    pub max_file_size_mb: u32,
    pub output_path: Option<String>,
    pub time_windows: Vec<TimeWindow>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecordingPolicyRequest {
    pub device_tag: String,
    pub channel_tag: String,
    pub policy_type: PolicyType,
    pub enabled: Option<bool>,
    pub schedule_type: Option<ScheduleType>,
    pub cron_expr: Option<String>,
    pub alarm_type: Option<String>,
    pub pre_record_secs: Option<u32>,
    pub post_record_secs: Option<u32>,
    pub format: Option<String>,
    pub max_duration_secs: Option<u32>,
    pub max_file_size_mb: Option<u32>,
    pub output_path: Option<String>,
    pub time_windows: Option<Vec<CreateTimeWindowRequest>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecordingPolicyRequest {
    pub enabled: Option<bool>,
    pub schedule_type: Option<ScheduleType>,
    pub cron_expr: Option<String>,
    pub alarm_type: Option<String>,
    pub pre_record_secs: Option<u32>,
    pub post_record_secs: Option<u32>,
    pub format: Option<String>,
    pub max_duration_secs: Option<u32>,
    pub max_file_size_mb: Option<u32>,
    pub output_path: Option<String>,
    pub time_windows: Option<Vec<CreateTimeWindowRequest>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTimeWindowRequest {
    pub start_time: String,
    pub end_time: String,
    pub days: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSegment {
    pub id: i64,
    pub recording_id: Option<i64>,
    pub device_tag: String,
    pub channel_tag: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_secs: u64,
    pub file_size: u64,
    pub file_path: String,
    pub server_tag: Option<String>,
    pub segment_type: SegmentType,
    pub alarm_id: Option<i64>,
    pub play_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSegmentQuery {
    pub device_tag: String,

    pub channel_tag: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub segment_type: Option<SegmentType>,
    pub limit: Option<usize>,
    pub page: Option<usize>,
}

impl RecordingSegmentQuery {
    pub fn page_limit(&self) -> usize {
        self.limit.unwrap_or(50).min(500)
    }
    pub fn page_offset(&self) -> usize {
        (self.page.unwrap_or(1).saturating_sub(1)) * self.page_limit()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingFileResponse {
    pub id: String,
    pub device_tag: String,
    pub channel_tag: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_secs: u64,
    pub file_path: String,
    pub server_tag: Option<String>,
    pub play_url: Option<String>,
    pub file_size: u64,
    pub segment_type: SegmentType,
    pub alarm_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedRecordingFiles {
    pub files: Vec<RecordingFileResponse>,
    pub total: i64,
    pub page: usize,
    pub limit: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayUrlResponse {
    pub id: String,
    pub play_url: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub token: Option<String>,
}
