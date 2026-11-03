pub mod adapter_link;
pub mod control;
pub mod event;
pub mod geofence;
pub mod query;

use tonic::transport::Server;

use crate::state::DroneState;

/// 启动 gRPC 服务端：
/// - AdapterLink：面向 adapter（双向流）
/// - ControlService / QueryService / GeofenceService / EventService：面向 rustcam-server
pub async fn serve(state: DroneState, addr: &str) -> anyhow::Result<()> {
    let addr = addr.to_string();
    let mut svc = adapter_link::AdapterLinkService::new(state.clone());
    svc.registry = crate::registry::Registry::new(state.clone(), None);

    let control = control::ControlServiceImpl::new(state.clone());
    let query = query::QueryServiceImpl::new(state.clone());
    let geofence = geofence::GeofenceServiceImpl::new(state.clone());
    let events = event::EventServiceImpl::new(state);

    tracing::info!(%addr, "[Drone] gRPC server starting");
    Server::builder()
        .add_service(rustcam_drone_proto::link::adapter_link_server::AdapterLinkServer::new(
            svc,
        ))
        .add_service(rustcam_drone_proto::control::control_service_server::ControlServiceServer::new(
            control,
        ))
        .add_service(rustcam_drone_proto::control::query_service_server::QueryServiceServer::new(
            query,
        ))
        .add_service(
            rustcam_drone_proto::control::geofence_service_server::GeofenceServiceServer::new(
                geofence,
            ),
        )
        .add_service(
            rustcam_drone_proto::control::event_service_server::EventServiceServer::new(events),
        )
        .serve(addr.parse()?)
        .await?;
    Ok(())
}
