use std::sync::Arc;
use tokio::signal;
use tokio::net::TcpListener;
use tracing::{info, error, Level};
use tracing_log::LogTracer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use rustcam_media::context::{init_registry, ServiceRegistry};
use rustcam_media::config::AppConfig;
use rustcam_media::transport::{
    SipTcpServer, SipTcpServerConfig, SipUdpServer, SipUdpServerConfig,
};
use rustcam_media::api::{AppState, create_router, FullState};

/// 优雅关闭信号处理
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
            info!("Received Ctrl+C signal");
        },
        _ = terminate => {
            info!("Received SIGTERM signal");
        },
    }
}

/// 初始化日志系统
fn init_tracing(level: &str, log_dir: &str) {
    let log_level = match level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    // 创建日志目录
    std::fs::create_dir_all(log_dir).ok();

    // 文件日志 - 每小时滚动一次
    let file_appender = RollingFileAppender::new(
        Rotation::HOURLY,
        log_dir,
        "rustcam.log",
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 构建 subscriber
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(log_level.into()))
        .with(
            fmt::layer()
                .with_target(false)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(std::io::stdout)
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

    // 保存 guard 让它一直存活
    std::mem::forget(_guard);
}

/// 编译时检查 FullState 是否满足 trait bounds
#[allow(dead_code)]
fn assert_fullstate_bounds() {
    fn assert_impl<T: Clone + Send + Sync + 'static>() {}
    assert_impl::<FullState>();
}

/// 主函数
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 0. Initialize JWT CryptoProvider (required for jsonwebtoken 11.x)
    jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER.install_default()
        .expect("Failed to install JWT CryptoProvider");

    // 1. 加载配置
    let config = AppConfig::load()?;

    // 2. 初始化日志系统
    init_tracing(&config.log.level, &config.log.dir);
    info!("Starting RustCam-Media v{}", env!("CARGO_PKG_VERSION"));
    info!("Log directory: {}", config.log.dir);
    info!("Server: {}:{}", config.server.host, config.server.port);

    // 3. 创建 ServiceRegistry
    info!("Initializing services...");
    let registry = Arc::new(ServiceRegistry::new(config).await?);
    init_registry(registry.clone());
    registry.start_all().await;
    info!("All services started");

    // 4. 构建 HTTP 路由
    let auth_state = registry.get_auth_state().await?;
    let app = create_router(AppState::from_registry(&registry), auth_state);

    // 6. 启动 HTTP 服务器
    let http_addr = format!(
        "{}:{}",
        registry.infra.config().server.host,
        registry.infra.config().server.port,
    );
    let listener = TcpListener::bind(&http_addr).await?;
    info!("HTTP API server listening on {}", http_addr);

    // 7. SIP TCP 信令服务器
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
    info!("SIP TCP server listening on {}", tcp_config.bind_addr);

    // 8. SIP UDP 服务器
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
    info!("SIP UDP server listening on {}", udp_config.bind_addr);

    // 9. RTP 服务器管理器
    let signaling_config = &registry.infra.config().signaling_server;
    rustcam_media::init_rtp_server_manager(
        signaling_config.rtp_port_start,
        signaling_config.rtp_port_end,
        signaling_config.rtp_bind_addr.clone(),
        signaling_config.rtp_idle_timeout_secs,
    )?;

    // 9.5 Media Session 管理器
    rustcam_media::init_media_session_manager().await;

    // 10. TCP RTSP RTP 服务器
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

    // 11. GB28181 全局配置

    // 12. 后台 nonce 清理任务（每 5 分钟清理过期 nonce）
    let _nonce_cleanup = tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            rustcam_media::protocol::gb28181::auth::cleanup_expired_nonces();
            rustcam_media::protocol::rtsp::auth::cleanup_expired_nonces();
            tracing::debug!("[Cleanup] Nonce stores cleaned up");
        }
    });

    // 13. RTSP 会话超时清理任务（每 60 秒）
    let rtsp_cluster = registry.media.cluster.clone();
    let _rtsp_cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let removed = rustcam_media::protocol::rtsp::cleanup_expired(rtsp_cluster.clone()).await;
            if removed > 0 {
                tracing::debug!("[RTSP] Cleaned up {} expired sessions", removed);
            }
        }
    });

    // 14. GB28181 订阅超时清理任务（每 5 分钟）
    let _gb28181_sub_cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            let removed = rustcam_media::protocol::cleanup_expired_subscriptions_all().await;
            if removed > 0 {
                tracing::debug!("[GB28181] Cleaned up {} expired subscriptions", removed);
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

    // 14.5 启动适配器事件处理器
    let event_bus = registry.infra.event_bus.clone();
    let _adapter_event_handle = tokio::spawn(async move {
        let mut rx = event_bus.subscribe();
        tracing::info!("[AdapterEvent] Subscribed to event bus, waiting for events");
        let event_bus_for_handler = event_bus.clone();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    tracing::debug!("[AdapterEvent] Received event from broadcast channel");
                    rustcam_media::protocol::adapter_event_handler::handle_adapter_event(event, event_bus_for_handler.clone()).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("[AdapterEvent] Event bus closed, stopping handler");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("[AdapterEvent] Event bus lagged {} events", n);
                }
            }
        }
    });

    // 15. 并发运行所有服务
    let http_server = axum::serve(listener, app);

    tokio::select! {
        result = http_server => {
            if let Err(e) = result {
                error!("HTTP server error: {}", e);
            }
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
            info!("Shutdown signal received");
        }
    }

    // 16. 优雅关闭
    info!("Initiating graceful shutdown...");
    registry.graceful_shutdown().await;


    info!("RustCam-Media stopped");
    Ok(())
}