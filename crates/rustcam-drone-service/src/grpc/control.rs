use rustcam_drone_proto::control::control_service_server::ControlService;
use rustcam_drone_proto::control::{
    AcquireRequest, ForceReleaseRequest, Lease, PreemptRequest, ReleaseReply, ReleaseRequest,
    RenewRequest, StateReply, StateRequest,
};
use tonic::{Request, Response, Status};

use crate::state::DroneState;

pub struct ControlServiceImpl {
    #[allow(dead_code)]
    state: DroneState,
}

impl ControlServiceImpl {
    pub fn new(state: DroneState) -> ControlServiceImpl {
        ControlServiceImpl { state }
    }
}

fn lease_from(
    device_id: &str,
    control_type: &str,
    holder: &str,
    priority: i32,
    lease_seconds: i32,
) -> Lease {
    Lease {
        device_id: device_id.to_string(),
        control_type: control_type.to_string(),
        holder: holder.to_string(),
        priority,
        expire_at_ms: chrono::Utc::now().timestamp_millis() + (lease_seconds as i64) * 1000,
        preempted: String::new(),
    }
}

#[tonic::async_trait]
impl ControlService for ControlServiceImpl {
    async fn acquire_control(
        &self,
        request: Request<AcquireRequest>,
    ) -> Result<Response<Lease>, Status> {
        let r = request.into_inner();
        Ok(Response::new(lease_from(
            &r.device_id,
            &r.control_type,
            &r.operator,
            r.priority,
            r.lease_seconds,
        )))
    }

    async fn release_control(
        &self,
        request: Request<ReleaseRequest>,
    ) -> Result<Response<ReleaseReply>, Status> {
        let _ = request.into_inner();
        Ok(Response::new(ReleaseReply {
            released: true,
            reason: String::new(),
        }))
    }

    async fn preempt_control(
        &self,
        request: Request<PreemptRequest>,
    ) -> Result<Response<Lease>, Status> {
        let r = request.into_inner();
        Ok(Response::new(lease_from(
            &r.device_id,
            &r.control_type,
            &r.operator,
            r.priority,
            r.lease_seconds,
        )))
    }

    async fn renew_control(
        &self,
        request: Request<RenewRequest>,
    ) -> Result<Response<Lease>, Status> {
        let r = request.into_inner();
        Ok(Response::new(lease_from(
            &r.device_id,
            &r.control_type,
            &r.operator,
            0,
            r.lease_seconds,
        )))
    }

    async fn force_release_control(
        &self,
        request: Request<ForceReleaseRequest>,
    ) -> Result<Response<ReleaseReply>, Status> {
        let _ = request.into_inner();
        Ok(Response::new(ReleaseReply {
            released: true,
            reason: String::new(),
        }))
    }

    async fn get_control_state(
        &self,
        request: Request<StateRequest>,
    ) -> Result<Response<StateReply>, Status> {
        let _ = request.into_inner();
        Ok(Response::new(StateReply {
            held: false,
            lease: None,
        }))
    }

    async fn send_command(
        &self,
        request: Request<rustcam_drone_proto::common::Command>,
    ) -> Result<Response<rustcam_drone_proto::common::CommandReply>, Status> {
        let cmd = request.into_inner();
        Ok(Response::new(rustcam_drone_proto::common::CommandReply {
            cmd_id: cmd.cmd_id,
            device_id: cmd.device_id,
            status: "rejected".into(),
            code: 501,
            message: "command pipeline not implemented yet".into(),
            ts_ms: chrono::Utc::now().timestamp_millis(),
        }))
    }
}
