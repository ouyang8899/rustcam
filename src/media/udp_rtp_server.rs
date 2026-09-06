use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use bytes::BytesMut;
use crate::media::errors::{MediaError, Result};
use crate::media::rtp::RtpHeader;

#[derive(Debug, Clone)]
pub struct RtpPacket {
    pub data: Vec<u8>,
    pub ssrc: u32,
    pub payload_type: u8,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub ssrc: u32,
    pub device_tag: String,
    pub allocated_at: std::time::Instant,
}

pub struct UDPRtpServer {
    rtp_socket: Arc<UdpSocket>,
    rtcp_socket: Arc<UdpSocket>,
    port: u16,
    sessions: RwLock<HashMap<u32, Session>>,
}

impl UDPRtpServer {
    pub async fn new(port: u16) -> Result<Self> {
        let rtp_socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).await
            .map_err(|e| MediaError::Transport(format!("RTP bind failed: {}", e)))?;
        let rtcp_socket = UdpSocket::bind(format!("0.0.0.0:{}", port + 1)).await
            .map_err(|e| MediaError::Transport(format!("RTCP bind failed: {}", e)))?;
        
        tracing::info!("[UDPRtpServer] Created on RTP port {} / RTCP port {}", port, port + 1);
        
        Ok(Self {
            rtp_socket: Arc::new(rtp_socket),
            rtcp_socket: Arc::new(rtcp_socket),
            port,
            sessions: RwLock::new(HashMap::new()),
        })
    }
    
    pub fn port(&self) -> u16 {
        self.port
    }
    
    pub async fn get_session(&self, ssrc: u32) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(&ssrc).cloned()
    }
    
    pub async fn allocate(&self, ssrc: u32, device_tag: String) -> bool {
        let mut sessions = self.sessions.write().await;
        if sessions.contains_key(&ssrc) {
            tracing::warn!("[UDPRtpServer] SSRC {} already allocated on port {}", ssrc, self.port);
            return false;
        }
        sessions.insert(ssrc, Session {
            ssrc,
            device_tag: device_tag.clone(),
            allocated_at: std::time::Instant::now(),
        });
        tracing::info!("[UDPRtpServer] Port {} allocated for SSRC {} / device {}", self.port, ssrc, device_tag);
        true
    }
    
    pub async fn release(&self, ssrc: u32) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(&ssrc) {
            tracing::info!("[UDPRtpServer] Port {} released from SSRC {} / device {}", self.port, ssrc, session.device_tag);
        }
    }
    
    pub async fn get_ssrc_count(&self) -> usize {
        self.sessions.read().await.len()
    }
    
    pub async fn start_recv_loop(
        self: Arc<Self>,
        ssrc_map: Arc<RwLock<HashMap<u32, mpsc::Sender<RtpPacket>>>>,
    ) {
        let socket = self.rtp_socket.clone();
        let port = self.port;
        
        tokio::spawn(async move {
            let mut buffer = vec![0u8; 2048];
            loop {
                match socket.recv_from(&mut buffer).await {
                    Ok((len, addr)) => {
                        let data = &buffer[..len];
                        
                        if let Ok(header) = RtpHeader::parse(data) {
                            let ssrc = header.ssrc;
                            
                            let map = ssrc_map.read().await;
                            if let Some(sender) = map.get(&ssrc) {
                                let packet = RtpPacket {
                                    data: data.to_vec(),
                                    ssrc,
                                    payload_type: header.payload_type,
                                };
                                if sender.send(packet).await.is_err() {
                                    tracing::debug!("[UDPRtpServer] Failed to send RTP packet for SSRC {} on port {}", ssrc, port);
                                }
                            } else {
                                tracing::debug!("[UDPRtpServer] No handler for SSRC {} on port {} from {}", ssrc, port, addr);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("[UDPRtpServer] Recv error on port {}: {}", port, e);
                    }
                }
            }
        });
        
        tracing::info!("[UDPRtpServer] Started recv loop on port {}", port);
    }
}

#[derive(Debug, Clone)]
pub struct UdpRtpPortRange {
    pub start: u16,
    pub end: u16,
}

impl UdpRtpPortRange {
    pub fn new(start: u16, end: u16) -> Self {
        Self { start, end }
    }
}

pub struct UDPRtpServerManager {
    servers: RwLock<HashMap<u16, Arc<UDPRtpServer>>>,
    idle_ports: RwLock<HashMap<u16, Arc<UDPRtpServer>>>,
    ssrc_to_port: RwLock<HashMap<u32, u16>>,
    port_to_ssrc: RwLock<HashMap<u16, u32>>,
    ssrc_to_sender: Arc<RwLock<HashMap<u32, mpsc::Sender<RtpPacket>>>>,
    timeout_secs: u64,
}

impl UDPRtpServerManager {
    pub fn new() -> Self {
        Self::with_timeout(30)
    }

    pub fn with_timeout(timeout_secs: u64) -> Self {
        tracing::info!("[UDPRtpServerManager] Created with timeout {}s", timeout_secs);
        Self {
            servers: RwLock::new(HashMap::new()),
            idle_ports: RwLock::new(HashMap::new()),
            ssrc_to_port: RwLock::new(HashMap::new()),
            port_to_ssrc: RwLock::new(HashMap::new()),
            ssrc_to_sender: Arc::new(RwLock::new(HashMap::new())),
            timeout_secs,
        }
    }

    pub fn with_default_timeout() -> Self {
        Self::with_timeout(30)
    }
    
    pub async fn init(&self, range: UdpRtpPortRange) -> Result<()> {
        tracing::info!("[UDPRtpServerManager] Initializing with port range {}-{}", range.start, range.end);
        
        let mut port = range.start;
        while port + 1 <= range.end {
            match UDPRtpServer::new(port).await {
                Ok(server) => {
                    let server = Arc::new(server);
                    let server_clone = server.clone();
                    
                    self.servers.write().await.insert(port, server.clone());
                    self.idle_ports.write().await.insert(port, server.clone());
                    
                    let ssrc_map = self.ssrc_to_sender.clone();
                    server_clone.start_recv_loop(ssrc_map).await;
                }
                Err(e) => {
                    tracing::warn!("[UDPRtpServerManager] Failed to create server on port {}: {}", port, e);
                }
            }
            port += 2;
        }
        
        let count = self.servers.read().await.len();
        tracing::info!("[UDPRtpServerManager] Initialized with {} servers", count);
        Ok(())
    }
    
    pub async fn allocate(&self, ssrc: u32, device_tag: &str) -> Result<(u16, mpsc::Receiver<RtpPacket>)> {
        let (tx, rx) = mpsc::channel(100);
        
        let port = {
            let mut idle = self.idle_ports.write().await;
            if let Some((port, server)) = idle.iter().next().map(|(k, v)| (*k, v.clone())) {
                idle.remove(&port);
                drop(idle);
                
                server.allocate(ssrc, device_tag.to_string()).await;
                
                self.ssrc_to_port.write().await.insert(ssrc, port);
                self.port_to_ssrc.write().await.insert(port, ssrc);
                self.ssrc_to_sender.write().await.insert(ssrc, tx);
                
                tracing::info!("[UDPRtpServerManager] Allocated port {} for SSRC {} / device {}", port, ssrc, device_tag);
                
                return Ok((port, rx));
            }
            None
        };
        
        port.ok_or_else(|| MediaError::PortAllocation("No available ports".into()))
    }
    
    pub async fn release(&self, ssrc: u32) {
        let port = {
            let mut ssrc_to_port = self.ssrc_to_port.write().await;
            ssrc_to_port.remove(&ssrc)
        };
        
        if let Some(port) = port {
            self.port_to_ssrc.write().await.remove(&port);
            self.ssrc_to_sender.write().await.remove(&ssrc);
            
            let server_opt = {
                let servers = self.servers.read().await;
                servers.get(&port).cloned()
            };
            
            if let Some(server) = server_opt {
                server.release(ssrc).await;
                tracing::info!("[UDPRtpServerManager] Released SSRC {} on port {}", ssrc, port);
            }
        }
    }

    pub async fn cleanup_timed_out(&self) {
        let timeout = std::time::Duration::from_secs(self.timeout_secs);
        let now = std::time::Instant::now();
        
        let timed_out_ssrcs: Vec<(u32, u16)> = {
            let servers = self.servers.read().await;
            let mut timed_out = Vec::new();
            
            for (port, server) in servers.iter() {
                let sessions = server.sessions.read().await;
                for (ssrc, session) in sessions.iter() {
                    if now.duration_since(session.allocated_at) > timeout {
                        timed_out.push((*ssrc, *port));
                    }
                }
            }
            timed_out
        };
        
        for (ssrc, port) in timed_out_ssrcs {
            tracing::warn!("[UDPRtpServerManager] Timeout cleanup for SSRC {} on port {}", ssrc, port);
            self.release(ssrc).await;
        }
    }
    
    pub async fn get_port_by_ssrc(&self, ssrc: u32) -> Option<u16> {
        let map = self.ssrc_to_port.read().await;
        map.get(&ssrc).copied()
    }

    pub async fn get_server(&self, port: u16) -> Option<Arc<UDPRtpServer>> {
        let servers = self.servers.read().await;
        servers.get(&port).cloned()
    }

    pub async fn get_rtp_socket(&self, ssrc: u32) -> Option<Arc<UdpSocket>> {
        if let Some(port) = self.get_port_by_ssrc(ssrc).await {
            if let Some(server) = self.get_server(port).await {
                return Some(server.rtp_socket.clone());
            }
        }
        None
    }
    
    pub async fn count_idle(&self) -> usize {
        self.idle_ports.read().await.len()
    }
    
    pub async fn count_in_use(&self) -> usize {
        let servers = self.servers.read().await;
        let idle = self.idle_ports.read().await;
        servers.len() - idle.len()
    }
}

impl Default for UDPRtpServerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_manager() {
        let manager = UDPRtpServerManager::new();
        
        let range = UdpRtpPortRange::new(16000, 16010);
        manager.init(range).await.unwrap();
        
        assert_eq!(manager.count_idle().await, 6);
        
        let (port, _rx) = manager.allocate(1001, "device1").await.unwrap();
        assert_eq!(manager.count_idle().await, 5);
        assert_eq!(manager.count_in_use().await, 1);
        assert_eq!(port, 16000);
        
        manager.release(1001).await;
        assert_eq!(manager.count_idle().await, 6);
    }
}
