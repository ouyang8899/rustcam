use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use dashmap::DashMap;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{RwLock, Mutex};
use tokio_util::sync::CancellationToken;

use crate::protocol::rtsp::RtspServerAdapter;
use crate::protocol::gb28181::Gb28181Adapter;
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::traits::ProtocolDeps;
use crate::protocol::event::SignalEvent;
use crate::infrastructure::event_bus::EventBus;
use crate::monitoring::Metrics;

#[derive(Debug, Clone)]
pub enum TcpRtpEvent {
    NewConnection {
        stream: Arc<TcpStream>,
        addr: SocketAddr,
        ssrc: u32,
    },
}

pub type TcpRtpEventCallback = Arc<dyn Fn(TcpRtpEvent) + Send + Sync>;

#[derive(Clone, Debug)]
pub struct TcpRtspRtpServerConfig {
    pub bind_addr: String,
    pub max_connections: usize,
    pub connection_timeout: u64,
    pub keepalive_interval_secs: u64,
    pub rtsp_port: u16,
}

impl Default for TcpRtspRtpServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:554".to_string(),
            max_connections: 1000,
            connection_timeout: 60,
            keepalive_interval_secs: 30,
            rtsp_port: 554,
        }
    }
}

#[derive(Debug)]
enum ProtocolType {
    Rtsp,
    Gb28181Sip,
    Gb28181Audio,
}

impl ProtocolType {
    fn detect(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        if data.starts_with(b"OPTIONS") || data.starts_with(b"DESCRIBE") ||
           data.starts_with(b"SETUP") || data.starts_with(b"PLAY") ||
           data.starts_with(b"PAUSE") || data.starts_with(b"TEARDOWN") ||
           data.starts_with(b"GET_PARAMETER") || data.starts_with(b"SET_PARAMETER") ||
           data.starts_with(b"ANNOUNCE") || data.starts_with(b"RECORD") {
            return Some(ProtocolType::Rtsp);
        }

        if data.starts_with(b"REGISTER") || data.starts_with(b"INVITE") ||
           data.starts_with(b"BYE") || data.starts_with(b"ACK") ||
           data.starts_with(b"MESSAGE") || data.starts_with(b"OPTIONS") ||
           data.starts_with(b"CANCEL") || data.starts_with(b"NOTIFY") ||
           data.starts_with(b"SUBSCRIBE") || data.starts_with(b"INFO") ||
           data.starts_with(b"PRACK") {
            if data.windows(4).any(|w| w == b" sip") || data.windows(7).any(|w| w == b" SIP/2.") {
                return Some(ProtocolType::Gb28181Sip);
            }
        }

        if data.len() >= 4 && (data[0] == 0x80 || data[0] == 0x90) {
            let len = u16::from_be_bytes([data[2], data[3]]) as usize;
            if data.len() >= 4 + len && len >= 12 && len <= 1500 {
                return Some(ProtocolType::Gb28181Audio);
            }
        }

        None
    }
}

struct ConnectionEntry {
    adapter: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>>,
    device_tag: Mutex<Option<String>>,
    connected_at: std::time::Instant,
    last_message_at: std::sync::atomic::AtomicI64,
    remote_addr: SocketAddr,
    protocol: ProtocolType,
}

impl ConnectionEntry {
    fn new(
        adapter: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>>,
        remote_addr: SocketAddr,
        protocol: ProtocolType,
    ) -> Self {
        Self {
            adapter,
            device_tag: Mutex::new(None),
            connected_at: std::time::Instant::now(),
            last_message_at: std::sync::atomic::AtomicI64::new(chrono::Utc::now().timestamp()),
            remote_addr,
            protocol,
        }
    }

    async fn update_device_tag(&self, tag: String) {
        *self.device_tag.lock().await = Some(tag);
    }

    async fn get_device_tag(&self) -> Option<String> {
        self.device_tag.lock().await.clone()
    }

