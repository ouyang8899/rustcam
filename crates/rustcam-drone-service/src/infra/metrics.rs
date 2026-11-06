//! 极简 Prometheus 文本端点：drone-service 只暴露内网 `/metrics`，不引入 HTTP 框架。

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::state::DroneState;

pub async fn serve(state: DroneState, addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "[Drone] metrics server listening");
    serve_listener(state, listener).await
}

/// 供测试注入已绑定的 listener。
pub async fn serve_listener(state: DroneState, listener: TcpListener) -> anyhow::Result<()> {
    loop {
        let (mut socket, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "[Drone] metrics accept failed");
                continue;
            }
        };
        let state = state.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            // 只关心请求行是否合法；读不到也照常返回指标，方便探活。
            let _ = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut buf)).await;
            let body = state.metrics_text();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
            tracing::debug!(peer = %peer, "[Drone] metrics scraped");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;

    #[tokio::test]
    async fn metrics_endpoint_serves_prometheus_text() {
        let state = DroneState::new(DroneConfig::default());
        state.metrics_inc("rustcam_drone_telemetry_total");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            serve_listener(state, listener).await.unwrap();
        });

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("rustcam_drone_telemetry_total 1"));
        assert!(response.contains("rustcam_drone_online_devices 0"));
    }
}
