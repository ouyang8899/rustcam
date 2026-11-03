use async_trait::async_trait;
use std::time::Duration;

use rustcam_drone_proto::control::control_service_client::ControlServiceClient;
use rustcam_drone_proto::control::geofence_service_client::GeofenceServiceClient;
use rustcam_drone_proto::control::query_service_client::QueryServiceClient;
use rustcam_drone_proto::control::{DeviceInfo, DeviceRequest};
use rustcam_drone_proto::link::adapter_link_client::AdapterLinkClient;
use tonic::transport::{Channel, Endpoint};

/// 网关侧统一使用的 drone 数据结构（与 proto 解耦，便于测试替身）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfoDto {
    pub device_id: String,
    pub protocol: String,
    pub adapter_id: String,
    pub online: bool,
    pub last_seen_ms: i64,
}

/// 主服务对 drone-service 的调用面；生产实现是 gRPC，测试用替身。
#[async_trait]
pub trait DroneApi: Send + Sync + 'static {
    async fn ping(&self) -> Result<(), String>;
    async fn list_devices(&self) -> Result<Vec<DeviceInfoDto>, String>;
    async fn get_device(&self, device_id: &str) -> Result<DeviceInfoDto, String>;
}

/// drone-service 未配置时的占位实现，所有调用返回错误。
pub struct NoopDroneApi;

#[async_trait]
impl DroneApi for NoopDroneApi {
    async fn ping(&self) -> Result<(), String> {
        Err("drone-service not configured".into())
    }
    async fn list_devices(&self) -> Result<Vec<DeviceInfoDto>, String> {
        Err("drone-service not configured".into())
    }
    async fn get_device(&self, _device_id: &str) -> Result<DeviceInfoDto, String> {
        Err("drone-service not configured".into())
    }
}

#[derive(Clone)]
pub struct DroneClient {
    pub control: ControlServiceClient<Channel>,
    pub query: QueryServiceClient<Channel>,
    pub geofence: GeofenceServiceClient<Channel>,
    pub adapter_link: AdapterLinkClient<Channel>,
}

impl DroneClient {
    pub async fn connect(service_addr: &str) -> anyhow::Result<DroneClient> {
        let endpoint = Endpoint::from_shared(service_addr.to_string())?
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10));
        let channel = endpoint.connect().await?;
        Ok(DroneClient {
            control: ControlServiceClient::new(channel.clone()),
            query: QueryServiceClient::new(channel.clone()),
            geofence: GeofenceServiceClient::new(channel.clone()),
            adapter_link: AdapterLinkClient::new(channel),
        })
    }
}

fn to_dto(info: DeviceInfo) -> DeviceInfoDto {
    DeviceInfoDto {
        device_id: info.device_id,
        protocol: info.protocol,
        adapter_id: info.adapter_id,
        online: info.online,
        last_seen_ms: info.last_seen_ms,
    }
}

#[async_trait]
impl DroneApi for DroneClient {
    async fn ping(&self) -> Result<(), String> {
        let mut query = self.query.clone();
        query
            .list_devices(rustcam_drone_proto::common::Empty {})
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn list_devices(&self) -> Result<Vec<DeviceInfoDto>, String> {
        let mut query = self.query.clone();
        let resp = query
            .list_devices(rustcam_drone_proto::common::Empty {})
            .await
            .map_err(|e| e.to_string())?;
        Ok(resp.into_inner().devices.into_iter().map(to_dto).collect())
    }

    async fn get_device(&self, device_id: &str) -> Result<DeviceInfoDto, String> {
        let mut query = self.query.clone();
        let resp = query
            .get_device(DeviceRequest {
                device_id: device_id.to_string(),
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(to_dto(resp.into_inner()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_config_defaults_to_local_drone_service() {
        let cfg = crate::config::DroneClientConfig::default();
        assert_eq!(cfg.service_addr, "http://127.0.0.1:50051");
    }

    #[tokio::test]
    async fn connect_rejects_invalid_address() {
        let err = DroneClient::connect("not a url").await.err();
        assert!(err.is_some(), "非法地址必须返回错误而不是 panic");
    }
}
