use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common_types::Protocol;
use crate::domain::SessionType;
use crate::media::MediaNodeId;

pub type ProtocolType = Protocol;

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
    StreamCreated {
        device_id: i64,
        device_tag: String,
        channel_tag: String,
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
        device_tag: Option<String>,
        session_id: String,
        channel_id: Option<String>,
        transport: TransportType,
        media_server_name: Option<String>,
        session_type: SessionType,
    },
    StopPlay {
        device_tag: Option<String>,
        session_id: String,
        session_type: SessionType,
    },
    PlaybackResult {
        device_id: i64,
        device_tag: Option<String>,
        session_id: String,
        success: bool,
        error_code: Option<u16>,
        error_message: Option<String>,
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
        device_tag: Option<String>,
    },
    QueryDeviceStatus {
        device_id: i64,
        device_tag: Option<String>,
    },
    QueryDeviceConfig {
        device_id: i64,
        device_tag: Option<String>,
        config_type: String,
    },
    SetDeviceConfig {
        device_id: i64,
        device_tag: Option<String>,
        config_type: String,
        config_value: String,
    },
    PresetQuery {
        device_id: i64,
        device_tag: Option<String>,
        channel_id: String,
    },
    PresetSet {
        device_id: i64,
        device_tag: Option<String>,
        channel_id: String,
        preset_name: String,
    },
    PresetGoto {
        device_id: i64,
        device_tag: Option<String>,
        channel_id: String,
        preset_index: u32,
    },
    PresetRemove {
        device_id: i64,
        device_tag: Option<String>,
        channel_id: String,
        preset_index: u32,
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
    DeviceSubscribe {
        device_id: i64,
        device_tag: Option<String>,
        event_types: Vec<String>,
        expires: u32,
    },
    DeviceNotify {
        device_id: i64,
        device_tag: Option<String>,
        event_type: String,
        expires: u32,
    },
    CatalogNotify {
        device_id: i64,
        device_tag: Option<String>,
        event_type: String,
        channels: Vec<CatalogChannel>,
    },
    // 事件驱动的音频通话
    StartAudioTalk {
        node_id: MediaNodeId,
    },
    StopAudioTalk {
        node_id: MediaNodeId,
    },
    AudioTalkResponse {
        node_id: MediaNodeId,
        status: TalkResponseStatus,
        message: Option<String>,
    },
    AudioData {
        device_id: i64,
        device_tag: Option<String>,
        data: Vec<u8>,
        timestamp: u32,
    },
    InterleavedMediaData {
        device_id: i64,
        device_tag: Option<String>,
        channel: u8,
        ssrc: u32,
        is_rtp: bool,
        data: Vec<u8>,
    },
    AudioTalkReady {
        node_id: MediaNodeId,
    },
    AudioBroadcastRequest {
        device_id: i64,
        device_tag: Option<String>,
        channel_id: String,
    },
    AudioBroadcastAccept {
        device_id: i64,
        device_tag: Option<String>,
        channel_id: String,
        audio_port: u16,
    },
    AudioBroadcastReject {
        device_id: i64,
        device_tag: Option<String>,
        channel_id: String,
        message: Option<String>,
    },
    AudioBroadcastInvite {
        device_id: i64,
        device_tag: Option<String>,
        ssrc: u32,
        sdp: String,
    },
    AudioBroadcastReady {
        device_id: i64,
        device_tag: Option<String>,
        port: u16,
    },
    HookPlay {
        media_server_tag: String,
        stream_id: String,
        client_ip: Option<String>,
    },
    HookPublish {
        media_server_tag: String,
        stream_id: String,
        client_ip: Option<String>,
        device_tag: Option<String>,
        channel_tag: Option<String>,
    },
    HookUnpublish {
        media_server_tag: String,
        stream_id: String,
    },
    HookStreamUpdate {
        media_server_tag: String,
        stream_id: String,
        regist: bool,
        reader_count: Option<u32>,
        total_reader_count: Option<u32>,
        bytes_speed: Option<u64>,
    },
    HookStreamChanged {
        media_server_tag: String,
        app: String,
        stream_id: String,
        regist: bool,
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
            SignalEvent::StreamCreated { .. } => "stream_created".to_string(),
            SignalEvent::StreamRecover { .. } => "stream_recover".to_string(),
            SignalEvent::StreamRetriesExhausted { .. } => "stream_retries_exhausted".to_string(),
            SignalEvent::StreamRestart { .. } => "stream_restart".to_string(),
            SignalEvent::StartPlay { .. } => "start_play".to_string(),
            SignalEvent::StopPlay { .. } => "stop_play".to_string(),
            SignalEvent::PlaybackResult { .. } => "playback_result".to_string(),
            SignalEvent::PtzControl { .. } => "ptz_control".to_string(),
            SignalEvent::Alarm { .. } => "alarm".to_string(),
            SignalEvent::QueryDeviceInfo { .. } => "query_device_info".to_string(),
            SignalEvent::QueryDeviceStatus { .. } => "query_device_status".to_string(),
            SignalEvent::QueryDeviceConfig { .. } => "query_device_config".to_string(),
            SignalEvent::SetDeviceConfig { .. } => "set_device_config".to_string(),
            SignalEvent::PresetQuery { .. } => "preset_query".to_string(),
            SignalEvent::PresetSet { .. } => "preset_set".to_string(),
            SignalEvent::PresetGoto { .. } => "preset_goto".to_string(),
            SignalEvent::PresetRemove { .. } => "preset_remove".to_string(),
            SignalEvent::CatalogResponse { .. } => "catalog_response".to_string(),
            SignalEvent::MediaServerOnline { .. } => "media_server_online".to_string(),
            SignalEvent::MediaServerOffline { .. } => "media_server_offline".to_string(),
            SignalEvent::DeviceSubscribe { .. } => "device_subscribe".to_string(),
            SignalEvent::DeviceNotify { .. } => "device_notify".to_string(),
            SignalEvent::CatalogNotify { .. } => "catalog_notify".to_string(),
            SignalEvent::StartAudioTalk { .. } => "start_audio_talk".to_string(),
            SignalEvent::StopAudioTalk { .. } => "stop_audio_talk".to_string(),
            SignalEvent::AudioData { .. } => "audio_data".to_string(),
            SignalEvent::AudioTalkReady { .. } => "audio_talk_ready".to_string(),
            SignalEvent::AudioBroadcastRequest { .. } => "audio_broadcast_request".to_string(),
            SignalEvent::AudioBroadcastAccept { .. } => "audio_broadcast_accept".to_string(),
            SignalEvent::AudioBroadcastReject { .. } => "audio_broadcast_reject".to_string(),
            SignalEvent::AudioBroadcastInvite { .. } => "audio_broadcast_invite".to_string(),
            SignalEvent::AudioBroadcastReady { .. } => "audio_broadcast_ready".to_string(),
            SignalEvent::StartAudioTalk { .. } => "start_audio_talk".to_string(),
            SignalEvent::StopAudioTalk { .. } => "stop_audio_talk".to_string(),
            SignalEvent::AudioTalkResponse { .. } => "audio_talk_response".to_string(),
            SignalEvent::InterleavedMediaData { .. } => "interleaved_media_data".to_string(),
            SignalEvent::HookPlay { .. } => "hook_play".to_string(),
            SignalEvent::HookPublish { .. } => "hook_publish".to_string(),
            SignalEvent::HookUnpublish { .. } => "hook_unpublish".to_string(),
            SignalEvent::HookStreamUpdate { .. } => "hook_stream_update".to_string(),
            SignalEvent::HookStreamChanged { .. } => "hook_stream_changed".to_string(),
        }
    }

    pub fn device_tag(&self) -> Option<String> {
        match self {
            SignalEvent::DeviceRegister { device_tag, .. } => device_tag.clone(),
            SignalEvent::DeviceKeepalive { device_tag, .. } => device_tag.clone(),
            SignalEvent::DeviceOffline { device_tag, .. } => device_tag.clone(),
            SignalEvent::DeviceOnline { device_tag, .. } => device_tag.clone(),
            SignalEvent::StartPlay { device_tag, .. } => device_tag.clone(),
            SignalEvent::StopPlay { device_tag, .. } => device_tag.clone(),
            SignalEvent::PlaybackResult { device_tag, .. } => device_tag.clone(),
            SignalEvent::PtzControl { device_id, .. } => Some(device_id.clone()),
            SignalEvent::Alarm { device_id, .. } => Some(device_id.clone()),
            SignalEvent::QueryDeviceInfo { device_tag, .. } => device_tag.clone(),
            SignalEvent::QueryDeviceStatus { device_tag, .. } => device_tag.clone(),
            SignalEvent::QueryDeviceConfig { device_tag, .. } => device_tag.clone(),
            SignalEvent::SetDeviceConfig { device_tag, .. } => device_tag.clone(),
            SignalEvent::PresetQuery { device_tag, .. } => device_tag.clone(),
            SignalEvent::PresetSet { device_tag, .. } => device_tag.clone(),
            SignalEvent::PresetGoto { device_tag, .. } => device_tag.clone(),
            SignalEvent::PresetRemove { device_tag, .. } => device_tag.clone(),
            SignalEvent::CatalogResponse { device_tag, .. } => device_tag.clone(),
            SignalEvent::DeviceSubscribe { device_tag, .. } => device_tag.clone(),
            SignalEvent::DeviceNotify { device_tag, .. } => device_tag.clone(),
            SignalEvent::CatalogNotify { device_tag, .. } => device_tag.clone(),
            SignalEvent::StartAudioTalk { node_id, .. } => Some(node_id.device_tag.clone()),
            SignalEvent::StopAudioTalk { node_id, .. } => Some(node_id.device_tag.clone()),
            SignalEvent::AudioData { device_tag, .. } => device_tag.clone(),
            SignalEvent::AudioTalkReady { node_id, .. } => Some(node_id.device_tag.clone()),
            SignalEvent::AudioBroadcastRequest { device_tag, .. } => device_tag.clone(),
            SignalEvent::AudioBroadcastAccept { device_tag, .. } => device_tag.clone(),
            SignalEvent::AudioBroadcastReject { device_tag, .. } => device_tag.clone(),
            SignalEvent::AudioBroadcastInvite { device_tag, .. } => device_tag.clone(),
            SignalEvent::AudioBroadcastReady { device_tag, .. } => device_tag.clone(),
            SignalEvent::AudioTalkResponse { node_id, .. } => Some(node_id.device_tag.clone()),
            SignalEvent::InterleavedMediaData { device_tag, .. } => device_tag.clone(),
            _ => None,
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
    pub civil_code: Option<String>,
    pub address: Option<String>,
    pub ip_address: Option<String>,
    pub port: Option<u16>,
    pub owner: Option<String>,
    pub secrecy: Option<u8>,
    pub device_type: Option<String>,
    pub ptz_type: Option<u8>,
    pub info: Option<DeviceInfoBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfoBlock {
    pub device_type: Option<String>,
    pub protocol: Option<String>,
    pub ptz_type: Option<u8>,
    pub video_input_number: Option<u8>,
    pub audio_input_number: Option<u8>,
    pub alarm_output_number: Option<u8>,
}

impl CatalogChannel {
    pub fn is_online(&self) -> bool {
        self.status == "ON"
    }

    pub fn is_directory(&self) -> bool {
        self.parental
    }

    pub fn get_device_type_code(&self) -> Option<u32> {
        if self.device_id.len() >= 13 {
            self.device_id[8..12].parse().ok()
        } else {
            None
        }
    }

    pub fn is_voice_input_channel(&self) -> bool {
        self.get_device_type_code() == Some(136)
    }

    pub fn is_voice_output_channel(&self) -> bool {
        self.get_device_type_code() == Some(137)
    }

    pub fn is_audio_channel(&self) -> bool {
        match self.get_device_type_code() {
            Some(136) | Some(137) => true,
            _ => false,
        }
    }

    pub fn is_video_channel(&self) -> bool {
        !self.is_audio_channel()
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
    IrisIn,
    IrisOut,
    Stop,
    ContinuousMove { pan: f64, tilt: f64, zoom: f64 },
    AbsoluteMove { pan: f64, tilt: f64, zoom: f64 },
    RelativeMove { pan: f64, tilt: f64, zoom: f64 },
    GotoPreset { preset_token: String },
    SetPreset { preset_name: Option<String> },
    RemovePreset { preset_token: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TalkResponseStatus {
    Ready,
    Error,
    Inviting,
}

pub use crate::common_types::TransportType;
