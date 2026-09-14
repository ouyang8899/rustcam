use crate::media::errors::{MediaError, Result};
use crate::media::node::{DataType, MediaNode, MediaTransportCallback};
use crate::media::rtp_server::TcpRtpServer;
use async_trait::async_trait;
use bytes::{BufMut, BytesMut};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

pub enum BroadcastMode {
    Udp {
        socket: Arc<UdpSocket>,
        remote: SocketAddr,
    },
    TcpPassive {
        server: Arc<TcpRtpServer>,
        stream: Arc<tokio::sync::Mutex<Option<TcpStream>>>,
    },
    TcpActive {
        server: Arc<TcpRtpServer>,
        stream: Arc<tokio::sync::Mutex<Option<TcpStream>>>,
        remote: SocketAddr,
    },
}

pub struct GB28181BroadcastVoiceTransport {
    mode: BroadcastMode,
    ssrc: u32,
    seq_num: AtomicU16,
    timestamp: AtomicU32,
    payload_type: u8,
}

impl GB28181BroadcastVoiceTransport {
    pub fn new_udp(
        socket: Arc<UdpSocket>,
        remote: SocketAddr,
        ssrc: u32,
        payload_type: u8,
    ) -> Self {
        Self {
            mode: BroadcastMode::Udp { socket, remote },
            ssrc,
            seq_num: AtomicU16::new(0),
            timestamp: AtomicU32::new(0),
            payload_type,
        }
    }

    pub fn new_tcp_passive(
        server: Arc<TcpRtpServer>,
        ssrc: u32,
        payload_type: u8,
    ) -> Self {
        tracing::info!("[GB28181Broadcast] TCP Passive created on port {}", server.port());
        Self {
            mode: BroadcastMode::TcpPassive {
                server,
                stream: Arc::new(tokio::sync::Mutex::new(None)),
            },
            ssrc,
            seq_num: AtomicU16::new(0),
            timestamp: AtomicU32::new(0),
            payload_type,
        }
    }

    pub fn new_tcp_active(
        server: Arc<TcpRtpServer>,
        remote: SocketAddr,
        ssrc: u32,
        payload_type: u8,
    ) -> Self {
        tracing::info!("[GB28181Broadcast] TCP Active created (connect pending) to {:?}", remote);
        Self {
            mode: BroadcastMode::TcpActive {
                server,
                stream: Arc::new(tokio::sync::Mutex::new(None)),
                remote,
            },
            ssrc,
            seq_num: AtomicU16::new(0),
            timestamp: AtomicU32::new(0),
            payload_type,
        }
    }
    
    pub async fn connect(&self) -> Result<()> {
        match &self.mode {
            BroadcastMode::TcpActive { server, stream, remote } => {
                if stream.lock().await.is_some() {
                    return Ok(());
                }
                let client_stream = server
                    .connect(*remote)
                    .await
                    .map_err(|e| MediaError::Transport(format!("TCP connect failed: {}", e)))?;
                let mut guard = stream.lock().await;
                *guard = Some(client_stream);
                tracing::info!("[GB28181Broadcast] TCP Active connected to {:?}", remote);
                Ok(())
            }
            _ => Err(MediaError::Transport("Not in TCP active mode".into())),
        }
    }

    fn build_rtp_header(&self) -> Vec<u8> {
        let seq = self.seq_num.fetch_add(1, Ordering::Relaxed);
        let ts = self.timestamp.fetch_add(160, Ordering::Relaxed);
        vec![
            0x80,
            0x80,
            (seq >> 8) as u8,
            seq as u8,
            (ts >> 24) as u8,
            (ts >> 16) as u8,
            (ts >> 8) as u8,
            ts as u8,
            (self.ssrc >> 24) as u8,
            (self.ssrc >> 16) as u8,
            (self.ssrc >> 8) as u8,
            self.ssrc as u8,
        ]
    }

    pub fn activate(self: Arc<Self>, node: Arc<dyn MediaNode>) {
        tokio::spawn(async move {
            loop {
                let data = match &self.mode {
                    BroadcastMode::Udp { socket, .. } => {
                        let mut buf = vec![0u8; 2048];
                        match socket.recv_from(&mut buf).await {
                            Ok((len, _)) => {
                                buf.truncate(len);
                                BytesMut::from(&buf[..])
                            }
                            Err(e) => {
                                tracing::warn!("[GB28181Broadcast] UDP recv error: {}", e);
                                break;
                            }
                        }
                    }
                    BroadcastMode::TcpPassive { stream, .. } | BroadcastMode::TcpActive { stream, .. } => {
                        match self.recv_tcp_frame(stream.clone()).await {
                            Ok(data) => data,
                            Err(e) => {
                                tracing::warn!("[GB28181Broadcast] TCP recv error: {}", e);
                                break;
                            }
                        }
                    }
                };

                let mut result = HashMap::new();
                let data_type = Self::classify_rtp_packet(&data);
                result.insert(data_type, data);
                node.on_media_data(result).await;
            }
        });
    }

    async fn recv_tcp_frame(&self, stream: Arc<tokio::sync::Mutex<Option<TcpStream>>>) -> Result<BytesMut> {
        let mut guard = stream.lock().await;
        let stream = guard.as_mut()
            .ok_or_else(|| MediaError::Transport("TCP stream not connected".into()))?;
        
        let mut header = [0u8; 2];
        stream
            .read_exact(&mut header)
            .await
            .map_err(|e| MediaError::TransportRecv(format!("TCP read header failed: {}", e)))?;
        let len = u16::from_be_bytes(header) as usize;
        if len > 2048 {
            return Err(MediaError::TransportRecv("Frame too large".into()));
        }
        let mut data = vec![0u8; len];
        stream
            .read_exact(&mut data)
            .await
            .map_err(|e| MediaError::TransportRecv(format!("TCP read data failed: {}", e)))?;
        Ok(BytesMut::from(&data[..]))
    }
    
