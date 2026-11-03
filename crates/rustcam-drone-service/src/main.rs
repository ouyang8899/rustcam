use rustcam_drone_service::config::DroneConfig;
use rustcam_drone_service::state::DroneState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cfg = DroneConfig::load()?;
    tracing::info!(
        grpc = %cfg.grpc_addr,
        metrics = %cfg.metrics_addr,
        "drone-service starting"
    );

    let state = DroneState::new(cfg);

    // gRPC 服务端（AdapterLink 面向 adapter；Control/Query/Geofence/Event 随后续任务注册）
    let grpc_addr = state.config.grpc_addr.clone();
    let s = state.clone();
    tokio::spawn(async move {
        if let Err(e) = rustcam_drone_service::grpc::serve(s, grpc_addr.as_str()).await {
            tracing::error!(error = %e, "drone gRPC server stopped");
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}
