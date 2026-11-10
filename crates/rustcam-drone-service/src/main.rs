use std::sync::Arc;

use rustcam_drone_service::config::DroneConfig;
use rustcam_drone_service::infra::db::DroneDb;
use rustcam_drone_service::infra::redis::DroneRedis;
use rustcam_drone_service::lease::LeaseScripts;
use rustcam_drone_service::state::DroneState;
use rustcam_drone_service::supervisor::spawn_supervised;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = DroneConfig::load()?;

    // 日志：控制台 + 按小时滚动的 logs/drone-service.<时间戳>。
    let mut log_cfg = rustcam_drone_core::logging::LogConfig {
        dir: cfg.log_dir.clone(),
        level: cfg.log_level.clone(),
        file_name: "drone-service".into(),
    };
    log_cfg.apply_env();
    rustcam_drone_core::logging::init(&log_cfg);

    tracing::info!(
        grpc = %cfg.grpc_addr,
        metrics = %cfg.metrics_addr,
        "drone-service starting"
    );

    // Redis：控制权仲裁的权威源。连不上不阻断启动，仲裁降级为进程内锁（规则 §11.6）。
    let redis = match DroneRedis::connect(&cfg.redis_url).await {
        Ok(client) => {
            let client = Arc::new(client);
            if let Err(e) = LeaseScripts::new().preload(&client).await {
                tracing::warn!(error = %e, "[Drone] lua scripts preload failed");
            }
            Some(client)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "[Drone] redis unavailable, control arbitration degrades to in-process lock"
            );
            None
        }
    };

    // PostgreSQL：drone_* 表与审计的权威存储。连不上不阻断启动（规则 §11.6）。
    let db = match DroneDb::connect(&cfg.database_url).await {
        Ok(client) => {
            let client = Arc::new(client);
            if let Err(e) = client.run_migrations().await {
                tracing::error!(error = %e, "[Drone] migrations failed, drone_* tables may be missing");
            }
            // 启动时 adapter 都还没接入：清掉上次进程遗留的在线标记。
            if let Err(e) = client.exec("UPDATE drone_adapter SET online = FALSE").await {
                tracing::warn!(error = %e, "[Drone] reset adapter online flags failed");
            }
            Some(client)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "[Drone] postgres unavailable, audits stay in memory until it recovers"
            );
            None
        }
    };

    let state = DroneState::new(cfg).with_redis(redis).with_db(db);

    // 归档队列：遥测与告警落 PostgreSQL（遥测走 TimescaleDB 超表）。
    // ingest 侧只投递不等待，写库由后台 worker 批量完成。
    let (archive_tx, archive_rx) = tokio::sync::mpsc::channel(
        rustcam_drone_service::archive::ARCHIVE_QUEUE_CAPACITY,
    );
    let state = state.with_archive(archive_tx);
    let archive_state = state.clone();
    spawn_supervised("archive-writer", async move {
        rustcam_drone_service::archive::run_archive_worker(archive_state, archive_rx).await;
    });

    // 航线/围栏以 PostgreSQL 为权威源；空库时把 state_file 的旧数据回填。
    if let Err(e) = state.load_persistent().await {
        tracing::error!(error = %e, "[Drone] failed to load routes/fences from PostgreSQL");
    }

    // 状态快照写入任务：合并写入，磁盘 IO 走阻塞线程池。
    if let Some(rx) = state.take_persist_receiver() {
        let writer_state = state.clone();
        spawn_supervised("snapshot-writer", async move {
            rustcam_drone_service::store::run_writer(writer_state, rx).await;
        });
    }

    // 指令超时回收器：没有回执的指令按 deadline 置为 timeout。
    let reaper_state = state.clone();
    spawn_supervised("command-reaper", async move {
        rustcam_drone_service::commands::run_reaper(reaper_state).await;
    });

    // 控制权租约过期回收。
    let lease_state = state.clone();
    spawn_supervised("lease-reaper", async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            ticker.tick().await;
            rustcam_drone_service::control::reap_expired(&lease_state).await;
        }
    });

    // adapter 心跳超时 → 设备离线（半开连接兜底）。
    let presence_state = state.clone();
    spawn_supervised("presence-reaper", async move {
        rustcam_drone_service::presence::run_presence_reaper(presence_state).await;
    });

    // 内网 /metrics（Prometheus 文本），与业务 gRPC 分开端口。
    let metrics_state = state.clone();
    let metrics_addr = state.config.metrics_addr.clone();
    spawn_supervised("metrics-server", async move {
        let mut backoff = rustcam_drone_core::reconnect::Backoff::from_secs_jittered(1, 30, 0.2);
        loop {
            match rustcam_drone_service::infra::metrics::serve(
                metrics_state.clone(),
                metrics_addr.as_str(),
            )
            .await
            {
                Ok(()) => {
                    tracing::info!("[Drone] metrics server stopped, restarting");
                    backoff.reset();
                }
                Err(e) => {
                    tracing::error!(error = %e, "[Drone] metrics server failed, restarting");
                }
            }
            tokio::time::sleep(backoff.next_delay()).await;
        }
    });

    // gRPC 服务端（AdapterLink 面向 adapter；Control/Query/Geofence/Event 面向 rustcam-server）
    // 服务异常退出后按退避重启，保证 adapter / rustcam-server 侧能自动重连。
    let grpc_addr = state.config.grpc_addr.clone();
    let s = state.clone();
    spawn_supervised("grpc-server", async move {
        let mut backoff = rustcam_drone_core::reconnect::Backoff::from_secs_jittered(1, 30, 0.2);
        loop {
            match rustcam_drone_service::grpc::serve(s.clone(), grpc_addr.as_str()).await {
                Ok(()) => {
                    tracing::info!("[Drone] gRPC server stopped, restarting");
                    backoff.reset();
                }
                Err(e) => {
                    tracing::error!(error = %e, "[Drone] gRPC server failed, restarting");
                }
            }
            tokio::time::sleep(backoff.next_delay()).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}
