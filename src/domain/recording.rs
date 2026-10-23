use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RecordingState {
    /// 等待设备/流就绪，策略已经生效但暂时还不能录
    Pending,
    Starting,
    Recording,
    Paused,
    Stopping,
    Completed,
    Error,
}

impl Default for RecordingState {
    fn default() -> Self {
        RecordingState::Starting
    }
}

impl fmt::Display for RecordingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordingState::Pending => write!(f, "pending"),
            RecordingState::Starting => write!(f, "starting"),
            RecordingState::Recording => write!(f, "recording"),
            RecordingState::Paused => write!(f, "paused"),
            RecordingState::Stopping => write!(f, "stopping"),
            RecordingState::Completed => write!(f, "completed"),
            RecordingState::Error => write!(f, "error"),
        }
    }
}

/// 录像启动失败的原因分类，决定后续是等待重试还是直接放弃。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartFailureKind {
    /// 流还没有在媒体服务器上就绪，等流就绪后即可开始
    StreamNotReady,
    /// 媒体服务器暂时不可用（超时、网络错误等），退避后重试
    MediaServerUnavailable,
    /// 参数、格式、权限等不可重试的问题
    Permanent,
}

/// 根据媒体服务器返回的错误信息判断失败类型。
pub fn classify_start_failure(message: &str) -> StartFailureKind {
    let m = message.to_lowercase();
    if m.contains("can not find the stream")
        || m.contains("cannot find the stream")
        || m.contains("not find the stream")
        || m.contains("no such stream")
    {
        return StartFailureKind::StreamNotReady;
    }
    if m.contains("timeout")
        || m.contains("timed out")
        || m.contains("connection")
        || m.contains("connect")
        || m.contains("broken pipe")
        || m.contains("refused")
    {
        return StartFailureKind::MediaServerUnavailable;
    }
    if m.contains("not support")
        || m.contains("invalid")
        || m.contains("permission")
        || m.contains("denied")
    {
        return StartFailureKind::Permanent;
    }
    // 未知错误按可重试处理，避免因为错误分类不准而漏录
    StartFailureKind::MediaServerUnavailable
}

