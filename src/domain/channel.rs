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
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,

    #[serde(default)]
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
            latitude: None,
            longitude: None,
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

/// serde 的双重 Option 惯用法：区分"字段缺省"与"显式 null"。
///
/// serde_json 默认把 `Option<Option<T>>` 的 `null` 解析成外层 `None`，
/// 与"字段缺省"完全一样，三态语义会塌缩成两态。
/// `deserialize_with` 只在字段**存在**时被调用（哪怕值是 null），
/// 再把解析结果包一层 `Some`，三种状态才得以区分：
/// 缺省 → `None`，`null` → `Some(None)`，数值 → `Some(Some(v))`。
fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    <Option<T> as Deserialize>::deserialize(deserializer).map(Some)
}

/// 更新通道的请求体。
///
/// 经纬度用双重 Option 表达三态：字段缺省 = 不修改，`null` = 清空，数值 = 设值。
/// `status` 与 `device_tag` / `channel_tag` 不在这里——前者由协议层维护，
/// 后两者是通道的自然键，不可修改。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub civil_code: Option<String>,
    pub address: Option<String>,
    pub ip_address: Option<String>,
    pub port: Option<u16>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub latitude: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub longitude: Option<Option<f64>>,
}

impl UpdateChannelRequest {
    /// 把描述性字段套用到通道上，不触碰经纬度。
    pub fn apply_to(&self, channel: &mut Channel) {
        if let Some(v) = &self.name {
            channel.name = v.clone();
        }
        if let Some(v) = &self.civil_code {
            channel.civil_code = Some(v.clone());
        }
        if let Some(v) = &self.address {
            channel.address = Some(v.clone());
        }
        if let Some(v) = &self.ip_address {
            channel.ip_address = Some(v.clone());
        }
        if let Some(v) = self.port {
            channel.port = v;
        }
        if let Some(v) = &self.manufacturer {
            channel.manufacturer = Some(v.clone());
        }
        if let Some(v) = &self.model {
            channel.model = Some(v.clone());
        }
    }

    /// 计算生效坐标。
    ///
    /// 返回 `None` 表示请求完全没涉及坐标，调用方不应写库。
    /// 返回 `Some((lat, lng))` 时两个维度都是最终要落库的值：
    /// 请求里出现过的维度取请求值（内层 `None` 表示清空），
    /// 没出现过的维度沿用当前值——因为落库语句会无条件写两列，
    /// 直接传 `NULL` 会误清空用户没打算改的那一维。
    pub fn effective_position(
        &self,
        current_latitude: Option<f64>,
        current_longitude: Option<f64>,
    ) -> Option<(Option<f64>, Option<f64>)> {
        if self.latitude.is_none() && self.longitude.is_none() {
            return None;
        }
        let latitude = match self.latitude {
            Some(v) => v,
            None => current_latitude,
        };
        let longitude = match self.longitude {
            Some(v) => v,
            None => current_longitude,
        };
        Some((latitude, longitude))
    }
}

/// 手工创建通道的请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    pub device_tag: String,
    pub channel_tag: String,
    pub name: String,
    pub civil_code: Option<String>,
    pub address: Option<String>,
    pub ip_address: Option<String>,
    pub port: Option<u16>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_request_distinguishes_absent_null_and_value() {
        let absent: UpdateChannelRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(absent.latitude, None);
        assert_eq!(absent.longitude, None);

        let cleared: UpdateChannelRequest =
            serde_json::from_str(r#"{"latitude": null, "longitude": null}"#).unwrap();
        assert_eq!(cleared.latitude, Some(None));
        assert_eq!(cleared.longitude, Some(None));

        let set: UpdateChannelRequest =
            serde_json::from_str(r#"{"latitude": 31.2304, "longitude": 121.4737}"#).unwrap();
        assert_eq!(set.latitude, Some(Some(31.2304)));
        assert_eq!(set.longitude, Some(Some(121.4737)));
    }

    #[test]
    fn test_effective_position_returns_none_when_untouched() {
        let req: UpdateChannelRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(req.effective_position(Some(10.0), Some(20.0)), None);
    }

    #[test]
    fn test_effective_position_keeps_the_untouched_dimension() {
        let req: UpdateChannelRequest = serde_json::from_str(r#"{"latitude": 31.2304}"#).unwrap();
        assert_eq!(
            req.effective_position(Some(10.0), Some(20.0)),
            Some((Some(31.2304), Some(20.0)))
        );
    }

    #[test]
    fn test_effective_position_can_clear_one_dimension() {
        let req: UpdateChannelRequest = serde_json::from_str(r#"{"latitude": null}"#).unwrap();
        assert_eq!(
            req.effective_position(Some(10.0), Some(20.0)),
            Some((None, Some(20.0)))
        );
    }

    #[test]
    fn test_apply_to_updates_descriptive_fields_only() {
        let mut channel = Channel::new("dev".to_string(), "ch".to_string(), "old".to_string());
        channel.latitude = Some(1.0);
        channel.longitude = Some(2.0);

        let req: UpdateChannelRequest =
            serde_json::from_str(r#"{"name": "new", "address": "somewhere", "port": 8554}"#).unwrap();
        req.apply_to(&mut channel);

        assert_eq!(channel.name, "new");
        assert_eq!(channel.address.as_deref(), Some("somewhere"));
        assert_eq!(channel.port, 8554);
        assert_eq!(channel.latitude, Some(1.0));
        assert_eq!(channel.longitude, Some(2.0));
    }

    #[test]
    fn test_channel_new_has_no_position() {
        let channel = Channel::new("dev".to_string(), "ch".to_string(), "n".to_string());
        assert_eq!(channel.latitude, None);
        assert_eq!(channel.longitude, None);
    }
}
