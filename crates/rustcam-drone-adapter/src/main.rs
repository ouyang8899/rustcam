mod config;
mod fc;
mod link;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::AdapterConfig::load()?;

    // 日志：控制台 + 按小时滚动的 logs/drone-adapter.<时间戳>。
    let mut log_cfg = rustcam_drone_core::logging::LogConfig {
        dir: cfg.log_dir.clone(),
        level: cfg.log_level.clone(),
        file_name: "drone-adapter".into(),
    };
    log_cfg.apply_env();
    rustcam_drone_core::logging::init(&log_cfg);

    tracing::info!(
        adapter = %cfg.adapter_id,
        service = %cfg.service_addr,
        autopilot = %cfg.autopilot,
        telemetry_hz = cfg.telemetry_hz,
        "adapter starting (devices and endpoints are assigned by drone-service)"
    );

    link::run(cfg).await
}
