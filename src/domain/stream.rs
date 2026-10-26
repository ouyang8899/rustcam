use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum StreamState {

    Idle,

    Starting,

    Recovering,

    Active,

    Stopping,

    Stopped,

    Error,
}

impl Default for StreamState {
    fn default() -> Self {
        StreamState::Idle
    }
}

impl std::fmt::Display for StreamState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamState::Idle => write!(f, "Idle"),
            StreamState::Starting => write!(f, "Starting"),
            StreamState::Recovering => write!(f, "Recovering"),
            StreamState::Active => write!(f, "Active"),
            StreamState::Stopping => write!(f, "Stopping"),
            StreamState::Stopped => write!(f, "Stopped"),
            StreamState::Error => write!(f, "Error"),
        }
    }
}

impl StreamState {

    pub fn is_alive(&self) -> bool {
        matches!(
            self,
            StreamState::Starting | StreamState::Recovering | StreamState::Active
        )
    }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stream {
    pub id: i64,
    pub device_tag: Option<String>,
    pub channel_tag: Option<String>,
    pub media_server_tag: String,
    pub app: String,
    pub token: String,
    pub state: StreamState,

    pub retry_count: u8,

    pub max_retries: u8,

    pub last_error: Option<String>,
    pub viewer_count: u32,
    pub bandwidth_in: u64,
    pub bandwidth_out: u64,
    pub last_keepalive_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl Stream {
    pub fn new(
        media_server_tag: String,
        device_tag: String,
        channel_tag: String,
        app: String,
        token: String,
    ) -> Self {
        Self {
            id: 0,
            device_tag: Some(device_tag),
            channel_tag: Some(channel_tag),
            media_server_tag,
            app,
            token,
            state: StreamState::Starting,
            retry_count: 0,
            max_retries: 20,
            last_error: None,
            viewer_count: 0,
            bandwidth_in: 0,
            bandwidth_out: 0,
            last_keepalive_at: Utc::now(),
            created_at: Utc::now(),
        }
    }

    pub fn start(&mut self) {
        self.state = StreamState::Active;
        self.retry_count = 0;
        self.last_error = None;
        self.last_keepalive_at = Utc::now();
    }

    pub fn start_recovering(&mut self) {
        self.state = StreamState::Recovering;
        self.last_keepalive_at = Utc::now();
    }

    pub fn stop(&mut self) {
        self.state = StreamState::Stopping;
    }

    pub fn stopped(&mut self) {
        self.state = StreamState::Stopped;
    }

    pub fn error(&mut self, reason: &str) {
        self.state = StreamState::Error;
        self.last_error = Some(reason.to_string());
    }

    pub fn should_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    pub fn increment_retry(&mut self) {
        self.retry_count = self.retry_count.saturating_add(1);
    }

    pub fn increment_viewers(&mut self) {
        self.viewer_count += 1;
    }

    pub fn decrement_viewers(&mut self) {
        if self.viewer_count > 0 {
            self.viewer_count -= 1;
        }
    }

    pub fn update_keepalive(&mut self) {
        self.last_keepalive_at = Utc::now();
    }

    pub fn update_bandwidth(&mut self, bytes_in: u64, bytes_out: u64) {
        self.bandwidth_in = bytes_in;
        self.bandwidth_out = bytes_out;
    }

    pub fn set_channel(&mut self, device_tag: String, channel_tag: String) {
        self.device_tag = Some(device_tag.clone());
        self.channel_tag = Some(channel_tag.clone());
    }
}
