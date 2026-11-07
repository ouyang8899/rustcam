mod config;
mod fc;
mod link;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cfg = config::AdapterConfig::load()?;
    tracing::info!(
        adapter = %cfg.adapter_id,
        service = %cfg.service_addr,
        autopilot = %cfg.autopilot,
        telemetry_hz = cfg.telemetry_hz,
        "adapter starting (devices and endpoints are assigned by drone-service)"
    );

    link::run(cfg).await
}
