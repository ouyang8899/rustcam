use rustcam_drone_proto::control::query_service_server::QueryService;
use rustcam_drone_proto::control::{
    AlertList, AlertListRequest, CommandStatus, CommandStatusRequest, DeviceInfo, DeviceList,
    DeviceRequest,
};
use tonic::{Request, Response, Status};

use crate::state::DroneState;

pub struct QueryServiceImpl {
    #[allow(dead_code)]
    state: DroneState,
}

impl QueryServiceImpl {
    pub fn new(state: DroneState) -> QueryServiceImpl {
        QueryServiceImpl { state }
    }
}

#[tonic::async_trait]
impl QueryService for QueryServiceImpl {
    async fn list_devices(
        &self,
        _request: Request<rustcam_drone_proto::common::Empty>,
    ) -> Result<Response<DeviceList>, Status> {
        Ok(Response::new(DeviceList { devices: vec![] }))
    }

    async fn get_device(
        &self,
        request: Request<DeviceRequest>,
    ) -> Result<Response<DeviceInfo>, Status> {
        let device_id = request.into_inner().device_id;
        Ok(Response::new(DeviceInfo {
            device_id,
            protocol: "unknown".into(),
            adapter_id: String::new(),
            online: false,
            last_seen_ms: 0,
            last_telemetry: None,
        }))
    }

    async fn get_telemetry(
        &self,
        request: Request<DeviceRequest>,
    ) -> Result<Response<rustcam_drone_proto::common::Telemetry>, Status> {
        let _ = request.into_inner();
        Ok(Response::new(rustcam_drone_proto::common::Telemetry::default()))
    }

    async fn get_command_status(
        &self,
        request: Request<CommandStatusRequest>,
    ) -> Result<Response<CommandStatus>, Status> {
        let cmd_id = request.into_inner().cmd_id;
        Ok(Response::new(CommandStatus {
            cmd_id,
            status: "unknown".into(),
            message: String::new(),
        }))
    }

    async fn list_alerts(
        &self,
        _request: Request<AlertListRequest>,
    ) -> Result<Response<AlertList>, Status> {
        Ok(Response::new(AlertList { alerts: vec![] }))
    }
}
