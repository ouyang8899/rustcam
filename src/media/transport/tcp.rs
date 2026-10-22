use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::io::{ AsyncReadExt, AsyncWriteExt };
use tokio::sync::Mutex;
use async_trait::async_trait;
use bytes::{BytesMut, BufMut};
use std::collections::HashMap;
use crate::media::errors::{MediaError, Result};
use crate::media::node::DataType;
use super::DataTransport;

const MAX_FRAME_SIZE: usize = 65536;

pub struct TcpTransport {
    stream: Arc<Mutex<TcpStream>>,
    remote: SocketAddr,
}

impl TcpTransport {
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        let stream = TcpStream::connect(addr).await
            .map_err(|e| MediaError::Transport(format!("TCP connect failed: {}", e)))?;
        let remote = stream.peer_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get peer addr: {}", e)))?;
        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            remote,
        })
    }
    
    pub fn with_stream(stream: TcpStream) -> Result<Self> {
        let remote = stream.peer_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get peer addr: {}", e)))?;
        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            remote,
        })
    }
    
    pub fn stream(&self) -> Arc<Mutex<TcpStream>> {
        self.stream.clone()
    }
}

#[async_trait]
impl DataTransport for TcpTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        self.stream.blocking_lock().local_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e)))
    }
    
    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.remote)
    }
    
    async fn send_raw(&self, data: &[u8]) -> Result<()> {
        let mut stream = self.stream.lock().await;
        stream.write_all(data).await
            .map_err(|e| MediaError::TransportSend(format!("TCP send failed: {}", e)))?;
        Ok(())
    }
    
    async fn recv_raw(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 4096];
        let mut stream = self.stream.lock().await;
        let n = stream.read(&mut buf).await
            .map_err(|e| MediaError::TransportRecv(format!("TCP recv failed: {}", e)))?;
        drop(stream);
        buf.truncate(n);
        Ok(buf)
    }
    
    async fn close(&self) -> Result<()> {
        if let Ok(mut stream) = self.stream.try_lock() {
            let _ = stream.shutdown().await;
        }
        Ok(())
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for TcpTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        self.stream.blocking_lock().local_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e)))
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.remote)
    }

    async fn send(&self, data: &[u8], _data_type: DataType) -> Result<()> {
        let mut stream = self.stream.lock().await;
        stream.write_all(data).await
            .map_err(|e| MediaError::TransportSend(format!("TCP send failed: {}", e)))?;
        Ok(())
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        let mut buf = vec![0u8; 4096];
        let mut stream = self.stream.lock().await;
        let n = stream.read(&mut buf).await
            .map_err(|e| MediaError::TransportRecv(format!("TCP recv failed: {}", e)))?;
        drop(stream);
        buf.truncate(n);
        let mut result = HashMap::new();
        result.insert(DataType::OrgData, BytesMut::from(&buf[..n]));
        Ok(result)
    }

    async fn close(&self) -> Result<()> {
        if let Ok(mut stream) = self.stream.try_lock() {
            let _ = stream.shutdown().await;
        }
        Ok(())
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for Gb28181TcpTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        self.stream.blocking_lock().local_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e)))
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.remote)
    }

    async fn send(&self, data: &[u8], data_type: DataType) -> Result<()> {
        match data_type {
            DataType::Rtcp => {
                self.send_raw(data).await
            }
            _ => {
                self.send_raw(data).await
            }
        }
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        let data = self.recv_raw().await?;
        let mut result = HashMap::new();
        result.insert(DataType::OrgData, BytesMut::from(data.as_slice()));
        Ok(result)
    }

    async fn close(&self) -> Result<()> {
        if let Ok(mut stream) = self.stream.try_lock() {
            let _ = stream.shutdown().await;
        }
        Ok(())
    }
}

pub struct Gb28181TcpTransport {
    stream: Arc<Mutex<TcpStream>>,
    remote: SocketAddr,
    recv_buf: Arc<Mutex<BytesMut>>,
}

impl Gb28181TcpTransport {
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        let stream = TcpStream::connect(addr).await
            .map_err(|e| MediaError::Transport(format!("TCP connect failed: {}", e)))?;
        let remote = stream.peer_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get peer addr: {}", e)))?;
        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            remote,
            recv_buf: Arc::new(Mutex::new(BytesMut::with_capacity(4096))),
        })
    }
    
    pub fn with_stream(stream: TcpStream) -> Result<Self> {
        let remote = stream.peer_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get peer addr: {}", e)))?;
        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            remote,
            recv_buf: Arc::new(Mutex::new(BytesMut::with_capacity(4096))),
        })
    }
    
    pub fn stream(&self) -> Arc<Mutex<TcpStream>> {
        self.stream.clone()
    }
}

#[async_trait]
impl DataTransport for Gb28181TcpTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        self.stream.blocking_lock().local_addr()
            .map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e)))
    }
    
    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.remote)
    }
    
    async fn send_raw(&self, data: &[u8]) -> Result<()> {
        let mut stream = self.stream.lock().await;
        let len = data.len() as u16;
        let mut frame = BytesMut::with_capacity(2 + data.len());
        frame.put_u16(len);
        frame.extend_from_slice(data);
        stream.write_all(&frame).await
            .map_err(|e| MediaError::TransportSend(format!("GB28181 TCP send failed: {}", e)))?;
        Ok(())
    }
    
    async fn recv_raw(&self) -> Result<Vec<u8>> {
        loop {
            let need_more = {
                let mut buf = self.recv_buf.lock().await;
                if buf.len() >= 2 {
                    let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
                    
                    if len > MAX_FRAME_SIZE {
                        return Err(MediaError::TransportRecv(format!(
                            "GB28181 TCP frame too large: {} bytes", len
                        )));
                    }
                    
                    let total_len = 2 + len;
                    if buf.len() >= total_len {
                        let payload = buf[2..total_len].to_vec();
                        let remaining = buf[total_len..].to_vec();
                        buf.clear();
                        buf.extend_from_slice(&remaining);
                        return Ok(payload);
                    }
                }
                buf.len() < 2
            };
            
            if need_more {
                let mut buf = vec![0u8; 2048];
                let mut stream = self.stream.lock().await;
                let n = stream.read(&mut buf).await
                    .map_err(|e| MediaError::TransportRecv(format!("GB28181 TCP recv failed: {}", e)))?;
                if n == 0 {
                    return Err(MediaError::TransportRecv("Connection closed".to_string()));
                }
                let mut recv_buf = self.recv_buf.lock().await;
                recv_buf.extend_from_slice(&buf[..n]);
            }
        }
    }
    
    async fn close(&self) -> Result<()> {
        if let Ok(mut stream) = self.stream.try_lock() {
            let _ = stream.shutdown().await;
        }
        Ok(())
    }
}
