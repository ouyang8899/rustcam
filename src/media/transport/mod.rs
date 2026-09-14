use std::net::SocketAddr;
use std::sync::Arc;
use async_trait::async_trait;
use bytes::BytesMut;
use std::collections::HashMap;
use crate::media::errors::{MediaError, Result};
use crate::media::node::DataType;

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
pub mod gb28181_broadcast;
pub mod websocket;

pub use udp::UdpTransport;
pub use tcp::{TcpTransport, Gb28181TcpTransport};
pub use gb28181_broadcast::GB28181BroadcastVoiceTransport;
pub use websocket::{WebSocketMediaTransport, WebSocketMediaNodeTransport};

pub struct NullTransport;

#[async_trait]
impl crate::media::node::MediaTransport for NullTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        Ok("0.0.0.0:0".parse().unwrap())
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok("0.0.0.0:0".parse().unwrap())
    }

    async fn send(&self, _data: &[u8], _data_type: DataType) -> Result<()> {
        Ok(())
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        Ok(HashMap::new())
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

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

    async fn send(&self, data: &[u8], _data_type: DataType) -> Result<()> {
        self.inner.send_raw(data).await
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        let data = self.inner.recv_raw().await?;
        let mut result = HashMap::new();
        result.insert(DataType::OrgData, BytesMut::from(data.as_slice()));
        Ok(result)
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}
