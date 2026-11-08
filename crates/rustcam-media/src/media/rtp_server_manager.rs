
use crate::media::errors::{MediaError, Result};
use crate::media::node::RtpPacketCallback;
use crate::media::rtp_server::{RtpPortRange, TcpRtpServer, UDPRtpServer};
use crate::protocol::event::TransportType;
use dashmap::DashMap;
use once_cell::sync::OnceCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

const PORT_COOLDOWN_SECS: u64 = 5;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub protocol: TransportType,
    pub device_tag: String,
    pub channel_tag: String,
    pub ssrc: Option<u32>,
    pub allocated_at: Instant,
    pub actor: String,
    pub active_at: Instant,
}

#[derive(Eq, Hash, PartialEq, Clone, Copy)]
struct PortKey(u16, TransportType);

pub struct RtpServerManager {
    port_range: RtpPortRange,
    bind_addr: String,
    udp_servers: DashMap<u16, Arc<UDPRtpServer>>,
    tcp_servers: DashMap<u16, Arc<TcpRtpServer>>,
    ports: DashMap<PortKey, PortInfo>,
    free_udp_ports: RwLock<Vec<(u16, u16, Instant)>>,
    free_tcp_ports: RwLock<Vec<(u16, Instant)>>,
    next_udp_port: AtomicU16,
    next_tcp_port: AtomicU16,
    allocation_lock: Mutex<()>,
    ssrc_counter: AtomicU32,
    timeout_secs: u64,
}

impl RtpServerManager {
    pub fn new(port_range: RtpPortRange, bind_addr: String, timeout_secs: u64) -> Self {
        let start = port_range.start;
        Self {
            port_range,
            bind_addr,
            udp_servers: DashMap::new(),
            tcp_servers: DashMap::new(),
            ports: DashMap::new(),
            free_udp_ports: RwLock::new(Vec::new()),
            free_tcp_ports: RwLock::new(Vec::new()),
            next_udp_port: AtomicU16::new(start),
            next_tcp_port: AtomicU16::new(start),
            allocation_lock: Mutex::new(()),
            ssrc_counter: AtomicU32::new(1),
            timeout_secs,
        }
    }

    async fn pick_udp_port(&self, exclude: &HashSet<u16>) -> Option<(u16, u16)> {
        let now = Instant::now();
        {
            let mut free = self.free_udp_ports.write().await;
            let ready_idx = free.iter().position(|(p, rp, at)| {
                now.duration_since(*at).as_secs() >= PORT_COOLDOWN_SECS
                    && !exclude.contains(p)
                    && !exclude.contains(rp)
                    && !self.udp_servers.contains_key(p)
            });
            if let Some(idx) = ready_idx {
                let (p, rp, _) = free.swap_remove(idx);
                return Some((p, rp));
            }
        }

        let start = self.port_range.start | 1;
        let start = start & !1;
        let end = self.port_range.end & !1;
        let span = end.saturating_sub(start);
        if span < 2 {
            return None;
        }
        let max_attempts = (span / 2) + 1;

        for _ in 0..max_attempts {
            let port = self.next_udp_port.fetch_add(2, Ordering::SeqCst);
            let port = if port > end || port < start {
                self.next_udp_port.store(start + 2, Ordering::SeqCst);
                start
            } else {
                port
            };

            if port + 1 > end {
                continue;
            }
            if exclude.contains(&port) || exclude.contains(&(port + 1)) {
                continue;
            }
            if self.udp_servers.contains_key(&port) {
                continue;
            }
            let cooling = {
                let free = self.free_udp_ports.read().await;
                free.iter().any(|(p, _, _)| *p == port)
            };
            if cooling {
                continue;
            }
            return Some((port, port + 1));
        }
        None
    }

    async fn pick_tcp_port(&self, exclude: &HashSet<u16>) -> Option<u16> {
        let now = Instant::now();
        {
            let mut free = self.free_tcp_ports.write().await;
            let ready_idx = free.iter().position(|(p, at)| {
                now.duration_since(*at).as_secs() >= PORT_COOLDOWN_SECS
                    && !exclude.contains(p)
                    && !self.tcp_servers.contains_key(p)
            });
            if let Some(idx) = ready_idx {
                let (p, _) = free.swap_remove(idx);
                return Some(p);
            }
        }

        let start = self.port_range.start;
        let end = self.port_range.end;

        for _ in 0..(end.saturating_sub(start) + 1) {
            let port = self.next_tcp_port.fetch_add(1, Ordering::SeqCst);
            let port = if port > end || port < start {
                self.next_tcp_port.store(start + 1, Ordering::SeqCst);
                start
            } else {
                port
            };

            if exclude.contains(&port) {
                continue;
            }
            if self.tcp_servers.contains_key(&port) {
                continue;
            }
            let cooling = {
                let free = self.free_tcp_ports.read().await;
                free.iter().any(|(p, _)| *p == port)
            };
            if cooling {
                continue;
            }
            return Some(port);
        }
        None
    }

