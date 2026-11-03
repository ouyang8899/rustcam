mod config;
mod link;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cfg = config::AdapterConfig::load()?;
    tracing::info!(
        adapter = %cfg.adapter_id,
        service = %cfg.service_addr,
        telemetry_hz = cfg.telemetry_hz,
        buffer_size = cfg.buffer_size,
        "adapter starting"
    );

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(link::run(cfg))
}
