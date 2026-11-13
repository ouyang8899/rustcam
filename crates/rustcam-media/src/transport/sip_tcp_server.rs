use std::sync::Arc;
use std::net::SocketAddr;
use std::time::Duration;
use dashmap::DashMap;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use crate::protocol::gb28181::Gb28181Adapter;
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::adapter_manager;
use crate::protocol::traits::ProtocolDeps;
use crate::protocol::event::SignalEvent;
use crate::infrastructure::event_bus::EventBus;
use crate::monitoring::Metrics;

#[derive(Clone, Debug)]
pub struct SipTcpServerConfig {
    pub bind_addr: String,
    pub max_connections: usize,
    pub read_buffer_size: usize,
    /// Fallback idle timeout (seconds) used when the protocol adapter does not
    /// define its own. Connections are reaped once they stop sending traffic,
    /// never because of their age.
    pub connection_timeout: u64,
    pub keepalive_interval_secs: u64,
    pub read_timeout_secs: u64,
}

impl Default for SipTcpServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:5060".to_string(),
            max_connections: 1000,
            read_buffer_size: 65536,
            connection_timeout: 60,
            keepalive_interval_secs: 30,
            read_timeout_secs: 300,
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

pub struct SipTcpServer {
    listener: TcpListener,
    event_bus: Arc<EventBus>,
    config: SipTcpServerConfig,
    metrics: Arc<Metrics>,
    protocol_deps: ProtocolDeps,
    connections: Arc<DashMap<String, Arc<TcpConnectionEntry>>>,
    connection_semaphore: Arc<Semaphore>,
    shutdown_token: CancellationToken,
    join_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl SipTcpServer {
    pub async fn new(
        config: SipTcpServerConfig,
        event_bus: Arc<EventBus>,
        metrics: Arc<Metrics>,
        protocol_deps: ProtocolDeps,
    ) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(&config.bind_addr).await?;
        let max_connections = config.max_connections;
        Ok(Self {
            listener,
            event_bus,
            config,
            metrics,
            protocol_deps,
            connections: Arc::new(DashMap::new()),
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
            shutdown_token: CancellationToken::new(),
            join_handles: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub async fn run(&self) -> anyhow::Result<()> {

        let cleanup_handle = self.start_cleanup_task();
        self.join_handles.lock().await.push(cleanup_handle);

        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            if self.connection_semaphore.available_permits() == 0 {
                                tracing::warn!("[SipTcpServer] Connection limit reached, rejecting {}", addr);
                                drop(stream);
                                continue;
                            }

                            self.metrics.connection_opened();
                            let permit = self.connection_semaphore.clone().acquire_owned().await.unwrap();
                            let event_bus = self.event_bus.clone();
                            let config = self.config.clone();
                            let metrics = self.metrics.clone();
                            let deps = self.protocol_deps.clone();
                            let connections = self.connections.clone();
                            let shutdown_token = self.shutdown_token.clone();

                            let handle = tokio::spawn(async move {
                                drop(permit);
                                if let Err(e) = Self::handle_connection(
                                    stream, addr, event_bus, config, metrics, deps, connections, shutdown_token,
                                ).await {
                                    tracing::error!("[SipTcpServer] Connection {} error: {}", addr, e);
                                }
                            });
                            self.join_handles.lock().await.push(handle);
                        }
                        Err(e) => {
                            tracing::error!("[SipTcpServer] Accept error: {}", e);
                        }
                    }
                }
                _ = self.shutdown_token.cancelled() => {
                    break;
                }
            }
        }

        self.shutdown().await;
        Ok(())
    }

    async fn shutdown(&self) {

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

    }

    fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let connections = self.connections.clone();
        let event_bus = self.event_bus.clone();
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
                            connection_timeout,
                        ).await;
                    }
                    _ = shutdown_token.cancelled() => {
                        break;
                    }
                }
            }
        })
    }

    /// Drops connections that stopped exchanging SIP traffic.
    ///
    /// Staleness is measured by inactivity, never by connection age: a device
    /// that keeps sending keepalives must stay connected for as long as it
    /// likes, while a connection that went silent is reaped.
    async fn cleanup_connections(
        connections: &Arc<DashMap<String, Arc<TcpConnectionEntry>>>,
        event_bus: &Arc<EventBus>,
        connection_timeout: u64,
    ) {
        let addrs: Vec<String> = connections.iter()
            .map(|e| e.key().clone())
            .collect();

        for addr_key in addrs {

            let entry_arc = { connections.get(&addr_key).map(|e| e.value().clone()) };
            let Some(entry_arc) = entry_arc else { continue; };
            let adapter = entry_arc.adapter.clone();
            let idle_timeout = {
                let guard = adapter.lock().await;
                if guard.needs_timeout_check() {
                    drop(guard);
                    let mut guard = adapter.lock().await;
                    guard.handle_registration_timeout().await;
                    guard.idle_timeout().unwrap_or(connection_timeout) as i64
                } else {
                    guard.idle_timeout().unwrap_or(connection_timeout) as i64
                }
            };

            if entry_arc.is_idle(idle_timeout) {
                if let Some((_, entry)) = connections.remove(&addr_key) {
                    tracing::warn!(
                        "[SipTcpServer] Removing idle connection {} (open for {}s, no traffic for {}s)",
                        addr_key,
                        entry.connected_at.elapsed().as_secs(),
                        idle_timeout
                    );

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
                            tracing::error!("[SipTcpServer] Failed to publish DeviceOffline: {}", e);
                        }
                    }
                }
            }
        }
    }

    async fn handle_connection(
        stream: TcpStream,
        addr: SocketAddr,
        event_bus: Arc<EventBus>,
        config: SipTcpServerConfig,
        metrics: Arc<Metrics>,
        deps: ProtocolDeps,
        connections: Arc<DashMap<String, Arc<TcpConnectionEntry>>>,
        shutdown_token: CancellationToken,
    ) -> anyhow::Result<()> {

        let mut adapter: Box<dyn SignalAdapter + Send + Sync> = Box::new(Gb28181Adapter::new(
            deps,
            Arc::new(|device_tag, entry| {
                adapter_manager::set_adapter(device_tag, entry);
            }),
            Arc::new(|device_tag| {
                adapter_manager::remove_adapter(&device_tag);
            }),
        ));

        adapter.on_connected(addr).await?;

        let local_addr = stream.local_addr().ok();
        let (reader, write_half) = stream.into_split();
        adapter.set_tcp_write(write_half);

        let adapter_arc: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>> = Arc::new(Mutex::new(adapter));
        {
            let mut guard = adapter_arc.lock().await;
            guard.set_adapter_entry(adapter_arc.clone());
            if let Some(local) = local_addr {
                let _ = guard.set_local_addr(local).await;
            }
        }
        let entry = Arc::new(TcpConnectionEntry::new(adapter_arc.clone(), addr));

        connections.insert(addr.to_string(), entry.clone());

        let mut reader = BufReader::new(reader);
        let mut buffer = vec![0u8; config.read_buffer_size];
        let mut keepalive_interval = tokio::time::interval(Duration::from_secs(config.keepalive_interval_secs));
        let read_timeout = Duration::from_secs(config.read_timeout_secs);

        loop {
            tokio::select! {
                result = tokio::time::timeout(read_timeout, reader.read(&mut buffer)) => {
                    match result {
                        Ok(Ok(0)) => {
                            break;
                        }
                        Ok(Ok(n)) => {
                            metrics.bytes_received(n);
                            entry.update_activity();

                            if let Ok(_msg) = String::from_utf8(buffer[..n].to_vec()) {
                            } else {
                            }

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
                                            tracing::error!("[SipTcpServer] Failed to publish event: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("[SipTcpServer] Parse error from {}: {}", addr, e);
                                    break;
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::error!("[SipTcpServer] Read error from {}: {}", addr, e);
                            break;
                        }
                        Err(_) => {
                            tracing::warn!("[SipTcpServer] Connection {} read timeout", addr);
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
                        tracing::warn!("[SipTcpServer] Connection {} keepalive timeout", addr);
                        break;
                    }
                }
                _ = shutdown_token.cancelled() => {
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
                tracing::error!("[SipTcpServer] Failed to publish DeviceOffline: {}", e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI64;

    const REMOTE: &str = "192.168.1.211:45957";

    fn test_entry(age: Duration, idle: Duration) -> Arc<TcpConnectionEntry> {
        let adapter: Box<dyn SignalAdapter + Send + Sync> = Box::new(Gb28181Adapter::new(
            ProtocolDeps::for_test(),
            Arc::new(|_, _| {}),
            Arc::new(|_| {}),
        ));

        Arc::new(TcpConnectionEntry {
            adapter: Arc::new(Mutex::new(adapter)),
            device_tag: Mutex::new(None),
            connected_at: std::time::Instant::now() - age,
            last_message_at: AtomicI64::new(
                chrono::Utc::now().timestamp() - idle.as_secs() as i64,
            ),
            remote_addr: REMOTE.parse().unwrap(),
        })
    }

    #[tokio::test]
    async fn cleanup_keeps_long_lived_active_connection() {
        let connections = Arc::new(DashMap::new());
        connections.insert(
            REMOTE.to_string(),
            test_entry(Duration::from_secs(3600), Duration::from_secs(0)),
        );

        SipTcpServer::cleanup_connections(&connections, &Arc::new(EventBus::new()), 60).await;

        assert!(
            connections.contains_key(REMOTE),
            "an active connection must not be dropped just because it has been open for a long time"
        );
    }

    #[tokio::test]
    async fn cleanup_removes_connection_without_traffic() {
        let connections = Arc::new(DashMap::new());
        connections.insert(
            REMOTE.to_string(),
            test_entry(Duration::from_secs(60), Duration::from_secs(3600)),
        );

        SipTcpServer::cleanup_connections(&connections, &Arc::new(EventBus::new()), 60).await;

        assert!(
            !connections.contains_key(REMOTE),
            "a connection that stopped sending traffic must be dropped"
        );
    }
}