    fn generate_ssrc(&self) -> u32 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u32)
            .unwrap_or(0);
        let counter = self.ssrc_counter.fetch_add(1, Ordering::Relaxed);
        let mixed = (nanos << 12) ^ counter;
        if mixed == 0 {
            1
        } else {
            mixed
        }
    }

    pub async fn allocate(
        &self,
        device_tag: &str,
        channel_tag: &str,
        actor: &str,
        protocol: TransportType,
        _callback: Arc<dyn RtpPacketCallback>,
    ) -> Result<(u16, u32)> {
        match protocol {
            TransportType::Udp => {
                let (port, ssrc, _) = self.allocate_udp(device_tag, channel_tag, actor).await?;
                Ok((port, ssrc))
            }
            TransportType::Tcp => {
                let (port, ssrc, _) = self.allocate_tcp(device_tag, channel_tag, actor).await?;
                Ok((port, ssrc))
            }
            _ => Err(MediaError::Transport(format!(
                "RtpServerManager 不支持的协议: {:?}",
                protocol
            ))),
        }
    }

    pub async fn allocate_udp(
        &self,
        device_tag: &str,
        channel_tag: &str,
        actor: &str,
    ) -> Result<(u16, u32, Arc<UDPRtpServer>)> {
        let _guard = self.allocation_lock.lock().await;

        let mut tried: HashSet<u16> = HashSet::new();
        loop {
            let Some((rtp_port, _rtcp_port)) = self.pick_udp_port(&tried).await else {
                return Err(MediaError::PortAllocation(
                    "无可用 UDP 端口（所有候选端口绑定失败）".into(),
                ));
            };

            let server = match UDPRtpServer::new(&self.bind_addr, rtp_port, &self.port_range).await {
                Ok(server) => Arc::new(server),
                Err(e) => {
                    tracing::warn!(
                        "[RtpServerManager] UDP 端口对 {}/{} 绑定失败，尝试下一个端口: {}",
                        rtp_port,
                        rtp_port + 1,
                        e
                    );
                    tried.insert(rtp_port);
                    continue;
                }
            };
            let ssrc = self.generate_ssrc();

            if !server
                .allocate(ssrc, device_tag.to_string(), channel_tag.to_string())
                .await
            {
                return Err(MediaError::PortAllocation(format!(
                    "UDP 端口 {} 已被分配",
                    rtp_port
                )));
            }

            self.udp_servers.insert(rtp_port, server.clone());
            let now = Instant::now();
            self.ports.insert(
                PortKey(rtp_port, TransportType::Udp),
                PortInfo {
                    protocol: TransportType::Udp,
                    device_tag: device_tag.to_string(),
                    channel_tag: channel_tag.to_string(),
                    ssrc: Some(ssrc),
                    allocated_at: now,
                    actor: actor.to_string(),
                    active_at: now,
                },
            );

            return Ok((rtp_port, ssrc, server));
        }
    }

    pub async fn allocate_tcp(
        &self,
        device_tag: &str,
        channel_tag: &str,
        actor: &str,
    ) -> Result<(u16, u32, Arc<TcpRtpServer>)> {
        let _guard = self.allocation_lock.lock().await;

        let mut tried: HashSet<u16> = HashSet::new();
        loop {
            let Some(port) = self.pick_tcp_port(&tried).await else {
                return Err(MediaError::PortAllocation(
                    "无可用 TCP 端口（所有候选端口绑定失败）".into(),
                ));
            };

            let server = match TcpRtpServer::new_passive(&self.bind_addr, port).await {
                Ok(server) => Arc::new(server),
                Err(e) => {
                    tracing::warn!(
                        "[RtpServerManager] TCP 端口 {} 绑定失败，尝试下一个端口: {}",
                        port,
                        e
                    );
                    tried.insert(port);
                    continue;
                }
            };
            let ssrc = self.generate_ssrc();

            if !server
                .allocate(ssrc, device_tag.to_string(), channel_tag.to_string())
                .await
            {
                return Err(MediaError::PortAllocation(format!(
                    "TCP 端口 {} 已被分配",
                    port
                )));
            }

            self.tcp_servers.insert(port, server.clone());
            let now = Instant::now();
            self.ports.insert(
                PortKey(port, TransportType::Tcp),
                PortInfo {
                    protocol: TransportType::Tcp,
                    device_tag: device_tag.to_string(),
                    channel_tag: channel_tag.to_string(),
                    ssrc: Some(ssrc),
                    allocated_at: now,
                    actor: actor.to_string(),
                    active_at: now,
                },
            );

            return Ok((port, ssrc, server));
        }
    }

    pub async fn allocate_tcp_active(&self) -> Result<(u16, u32, Arc<TcpRtpServer>)> {
        let _guard = self.allocation_lock.lock().await;

        let mut tried: HashSet<u16> = HashSet::new();
        loop {
            let Some(port) = self.pick_tcp_port(&tried).await else {
                return Err(MediaError::PortAllocation(
                    "无可用 TCP 端口（所有候选端口绑定失败）".into(),
                ));
            };

            let server = match TcpRtpServer::new_active(&self.bind_addr, port).await {
                Ok(server) => Arc::new(server),
                Err(e) => {
                    tracing::warn!(
                        "[RtpServerManager] TCP 端口 {} 绑定失败，尝试下一个端口: {}",
                        port,
                        e
                    );
                    tried.insert(port);
                    continue;
                }
            };
            let ssrc = self.generate_ssrc();

            self.tcp_servers.insert(port, server.clone());
            let now = Instant::now();
            self.ports.insert(
                PortKey(port, TransportType::Tcp),
                PortInfo {
                    protocol: TransportType::Tcp,
                    device_tag: String::new(),
                    channel_tag: String::new(),
                    ssrc: Some(ssrc),
                    allocated_at: now,
                    actor: String::new(),
                    active_at: now,
                },
            );

            return Ok((port, ssrc, server));
        }
    }

    pub async fn release(&self, port: u16, protocol: TransportType) {
        let key = PortKey(port, protocol);

        if let Some(_info) = self.ports.get(&key) {
        }
        self.ports.remove(&key);

        match protocol {
            TransportType::Udp => {
                if let Some((_, server)) = self.udp_servers.remove(&port) {
                    server.release().await;
                }
                let now = Instant::now();
                let mut free = self.free_udp_ports.write().await;
                if let Some(entry) = free.iter_mut().find(|(p, _, _)| *p == port) {
                    entry.2 = now;
                } else {
                    free.push((port, port + 1, now));
                }
            }
            TransportType::Tcp => {
                if let Some((_, server)) = self.tcp_servers.remove(&port) {
                    server.release().await;
                }
                let now = Instant::now();
                let mut free = self.free_tcp_ports.write().await;
                if let Some(entry) = free.iter_mut().find(|(p, _)| *p == port) {
                    entry.1 = now;
                } else {
                    free.push((port, now));
                }
            }
            _ => tracing::warn!(
                "[RtpServerManager] release 收到不支持的协议: {:?}",
                protocol
            ),
        }
    }

    pub async fn release_by_device_channel(
        &self,
        device_tag: &str,
        channel_tag: &str,
        actor: &str,
        protocol: TransportType,
    ) {
        let matches: Vec<(u16, TransportType)> = self
            .ports
            .iter()
            .filter(|e| {
                let (k, info) = e.pair();
                k.1 == protocol
                    && info.device_tag == device_tag
                    && info.channel_tag == channel_tag
                    && info.actor == actor
            })
            .map(|e| (e.key().0, e.key().1))
            .collect();

        for (port, proto) in matches {
            self.release(port, proto).await;
        }
    }

    pub async fn get_udp_server(&self, port: u16) -> Option<Arc<UDPRtpServer>> {
        self.udp_servers.get(&port).map(|v| v.clone())
    }

    pub async fn get_tcp_server(&self, port: u16) -> Option<Arc<TcpRtpServer>> {
        self.tcp_servers.get(&port).map(|v| v.clone())
    }

    pub async fn get_port_info(&self, port: u16, protocol: TransportType) -> Option<PortInfo> {
        self.ports
            .get(&PortKey(port, protocol))
            .map(|v| (*v).clone())
    }

    pub async fn cleanup_timed_out(&self) {
        let timeout = Duration::from_secs(self.timeout_secs);
        let now = Instant::now();

        let snapshot: Vec<(u16, TransportType)> = self
            .ports
            .iter()
            .map(|e| (e.key().0, e.key().1))
            .collect();

        let mut to_release = Vec::new();

        for (port, proto) in snapshot {

            let session = match proto {
                TransportType::Udp => {
                    let server = { self.udp_servers.get(&port).map(|s| s.value().clone()) };
                    match server {
                        Some(s) => s.get_session().await,
                        None => None,
                    }
                }
                TransportType::Tcp => {
                    let server = { self.tcp_servers.get(&port).map(|s| s.value().clone()) };
                    match server {
                        Some(s) => s.get_session().await,
                        None => None,
                    }
                }
                _ => None,
            };

            let Some(session) = session else {
                continue;
            };

            if now.duration_since(session.active_at) > timeout {
                to_release.push((port, proto));
            }
        }

        for (port, proto) in to_release {
            tracing::warn!(
                "[RtpServerManager] Timeout cleanup {} ({:?})",
                port,
                proto
            );
            self.release(port, proto).await;
        }
    }

    pub fn count_allocated(&self) -> usize {
        self.ports.len()
    }
}

