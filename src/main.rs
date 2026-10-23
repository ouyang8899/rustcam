use std::sync::Arc;
use tokio::signal;
use tokio::net::TcpListener;
use tracing::{error, Level};
use tracing_log::LogTracer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use rustcam_media::context::{init_registry, ServiceRegistry};
use rustcam_media::config::AppConfig;
use rustcam_media::transport::{
    SipTcpServer, SipTcpServerConfig, SipUdpServer, SipUdpServerConfig,
};
use rustcam_media::api::{AppState, create_router, FullState};

use futures::FutureExt;

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
        },
        _ = terminate => {
        },
    }
}

fn init_tracing(level: &str, log_dir: &str) {
    let log_level = match level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    std::fs::create_dir_all(log_dir).ok();

    let file_appender = RollingFileAppender::new(
        Rotation::HOURLY,
        log_dir,
        "rustcam.log",
    );

    let (stdout_nb, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
    let (non_blocking, _guard) =
        tracing_appender::non_blocking::NonBlockingBuilder::default()
            .lossy(true)
            .buffered_lines_limit(1024)
            .finish(file_appender);

    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(log_level.into()))
        .with(
            fmt::layer()
                .with_target(false)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(stdout_nb)
        )
        .with(
            fmt::layer()
                .with_target(false)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(non_blocking)
        );

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    LogTracer::init().expect("Failed to init LogTracer");

    std::mem::forget(_guard);
    std::mem::forget(stdout_guard);
}

#[allow(dead_code)]
fn assert_fullstate_bounds() {
    fn assert_impl<T: Clone + Send + Sync + 'static>() {}
    assert_impl::<FullState>();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER.install_default()
        .expect("Failed to install JWT CryptoProvider");

    let config = AppConfig::load()?;

    init_tracing(&config.log.level, &config.log.dir);
    rustcam_media::protocol::gb28181::sip_log::set_enabled(config.log.sip);

    rustcam_media::infrastructure::watchdog::spawn_watchdog(&config.log.dir);
    rustcam_media::infrastructure::watchdog::spawn_heartbeat();

    let registry = Arc::new(ServiceRegistry::new(config).await?);
    init_registry(registry.clone());
    registry.start_all().await;

    let auth_state = registry.get_auth_state().await?;
    let app = create_router(AppState::from_registry(&registry), auth_state);

    let http_addr = format!(
        "{}:{}",
        registry.infra.config().server.host,
        registry.infra.config().server.port,
    );
    let _server_config = &registry.infra.config().server;

    let listener = TcpListener::bind(&http_addr).await?;
    let http_server = async move {
        let server = axum::serve(listener, app);
        if let Err(e) = server.await {
            error!("HTTP server error: {}", e);
        }
    }.boxed();

    let sig_cfg = &registry.infra.config().signaling_server;
    let tcp_config = SipTcpServerConfig {
        bind_addr: format!("{}:{}", sig_cfg.bind_ip, sig_cfg.tcp_signaling_port),
        max_connections: sig_cfg.tcp_max_connections,
        read_buffer_size: sig_cfg.tcp_read_buffer_size,
        connection_timeout: sig_cfg.tcp_connection_timeout,
        keepalive_interval_secs: sig_cfg.tcp_keepalive_interval_secs,
        read_timeout_secs: sig_cfg.tcp_read_timeout_secs,
    };
    let sip_tcp_server = SipTcpServer::new(
        tcp_config.clone(),
        registry.infra.event_bus.clone(),
        registry.infra.metrics.clone(),
        registry.protocol_deps.clone(),
    )
    .await?;

    let udp_config = SipUdpServerConfig {
        bind_addr: format!("{}:{}", sig_cfg.bind_ip, sig_cfg.udp_signaling_port),
        max_packet_size: sig_cfg.udp_max_packet_size,
        adapter_timeout_secs: sig_cfg.udp_adapter_timeout_secs,
        max_adapters: sig_cfg.udp_max_adapters,
    };
    let sip_udp_server = SipUdpServer::new(
        udp_config.clone(),
        registry.infra.event_bus.clone(),
        registry.infra.metrics.clone(),
        registry.protocol_deps.clone(),
    )
    .await?;

    let signaling_config = &registry.infra.config().signaling_server;
    rustcam_media::init_rtp_server_manager(
        signaling_config.rtp_port_start,
        signaling_config.rtp_port_end,
        signaling_config.rtp_bind_addr.clone(),
        signaling_config.rtp_idle_timeout_secs,
    )?;

    rustcam_media::init_media_session_manager().await;

    let tcp_rtsp_rtp_config = registry.infra.config().rtsp_server.clone();
    let tcp_rtsp_rtp_server = match rustcam_media::init_tcp_rtsp_rtp_server(
        tcp_rtsp_rtp_config,
        registry.infra.event_bus.clone(),
        registry.infra.metrics.clone(),
        registry.protocol_deps.clone(),
    ).await {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to initialize TCP RTSP RTP server: {}", e);
            return Err(e.into());
        }
    };

    let _nonce_cleanup = tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            rustcam_media::protocol::gb28181::auth::cleanup_expired_nonces();
            rustcam_media::protocol::rtsp::auth::cleanup_expired_nonces();
        }
    });

    let rtsp_cluster = registry.media.cluster.clone();
    let _rtsp_cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let removed = rustcam_media::protocol::rtsp::cleanup_expired(rtsp_cluster.clone()).await;
            if removed > 0 {
            }
        }
    });

    let _gb28181_sub_cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            let removed = rustcam_media::protocol::cleanup_expired_subscriptions_all().await;
            if removed > 0 {
            }
        }
    });

    rustcam_media::protocol::gb28181::set_udp_sender(sip_udp_server.socket());
    rustcam_media::protocol::gb28181::set_gb28181_platform_config(
        registry.infra.config().signaling_server.server_gb_id.clone(),
        registry.infra.config().signaling_server.server_gb_domain.clone(),
        registry.infra.config().signaling_server.bind_ip.clone(),
        registry.infra.config().signaling_server.tcp_signaling_port,
    );

    let event_bus = registry.infra.event_bus.clone();
    let _adapter_event_handle = tokio::spawn(async move {
        let mut rx = event_bus.subscribe();
        let event_bus_for_handler = event_bus.clone();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    rustcam_media::protocol::adapter_event_handler::handle_adapter_event(event, event_bus_for_handler.clone()).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("[AdapterEvent] Event bus lagged {} events", n);
                }
            }
        }
    });

    let hook_handler = rustcam_media::application::hook_event_handler::HookEventHandler::new(
        registry.infra.db.clone(),
        registry.infra.redis.clone(),
        registry.infra.event_bus.clone(),
        registry.infra.config().session.expiration_secs,
        registry.recording_service.clone(),
    );
    hook_handler.start().await;

    tokio::select! {
        _result = http_server => {
        }
        result = sip_tcp_server.run() => {
            if let Err(e) = result {
                error!("SIP TCP server error: {}", e);
            }
        }
        result = sip_udp_server.run() => {
            if let Err(e) = result {
                error!("SIP UDP server error: {}", e);
            }
        }
        result = tcp_rtsp_rtp_server.run() => {
            if let Err(e) = result {
                error!("TCP RTSP RTP server error: {}", e);
            }
        }
        _ = shutdown_signal() => {
        }
    }

    registry.graceful_shutdown().await;

    Ok(())
}