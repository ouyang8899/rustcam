pub mod adapter_link;
pub mod admin;
pub mod control;
pub mod event;
pub mod geofence;
pub mod query;

use rustcam_drone_core::reconnect::Backoff;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

use crate::state::DroneState;

/// 启动 gRPC 服务端：
/// - AdapterLink：面向 adapter（双向流）
/// - Control/Query/Geofence/Admin/Event：面向 rustcam-server
pub async fn serve(state: DroneState, addr: &str) -> anyhow::Result<()> {
    let addr = addr.to_string();
    let listener = bind_with_retry(&addr).await?;

    let mut svc = adapter_link::AdapterLinkService::new(state.clone());
    svc.registry = crate::registry::Registry::new(state.clone(), state.redis.clone());

    let control = control::ControlServiceImpl::new(state.clone());
    let query = query::QueryServiceImpl::new(state.clone());
    let geofence = geofence::GeofenceServiceImpl::new(state.clone());
    let admin = admin::AdminServiceImpl::new(state.clone());
    let events = event::EventServiceImpl::new(state);

    tracing::info!(%addr, "[Drone] gRPC server starting");
    Server::builder()
        .add_service(rustcam_drone_proto::link::adapter_link_server::AdapterLinkServer::new(svc))
        .add_service(
            rustcam_drone_proto::control::control_service_server::ControlServiceServer::new(
                control,
            ),
        )
        .add_service(
            rustcam_drone_proto::control::query_service_server::QueryServiceServer::new(query),
        )
        .add_service(
            rustcam_drone_proto::control::geofence_service_server::GeofenceServiceServer::new(
                geofence,
            ),
        )
        .add_service(
            rustcam_drone_proto::control::admin_service_server::AdminServiceServer::new(admin),
        )
        .add_service(
            rustcam_drone_proto::control::event_service_server::EventServiceServer::new(events),
        )
        .serve_with_incoming(TcpListenerStream::new(listener))
        .await?;
    Ok(())
}

/// 绑定 gRPC 监听端口；端口被占用（旧实例尚未退出）时按退避重试。
///
/// 服务重启时新旧进程可能短暂共存，此时新进程不应直接退出，
/// 否则平台侧要等人工再拉一次。
async fn bind_with_retry(addr: &str) -> anyhow::Result<TcpListener> {
    let mut backoff = Backoff::from_secs_jittered(1, 30, 0.2);
    loop {
        match TcpListener::bind(addr).await {
            Ok(listener) => return Ok(listener),
            Err(e) => {
                tracing::warn!(error = %e, %addr, "[Drone] gRPC 端口占用，退避后重试绑定");
                tokio::time::sleep(backoff.next_delay()).await;
            }
        }
    }
}