static RTP_SERVER_MANAGER: OnceCell<Arc<RtpServerManager>> = OnceCell::new();

pub fn init_rtp_server_manager(
    port_start: u16,
    port_end: u16,
    bind_addr: String,
    timeout_secs: u64,
) -> Result<()> {
    if RTP_SERVER_MANAGER.get().is_some() {
        return Ok(());
    }

    let manager = Arc::new(RtpServerManager::new(
        RtpPortRange::new(port_start, port_end),
        bind_addr,
        timeout_secs,
    ));

    RTP_SERVER_MANAGER
        .set(manager.clone())
        .map_err(|_| MediaError::Transport("RtpServerManager 初始化失败（已被其他线程初始化）".into()))?;

    let manager_for_task = manager.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            manager_for_task.cleanup_timed_out().await;
        }
    });

    Ok(())
}

pub fn get_rtp_server_manager() -> Option<&'static Arc<RtpServerManager>> {
    RTP_SERVER_MANAGER.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager(start: u16, end: u16) -> RtpServerManager {
        RtpServerManager::new(RtpPortRange::new(start, end), "127.0.0.1".to_string(), 60)
    }

    #[tokio::test]
    async fn allocate_udp_skips_pair_when_rtp_port_used_externally() {
        let manager = test_manager(45000, 45010);
        let _held = std::net::UdpSocket::bind("127.0.0.1:45000").expect("bind external RTP");

        let (port, _, _server) = manager
            .allocate_udp("dev", "ch", "test")
            .await
            .expect("should skip externally used RTP port");

        assert_eq!(port, 45002);
        assert!(manager.get_udp_server(45000).await.is_none());
    }

    #[tokio::test]
    async fn allocate_udp_skips_pair_when_rtcp_port_used_externally() {
        let manager = test_manager(45300, 45310);
        let _held = std::net::UdpSocket::bind("127.0.0.1:45301").expect("bind external RTCP");

        let (port, _, _server) = manager
            .allocate_udp("dev", "ch", "test")
            .await
            .expect("should skip pair with externally used RTCP port");

        assert_eq!(port, 45302);
        assert!(manager.get_udp_server(45300).await.is_none());
    }

    #[tokio::test]
    async fn allocate_udp_returns_error_when_all_pairs_used_externally() {
        let manager = test_manager(45200, 45203);
        let _rtp0 = std::net::UdpSocket::bind("127.0.0.1:45200").expect("bind external RTP 0");
        let _rtcp0 = std::net::UdpSocket::bind("127.0.0.1:45201").expect("bind external RTCP 0");
        let _rtp1 = std::net::UdpSocket::bind("127.0.0.1:45202").expect("bind external RTP 1");
        let _rtcp1 = std::net::UdpSocket::bind("127.0.0.1:45203").expect("bind external RTCP 1");

        let result = manager.allocate_udp("dev", "ch", "test").await;
        assert!(matches!(result, Err(MediaError::PortAllocation(_))));
    }

    #[tokio::test]
    async fn allocate_tcp_skips_port_used_externally() {
        let manager = test_manager(45100, 45110);
        let _held = std::net::TcpListener::bind("127.0.0.1:45100").expect("bind external TCP");

        let (port, _, _server) = manager
            .allocate_tcp("dev", "ch", "test")
            .await
            .expect("should skip externally used TCP port");

        assert_eq!(port, 45101);
        assert!(manager.get_tcp_server(45100).await.is_none());
    }
}