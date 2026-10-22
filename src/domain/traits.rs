use async_trait::async_trait;

use crate::domain::{ Device, Protocol, Stream, Session, SessionType, Recording };
use crate::adapter::media_server::{ StreamInfo };
use crate::protocol::event::SignalEvent;
use crate::error::AppError;

#[async_trait]
pub trait DeviceLookup: Send + Sync {
    async fn find_by_tag(&self, tag: &str) -> Option<Device>;
    async fn find_by_stream_key(&self, stream_key: &str) -> Option<Device>;
    async fn find_by_protocol_and_host(&self, protocol: &Protocol, host: &str) -> Option<Device>;
    async fn get_device(&self, id: i64) -> Result<Option<Device>, AppError>;
    async fn set_online(&self, tag: &str) -> Result<(), AppError>;
    async fn set_offline(&self, tag: &str, reason: Option<&str>) -> Result<(), AppError>;
    async fn update_device_host_port(&self, tag: &str, host: &str, port: u16) -> Result<(), AppError>;
    async fn log_ptz_control(
        &self,
        user_id: Option<uuid::Uuid>,
        device_id: i64,
        command: &str,
        speed: u8,
        result: bool,
        error_message: Option<String>,
        call_id: Option<String>,
    ) -> Result<(), AppError>;
    async fn log_ptz_result(
        &self,
        device_id: i64,
        call_id: Option<&str>,
        sip_code: Option<u16>,
        status: &str,
        message: Option<String>,
    ) -> Result<(), AppError>;
    fn broadcast_ptz_result(
        &self,
        device_id: i64,
        call_id: &str,
        command: &str,
        status: &str,
        sip_code: Option<u16>,
        message: Option<&str>,
    );
    fn get_stats(&self) -> serde_json::Value;
    async fn list_online_devices(&self) -> Vec<Device>;
}

#[async_trait]
pub trait StreamManager: Send + Sync {
    async fn start_pull_stream(
        &self, device_tag: &str, channel_tag: &str, rtsp_url: &str,
    ) -> Result<StreamInfo, AppError>;
    async fn start_gb28181_stream(
        &self, device_tag: &str, channel_tag: &str, stream_key: &str,
    ) -> Result<StreamInfo, AppError>;
    async fn stop_stream(&self, app: &str, stream_key: &str) -> Result<(), AppError>;
    async fn stop_streams_by_device(&self, device_tag: &str) -> Result<(), AppError>;
    async fn stop_streams_by_channel(&self, device_tag: &str, channel_tag: &str) -> Result<(), AppError>;
    async fn generate_token(&self, device_id: &str) -> Result<String, AppError>;
    async fn validate_token(&self, token: &str) -> Option<String>;
    async fn build_play_links(
        &self, device: &Device, token: &str, stream_id: &str,
    ) -> crate::domain::device::PlayLinks;
    async fn get_stats(&self) -> serde_json::Value;
    async fn get_stream_by_stream_key(&self, app: &str, stream_key: &str) -> Option<Stream>;
    async fn update_stream_state(&self, stream: &Stream) -> Result<(), AppError>;
}

#[async_trait]
pub trait SessionManager: Send + Sync {
    async fn create_session(
        &self, session_type: SessionType, device_id: i64, user_id: i64,
    ) -> Result<Session, AppError>;
    async fn activate_session(
        &self, session_id: i64, rtsp_url: &str,
    ) -> Result<StreamInfo, AppError>;
    async fn deactivate_session(&self, session_id: i64) -> Result<(), AppError>;
    fn get_session(&self, id: i64) -> Result<Option<Session>, AppError>;
    fn get_active_count(&self) -> usize;
}

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: SignalEvent) -> Result<(), AppError>;
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SignalEvent>;
}

#[async_trait]
pub trait RecordingManager: Send + Sync {
    async fn start_recording(
        &self, device_id: i64, stream_key: &str, media_server: &str,
    ) -> Result<Recording, AppError>;
    async fn stop_recording(&self, recording_id: i64) -> Result<(), AppError>;
    async fn stop_recording_by_stream_key(&self, stream_key: &str) -> Result<(), AppError>;
    fn get_stats(&self) -> serde_json::Value;
}

#[async_trait]
pub trait CacheStore: Send + Sync {

    async fn set_json(&self, key: &str, value: &serde_json::Value, ttl_secs: Option<u64>) -> Result<(), AppError>;

    async fn get_json(&self, key: &str) -> Result<Option<serde_json::Value>, AppError>;

    async fn del(&self, key: &str) -> Result<(), AppError>;

    async fn set_stream_info(&self, stream_key: &str, info: &StreamInfo) -> Result<(), AppError>;

    async fn get_stream_info(&self, stream_key: &str) -> Result<Option<StreamInfo>, AppError>;

    async fn delete_stream_info(&self, stream_key: &str) -> Result<(), AppError>;

    async fn publish_event_json(&self, event: &str, data: &serde_json::Value) -> Result<(), AppError>;

    async fn incr(&self, key: &str) -> Result<i64, AppError>;

    async fn exists(&self, key: &str) -> Result<bool, AppError>;

    async fn expire(&self, key: &str, seconds: u64) -> Result<(), AppError>;

    async fn sadd(&self, key: &str, member: &str) -> Result<(), AppError>;

    async fn srem(&self, key: &str, member: &str) -> Result<(), AppError>;

    async fn smembers(&self, key: &str) -> Result<Vec<String>, AppError>;

    async fn hset(&self, key: &str, field: &str, value: &str) -> Result<(), AppError>;

    async fn hmset(&self, key: &str, fields: &[(&str, &str)]) -> Result<(), AppError>;

    async fn hget(&self, key: &str, field: &str) -> Result<Option<String>, AppError>;

    async fn hgetall(&self, key: &str) -> Result<std::collections::HashMap<String, String>, AppError>;
}

#[async_trait]
pub trait CacheStoreExt: CacheStore {

    async fn set<T: serde::Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        ttl_secs: Option<u64>,
    ) -> Result<(), AppError> {
        let json = serde_json::to_value(value)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.set_json(key, &json, ttl_secs).await
    }

    async fn get<T: serde::de::DeserializeOwned + Send + Sync>(
        &self,
        key: &str,
    ) -> Result<Option<T>, AppError> {
        match self.get_json(key).await? {
            Some(json) => {
                let value = serde_json::from_value(json)
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    async fn publish_event<T: serde::Serialize + Send + Sync>(
        &self,
        event: &str,
        data: &T,
    ) -> Result<(), AppError> {
        let json = serde_json::to_value(data)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        self.publish_event_json(event, &json).await
    }
}

#[async_trait]
impl<T: CacheStore + ?Sized> CacheStoreExt for T {}