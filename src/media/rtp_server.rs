use std::sync::Arc;
use tokio::sync::RwLock;
use crate::media::errors::{MediaError, Result};

#[derive(Debug, Clone)]
pub struct RtpPortRange {
    pub start: u16,
    pub end: u16,
}

impl RtpPortRange {
    pub fn new(start: u16, end: u16) -> Self {
        Self { start, end }
    }
    
    pub fn contains(&self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }
}

#[derive(Debug, Clone)]
pub struct RtpSession {
    pub ssrc: u32,
    pub device_tag: String,
    pub allocated_at: std::time::Instant,
}

pub struct UDPRtpServer {
    rtp_socket: Arc<tokio::net::UdpSocket>,
    rtcp_socket: Arc<tokio::net::UdpSocket>,
    port: u16,
    ssrc: RwLock<Option<u32>>,
    device_tag: RwLock<Option<String>>,
    channel_tag: RwLock<Option<String>>,
    allocated_at: RwLock<Option<std::time::Instant>>,
}

impl UDPRtpServer {
    pub async fn new(port: u16) -> Result<Self> {
        let rtp_socket = tokio::net::UdpSocket::bind(format!("0.0.0.0:{}", port)).await
            .map_err(|e| MediaError::Transport(format!("RTP bind failed: {}", e)))?;
        let rtcp_socket = tokio::net::UdpSocket::bind(format!("0.0.0.0:{}", port + 1)).await
            .map_err(|e| MediaError::Transport(format!("RTCP bind failed: {}", e)))?;
        
        tracing::info!("[UDPRtpServer] Created on RTP port {} / RTCP port {}", port, port + 1);
        
        Ok(Self {
            rtp_socket: Arc::new(rtp_socket),
            rtcp_socket: Arc::new(rtcp_socket),
            port,
            ssrc: RwLock::new(None),
            device_tag: RwLock::new(None),
            channel_tag: RwLock::new(None),
            allocated_at: RwLock::new(None),
        })
    }
    
    pub fn port(&self) -> u16 {
        self.port
    }
    
    pub fn rtp_socket(&self) -> Arc<tokio::net::UdpSocket> {
        self.rtp_socket.clone()
    }
    
    pub fn rtcp_socket(&self) -> Arc<tokio::net::UdpSocket> {
        self.rtcp_socket.clone()
    }
    
    pub async fn get_ssrc(&self) -> Option<u32> {
        *self.ssrc.read().await
    }
    
    pub async fn allocate(&self, ssrc: u32, device_tag: String, channel_tag: String) -> bool {
        let mut ssrc_guard = self.ssrc.write().await;
        if ssrc_guard.is_some() {
            tracing::warn!("[UDPRtpServer] Port {} already allocated for SSRC {:?}", self.port, *ssrc_guard);
            return false;
        }
        *ssrc_guard = Some(ssrc);
        drop(ssrc_guard);
        
        let mut tag_guard = self.device_tag.write().await;
        *tag_guard = Some(device_tag.clone());
        drop(tag_guard);
        
        let mut channel_guard = self.channel_tag.write().await;
        *channel_guard = Some(channel_tag.clone());
        drop(channel_guard);
        
        let mut time_guard = self.allocated_at.write().await;
        *time_guard = Some(std::time::Instant::now());
        
        tracing::info!("[UDPRtpServer] Port {} allocated for SSRC {} / {}/{}", self.port, ssrc, device_tag, channel_tag);
        true
    }
    
    pub async fn release(&self) {
        let ssrc = {
            let mut guard = self.ssrc.write().await;
            guard.take()
        };
        if let Some(ssrc) = ssrc {
            let tag = {
                let mut guard = self.device_tag.write().await;
                guard.take()
            };
            let mut time_guard = self.allocated_at.write().await;
            time_guard.take();
            tracing::info!("[UDPRtpServer] Port {} released from SSRC {} / device {}", self.port, ssrc, tag.unwrap_or_default());
        }
    }
    
    pub fn is_allocated_sync(&self) -> bool {
        self.ssrc.try_read().map(|g| g.is_some()).unwrap_or(false)
    }
    
