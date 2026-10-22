
use crate::media::errors::{MediaError, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait::async_trait]
pub trait RtpServerActive: Send + Sync {
    async fn touch_active(&self);
}

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
    pub active_at: std::time::Instant,
}

pub struct UDPRtpServer {
    rtp_socket: Arc<tokio::net::UdpSocket>,
    rtcp_socket: Arc<tokio::net::UdpSocket>,
    port: u16,
    ssrc: RwLock<Option<u32>>,
    device_tag: RwLock<Option<String>>,
    channel_tag: RwLock<Option<String>>,
    allocated_at: RwLock<Option<std::time::Instant>>,
    active_at: RwLock<Option<std::time::Instant>>,
}

impl UDPRtpServer {
    pub async fn new(bind_addr: &str, port: u16, port_range: &RtpPortRange) -> Result<Self> {
        if port + 1 > port_range.end {
            return Err(MediaError::Transport(format!(
                "RTP port {} + 1 超出范围 {}",
                port,
                port_range.end
            )));
        }

        let rtp_socket = tokio::net::UdpSocket::bind(format!("{}:{}", bind_addr, port))
            .await
            .map_err(|e| MediaError::Transport(format!("RTP bind {}:{} failed: {}", bind_addr, port, e)))?;

        let rtcp_socket = tokio::net::UdpSocket::bind(format!("{}:{}", bind_addr, port + 1))
            .await
            .map_err(|e| MediaError::Transport(format!("RTCP bind {}:{} failed: {}", bind_addr, port + 1, e)))?;

        Ok(Self {
            rtp_socket: Arc::new(rtp_socket),
            rtcp_socket: Arc::new(rtcp_socket),
            port,
            ssrc: RwLock::new(None),
            device_tag: RwLock::new(None),
            channel_tag: RwLock::new(None),
            allocated_at: RwLock::new(None),
            active_at: RwLock::new(None),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn rtcp_port(&self) -> u16 {
        self.port + 1
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
        {
            let mut ssrc_guard = self.ssrc.write().await;
            if ssrc_guard.is_some() {
                tracing::warn!(
                    "[UDPRtpServer] Port {} already allocated for SSRC {:?}",
                    self.port,
                    *ssrc_guard
                );
                return false;
            }
            *ssrc_guard = Some(ssrc);
        }
        *self.device_tag.write().await = Some(device_tag.clone());
        *self.channel_tag.write().await = Some(channel_tag.clone());

        let now = std::time::Instant::now();
        *self.allocated_at.write().await = Some(now);
        *self.active_at.write().await = Some(now);

        true
    }

    pub async fn release(&self) {
        let _ssrc = { self.ssrc.write().await.take() };
        let _tag = { self.device_tag.write().await.take() };
        self.channel_tag.write().await.take();
        self.allocated_at.write().await.take();
        self.active_at.write().await.take();
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
        let active_at = *self.active_at.read().await;

        match (ssrc, device_tag, allocated_at, active_at) {
            (Some(ssrc), Some(device_tag), Some(allocated_at), Some(active_at)) => {
                Some(RtpSession {
                    ssrc,
                    device_tag,
                    allocated_at,
                    active_at,
                })
            }
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl RtpServerActive for UDPRtpServer {
    async fn touch_active(&self) {
        *self.active_at.write().await = Some(std::time::Instant::now());
    }
}

#[derive(Debug, Clone)]
pub struct UdpRtpPacket {
    pub data: Vec<u8>,
    pub ssrc: u32,
    pub payload_type: u8,
}

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::TcpStream;
use tokio::sync::Mutex as TokioMutex;

pub enum TcpMode {
    Passive,
    Active,
}

pub struct TcpRtpServer {
    port: u16,
    mode: TcpMode,
    listener: Option<tokio::net::TcpListener>,
    socket: Arc<TokioMutex<Option<tokio::net::TcpSocket>>>,
    ssrc: RwLock<Option<u32>>,
    device_tag: RwLock<Option<String>>,
    channel_tag: RwLock<Option<String>>,
    allocated_at: RwLock<Option<std::time::Instant>>,
    active_at: RwLock<Option<std::time::Instant>>,
    shutdown: AtomicBool,
}

impl TcpRtpServer {
    pub async fn new_passive(bind_addr: &str, port: u16) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind(format!("{}:{}", bind_addr, port))
            .await
            .map_err(|e| MediaError::Transport(format!("TCP RTP bind {}:{} failed: {}", bind_addr, port, e)))?;

        Ok(Self {
            port,
            mode: TcpMode::Passive,
            listener: Some(listener),
            socket: Arc::new(TokioMutex::new(None)),
            ssrc: RwLock::new(None),
            device_tag: RwLock::new(None),
            channel_tag: RwLock::new(None),
            allocated_at: RwLock::new(None),
            active_at: RwLock::new(None),
            shutdown: AtomicBool::new(false),
        })
    }

    pub async fn new_active(bind_addr: &str, port: u16) -> Result<Self> {
        let socket = tokio::net::TcpSocket::new_v4()
            .map_err(|e| MediaError::Transport(format!("TCP socket create failed: {}", e)))?;

        let addr: std::net::SocketAddr = format!("{}:{}", bind_addr, port)
            .parse()
            .map_err(|e| MediaError::Transport(format!("Invalid address {}:{}: {}", bind_addr, port, e)))?;

        socket
            .bind(addr)
            .map_err(|e| MediaError::Transport(format!("TCP bind {} failed: {}", addr, e)))?;

        Ok(Self {
            port,
            mode: TcpMode::Active,
            listener: None,
            socket: Arc::new(TokioMutex::new(Some(socket))),
            ssrc: RwLock::new(None),
            device_tag: RwLock::new(None),
            channel_tag: RwLock::new(None),
            allocated_at: RwLock::new(None),
            active_at: RwLock::new(None),
            shutdown: AtomicBool::new(false),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn mode(&self) -> &TcpMode {
        &self.mode
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    pub async fn allocate(&self, ssrc: u32, device_tag: String, channel_tag: String) -> bool {
        {
            let mut ssrc_guard = self.ssrc.write().await;
            if ssrc_guard.is_some() {
                tracing::warn!(
                    "[TcpRtpServer] Port {} already allocated for SSRC {:?}",
                    self.port,
                    *ssrc_guard
                );
                return false;
            }
            *ssrc_guard = Some(ssrc);
        }
        *self.device_tag.write().await = Some(device_tag.clone());
        *self.channel_tag.write().await = Some(channel_tag.clone());

        let now = std::time::Instant::now();
        *self.allocated_at.write().await = Some(now);
        *self.active_at.write().await = Some(now);

        true
    }

    pub async fn release(&self) {
        let ssrc = { self.ssrc.write().await.take() };
        let _tag = { self.device_tag.write().await.take() };
        self.channel_tag.write().await.take();
        self.allocated_at.write().await.take();
        self.active_at.write().await.take();

        if let Some(ssrc) = ssrc {
        }
    }

    pub async fn get_session(&self) -> Option<RtpSession> {
        let ssrc = *self.ssrc.read().await;
        let device_tag = self.device_tag.read().await.clone();
        let allocated_at = *self.allocated_at.read().await;
        let active_at = *self.active_at.read().await;

        match (ssrc, device_tag, allocated_at, active_at) {
            (Some(ssrc), Some(device_tag), Some(allocated_at), Some(active_at)) => {
                Some(RtpSession {
                    ssrc,
                    device_tag,
                    allocated_at,
                    active_at,
                })
            }
            _ => None,
        }
    }

    pub async fn accept(&self) -> std::io::Result<(TcpStream, std::net::SocketAddr)> {
        match &self.listener {
            Some(listener) => listener.accept().await,
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Not in passive mode",
            )),
        }
    }

    pub async fn connect(&self, remote: std::net::SocketAddr) -> std::io::Result<TcpStream> {
        let socket = {
            let mut guard = self.socket.lock().await;
            guard.take()
        };
        match socket {
            Some(socket) => socket.connect(remote).await,
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Socket already used or not in active mode",
            )),
        }
    }
}

#[async_trait::async_trait]
impl RtpServerActive for TcpRtpServer {
    async fn touch_active(&self) {
        *self.active_at.write().await = Some(std::time::Instant::now());
    }
}