    fn update_activity(&self) {
        self.last_message_at.store(
            chrono::Utc::now().timestamp(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    fn is_idle(&self, idle_secs: i64) -> bool {
        let now = chrono::Utc::now().timestamp();
        now - self.last_message_at.load(std::sync::atomic::Ordering::Relaxed) > idle_secs
    }
}

pub struct TcpRtspRtpServer {
    listener: TcpListener,
    event_bus: Arc<EventBus>,
    config: TcpRtspRtpServerConfig,
    metrics: Arc<Metrics>,
    protocol_deps: ProtocolDeps,
    connections: Arc<DashMap<String, Arc<ConnectionEntry>>>,
    shutdown_token: CancellationToken,
    join_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    event_callback: RwLock<Option<TcpRtpEventCallback>>,
}

impl TcpRtspRtpServer {
    pub async fn new(
        config: TcpRtspRtpServerConfig,
        event_bus: Arc<EventBus>,
        metrics: Arc<Metrics>,
        protocol_deps: ProtocolDeps,
    ) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(&config.bind_addr).await?;
        tracing::info!("[TcpRtspRtpServer] Listening on {}", config.bind_addr);
        Ok(Self {
            listener,
            event_bus,
            config,
            metrics,
            protocol_deps,
            connections: Arc::new(DashMap::new()),
            shutdown_token: CancellationToken::new(),
            join_handles: Arc::new(Mutex::new(Vec::new())),
            event_callback: RwLock::new(None),
        })
    }
    
    pub async fn set_event_callback(&self, callback: TcpRtpEventCallback) {
        let mut guard = self.event_callback.write().await;
        *guard = Some(callback);
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        tracing::info!("[TcpRtspRtpServer] Started on {}", self.config.bind_addr);
        
        let cleanup_handle = self.start_cleanup_task();
        self.join_handles.lock().await.push(cleanup_handle);

        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            if self.connections.len() >= self.config.max_connections {
                                tracing::warn!("[TcpRtspRtpServer] Connection limit reached, rejecting {}", addr);
                                drop(stream);
                                continue;
                            }
                            
                            self.metrics.connection_opened();
                            
                            let event_bus = self.event_bus.clone();
                            let config = self.config.clone();
                            let metrics = self.metrics.clone();
                            let deps = self.protocol_deps.clone();
                            let connections = self.connections.clone();
                            let shutdown_token = self.shutdown_token.clone();

                            let handle = tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(
                                    stream, addr, event_bus, config, metrics, deps, connections, shutdown_token,
                                ).await {
                                    tracing::error!("[TcpRtspRtpServer] Connection {} error: {}", addr, e);
                                }
                            });
                            self.join_handles.lock().await.push(handle);
                        }
                        Err(e) => {
                            tracing::error!("[TcpRtspRtpServer] Accept error: {}", e);
                        }
                    }
                }
                _ = self.shutdown_token.cancelled() => {
                    tracing::info!("[TcpRtspRtpServer] Shutdown signal received");
                    break;
                }
            }
        }

        self.shutdown().await;
        Ok(())
    }

    async fn shutdown(&self) {
        tracing::info!("[TcpRtspRtpServer] Shutting down...");
        
        self.shutdown_token.cancel();
        
        let addrs: Vec<String> = self.connections.iter()
            .map(|e| e.key().clone())
            .collect();
        
        for addr_key in addrs {
            if let Some((_, entry)) = self.connections.remove(&addr_key) {
                let adapter = entry.adapter.clone();
                let mut guard = adapter.lock().await;
                let _ = guard.on_disconnected().await;
            }
        }

        let mut handles = self.join_handles.lock().await;
        for handle in handles.drain(..) {
            let _ = handle.await;
        }
        
        tracing::info!("[TcpRtspRtpServer] Shutdown complete");
    }

    fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let connections = self.connections.clone();
        let event_bus = self.event_bus.clone();
        let idle_timeout_secs = self.config.keepalive_interval_secs as i64;
        let connection_timeout = self.config.connection_timeout;
        let shutdown_token = self.shutdown_token.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        Self::cleanup_connections(
                            &connections,
                            &event_bus,
                            idle_timeout_secs,
                            connection_timeout,
                        ).await;
                    }
                    _ = shutdown_token.cancelled() => {
                        tracing::info!("[TcpRtspRtpServer] Cleanup task received shutdown signal");
                        break;
                    }
                }
            }
        })
    }

    async fn cleanup_connections(
        connections: &Arc<DashMap<String, Arc<ConnectionEntry>>>,
        event_bus: &Arc<EventBus>,
        idle_timeout_secs: i64,
        connection_timeout: u64,
    ) {
        let addrs: Vec<String> = connections.iter()
            .map(|e| e.key().clone())
            .collect();

        for addr_key in addrs {
            if let Some(entry) = connections.get(&addr_key) {
                if entry.is_idle(idle_timeout_secs) {
                    if let Some((_, entry)) = connections.remove(&addr_key) {
                        tracing::info!("[TcpRtspRtpServer] Connection {} idle timeout, cleaning up", addr_key);
                        
                        let remote_addr = entry.remote_addr;
                        let device_tag = entry.get_device_tag().await;
                        
                        let adapter = entry.adapter.clone();
                        {
                            let mut guard = adapter.lock().await;
                            let _ = guard.on_disconnected().await;
                        }
                        
                        if let Some(tag) = device_tag {
                            let event = SignalEvent::DeviceOffline {
                                device_id: 0,
                                device_tag: Some(tag),
                                reason: Some(format!("TCP idle timeout from {}", remote_addr)),
                            };
                            if let Err(e) = event_bus.publish(event).await {
                                tracing::error!("[TcpRtspRtpServer] Failed to publish DeviceOffline: {}", e);
                            }
                        }
                    }
                }
            }
        }

        let stale_timeout = connection_timeout as u64 * 3;
        let stale: Vec<String> = connections.iter()
            .filter(|entry| {
                entry.value().connected_at.elapsed().as_secs() > stale_timeout
            })
            .map(|entry| entry.key().clone())
            .collect();

        for addr_key in stale {
            if let Some((_, entry)) = connections.remove(&addr_key) {
                tracing::warn!("[TcpRtspRtpServer] Removing stale connection {}", addr_key);
                let adapter = entry.adapter.clone();
                let mut guard = adapter.lock().await;
                let _ = guard.on_disconnected().await;
            }
        }
    }

    async fn handle_connection(
        stream: TcpStream,
        addr: SocketAddr,
        event_bus: Arc<EventBus>,
        config: TcpRtspRtpServerConfig,
        metrics: Arc<Metrics>,
        deps: ProtocolDeps,
        connections: Arc<DashMap<String, Arc<ConnectionEntry>>>,
        shutdown_token: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!("[TcpRtspRtpServer] New connection from {}", addr);

        let mut peek_buf = vec![0u8; 1024];
        
        let protocol = tokio::time::timeout(
            Duration::from_secs(5),
            stream.peek(&mut peek_buf),
        ).await;

        let protocol = match protocol {
            Ok(Ok(n)) => {
                if n == 0 {
                    tracing::debug!("[TcpRtspRtpServer] Empty connection from {}", addr);
                    return Ok(());
                }
                match ProtocolType::detect(&peek_buf[..n]) {
                    Some(p) => {
                        tracing::info!("[TcpRtspRtpServer] Detected protocol {:?} from {}", p, addr);
                        p
                    }
                    None => {
                        tracing::warn!("[TcpRtspRtpServer] Unknown protocol from {}", addr);
                        ProtocolType::Gb28181Audio
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::error!("[TcpRtspRtpServer] Peek error from {}: {}", addr, e);
                return Ok(());
            }
            Err(_) => {
                tracing::warn!("[TcpRtspRtpServer] Connection timeout from {}", addr);
                return Ok(());
            }
        };

        let adapter: Box<dyn SignalAdapter + Send + Sync> = match &protocol {
            ProtocolType::Rtsp => {
                Box::new(RtspServerAdapter::new(deps))
            }
            ProtocolType::Gb28181Sip | ProtocolType::Gb28181Audio => {
                Box::new(Gb28181Adapter::new(
                    deps,
                    Arc::new(|_, _| {}),
                    Arc::new(|_| {}),
                ))
            }
        };

        let mut adapter = adapter;
        adapter.on_connected(addr).await?;

        let (reader, write_half) = stream.into_split();
        adapter.set_tcp_write(write_half);

        let adapter_arc: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>> = Arc::new(Mutex::new(adapter));
        let entry = Arc::new(ConnectionEntry::new(adapter_arc.clone(), addr, protocol));
        
        connections.insert(addr.to_string(), entry.clone());

        let mut reader = BufReader::new(reader);
        let mut buffer = vec![0u8; 65536];
        let mut keepalive_interval = tokio::time::interval(Duration::from_secs(config.keepalive_interval_secs));
        let connection_timeout = Duration::from_secs(config.connection_timeout);

        loop {
            tokio::select! {
                result = tokio::time::timeout(connection_timeout, reader.read(&mut buffer)) => {
                    match result {
                        Ok(Ok(0)) => {
                            tracing::debug!("[TcpRtspRtpServer] Connection {} closed by peer", addr);
                            break;
                        }
                        Ok(Ok(n)) => {
                            metrics.bytes_received(n);
                            entry.update_activity();
                            
                            let parse_result = {
                                let mut guard = adapter_arc.lock().await;
                                guard.parse(&buffer[..n]).await
                            };
                            
                            match parse_result {
                                Ok(events) => {
                                    for event in &events {
                                        if let SignalEvent::DeviceRegister { ref device_tag, .. } = event {
                                            if let Some(ref tag) = device_tag {
                                                entry.update_device_tag(tag.clone()).await;
                                            }
                                        }
                                    }
                                    
                                    for event in events {
                                        metrics.event_received();
                                        if let Err(e) = event_bus.publish(event).await {
                                            tracing::error!("[TcpRtspRtpServer] Failed to publish event: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("[TcpRtspRtpServer] Parse error from {}: {}", addr, e);
                                    break;
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::error!("[TcpRtspRtpServer] Read error from {}: {}", addr, e);
                            break;
                        }
                        Err(_) => {
                            tracing::warn!("[TcpRtspRtpServer] Connection {} read timeout", addr);
                            break;
                        }
                    }
                }
                _ = keepalive_interval.tick() => {
                    let idle_timeout = {
                        let guard = adapter_arc.lock().await;
                        guard.idle_timeout().unwrap_or(300)
                    };
                    if entry.is_idle(idle_timeout as i64) {
                        tracing::warn!("[TcpRtspRtpServer] Connection {} keepalive timeout", addr);
                        break;
                    }
                }
                _ = shutdown_token.cancelled() => {
                    tracing::info!("[TcpRtspRtpServer] Connection {} shutdown requested", addr);
                    break;
                }
            }
        }

        connections.remove(&addr.to_string());
        
        let device_tag = entry.get_device_tag().await;
        let remote_addr = entry.remote_addr;
        {
            let mut guard = adapter_arc.lock().await;
            let _ = guard.on_disconnected().await;
        }
        
        if let Some(tag) = device_tag {
            let event = SignalEvent::DeviceOffline {
                device_id: 0,
                device_tag: Some(tag),
                reason: Some(format!("TCP connection closed from {}", remote_addr)),
            };
            if let Err(e) = event_bus.publish(event).await {
                tracing::error!("[TcpRtspRtpServer] Failed to publish DeviceOffline: {}", e);
            }
        }

        Ok(())
    }
}