    pub async fn accept(&self) -> Result<SocketAddr> {
        match &self.mode {
            BroadcastMode::TcpPassive { server, stream } => {
                let (client_stream, addr) = server
                    .accept()
                    .await
                    .map_err(|e| MediaError::Transport(format!("TCP accept failed: {}", e)))?;
                let mut guard = stream.lock().await;
                *guard = Some(client_stream);
                Ok(addr)
            }
            _ => Err(MediaError::Transport("Not in TCP passive mode".into())),
        }
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for GB28181BroadcastVoiceTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        match &self.mode {
            BroadcastMode::Udp { socket, .. } => socket
                .local_addr()
                .map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e))),
            BroadcastMode::TcpPassive { stream, .. } => {
                let guard = stream.blocking_lock();
                if let Some(s) = guard.as_ref() {
                    s.local_addr().map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e)))
                } else {
                    Err(MediaError::Transport("TCP not connected".into()))
                }
            }
            BroadcastMode::TcpActive { stream, .. } => {
                let guard = stream.blocking_lock();
                if let Some(s) = guard.as_ref() {
                    s.local_addr().map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e)))
                } else {
                    Err(MediaError::Transport("TCP not connected".into()))
                }
            }
        }
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        match &self.mode {
            BroadcastMode::Udp { remote, .. } => Ok(*remote),
            BroadcastMode::TcpPassive { stream, .. } => {
                let guard = stream.blocking_lock();
                if let Some(s) = guard.as_ref() {
                    s.peer_addr().map_err(|e| MediaError::Transport(format!("Failed to get peer addr: {}", e)))
                } else {
                    Err(MediaError::Transport("TCP not connected".into()))
                }
            }
            BroadcastMode::TcpActive { stream, .. } => {
                let guard = stream.blocking_lock();
                if let Some(s) = guard.as_ref() {
                    s.peer_addr().map_err(|e| MediaError::Transport(format!("Failed to get peer addr: {}", e)))
                } else {
                    Err(MediaError::Transport("TCP not connected".into()))
                }
            }
        }
    }

    async fn send(&self, data: &[u8], _data_type: DataType) -> Result<()> {
        let header = self.build_rtp_header();
        let payload_len = 12 + data.len();

        match &self.mode {
            BroadcastMode::Udp { socket, remote } => {
                let mut packet = Vec::with_capacity(12 + data.len());
                packet.extend_from_slice(&header);
                packet.extend_from_slice(data);
                socket
                    .send_to(&packet, remote)
                    .await
                    .map_err(|e| MediaError::TransportSend(format!("UDP send failed: {}", e)))?;
            }
            BroadcastMode::TcpPassive { stream, .. } | BroadcastMode::TcpActive { stream, .. } => {
                let mut guard = stream.lock().await;
                let stream = guard.as_mut()
                    .ok_or_else(|| MediaError::Transport("TCP not connected".into()))?;
                let mut frame = Vec::with_capacity(2 + payload_len);
                frame.put_u16(payload_len as u16);
                frame.extend_from_slice(&header);
                frame.extend_from_slice(data);
                stream
                    .write_all(&frame)
                    .await
                    .map_err(|e| MediaError::TransportSend(format!("TCP send failed: {}", e)))?;
            }
        }
        Ok(())
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        let data = match &self.mode {
            BroadcastMode::Udp { socket, .. } => {
                let mut buf = vec![0u8; 2048];
                let (len, _) = socket
                    .recv_from(&mut buf)
                    .await
                    .map_err(|e| MediaError::TransportRecv(format!("UDP recv failed: {}", e)))?;
                buf.truncate(len);
                BytesMut::from(&buf[..len])
            }
            BroadcastMode::TcpPassive { stream, .. } | BroadcastMode::TcpActive { stream, .. } => {
                let mut guard = stream.lock().await;
                let stream = guard.as_mut()
                    .ok_or_else(|| MediaError::Transport("TCP not connected".into()))?;
                let mut header = [0u8; 2];
                stream.read_exact(&mut header).await.map_err(|e| {
                    MediaError::TransportRecv(format!("TCP read header failed: {}", e))
                })?;
                let len = u16::from_be_bytes(header) as usize;
                if len > 2048 {
                    return Err(MediaError::TransportRecv("Frame too large".into()));
                }
                let mut data = vec![0u8; len];
                stream.read_exact(&mut data).await.map_err(|e| {
                    MediaError::TransportRecv(format!("TCP read data failed: {}", e))
                })?;
                BytesMut::from(&data[..])
            }
        };

        let mut result = HashMap::new();
        let data_type = Self::classify_rtp_packet(&data);
        result.insert(data_type, data);
        Ok(result)
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

impl GB28181BroadcastVoiceTransport {
    fn classify_rtp_packet(data: &[u8]) -> DataType {
        if data.len() < 2 {
            return DataType::Audio;
        }

        let first = data[0];
        let second = data[1];
        let version = (first >> 6) & 0x03;
        let payload_type = second & 0x7F;

        if version != 2 {
            return DataType::Audio;
        }

        if payload_type >= 200 && payload_type <= 204 {
            return DataType::Rtcp;
        }

        match payload_type {
            0 | 8 => DataType::Audio,
            98 | 99 | 104 | 105 | 106 | 107 | 108 | 109 | 110 | 111 | 112 => DataType::Video,
            _ => DataType::Audio,
        }
    }
}
