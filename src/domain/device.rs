use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::domain::recording::RecordingFormat;
use crate::domain::server::ServerProtocolPorts;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {

    #[serde(
        default = "default_video_codec",
        skip_serializing_if = "Option::is_none"
    )]
    pub video_codec: Option<String>,

    #[serde(
        default = "default_audio_codec",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio_codec: Option<String>,

    #[serde(default = "default_video_pt", skip_serializing_if = "Option::is_none")]
    pub video_payload_type: Option<u8>,

    #[serde(default = "default_audio_pt", skip_serializing_if = "Option::is_none")]
    pub audio_payload_type: Option<u8>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_level_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packaging_mode: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprop_parameter_sets: Option<String>,

    #[serde(
        default = "default_stream_mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub stream_mode: Option<String>,
}

fn default_video_codec() -> Option<String> {
    Some("PS".to_string())
}
fn default_audio_codec() -> Option<String> {
    Some("PCMA".to_string())
}
fn default_video_pt() -> Option<u8> {
    Some(96)
}
fn default_audio_pt() -> Option<u8> {
    Some(8)
}
fn default_stream_mode() -> Option<String> {
    Some("recvonly".to_string())
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            video_codec: default_video_codec(),
            audio_codec: default_audio_codec(),
            video_payload_type: default_video_pt(),
            audio_payload_type: default_audio_pt(),
            profile_level_id: Some("4D001F".to_string()),
            packaging_mode: Some("HIS".to_string()),
            sprop_parameter_sets: None,
            stream_mode: Some(default_stream_mode().unwrap()),
        }
    }
}

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

pub use crate::common_types::Protocol;

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

/// Fallback position for devices that have no coordinates configured.
/// The map uses it so that every device has a point on the map.
pub const DEFAULT_MAP_LATITUDE: f64 = 31.2304;
pub const DEFAULT_MAP_LONGITUDE: f64 = 121.4737;

/// Deterministic fallback position around the Shanghai center.
///
/// Devices that have no coordinates are spread over a small area instead of
/// being stacked on a single point, so that they can be told apart on the
/// map and can be clustered by zoom level. The position only depends on the
/// key, therefore it stays stable between restarts.
pub fn fallback_map_position(key: &str) -> (f64, f64) {
    // FNV-1a
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }

    // Golden angle spiral: an even spread without overlaps.
    let index = (hash % 512) as f64;
    let angle = index * 2.399_963_229_728_653;
    let radius = 0.0025 * (1.0 + index).sqrt();

    (
        DEFAULT_MAP_LATITUDE + radius * angle.sin(),
        DEFAULT_MAP_LONGITUDE + radius * angle.cos(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DeviceType {
    NVR,
    IPC,
    DVR,
    Encoder,
    VideoServer,
    Camera,
    Platform,
    Other,
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceType::NVR => write!(f, "NVR"),
            DeviceType::IPC => write!(f, "IPC"),
            DeviceType::DVR => write!(f, "DVR"),
            DeviceType::Encoder => write!(f, "Encoder"),
            DeviceType::VideoServer => write!(f, "VideoServer"),
            DeviceType::Camera => write!(f, "Camera"),
            DeviceType::Platform => write!(f, "Platform"),
            DeviceType::Other => write!(f, "Other"),
        }
    }
}

impl Default for DeviceType {
    fn default() -> Self {
        DeviceType::Other
    }
}

impl DeviceType {
    pub fn from_gb28181_code(code: &str) -> Self {
        if code.len() < 13 {
            return DeviceType::Other;
        }
        let type_code = &code[10..13];
        match type_code {
            "111" => DeviceType::DVR,
            "112" => DeviceType::VideoServer,
            "113" => DeviceType::Encoder,
            "118" => DeviceType::NVR,
            "130" => DeviceType::DVR,
            "131" => DeviceType::Camera,
            "132" => DeviceType::IPC,
            _ => {
                if let Ok(num) = type_code.parse::<u32>() {
                    if (200..=216).contains(&num) {
                        return DeviceType::Platform;
                    }
                }
                DeviceType::Other
            }
        }
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
    pub app: Option<String>,

    pub device_tag: Option<String>,
    pub parent_device_tag: Option<String>,

    pub region_code: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

    pub group_id: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended: Option<serde_json::Value>,

    #[serde(skip_serializing, default)]
    pub is_online: bool,

    #[serde(default)]
    pub has_stream: bool,

    #[serde(default)]
    pub device_type: DeviceType,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type_code: Option<String>,

    #[serde(default)]
    pub channel_count: i32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_config: Option<StreamConfig>,
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
            app: None,
            device_tag: None,
            parent_device_tag: None,
            region_code: None,
            latitude: None,
            longitude: None,
            group_id: None,
            extended: None,
            is_online: false,
            has_stream: false,
            device_type: DeviceType::Other,
            device_type_code: None,
            channel_count: 0,
            stream_config: None,
        }
    }

    pub fn set_online(&mut self) {
        // 维护模式是运维设置：设备重新注册/上线不能把它覆盖掉，
        // 否则维护中的设备会被自动拉流、告警等流程重新接管。
        if self.status != DeviceStatus::Maintaining {
            self.status = DeviceStatus::Online;
        }
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
    pub app: Option<String>,
    pub device_tag: Option<String>,
    pub parent_device_tag: Option<String>,
    pub region_code: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub is_public: Option<bool>,
    pub extended: Option<serde_json::Value>,
    #[serde(default)]
    pub device_type: Option<DeviceType>,
    pub device_type_code: Option<String>,
    #[serde(default)]
    pub rtsp_url: Option<String>,
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
            app: None,
            device_tag: None,
            parent_device_tag: None,
            region_code: None,
            latitude: None,
            longitude: None,
            is_public: Some(false),
            extended: None,
            device_type: None,
            device_type_code: None,
            rtsp_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UpdateDeviceRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub protocol: Option<Protocol>,
    pub device_username: Option<String>,
    pub device_password: Option<String>,
    pub push_urls: Option<Vec<PushUrl>>,
    pub pull_urls: Option<Vec<PullUrl>>,
    pub playback_username: Option<String>,
    pub playback_password: Option<String>,
    pub media_server_tag: Option<String>,
    pub app: Option<String>,
    pub device_tag: Option<String>,
    pub parent_device_tag: Option<String>,
    pub region_code: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub group_id: Option<i64>,
    pub is_public: Option<bool>,
    pub extended: Option<serde_json::Value>,
    pub status: Option<DeviceStatus>,
    pub device_type: Option<DeviceType>,
    pub device_type_code: Option<String>,
    pub rtsp_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceWithChildren {
    #[serde(flatten)]
    pub device: Device,
    pub children: Vec<Device>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_online_keeps_the_maintaining_status() {
        let mut device = Device::new("Test".to_string(), Protocol::Rtsp);
        device.status = DeviceStatus::Maintaining;

        device.set_online();

        assert_eq!(
            device.status,
            DeviceStatus::Maintaining,
            "维护模式是运维设置，设备上线不能覆盖它"
        );
        assert!(device.is_online, "设备本身仍然是可达的");
    }

    #[test]
    fn test_set_online_restores_the_online_status() {
        let mut device = Device::new("Test".to_string(), Protocol::Rtsp);
        device.set_offline();

        device.set_online();

        assert_eq!(device.status, DeviceStatus::Online);
        assert!(device.is_online);
    }
}
