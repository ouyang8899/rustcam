use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: SignalEvent);
}
pub struct FnEventHandler<F> {
    f: F,  // 存储闭包
}
impl<F, Fut> FnEventHandler<F>
where
    F: Fn(SignalEvent) -> Fut + Send + Sync,  // F 是接收事件、返回 Future 的闭包
    Fut: std::future::Future<Output = ()> + Send,  // Future 必须可 Send
{
    pub fn new(f: F) -> Self {
        Self { f }
    }
}
#[async_trait]
impl<F, Fut> EventHandler for FnEventHandler<F>
where
    F: Fn(SignalEvent) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = ()> + Send,
{
    async fn handle(&self, event: SignalEvent) {
        (self.f)(event).await;
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SignalEvent {
    DeviceRegister {
        device_id: i64,
        device_tag: Option<String>,
        name: String,
        stream_key: Option<String>,
        manufacturer: Option<String>,
        model: Option<String>,
        protocol: ProtocolType,
    },
    DeviceKeepalive {
        device_id: i64,
        device_tag: Option<String>,
        timestamp: DateTime<Utc>,
    },
    DeviceOffline {
        device_id: i64,
        device_tag: Option<String>,
        reason: Option<String>,
    },
    DeviceOnline {
        device_id: i64,
        device_tag: Option<String>,
    },
    StreamStart {
        device_id: i64,
        stream_key: String,
    },
    StreamStop {
        device_id: i64,
        stream_key: String,
        reason: Option<String>,
    },
    StreamRecover {
        device_id: i64,
        stream_key: String,
    },
    StreamRetriesExhausted {
        device_id: i64,
        stream_key: String,
        retry_count: u8,
        last_error: Option<String>,
    },
    StreamRestart {
        device_id: i64,
        stream_key: String,
    },
    StartPlay {
        device_id: i64,
        device_tag: Option<String>,
        session_id: String,
        channel_id: Option<String>,
        transport: TransportType,
        media_server_name: Option<String>,
    },
    StopPlay {
        device_id: i64,
        device_tag: Option<String>,
        session_id: String,
    },
    PtzControl {
        device_id: String,
        command: PtzCommand,
        speed: Option<u8>,
    },
    Alarm {
        device_id: String,
        alarm_type: String,
        message: String,
        timestamp: DateTime<Utc>,
    },
    QueryDeviceInfo {
        device_id: i64,
        device_tag: Option<String>,
    },
    QueryDeviceStatus {
        device_id: i64,
        device_tag: Option<String>,
    },
    CatalogResponse {
        device_id: i64,
        device_tag: Option<String>,
        channels: Vec<CatalogChannel>,
    },
    MediaServerOnline {
        media_server_tag: String,
    },
    MediaServerOffline {
        media_server_tag: String,
        reason: Option<String>,
    },
}
impl SignalEvent {
    /// 获取事件类型字符串
    pub fn event_type(&self) -> String {
        match self {
            SignalEvent::DeviceRegister { .. } => "device_register".to_string(),
            SignalEvent::DeviceKeepalive { .. } => "device_keepalive".to_string(),
            SignalEvent::DeviceOffline { .. } => "device_offline".to_string(),
            SignalEvent::DeviceOnline { .. } => "device_online".to_string(),
            SignalEvent::StreamStart { .. } => "stream_start".to_string(),
            SignalEvent::StreamStop { .. } => "stream_stop".to_string(),
            SignalEvent::StreamRecover { .. } => "stream_recover".to_string(),
            SignalEvent::StreamRetriesExhausted { .. } => "stream_retries_exhausted".to_string(),
            SignalEvent::StreamRestart { .. } => "stream_restart".to_string(),
            SignalEvent::StartPlay { .. } => "start_play".to_string(),
            SignalEvent::StopPlay { .. } => "stop_play".to_string(),
            SignalEvent::PtzControl { .. } => "ptz_control".to_string(),
            SignalEvent::Alarm { .. } => "alarm".to_string(),
            SignalEvent::QueryDeviceInfo { .. } => "query_device_info".to_string(),
            SignalEvent::QueryDeviceStatus { .. } => "query_device_status".to_string(),
            SignalEvent::CatalogResponse { .. } => "catalog_response".to_string(),
            SignalEvent::MediaServerOnline { .. } => "media_server_online".to_string(),
            SignalEvent::MediaServerOffline { .. } => "media_server_offline".to_string(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogChannel {
    pub device_id: String,
    pub name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub status: String,
    pub parental: bool,
    pub parent_id: Option<String>,
}

impl CatalogChannel {
    pub fn is_online(&self) -> bool {
        self.status == "ON"
    }

    pub fn is_directory(&self) -> bool {
        self.parental
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProtocolType {
    Gb28181,
    Onvif,
    Rtsp,
    WebRtc,
    Custom(String),
}

impl std::fmt::Display for ProtocolType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolType::Gb28181 => write!(f, "GB28181"),
            ProtocolType::Onvif => write!(f, "ONVIF"),
            ProtocolType::Rtsp => write!(f, "RTSP"),
            ProtocolType::WebRtc => write!(f, "WebRTC"),
            ProtocolType::Custom(name) => write!(f, "{}", name),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PtzCommand {
    Up,
    Down,
    Left,
    Right,
    ZoomIn,
    ZoomOut,
    FocusIn,
    FocusOut,
    Stop,
    ContinuousMove { pan: f64, tilt: f64, zoom: f64 },
    AbsoluteMove { pan: f64, tilt: f64, zoom: f64 },
    RelativeMove { pan: f64, tilt: f64, zoom: f64 },
    GotoPreset { preset_token: String },
    SetPreset { preset_name: Option<String> },
    RemovePreset { preset_token: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportType {
    UDP,
    TCP,
    HTTP,
    WebSocket,
}
