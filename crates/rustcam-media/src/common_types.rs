use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum TransportType {
    Udp,
    Tcp,
    Http,
    WebSocket,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::Udp => write!(f, "UDP"),
            TransportType::Tcp => write!(f, "TCP"),
            TransportType::Http => write!(f, "HTTP"),
            TransportType::WebSocket => write!(f, "WebSocket"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Protocol {
    Gb28181,
    Onvif,
    Rtsp,
    Rtmp,
    Hls,
    WebRTC,
    Custom(String),
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Gb28181 => write!(f, "GB28181"),
            Protocol::Onvif => write!(f, "ONVIF"),
            Protocol::Rtsp => write!(f, "RTSP"),
            Protocol::Rtmp => write!(f, "RTMP"),
            Protocol::Hls => write!(f, "HLS"),
            Protocol::WebRTC => write!(f, "WebRTC"),
            Protocol::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

impl Protocol {
    /// API 对外暴露的形式（与 serde 的 PascalCase 表示一致）。
    ///
    /// 与 `Display` 不同：`Display` 面向数据库列（`GB28181`），
    /// `api_name` 面向 JSON 响应（`Gb28181`），与 `GET /devices` 保持一致。
    pub fn api_name(&self) -> String {
        match self {
            Protocol::Gb28181 => "Gb28181".to_string(),
            Protocol::Onvif => "Onvif".to_string(),
            Protocol::Rtsp => "Rtsp".to_string(),
            Protocol::Rtmp => "Rtmp".to_string(),
            Protocol::Hls => "Hls".to_string(),
            Protocol::WebRTC => "WebRTC".to_string(),
            Protocol::Custom(name) => name.clone(),
        }
    }
}