    pub async fn is_allocated(&self) -> bool {
        self.ssrc.read().await.is_some()
    }
    
    pub async fn get_session(&self) -> Option<RtpSession> {
        let ssrc = *self.ssrc.read().await;
        let device_tag = self.device_tag.read().await.clone();
        let allocated_at = *self.allocated_at.read().await;
        
        match (ssrc, device_tag, allocated_at) {
            (Some(ssrc), Some(device_tag), Some(allocated_at)) => {
                Some(RtpSession { ssrc, device_tag, allocated_at })
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct UdpRtpPacket {
    pub data: Vec<u8>,
    pub ssrc: u32,
    pub payload_type: u8,
}

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

pub struct TcpRtpServer {
    port: u16,
    listener: tokio::net::TcpListener,
    ssrc: RwLock<Option<u32>>,
    device_tag: RwLock<Option<String>>,
    channel_tag: RwLock<Option<String>>,
    allocated_at: RwLock<Option<std::time::Instant>>,
    shutdown: AtomicBool,
    accepted_stream: RwLock<Option<(TcpStream, std::net::SocketAddr)>>,
}

impl TcpRtpServer {
    pub async fn new(port: u16) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await
            .map_err(|e| MediaError::Transport(format!("TCP RTP bind failed: {}", e)))?;
        
        tracing::info!("[TcpRtpServer] Created on port {}", port);
        
        Ok(Self {
            port,
            listener,
            ssrc: RwLock::new(None),
            device_tag: RwLock::new(None),
            channel_tag: RwLock::new(None),
            allocated_at: RwLock::new(None),
            shutdown: AtomicBool::new(false),
            accepted_stream: RwLock::new(None),
        })
    }
    
    pub fn port(&self) -> u16 {
        self.port
    }
    
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }
    
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
   
    pub async fn allocate(&self, ssrc: u32, device_tag: String, channel_tag: String) -> bool {
        let mut ssrc_guard = self.ssrc.write().await;
        if ssrc_guard.is_some() {
            tracing::warn!("[TcpRtpServer] Port {} already allocated for SSRC {:?}", self.port, *ssrc_guard);
            return false;
        }
        *ssrc_guard = Some(ssrc);
        drop(ssrc_guard);
        
        let mut tag_guard = self.device_tag.write().await;
        *tag_guard = Some(device_tag.clone());
        drop(tag_guard);
        
        let mut channel_guard = self.channel_tag.write().await;
        *channel_guard = Some(channel_tag.clone());
        drop(channel_guard);
        
        let mut time_guard = self.allocated_at.write().await;
        *time_guard = Some(std::time::Instant::now());
        
        tracing::info!("[TcpRtpServer] Port {} allocated for SSRC {} / device {}", self.port, ssrc, device_tag);
        true
    }
    
    pub async fn release(&self) {
        let ssrc = {
            let mut guard = self.ssrc.write().await;
            guard.take()
        };
        if let Some(ssrc) = ssrc {
            let tag = {
                let mut guard = self.device_tag.write().await;
                guard.take()
            };
            tracing::info!("[TcpRtpServer] Port {} released from SSRC {} / device {}", self.port, ssrc, tag.unwrap_or_default());
        }
    }
    
    pub async fn get_session(&self) -> Option<RtpSession> {
        let ssrc = *self.ssrc.read().await;
        let device_tag = self.device_tag.read().await.clone();
        let allocated_at = *self.allocated_at.read().await;
        
        match (ssrc, device_tag, allocated_at) {
            (Some(ssrc), Some(device_tag), Some(allocated_at)) => {
                Some(RtpSession { ssrc, device_tag, allocated_at })
            }
            _ => None,
        }
    }
    
    pub async fn accept(&self) -> std::io::Result<(TcpStream, std::net::SocketAddr)> {
        self.listener.accept().await
    }
    
    pub async fn take_accepted_stream(&self) -> Option<(TcpStream, std::net::SocketAddr)> {
        self.listener.accept().await.ok()
    }
}
