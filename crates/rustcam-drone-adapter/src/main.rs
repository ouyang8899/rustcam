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
        mavlink = %cfg.mavlink_addr,
        autopilot = %cfg.autopilot,
        device = %cfg.effective_device_id(),
        telemetry_hz = cfg.telemetry_hz,
        "adapter starting"
    );

    link::run(cfg).await
}
