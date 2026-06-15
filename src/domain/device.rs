use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::domain::recording::RecordingFormat;
use crate::domain::server::ServerProtocolPorts;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub format: Option<RecordingFormat>,
    #[serde(default)]
    pub max_duration_secs: Option<u32>,
    #[serde(default)]
    pub max_file_size_mb: Option<u32>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            format: None,
            max_duration_secs: None,
            max_file_size_mb: None,
            labels: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Protocol {
    Gb28181,
    Onvif,
    Rtsp,
    Rtmp,
    Hls,
    WebRTC,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Gb28181 => write!(f, "GB28181"),
            Protocol::Onvif => write!(f, "ONVIF"),
            Protocol::Rtsp => write!(f, "RTSP"),
            Protocol::Rtmp => write!(f, "RTMP"),
            Protocol::Hls => write!(f, "HLS"),
            Protocol::WebRTC => write!(f, "WebRTC"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DeviceStatus {
    Offline,
    Online,
    Error,
    Maintaining,
}

impl fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceStatus::Offline => write!(f, "offline"),
            DeviceStatus::Online => write!(f, "online"),
            DeviceStatus::Error => write!(f, "error"),
            DeviceStatus::Maintaining => write!(f, "maintaining"),
        }
    }
}

impl Default for DeviceStatus {
    fn default() -> Self {
        DeviceStatus::Offline
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamProtocol {
    Rtsp,
    Rtmp,
    Gb28181,
    HttpFlv,
}

impl fmt::Display for StreamProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamProtocol::Rtsp => write!(f, "RTSP"),
            StreamProtocol::Rtmp => write!(f, "RTMP"),
            StreamProtocol::Gb28181 => write!(f, "GB28181"),
            StreamProtocol::HttpFlv => write!(f, "HTTP-FLV"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushUrl {
    pub protocol: StreamProtocol,
    pub url: String,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullUrl {
    pub protocol: StreamProtocol,
    pub url: String,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: i64,
    pub name: String,
    pub protocol: Protocol,
    pub status: DeviceStatus,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,

    pub host: String,
    pub port: u16,

    pub device_username: Option<String>,
    pub device_password: Option<String>,

    pub push_urls: Vec<PushUrl>,
    pub pull_urls: Vec<PullUrl>,

    pub playback_username: Option<String>,
    pub playback_password: Option<String>,

    pub media_server_tag: Option<String>,
    pub stream_key: Option<String>,
    pub app: Option<String>,

    pub device_tag: Option<String>,
    pub parent_device_tag: Option<String>,

    pub region_code: Option<String>,

    pub group_id: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended: Option<serde_json::Value>,

    #[serde(skip_serializing, default)]
    pub is_online: bool,

    #[serde(default)]
    pub has_stream: bool,
}

impl Device {
    pub fn new(name: String, protocol: Protocol) -> Self {
        Self {
            id: 0,
            name,
            protocol,
            status: DeviceStatus::Offline,
            is_public: false,
            created_at: Utc::now(),
            host: String::new(),
            port: 0,
            device_username: None,
            device_password: None,
            push_urls: Vec::new(),
            pull_urls: Vec::new(),
            playback_username: None,
            playback_password: None,
            media_server_tag: None,
            stream_key: None,
            app: None,
            device_tag: None,
            parent_device_tag: None,
            region_code: None,
            group_id: None,
            extended: None,
            is_online: false,
            has_stream: false,
        }
    }

    pub fn set_online(&mut self) {
        self.status = DeviceStatus::Online;
        self.is_online = true;
    }

    pub fn set_offline(&mut self) {
        self.status = DeviceStatus::Offline;
        self.is_online = false;
    }

    pub fn select_source(&self) -> Option<(StreamProtocol, String)> {
        self.pull_urls
            .iter()
            .min_by_key(|p| p.priority)
            .map(|p| (p.protocol, p.url.clone()))
            .or_else(|| {
                self.push_urls
                    .iter()
                    .min_by_key(|p| p.priority)
                    .map(|p| (p.protocol, p.url.clone()))
            })
            .or_else(|| {
                self.extended
                    .as_ref()
                    .and_then(|e| e.get("rtsp_full_url"))
                    .and_then(|v| v.as_str())
                    .filter(|url| !url.is_empty())
                    .map(|url| (StreamProtocol::Rtsp, url.to_string()))
            })
    }

    pub fn is_push_mode(&self) -> bool {
        !self.push_urls.is_empty()
    }

    pub fn is_pull_mode(&self) -> bool {
        self.push_urls.is_empty() && !self.pull_urls.is_empty()
    }

    pub fn recording_config(&self) -> RecordingConfig {
        self.extended
            .as_ref()
            .and_then(|e| serde_json::from_value(e.clone()).ok())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayLinks {
    pub token: String,
    pub stream_id: String,
    pub expires_at: i64,
    pub ports: ServerProtocolPorts,
    pub rtsp_signaling: Option<String>,
    pub rtsp_media: Option<String>,
    pub flv: Option<String>,
    pub hls: Option<String>,
    pub webrtc: Option<String>,
    pub web_flv: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CreateDeviceRequest {
    pub name: String,
    pub protocol: Protocol,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    pub device_username: Option<String>,
    pub device_password: Option<String>,
    pub push_urls: Option<Vec<PushUrl>>,
    pub pull_urls: Option<Vec<PullUrl>>,
    pub playback_username: Option<String>,
    pub playback_password: Option<String>,
    pub media_server_tag: Option<String>,
    pub stream_key: Option<String>,
    pub app: Option<String>,
    pub device_tag: Option<String>,
    pub parent_device_tag: Option<String>,
    pub region_code: Option<String>,
    pub is_public: Option<bool>,
    pub extended: Option<serde_json::Value>,
}

impl Default for CreateDeviceRequest {
    fn default() -> Self {
        Self {
            name: String::new(),
            protocol: Protocol::Rtsp,
            host: None,
            port: None,
            device_username: None,
            device_password: None,
            push_urls: None,
            pull_urls: None,
            playback_username: None,
            playback_password: None,
            media_server_tag: None,
            stream_key: None,
            app: None,
            device_tag: None,
            parent_device_tag: None,
            region_code: None,
            is_public: Some(false),
            extended: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UpdateDeviceRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub device_username: Option<String>,
    pub device_password: Option<String>,
    pub push_urls: Option<Vec<PushUrl>>,
    pub pull_urls: Option<Vec<PullUrl>>,
    pub playback_username: Option<String>,
    pub playback_password: Option<String>,
    pub media_server_tag: Option<String>,
    pub stream_key: Option<String>,
    pub app: Option<String>,
    pub device_tag: Option<String>,
    pub parent_device_tag: Option<String>,
    pub region_code: Option<String>,
    pub group_id: Option<i64>,
    pub is_public: Option<bool>,
    pub extended: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceWithChildren {
    #[serde(flatten)]
    pub device: Device,
    pub children: Vec<DeviceWithChildren>,
}
