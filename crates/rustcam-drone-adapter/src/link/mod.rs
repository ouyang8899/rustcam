use rustcam_drone_proto::link::adapter_link_client::AdapterLinkClient;
use rustcam_drone_proto::link::adapter_message::Payload;
use rustcam_drone_proto::link::{AdapterMessage, Heartbeat, Register};
use tonic::transport::Endpoint;

use crate::config::AdapterConfig;

/// 连接 drone-service 并注册；断线后指数退避重连。
pub async fn run(cfg: AdapterConfig) -> anyhow::Result<()> {
    let min_backoff = cfg.reconnect_min_secs.max(1);
    let max_backoff = cfg.reconnect_max_secs.max(min_backoff);
    let mut backoff_secs = min_backoff;
    loop {
        match connect_once(&cfg).await {
            Ok(()) => backoff_secs = min_backoff,
            Err(e) => {
                tracing::warn!(error = %e, backoff_secs, "[Adapter] link lost, retrying");
                tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = backoff_secs.saturating_mul(2).min(max_backoff);
            }
        }
    }
}

async fn connect_once(cfg: &AdapterConfig) -> anyhow::Result<()> {
    let endpoint = Endpoint::from_shared(cfg.service_addr.clone())?;
    let channel = endpoint.connect().await?;
    let mut client = AdapterLinkClient::new(channel);
    let (tx, rx) = tokio::sync::mpsc::channel::<AdapterMessage>(1024);

    tx.send(AdapterMessage {
        payload: Some(Payload::Register(Register {
            adapter_id: cfg.adapter_id.clone(),
            protocol: cfg.protocol.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            devices: cfg.devices.clone(),
        })),
    })
    .await?;

    let heartbeat_tx = tx.clone();
    let interval = std::time::Duration::from_secs(10);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let msg = AdapterMessage {
                payload: Some(Payload::Heartbeat(Heartbeat {
                    ts_ms: chrono::Utc::now().timestamp_millis(),
                    device_count: 0,
                    inflight_commands: 0,
                    adapter_state: "ok".into(),
                })),
            };
            if heartbeat_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut stream = client
        .open(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await?
        .into_inner();

    while let Some(_msg) = stream.message().await? {
        // 真实 MAVLink 处理在后续计划实现。
    }
    Err(anyhow::anyhow!("adapter link closed"))
}
