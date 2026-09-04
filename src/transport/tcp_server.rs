use std::sync::Arc;
use std::net::SocketAddr;
use std::time::Duration;
use dashmap::DashMap;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::Mutex;
use crate::protocol::adapter::{AdapterRegistry, SignalAdapter};
use crate::protocol::traits::ProtocolDeps;
use crate::protocol::event::SignalEvent;
use crate::infrastructure::event_bus::EventBus;
use crate::monitoring::Metrics;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub max_packet_size: usize,
    pub read_buffer_size: usize,
    pub connection_timeout: u64,
    pub keepalive_interval_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".to_string(),
            max_packet_size: 1024 * 1024,
            read_buffer_size: 65536,
            connection_timeout: 60,
            keepalive_interval_secs: 30,
        }
    }
}

struct TcpConnectionEntry {
    adapter: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>>,
    device_tag: Mutex<Option<String>>,
    connected_at: std::time::Instant,
    last_message_at: std::sync::atomic::AtomicI64,
    remote_addr: SocketAddr,
}

impl TcpConnectionEntry {
    fn new(
        adapter: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>>,
        remote_addr: SocketAddr,
    ) -> Self {
        Self {
            adapter,
            device_tag: Mutex::new(None),
            connected_at: std::time::Instant::now(),
            last_message_at: std::sync::atomic::AtomicI64::new(chrono::Utc::now().timestamp()),
            remote_addr,
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

pub struct TcpServer {
    listener: TcpListener,
    registry: Arc<AdapterRegistry>,
    event_bus: Arc<EventBus>,
    config: ServerConfig,
    metrics: Arc<Metrics>,
    protocol_deps: ProtocolDeps,
    connections: Arc<DashMap<String, Arc<TcpConnectionEntry>>>,
}

impl TcpServer {
    pub async fn new(
        config: ServerConfig,
        registry: Arc<AdapterRegistry>,
        event_bus: Arc<EventBus>,
        metrics: Arc<Metrics>,
        protocol_deps: ProtocolDeps,
    ) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(&config.bind_addr).await?;
        tracing::info!("[TcpServer] Bound to {}", config.bind_addr);
        Ok(Self {
            listener,
            registry,
            event_bus,
            config,
            metrics,
            protocol_deps,
            connections: Arc::new(DashMap::new()),
        })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("[TcpServer] Listening on {}", self.config.bind_addr);
        self.start_cleanup_task();

        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    self.metrics.connection_opened();

                    let registry = self.registry.clone();
                    let event_bus = self.event_bus.clone();
                    let config = self.config.clone();
                    let metrics = self.metrics.clone();
                    let deps = self.protocol_deps.clone();
                    let connections = self.connections.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            stream, addr, registry, event_bus, config, metrics.clone(), deps, connections,
                        ).await {
                            tracing::error!("[TcpServer] Connection {} error: {}", addr, e);
                        }
                        metrics.connection_closed();
                    });
                }
                Err(e) => {
                    tracing::error!("[TcpServer] Accept error: {}", e);
                }
            }
        }
    }

    fn start_cleanup_task(&self) {
        let connections = self.connections.clone();
        let event_bus = self.event_bus.clone();
        let idle_timeout_secs = self.config.keepalive_interval_secs as i64;
        let connection_timeout = self.config.connection_timeout;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;

                let addrs: Vec<String> = connections.iter()
                    .map(|e| e.key().clone())
                    .collect();

                for addr_key in addrs {
                    if let Some(entry) = connections.get(&addr_key) {
                        let adapter = entry.adapter.clone();
                        
                        let guard = adapter.lock().await;
                        if guard.needs_timeout_check() {
                            tracing::info!("[TcpServer] Adapter {} registration timeout, handling", addr_key);
                            drop(guard);
                            let mut guard = adapter.lock().await;
                            guard.handle_registration_timeout().await;
                        } else {
                            drop(guard);
                        }
                    }
                }

                let now = chrono::Utc::now().timestamp();
                let expired: Vec<String> = connections.iter()
                    .filter(|entry| {
                        let last = entry.value().last_message_at.load(std::sync::atomic::Ordering::Relaxed);
                        now - last > idle_timeout_secs
                    })
                    .map(|entry| entry.key().clone())
                    .collect();

                for addr_key in expired {
                    if let Some((_, entry)) = connections.remove(&addr_key) {
                        tracing::info!("[TcpServer] Connection {} idle timeout, cleaning up", addr_key);
                        
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
                                tracing::error!("[TcpServer] Failed to publish DeviceOffline: {}", e);
                            }
                        }
                    }
                }

                let stale_timeout = connection_timeout as u64 * 3;
                let now_instant = std::time::Instant::now();
                let stale: Vec<String> = connections.iter()
                    .filter(|entry| {
                        entry.value().connected_at.elapsed().as_secs() > stale_timeout
                    })
                    .map(|entry| entry.key().clone())
                    .collect();

                for addr_key in stale {
                    if let Some((_, entry)) = connections.remove(&addr_key) {
                        tracing::warn!("[TcpServer] Removing stale connection {}", addr_key);
                        let adapter = entry.adapter.clone();
                        let mut guard = adapter.lock().await;
                        let _ = guard.on_disconnected().await;
                    }
                }
            }
        });
    }

    async fn handle_connection(
        mut stream: TcpStream,
        addr: SocketAddr,
        registry: Arc<AdapterRegistry>,
        event_bus: Arc<EventBus>,
        config: ServerConfig,
        metrics: Arc<Metrics>,
        deps: ProtocolDeps,
        connections: Arc<DashMap<String, Arc<TcpConnectionEntry>>>,
    ) -> anyhow::Result<()> {
        let mut peek_buf = vec![0u8; 1024];

        match tokio::time::timeout(
            Duration::from_secs(5),
            stream.peek(&mut peek_buf),
        ).await {
            Ok(Ok(n)) if n > 0 => {
                metrics.bytes_received(n);

                let protocol = match registry.match_protocol(&peek_buf[..n]) {
                    Some(p) => p,
                    None => {
                        tracing::warn!("[TcpServer] Unknown protocol from {}: {:?}", addr, &peek_buf[..n.min(100)]);
                        return Ok(());
                    }
                };

                tracing::info!("[TcpServer] Detected {:?} from {}", protocol, addr);

                let mut adapter = match registry.create_adapter(&protocol, deps) {
                    Some(a) => a,
                    None => {
                        tracing::warn!("[TcpServer] No adapter for {:?}", protocol);
                        return Ok(());
                    }
                };
                
                adapter.on_connected(addr).await?;

                let (reader, write_half) = stream.into_split();
                adapter.set_tcp_write(write_half);

                let adapter_arc: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>> = Arc::new(Mutex::new(adapter));
                let entry = Arc::new(TcpConnectionEntry::new(adapter_arc.clone(), addr));
                
                connections.insert(addr.to_string(), entry.clone());

                let mut reader = BufReader::new(reader);
                let mut buffer = vec![0u8; config.read_buffer_size];
                let mut keepalive_interval = tokio::time::interval(Duration::from_secs(config.keepalive_interval_secs));

                loop {
                    tokio::select! {
                        result = reader.read(&mut buffer) => {
                            match result {
                                Ok(0) => {
                                    tracing::debug!("[TcpServer] Connection {} closed by peer", addr);
                                    break;
                                }
                                Ok(n) => {
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
                                                    tracing::error!("[TcpServer] Failed to publish event: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("[TcpServer] Parse error from {}: {}", addr, e);
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("[TcpServer] Read error from {}: {}", addr, e);
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
                                tracing::warn!("[TcpServer] Connection {} keepalive timeout", addr);
                                break;
                            }
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
                        tracing::error!("[TcpServer] Failed to publish DeviceOffline: {}", e);
                    }
                }
            }
            Ok(Ok(0)) => {
                tracing::debug!("[TcpServer] Empty connection from {}", addr);
            }
            Ok(Err(e)) => {
                tracing::error!("[TcpServer] Peek error from {}: {}", addr, e);
            }
            Err(_) => {
                tracing::warn!("[TcpServer] Connection timeout from {}", addr);
            }
            _ => {}
        }

        Ok(())
    }
}
