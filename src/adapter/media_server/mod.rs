use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Default)]
pub enum RtpTransport {
    #[default]
    Udp,
    TcpPassive,
    TcpActive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub id: String,
    pub client_ip: String,
    pub stream: String,
    pub app: String,
    pub protocol: String,
    pub connected_at: i64,
    pub alive_seconds: u64,
}

pub mod zlmediakit;
pub mod srs;
pub mod xiu;
mod client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub name: String,
    pub server_type: String,
    pub online: bool,
    pub session_count: u32,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub bandwidth_in: u64,
    pub bandwidth_out: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat: Option<i64>,
}

impl fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}] - sessions: {}, CPU: {:.1}%, Mem: {:.1}%",
               self.name, self.server_type, self.session_count, self.cpu_usage, self.memory_usage)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub stream_key: String,
    pub play_url: String,
    pub rtsp_url: String,
    pub rtmp_url: String,
    pub hls_url: String,
    pub webrtc_url: String,
    #[serde(default)]
    pub flv_url: Option<String>,
    #[serde(default)]
    pub web_flv_url: Option<String>,
    pub media_server_id: String,
    pub media_server_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Rtsp,
    Rtmp,
    Hls,
    WebRTC,
    Flv,
    WsFlv,
}

impl Default for Protocol {
    fn default() -> Self { Protocol::Hls }
}

#[async_trait]
pub trait MediaServerAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn tag(&self) -> &str;
    fn server_type(&self) -> &str;
    async fn is_online(&self) -> bool;
    async fn get_status(&self) -> anyhow::Result<ServerStatus>;
    async fn add_stream_proxy(&self, stream_key: &str, app: &str, rtsp_url: &str) -> anyhow::Result<StreamInfo>;
    async fn remove_stream_proxy(&self, stream_key: &str, app: &str) -> anyhow::Result<()>;
    async fn get_play_url(&self, stream_key: &str, app: &str, protocol: Protocol) -> anyhow::Result<String>;
    async fn get_session_count(&self) -> anyhow::Result<u32>;
    async fn get_sessions(&self) -> anyhow::Result<Vec<serde_json::Value>>;
    async fn is_stream_online(&self, stream_key: &str, app: &str) -> anyhow::Result<bool>;
    async fn ptz_control(&self, stream_key: &str, command: &str, channel: u8) -> anyhow::Result<()>;
    async fn start_recording(&self, stream_key: &str, app: &str, format: &str, output_path: Option<&str>) -> anyhow::Result<RecordingInfo>;
    async fn stop_recording(&self, stream_key: &str, app: &str, format: &str) -> anyhow::Result<()>;
    async fn is_recording(&self, stream_key: &str, format: &str) -> anyhow::Result<bool>;
    async fn list_recordings(&self, stream_key: &str, app: &str) -> anyhow::Result<Vec<RecordingFile>>;
    async fn open_rtp_server(&self, stream_id: &str, port: u16, transport: RtpTransport) -> anyhow::Result<(u16, String)>;
    async fn close_rtp_server(&self, stream_id: &str) -> anyhow::Result<()>;
    async fn get_media_info(&self, stream_key: &str, app: &str) -> anyhow::Result<Option<serde_json::Value>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingInfo {
    pub stream_key: String,
    pub output_path: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingFile {
    pub filename: String,
    pub path: String,
    pub size: u64,
    pub duration_secs: u64,
    pub created_at: i64,
    pub stream_key: Option<String>,
    pub media_server_name: Option<String>,
}