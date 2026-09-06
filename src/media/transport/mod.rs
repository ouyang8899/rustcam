use std::net::SocketAddr;
use std::sync::Arc;
use async_trait::async_trait;
use bytes::BytesMut;
use crate::media::errors::{MediaError, Result};

#[async_trait]
pub trait DataTransport: Send + Sync {
    fn local_addr(&self) -> Result<SocketAddr>;
    fn peer_addr(&self) -> Result<SocketAddr>;
    
    async fn send_raw(&self, data: &[u8]) -> Result<()>;
    async fn recv_raw(&self) -> Result<Vec<u8>>;
    
    async fn close(&self) -> Result<()>;
}

pub mod udp;
pub mod tcp;

pub use udp::UdpTransport;
pub use tcp::{TcpTransport, Gb28181TcpTransport};

pub struct DataTransportAdapter {
    inner: Arc<dyn DataTransport>,
}

impl DataTransportAdapter {
    pub fn new(inner: Arc<dyn DataTransport>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for DataTransportAdapter {
    fn local_addr(&self) -> Result<SocketAddr> {
        self.inner.local_addr()
    }
    
    fn peer_addr(&self) -> Result<SocketAddr> {
        self.inner.peer_addr()
    }
    
    async fn send(&self, data: &[u8]) -> Result<()> {
        self.inner.send_raw(data).await
    }
    
    async fn recv(&self) -> Result<BytesMut> {
        let data = self.inner.recv_raw().await?;
        Ok(BytesMut::from(data.as_slice()))
    }
    
    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}
