use rustcam_drone_proto::control::control_service_server::ControlService;
use rustcam_drone_proto::control::{
    AcquireRequest, ForceReleaseRequest, Lease, PreemptRequest, ReleaseReply, ReleaseRequest,
    RenewRequest, StateReply, StateRequest,
};
use tonic::{Request, Response, Status};

use crate::control;
use crate::state::DroneState;

pub struct ControlServiceImpl {
    state: DroneState,
}

impl ControlServiceImpl {
    pub fn new(state: DroneState) -> ControlServiceImpl {
        ControlServiceImpl { state }
    }
}

fn to_lease(outcome: &control::LeaseOutcome) -> Lease {
    Lease {
        drone_tag: outcome.lease.drone_tag.clone(),
        control_type: outcome.lease.control_type.clone(),
        holder: outcome.lease.holder.clone(),
        priority: outcome.lease.priority,
        expire_at_ms: outcome.lease.expire_at_ms,
        preempted: outcome.preempted.clone(),
        granted: outcome.granted,
        reason: outcome.reason.clone(),
    }
}

#[tonic::async_trait]
impl ControlService for ControlServiceImpl {
    async fn acquire_control(
        &self,
        request: Request<AcquireRequest>,
    ) -> Result<Response<Lease>, Status> {
        let r = request.into_inner();
        let outcome = control::acquire(
            &self.state,
            &r.drone_tag,
            &r.control_type,
            &r.operator,
            r.priority,
            r.lease_seconds,
        )
        .await
        .map_err(Status::invalid_argument)?;
        Ok(Response::new(to_lease(&outcome)))
    }

    async fn release_control(
        &self,
        request: Request<ReleaseRequest>,
    ) -> Result<Response<ReleaseReply>, Status> {
        let r = request.into_inner();
        match control::release(
            &self.state,
            &r.drone_tag,
            &r.control_type,
            &r.operator,
            false,
        )
        .await
        {
            Ok(lease) => Ok(Response::new(ReleaseReply {
                released: true,
                reason: format!("released by {}", lease.holder),
            })),
            Err(e) => Ok(Response::new(ReleaseReply {
                released: false,
                reason: e,
            })),
        }
    }

    async fn preempt_control(
        &self,
        request: Request<PreemptRequest>,
    ) -> Result<Response<Lease>, Status> {
        let r = request.into_inner();
        let outcome = control::preempt(
            &self.state,
            &r.drone_tag,
            &r.control_type,
            &r.operator,
            r.priority,
            r.lease_seconds,
        )
        .await
        .map_err(Status::invalid_argument)?;
        Ok(Response::new(to_lease(&outcome)))
    }

    async fn renew_control(
        &self,
        request: Request<RenewRequest>,
    ) -> Result<Response<Lease>, Status> {
        let r = request.into_inner();
        match control::renew(
            &self.state,
            &r.drone_tag,
            &r.control_type,
            &r.operator,
            r.lease_seconds,
        )
        .await
        {
            Ok(outcome) => Ok(Response::new(to_lease(&outcome))),
            Err(e) => Ok(Response::new(Lease {
                drone_tag: r.drone_tag,
                control_type: r.control_type,
                granted: false,
                reason: e,
                ..Default::default()
            })),
        }
    }

    async fn force_release_control(
        &self,
        request: Request<ForceReleaseRequest>,
    ) -> Result<Response<ReleaseReply>, Status> {
        let r = request.into_inner();
        match control::release(
            &self.state,
            &r.drone_tag,
            &r.control_type,
            &r.operator,
            true,
        )
        .await
        {
            Ok(_) => Ok(Response::new(ReleaseReply {
                released: true,
                reason: "force released".into(),
            })),
            Err(e) => Ok(Response::new(ReleaseReply {
                released: false,
                reason: e,
            })),
        }
    }

    async fn get_control_state(
        &self,
        request: Request<StateRequest>,
    ) -> Result<Response<StateReply>, Status> {
        let r = request.into_inner();
        let lease = control::get(&self.state, &r.drone_tag, &r.control_type)
            .await
            .map(|l| Lease {
                drone_tag: l.drone_tag,
                control_type: l.control_type,
                holder: l.holder,
                priority: l.priority,
                expire_at_ms: l.expire_at_ms,
                preempted: String::new(),
                granted: true,
                reason: String::new(),
            });
        Ok(Response::new(StateReply {
            held: lease.is_some(),
            lease,
        }))
    }

    async fn send_command(
        &self,
        request: Request<rustcam_drone_proto::common::Command>,
    ) -> Result<Response<rustcam_drone_proto::common::CommandReply>, Status> {
        let cmd = request.into_inner();
        Ok(Response::new(
            crate::commands::submit(&self.state, cmd).await,
        ))
    }
}
