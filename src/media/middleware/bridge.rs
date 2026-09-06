use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;
use bytes::BytesMut;
use async_trait::async_trait;
use tokio::net::UdpSocket;

use crate::media::{
    Middleware, MiddlewareResult, MediaContext, Next,
};

pub struct BridgeMiddleware {
    target_addr: SocketAddr,
    socket: Option<Arc<UdpSocket>>,
}

impl BridgeMiddleware {
    pub fn new(target_addr: SocketAddr) -> Self {
        Self {
            target_addr,
            socket: None,
        }
    }
    
    pub async fn with_socket(mut self, socket: Arc<UdpSocket>) -> Self {
        self.socket = Some(socket);
        self
    }
}

#[async_trait]
impl Middleware for BridgeMiddleware {
    fn name(&self) -> &str {
        "bridge"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        let data = ctx.data.clone();
        
        if let Some(ref socket) = self.socket {
            match socket.send_to(&data, self.target_addr).await {
                Ok(_) => {
                    tracing::debug!(
                        "[Bridge] Forwarded {} bytes to {}",
                        data.len(),
                        self.target_addr
                    );
                }
                Err(e) => {
                    tracing::warn!("[Bridge] Forward failed to {}: {}", self.target_addr, e);
                }
            }
        } else {
            tracing::debug!(
                "[Bridge] No socket configured, skipping forward to {}",
                self.target_addr
            );
        }
        
        next.run(ctx).await
    }
}

pub struct MultiBridgeMiddleware {
    targets: Vec<SocketAddr>,
}

impl MultiBridgeMiddleware {
    pub fn new(targets: Vec<SocketAddr>) -> Self {
        Self { targets }
    }
}

#[async_trait]
impl Middleware for MultiBridgeMiddleware {
    fn name(&self) -> &str {
        "multi_bridge"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        let data = ctx.data.clone();
        
        for target in &self.targets {
            tracing::debug!("[MultiBridge] Forwarding to {}", target);
        }
        
        next.run(ctx).await
    }
}
