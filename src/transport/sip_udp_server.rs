use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use dashmap::DashMap;
use tokio::net::UdpSocket;
use tokio::sync::Mutex as TokioMutex;
use tokio_util::sync::CancellationToken;

use crate::protocol::gb28181::Gb28181Adapter;
use crate::protocol::adapter::SignalAdapter;

use crate::protocol::event::SignalEvent;
use crate::protocol::traits::ProtocolDeps;
use crate::infrastructure::event_bus::EventBus;
use crate::monitoring::Metrics;

#[derive(Clone)]
pub struct SipUdpServerConfig {
    pub bind_addr: String,
    pub max_packet_size: usize,
    pub adapter_timeout_secs: u64,
    pub max_adapters: usize,
}

impl Default for SipUdpServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:5061".to_string(),
            max_packet_size: 65536,
            adapter_timeout_secs: 300,
            max_adapters: 10000,
        }
    }
}

struct UdpAdapterEntry {
    adapter: Arc<tokio::sync::Mutex<Box<dyn SignalAdapter + Send + Sync>>>,
    last_active: std::sync::atomic::AtomicI64,
}

pub struct SipUdpServer {
    socket: Arc<UdpSocket>,
    event_bus: Arc<EventBus>,
    config: SipUdpServerConfig,
    metrics: Arc<Metrics>,
    deps: ProtocolDeps,
    adapters: Arc<DashMap<String, Arc<UdpAdapterEntry>>>,
    shutdown_token: CancellationToken,
    join_handles: Arc<TokioMutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl SipUdpServer {
    pub async fn new(
        config: SipUdpServerConfig,
        event_bus: Arc<EventBus>,
        metrics: Arc<Metrics>,
        deps: ProtocolDeps,
    ) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind(&config.bind_addr).await?;
        tracing::info!("[SipUdpServer] Bound to {}", config.bind_addr);
        Ok(Self {
            socket: Arc::new(socket),
            event_bus,
            config,
            metrics,
            deps,
            adapters: Arc::new(DashMap::new()),
            shutdown_token: CancellationToken::new(),
            join_handles: Arc::new(TokioMutex::new(Vec::new())),
        })
    }

