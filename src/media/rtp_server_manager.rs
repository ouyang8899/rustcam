use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use dashmap::DashMap;
use tokio::sync::RwLock;
use once_cell::sync::OnceCell;
use crate::media::errors::{MediaError, Result};
use crate::media::node::RtpPacketCallback;
use crate::media::rtp_server::{RtpPortRange, UDPRtpServer, TcpRtpServer};
use crate::media::transport::gb28181_broadcast::GB28181BroadcastVoiceTransport;
use crate::protocol::event::TransportType;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub protocol: TransportType,
    pub device_tag: String,
    pub channel_tag: String,
    pub ssrc: Option<u32>,
    pub allocated_at: std::time::Instant,
}

#[derive(Eq, Hash, PartialEq)]
struct PortKey(u16, TransportType);

pub struct RtpServerManager {
    port_range: RtpPortRange,
    bind_addr: String,
    udp_servers: DashMap<u16, Arc<UDPRtpServer>>,
    tcp_servers: DashMap<u16, Arc<TcpRtpServer>>,
    ports: DashMap<PortKey, PortInfo>,
    free_udp_ports: RwLock<Vec<(u16, u16)>>,  // (rtp_port, rtcp_port) 成对
    free_tcp_ports: RwLock<Vec<u16>>,
    next_udp_port: AtomicU16,
    timeout_secs: u64,
}

impl RtpServerManager {
    pub fn new(port_range: RtpPortRange, bind_addr: String, timeout_secs: u64) -> Self {
        let start = port_range.start;
        tracing::info!("[RtpServerManager] Created with port range {}-{}, bind {}, timeout {}s", 
            port_range.start, port_range.end, bind_addr, timeout_secs);
        Self {
            port_range,
            bind_addr,
            udp_servers: DashMap::new(),
            tcp_servers: DashMap::new(),
            ports: DashMap::new(),
            free_udp_ports: RwLock::new(Vec::new()),
            free_tcp_ports: RwLock::new(Vec::new()),
            next_udp_port: AtomicU16::new(start),
            timeout_secs,
        }
    }
    
    async fn get_next_udp_port(&self) -> Option<(u16, u16)> {
        // 优先从空闲队列获取 (rtp_port, rtcp_port) 对
        {
            let mut free = self.free_udp_ports.write().await;
            if let Some(pair) = free.pop() {
                if !self.udp_servers.contains_key(&pair.0) {
                    return Some(pair);
                }
            }
        }
        
        // 空闲队列为空，生成新端口对
        let max_attempts = (self.port_range.end - self.port_range.start) / 2 + 1;
        let start = self.port_range.start;
        let end = self.port_range.end;
        
        for _ in 0..max_attempts {
            let port = self.next_udp_port.fetch_add(2, Ordering::SeqCst);
            let port = if port > end || port == 0 {
                self.next_udp_port.store(start, Ordering::SeqCst);
                start
            } else {
                port
            };
            
            if self.udp_servers.contains_key(&port) {
                continue;
            }
            
            return Some((port, port + 1));
        }
        None
    }
    
    async fn get_next_tcp_port(&self) -> Option<u16> {
        // 优先从空闲队列获取
        {
            let mut free = self.free_tcp_ports.write().await;
            if let Some(port) = free.pop() {
                if !self.tcp_servers.contains_key(&port) {
                    return Some(port);
                }
            }
        }
        
        // 空闲队列为空，生成新端口
        let port_range_end = self.port_range.end.max(self.port_range.start + 100);
        let mut port = self.port_range.start;
        while port <= port_range_end {
            if !self.tcp_servers.contains_key(&port) {
                return Some(port);
            }
            port += 1;
        }
        None
    }
    