/// 录像启动失败后的重试间隔（秒）。
/// 流未就绪时用较短的间隔（流随时可能就绪，且有事件触发兜底），
/// 其余情况随失败次数指数退避并封顶。
pub fn retry_backoff_secs(kind: StartFailureKind, retry_count: i32) -> i64 {
    if kind == StartFailureKind::StreamNotReady {
        return 15;
    }
    match retry_count {
        i32::MIN..=1 => 30,
        2 => 60,
        3 => 120,
        4 => 300,
        _ => 900,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RecordingFormat {
    Hls,
    Mp4,
    Flv,
    Ts,
}

impl fmt::Display for RecordingFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordingFormat::Hls => write!(f, "HLS"),
            RecordingFormat::Mp4 => write!(f, "MP4"),
            RecordingFormat::Flv => write!(f, "FLV"),
            RecordingFormat::Ts => write!(f, "TS"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: i64,
    pub device_tag: Option<String>,
    pub channel_tag: Option<String>,
    pub server_tag: String,
    pub state: RecordingState,
    pub format: RecordingFormat,
    pub output_path: Option<String>,
    pub file_size: u64,
    pub duration_secs: u64,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub labels: Vec<String>,
    pub filename: Option<String>,
    pub pre_record_secs: Option<u32>,

    pub max_duration_secs: Option<u32>,
    /// 启动失败的重试次数
    pub retry_count: i32,
    /// 下一次允许尝试启动的时间
    pub next_retry_at: Option<DateTime<Utc>>,
}

impl Recording {
    pub fn new(
        device_tag: String,
        channel_tag: String,
        server_tag: String,
        format: RecordingFormat,
    ) -> Self {
        Self {
            id: 0,
            device_tag: Some(device_tag),
            channel_tag: Some(channel_tag),
            server_tag,
            state: RecordingState::Starting,
            format,
            output_path: None,
            file_size: 0,
            duration_secs: 0,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
            error_message: None,
            labels: Vec::new(),
            filename: None,
            pre_record_secs: None,
            max_duration_secs: None,
            retry_count: 0,
            next_retry_at: None,
        }
    }

    pub fn stream_key(&self) -> String {
        let device = self.device_tag.as_deref().unwrap_or("");
        let channel = self.channel_tag.as_deref().unwrap_or("");
        crate::domain::stream::make_stream_key(device, channel)
    }

    pub fn start(&mut self) {
        self.state = RecordingState::Recording;
        self.started_at = Some(Utc::now());
    }

    pub fn pause(&mut self) {
        if self.state == RecordingState::Recording {
            self.state = RecordingState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.state == RecordingState::Paused {
            self.state = RecordingState::Recording;
        }
    }

    pub fn stop(&mut self) {
        self.state = RecordingState::Stopping;
        self.stopped_at = Some(Utc::now());
    }

    pub fn complete(&mut self) {
        self.state = RecordingState::Completed;
        self.stopped_at = Some(Utc::now());
        if let (Some(start), Some(stop)) = (self.started_at, self.stopped_at) {
            self.duration_secs = (stop - start).num_seconds() as u64;
        }
    }

    pub fn set_error(&mut self, message: String) {
        self.state = RecordingState::Error;
        self.error_message = Some(message);
    }

    /// 标记为“等待设备/流就绪”，并安排下一次重试。
    pub fn set_pending(&mut self, message: String, next_retry_at: DateTime<Utc>) {
        self.state = RecordingState::Pending;
        self.error_message = Some(message);
        self.retry_count += 1;
        self.next_retry_at = Some(next_retry_at);
    }

    pub fn is_pending(&self) -> bool {
        self.state == RecordingState::Pending
    }

    /// 是否已经到了允许重试的时间点。
    pub fn can_retry_at(&self, now: DateTime<Utc>) -> bool {
        match self.next_retry_at {
            Some(at) => now >= at,
            None => true,
        }
    }

    pub fn set_output(&mut self, path: String, file_size: u64) {
        self.output_path = Some(path);
        self.file_size = file_size;
    }

    pub fn file_size_mb(&self) -> f64 {
        self.file_size as f64 / (1024.0 * 1024.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecordingRequest {
    pub device_tag: String,
    pub channel_tag: String,
    pub format: Option<RecordingFormat>,
    pub duration_secs: Option<u32>,
    pub max_file_size_mb: Option<u32>,
    pub output_path: Option<String>,
    pub labels: Option<Vec<String>>,
    pub pre_record_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingListQuery {
    pub device_tag: Option<String>,
    pub state: Option<RecordingState>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording() -> Recording {
        Recording::new(
            "12010100001320000011".to_string(),
            "12010100001320000011".to_string(),
            "default".to_string(),
            RecordingFormat::Mp4,
        )
    }

    #[test]
    fn test_pending_state_is_serialized_as_pending() {
        assert_eq!(RecordingState::Pending.to_string(), "pending");
        assert_eq!(RecordingState::Pending, RecordingState::Pending);
    }

    #[test]
    fn test_set_pending_records_the_reason_and_schedules_the_retry() {
        let mut r = recording();
        let at = Utc::now() + chrono::TimeDelta::seconds(30);

        r.set_pending("等待设备上线".to_string(), at);

        assert_eq!(r.state, RecordingState::Pending);
        assert!(r.is_pending());
        assert_eq!(r.error_message.as_deref(), Some("等待设备上线"));
        assert_eq!(r.retry_count, 1);
        assert_eq!(r.next_retry_at, Some(at));
    }

    #[test]
    fn test_can_retry_respects_the_scheduled_time() {
        let mut r = recording();
        assert!(r.can_retry_at(Utc::now()), "没有安排重试时间时应立即可重试");

        r.next_retry_at = Some(Utc::now() + chrono::TimeDelta::seconds(60));
        assert!(!r.can_retry_at(Utc::now()));
        assert!(r.can_retry_at(Utc::now() + chrono::TimeDelta::seconds(61)));
    }

    #[test]
    fn test_classify_start_failure() {
        assert_eq!(
            classify_start_failure("ZLMediaKit startRecord failed: can not find the stream"),
            StartFailureKind::StreamNotReady
        );
        assert_eq!(
            classify_start_failure("GET request timeout after 4s"),
            StartFailureKind::MediaServerUnavailable
        );
        assert_eq!(
            classify_start_failure("format not support"),
            StartFailureKind::Permanent
        );
        assert_eq!(
            classify_start_failure("something else entirely"),
            StartFailureKind::MediaServerUnavailable,
            "未知错误按可重试处理，避免漏录"
        );
    }

    #[test]
    fn test_retry_backoff_grows_and_is_capped() {
        assert_eq!(retry_backoff_secs(StartFailureKind::StreamNotReady, 9), 15);
        assert_eq!(retry_backoff_secs(StartFailureKind::MediaServerUnavailable, 1), 30);
        assert_eq!(retry_backoff_secs(StartFailureKind::MediaServerUnavailable, 2), 60);
        assert_eq!(retry_backoff_secs(StartFailureKind::MediaServerUnavailable, 3), 120);
        assert_eq!(retry_backoff_secs(StartFailureKind::MediaServerUnavailable, 4), 300);
        assert_eq!(retry_backoff_secs(StartFailureKind::MediaServerUnavailable, 50), 900);
    }
}
