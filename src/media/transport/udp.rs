use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use async_trait::async_trait;
use bytes::BytesMut;
use std::collections::HashMap;
use crate::media::errors::{MediaError, Result};
use crate::media::node::DataType;
use super::DataTransport;

pub struct UdpTransport {
    socket: Arc<UdpSocket>,
    remote: SocketAddr,
}

impl UdpTransport {
    pub async fn new(local: SocketAddr, remote: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind(local).await
            .map_err(|e| MediaError::Transport(format!("Failed to bind UDP socket: {}", e)))?;
        
        socket.connect(remote).await
            .map_err(|e| MediaError::Transport(format!("Failed to connect UDP: {}", e)))?;
        
        Ok(Self {
            socket: Arc::new(socket),
            remote,
        })
    }
    
    pub async fn bind(local: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind(local).await
            .map_err(|e| MediaError::Transport(format!("Failed to bind UDP socket: {}", e)))?;
        
        let remote = socket.peer_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get peer addr: {}", e)))?;
        
        Ok(Self {
            socket: Arc::new(socket),
            remote,
        })
    }
    
    pub fn with_socket(socket: Arc<UdpSocket>, remote: SocketAddr) -> Self {
        Self { socket, remote }
    }
    
    pub fn socket(&self) -> Arc<UdpSocket> {
        self.socket.clone()
    }
}

#[async_trait]
impl DataTransport for UdpTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e)))
    }
    
    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.remote)
    }
    
    async fn send_raw(&self, data: &[u8]) -> Result<()> {
        self.socket.send(data).await
            .map_err(|e| MediaError::TransportSend(format!("UDP send failed: {}", e)))?;
        Ok(())
    }
    
    async fn recv_raw(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 2048];
        let (len, _) = self.socket.recv_from(&mut buf).await
            .map_err(|e| MediaError::TransportRecv(format!("UDP recv failed: {}", e)))?;
        buf.truncate(len);
        Ok(buf)
    }
    
    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for UdpTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e)))
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.remote)
    }

    async fn send(&self, data: &[u8], _data_type: DataType) -> Result<()> {
        self.socket.send(data).await
            .map_err(|e| MediaError::TransportSend(format!("UDP send failed: {}", e)))?;
        Ok(())
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        let mut buf = vec![0u8; 2048];
        let (len, _) = self.socket.recv_from(&mut buf).await
            .map_err(|e| MediaError::TransportRecv(format!("UDP recv failed: {}", e)))?;
        buf.truncate(len);
        let mut result = HashMap::new();
        result.insert(DataType::OrgData, BytesMut::from(&buf[..len]));
        Ok(result)
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
