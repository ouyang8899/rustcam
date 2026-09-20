use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::device::{DeviceStatus, DeviceType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: i64,

    pub device_tag: String,

    pub channel_tag: String,

    pub name: String,
    pub status: DeviceStatus,

    pub device_type: DeviceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type_code: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_extended: Option<serde_json::Value>,

    #[serde(default)]
    pub is_default: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_channel_tag: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub civil_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(default)]
    pub parental: i32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended: Option<serde_json::Value>,

    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl Channel {
    pub fn new(device_tag: String, channel_tag: String, name: String) -> Self {
        Self {
            id: 0,
            device_tag,
            channel_tag,
            name,
            status: DeviceStatus::Offline,
            device_type: DeviceType::Other,
            device_type_code: None,
            channel_extended: None,
            is_default: false,
            parent_channel_tag: None,
            civil_code: None,
            address: None,
            ip_address: None,
            port: 0,
            manufacturer: None,
            model: None,
            parental: 0,
            extended: None,
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    pub fn is_online(&self) -> bool {
        self.status == DeviceStatus::Online
    }

    pub fn is_directory(&self) -> bool {
        self.parental > 0
    }

    pub fn get_device_type_code(&self) -> Option<u32> {
        if self.channel_tag.len() >= 13 {
            self.channel_tag[10..13].parse().ok()
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
        self.is_voice_input_channel() || self.is_voice_output_channel()
    }

    pub fn is_ipc(&self) -> bool {
        self.device_type == DeviceType::IPC || self.device_type_code.as_deref() == Some("132")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelWithDevice {
    #[serde(flatten)]
    pub channel: Channel,
    pub device_name: Option<String>,
    pub device_status: Option<DeviceStatus>,
}