    pub fn socket(&self) -> Arc<UdpSocket> {
        self.socket.clone()
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("[SipUdpServer] Listening on {}", self.config.bind_addr);
        
        let cleanup_handle = self.start_cleanup_task();
        self.join_handles.lock().await.push(cleanup_handle);

        let mut buf = vec![0u8; self.config.max_packet_size];

        loop {
            tokio::select! {
                result = self.socket.recv_from(&mut buf) => {
                    match result {
                        Ok((n, addr)) => {
                            if self.adapters.len() >= self.config.max_adapters {
                                tracing::warn!("[SipUdpServer] Max adapters limit reached, dropping packet from {}", addr);
                                continue;
                            }
                            
                            self.metrics.bytes_received(n);
                            let data = buf[..n].to_vec();
                            let addr_key = addr.to_string();

                            tracing::debug!("[SipUdpServer] Received {} bytes from {}", n, addr_key);

                            let entry = if let Some(existing) = self.adapters.get(&addr_key) {
                                tracing::debug!("[SipUdpServer] Found existing adapter for {}", addr_key);
                                existing.value().clone()
                            } else {
                                tracing::debug!("[SipUdpServer] Creating new SIP adapter for {}", addr_key);
                                match self.get_or_create_adapter(&addr_key).await {
                                    Ok(entry) => entry,
                                    Err(e) => {
                                        tracing::error!("[SipUdpServer] Failed to create adapter for {}: {}", addr_key, e);
                                        continue;
                                    }
                                }
                            };
                            entry.last_active.store(chrono::Utc::now().timestamp(), std::sync::atomic::Ordering::Relaxed);

                            let adapter = entry.adapter.clone();
                            let mut guard = adapter.lock().await;

                            match guard.parse(&data).await {
                                Ok(events) => {
                                    for event in events {
                                        self.metrics.event_received();

                                        if matches!(&event,
                                            SignalEvent::StopPlay { .. } |
                                            SignalEvent::DeviceOffline { .. }
                                        ) {
                                            tracing::info!("[SipUdpServer] Device {} sent disconnect, removing adapter", addr_key);
                                            let _ = guard.on_disconnected().await;
                                            drop(guard);
                                            self.adapters.remove(&addr_key);
                                            if let Err(e) = self.event_bus.publish(event).await {
                                                tracing::error!("[SipUdpServer] Failed to publish event: {}", e);
                                            }
                                            break;
                                        }

                                        if let Err(e) = self.event_bus.publish(event).await {
                                            tracing::error!("[SipUdpServer] Failed to publish event: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("[SipUdpServer] Parse error from {}: {}", addr, e);
                                    let _ = guard.on_disconnected().await;
                                    drop(guard);
                                    self.adapters.remove(&addr_key);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("[SipUdpServer] Recv error: {}", e);
                        }
                    }
                }
                _ = self.shutdown_token.cancelled() => {
                    tracing::info!("[SipUdpServer] Shutdown signal received");
                    break;
                }
            }
        }

        self.shutdown().await;
        Ok(())
    }

    async fn shutdown(&self) {
        tracing::info!("[SipUdpServer] Shutting down...");
        
        self.shutdown_token.cancel();
        
        let keys: Vec<String> = self.adapters.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            if let Some((_, entry)) = self.adapters.remove(&key) {
                let mut guard = entry.adapter.lock().await;
                let _ = guard.on_disconnected().await;
            }
        }

        let mut handles = self.join_handles.lock().await;
        for handle in handles.drain(..) {
            let _ = handle.await;
        }
        
        tracing::info!("[SipUdpServer] Shutdown complete");
    }

    async fn get_or_create_adapter(
        &self,
        addr_key: &str,
    ) -> anyhow::Result<Arc<UdpAdapterEntry>> {
        if let Some(entry) = self.adapters.get(addr_key) {
            tracing::debug!("[SipUdpServer] get_or_create: found existing for {}", addr_key);
            return Ok(entry.value().clone());
        }

        tracing::info!("[SipUdpServer] Creating new SIP adapter for {}", addr_key);

        let mut adapter: Box<dyn SignalAdapter + Send + Sync> = Box::new(Gb28181Adapter::new(
            self.deps.clone(),
            Arc::new(|_, _| {}),
            Arc::new(|_| {}),
        ));

        let addr: SocketAddr = addr_key.parse().map_err(|e| {
            anyhow::anyhow!("Invalid address {}: {}", addr_key, e)
        })?;
        let _ = adapter.set_udp_peer(addr).await;
        let _ = adapter.on_connected(addr).await;
        
        if let Ok(local_addr) = self.socket.local_addr() {
            tracing::info!("[SipUdpServer] Setting local_addr to {} for adapter", local_addr);
            let _ = adapter.set_local_addr(local_addr).await;
        }

        let entry = Arc::new(UdpAdapterEntry {
            adapter: Arc::new(tokio::sync::Mutex::new(adapter)),
            last_active: std::sync::atomic::AtomicI64::new(chrono::Utc::now().timestamp()),
        });

        self.adapters.insert(addr_key.to_string(), entry.clone());
        Ok(entry)
    }

    fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let adapters = self.adapters.clone();
        let timeout_secs = self.config.adapter_timeout_secs as i64;
        let event_bus = self.event_bus.clone();
        let shutdown_token = self.shutdown_token.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let keys: Vec<String> = adapters.iter().map(|e| e.key().clone()).collect();
                        for key in keys {
                            if let Some(entry) = adapters.get(&key) {
                                let mut guard = entry.value().adapter.lock().await;
                                if guard.needs_timeout_check() {
                                    tracing::info!("[SipUdpServer] Adapter {} registration timeout, handling", key);
                                    guard.handle_registration_timeout().await;
                                }
                            }
                        }

                        let now = chrono::Utc::now().timestamp();
                        let expired: Vec<String> = adapters.iter()
                            .filter(|entry| {
                                let last = entry.value().last_active.load(std::sync::atomic::Ordering::Relaxed);
                                now - last > timeout_secs
                            })
                            .map(|entry| entry.key().clone())
                            .collect();

                        for key in expired {
                            if let Some((_, entry)) = adapters.remove(&key) {
                                tracing::info!("[SipUdpServer] Adapter {} expired, cleaning up", key);
                                let mut guard = entry.adapter.lock().await;
                                let _ = guard.on_disconnected().await;
                                let event = SignalEvent::DeviceOffline {
                                    device_id: 0,
                                    device_tag: Some(key.clone()),
                                    reason: Some("UDP timeout".to_string()),
                                };
                                if let Err(e) = event_bus.publish(event).await {
                                    tracing::error!("[SipUdpServer] Failed to publish DeviceOffline: {}", e);
                                }
                            }
                        }
                    }
                    _ = shutdown_token.cancelled() => {
                        tracing::info!("[SipUdpServer] Cleanup task received shutdown signal");
                        break;
                    }
                }
            }
        })
    }
}