    fn generate_ssrc() -> u32 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        (nanos as u32) ^ (rand_simple() as u32)
    }
    
    pub async fn allocate(&self, device_tag: &str, channel_tag: &str, protocol: TransportType, _callback: Arc<dyn RtpPacketCallback>) -> Result<(u16, u32)> {
        match protocol {
            TransportType::Udp => {
                let (port, ssrc, _) = self.allocate_udp(device_tag, channel_tag).await?;
                Ok((port, ssrc))
            }
            TransportType::Tcp => {
                let (port, ssrc, _) = self.allocate_tcp(device_tag, channel_tag).await?;
                Ok((port, ssrc))
            }
            _ => unreachable!("RtpServerManager only supports UDP and TCP"),
        }
    }
    
    pub async fn allocate_udp(&self, device_tag: &str, channel_tag: &str) -> Result<(u16, u32, Arc<UDPRtpServer>)> {
        let ssrc = Self::generate_ssrc();
        
        let (rtp_port, _rtcp_port) = self.get_next_udp_port().await
            .ok_or_else(|| MediaError::PortAllocation("No available UDP ports".into()))?;
        
        let server = Arc::new(UDPRtpServer::new(&self.bind_addr, rtp_port).await?);
        
        if !server.allocate(ssrc, device_tag.to_string(), channel_tag.to_string()).await {
            return Err(MediaError::PortAllocation(format!("Port {} already allocated", rtp_port)));
        }
        
        self.udp_servers.insert(rtp_port, server.clone());
        self.ports.insert(PortKey(rtp_port, TransportType::Udp), PortInfo {
            protocol: TransportType::Udp,
            device_tag: device_tag.to_string(),
            channel_tag: channel_tag.to_string(),
            ssrc: Some(ssrc),
            allocated_at: std::time::Instant::now(),
        });
        
        tracing::info!("[RtpServerManager] Allocated UDP port {} (SSRC {}) for {}/{}", rtp_port, ssrc, device_tag, channel_tag);
        Ok((rtp_port, ssrc, server))
    }
    
    pub async fn allocate_tcp(&self, device_tag: &str, channel_tag: &str) -> Result<(u16, u32, Arc<TcpRtpServer>)> {
        let ssrc = Self::generate_ssrc();
        
        let port = self.get_next_tcp_port().await
            .ok_or_else(|| MediaError::PortAllocation("No available TCP ports".into()))?;
        
        let server = Arc::new(TcpRtpServer::new_passive(&self.bind_addr, port).await?);
        
        if !server.allocate(ssrc, device_tag.to_string(), channel_tag.to_string()).await {
            return Err(MediaError::PortAllocation(format!("TCP Port {} already allocated", port)));
        }
        
        self.tcp_servers.insert(port, server.clone());
        self.ports.insert(PortKey(port, TransportType::Tcp), PortInfo {
            protocol: TransportType::Tcp,
            device_tag: device_tag.to_string(),
            channel_tag: channel_tag.to_string(),
            ssrc: Some(ssrc),
            allocated_at: std::time::Instant::now(),
        });
        
        tracing::info!("[RtpServerManager] Allocated TCP port {} (SSRC {}) for {}/{}", port, ssrc, device_tag, channel_tag);
        Ok((port, ssrc, server))
    }

    pub async fn allocate_tcp_active(&self) -> Result<(u16, u32, Arc<TcpRtpServer>)> {
        let ssrc = Self::generate_ssrc();
        
        let port = self.get_next_tcp_port().await
            .ok_or_else(|| MediaError::PortAllocation("No available TCP ports".into()))?;
        
        let server = Arc::new(TcpRtpServer::new_active(&self.bind_addr, port).await?);
        
        self.tcp_servers.insert(port, server.clone());
        self.ports.insert(PortKey(port, TransportType::Tcp), PortInfo {
            protocol: TransportType::Tcp,
            device_tag: String::new(),
            channel_tag: String::new(),
            ssrc: Some(ssrc),
            allocated_at: std::time::Instant::now(),
        });
        
        tracing::info!("[RtpServerManager] Allocated TCP active server on port {} (SSRC {})", port, ssrc);
        Ok((port, ssrc, server))
    }

    pub async fn release(&self, port: u16, protocol: TransportType) {
        let key = PortKey(port, protocol);
        
        if let Some(info) = self.ports.get(&key) {
            let device_tag = info.device_tag.clone();
            let channel_tag = info.channel_tag.clone();
            let ssrc = info.ssrc;
            tracing::info!("[RtpServerManager] Release port {} ({:?}) for {}/{} (was SSRC {:?})", 
                port, protocol, device_tag, channel_tag, ssrc);
        }
        self.ports.remove(&key);
        
        match protocol {
            TransportType::Udp => {
                if let Some((_, server)) = self.udp_servers.remove(&port) {
                    server.release().await;
                }
                let mut free = self.free_udp_ports.write().await;
                free.push((port, port + 1));
            }
            TransportType::Tcp => {
                if let Some((_, server)) = self.tcp_servers.remove(&port) {
                    server.release().await;
                }
                let mut free = self.free_tcp_ports.write().await;
                free.push(port);
            }
            _ => unreachable!("RtpServerManager only supports UDP and TCP"),
        }
    }
    
    pub async fn release_by_device_channel(&self, device_tag: &str, channel_tag: &str, protocol: TransportType) {
        let key = PortKey(0, protocol); // port=0 只是占位，实际用 device_tag/channel_tag 查找
        
        for entry in self.ports.iter() {
            let (k, info) = entry.pair();
            if info.device_tag == device_tag && info.channel_tag == channel_tag && k.1 == protocol {
                self.release(k.0, protocol).await;
                return;
            }
        }
    }
    
    pub async fn get_udp_server(&self, port: u16) -> Option<Arc<UDPRtpServer>> {
        self.udp_servers.get(&port).map(|v| v.clone())
    }
    
    pub async fn get_tcp_server(&self, port: u16) -> Option<Arc<TcpRtpServer>> {
        self.tcp_servers.get(&port).map(|v| v.clone())
    }
    
    pub async fn get_port_info(&self, port: u16, protocol: TransportType) -> Option<PortInfo> {
        self.ports.get(&PortKey(port, protocol)).map(|v| (*v).clone())
    }
    
    pub async fn cleanup_timed_out(&self) {
        let timeout = std::time::Duration::from_secs(self.timeout_secs);
        let now = std::time::Instant::now();
        
        let timed_out: Vec<(u16, TransportType)> = {
            let mut to_release = Vec::new();
            for entry in self.ports.iter() {
                let (key, info) = entry.pair();
                let port = key.0;
                let proto = key.1;
                
                let is_timed_out = match info.protocol {
                    TransportType::Udp => {
                        if let Some(server) = self.udp_servers.get(&port) {
                            if let Some(session) = server.get_session().await {
                                now.duration_since(session.allocated_at) > timeout
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                    TransportType::Tcp => {
                        if let Some(server) = self.tcp_servers.get(&port) {
                            if let Some(session) = server.get_session().await {
                                now.duration_since(session.allocated_at) > timeout
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                
                if is_timed_out {
                    to_release.push((port, proto));
                }
            }
            to_release
        };
        
        for (port, proto) in timed_out {
            tracing::warn!("[RtpServerManager] Timeout cleanup for port {} ({:?})", port, proto);
            self.release(port, proto).await;
        }
    }
    
    pub fn count_allocated(&self) -> usize {
        self.ports.len()
    }
}

fn rand_simple() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

static RTP_SERVER_MANAGER: OnceCell<Arc<RtpServerManager>> = OnceCell::new();

pub async fn init_rtp_server_manager(port_start: u16, port_end: u16, bind_addr: String, timeout_secs: u64) {
    let manager = Arc::new(RtpServerManager::new(
        RtpPortRange::new(port_start, port_end),
        bind_addr,
        timeout_secs,
    ));
    RTP_SERVER_MANAGER.set(manager.clone()).ok();
    
    let manager_for_task = manager.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            manager_for_task.cleanup_timed_out().await;
        }
    });
    
    tracing::info!("[RtpServerManager] Initialized with port range {}-{}", port_start, port_end);
}

pub fn get_rtp_server_manager() -> Option<&'static Arc<RtpServerManager>> {
    RTP_SERVER_MANAGER.get()
}
