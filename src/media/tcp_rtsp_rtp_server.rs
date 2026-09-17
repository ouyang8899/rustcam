use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::infrastructure::event_bus::EventBus;
use crate::monitoring::Metrics;
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::event::SignalEvent;
use crate::protocol::rtsp::RtspServerAdapter;
use crate::protocol::traits::ProtocolDeps;

const KEEPALIVE_TICK_SECS: u64 = 10;
const EVENT_POLL_INTERVAL_MS: u64 = 20;
const READ_CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub enum TcpRtpEvent {
    NewConnection { addr: SocketAddr },
    Disconnected { addr: SocketAddr },
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
            connection_timeout: 300,
            keepalive_interval_secs: 30,
            rtsp_port: 554,
        }
    }
}

struct ConnectionEntry {
    adapter: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>>,
    device_id: AtomicI64,
    device_tag: Mutex<Option<String>>,
    connected_at: std::time::Instant,
    last_message_at: AtomicI64,
    remote_addr: SocketAddr,
    disconnected: AtomicBool,
    abort: CancellationToken,
    idle_timeout_secs: i64,
}

impl ConnectionEntry {
    fn new(
        adapter: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>>,
        remote_addr: SocketAddr,
        idle_timeout_secs: i64,
    ) -> Self {
        Self {
            adapter,
            device_id: AtomicI64::new(0),
            device_tag: Mutex::new(None),
            connected_at: std::time::Instant::now(),
            last_message_at: AtomicI64::new(chrono::Utc::now().timestamp()),
            remote_addr,
            disconnected: AtomicBool::new(false),
            abort: CancellationToken::new(),
            idle_timeout_secs,
        }
    }

    async fn update_device_tag(&self, tag: String) {
        *self.device_tag.lock().await = Some(tag);
    }

    async fn get_device_tag(&self) -> Option<String> {
        self.device_tag.lock().await.clone()
    }

    fn update_device_id(&self, id: i64) {
        self.device_id.store(id, Ordering::Relaxed);
    }

    fn get_device_id(&self) -> i64 {
        self.device_id.load(Ordering::Relaxed)
    }

    fn update_activity(&self) {
        self.last_message_at
            .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
    }

    fn is_idle(&self, idle_secs: i64) -> bool {
        chrono::Utc::now().timestamp() - self.last_message_at.load(Ordering::Relaxed) > idle_secs
    }

    fn try_mark_disconnected(&self) -> bool {
        !self.disconnected.swap(true, Ordering::AcqRel)
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
        tracing::info!("[TcpRtspRtpServer] 监听 {}", config.bind_addr);
        Ok(Self {
            listener,
            event_bus,
            config,
            metrics,
            protocol_deps,
            connections: Arc::new(DashMap::new()),
            shutdown_token: CancellationToken::new(),
            event_callback: RwLock::new(None),
        })
    }

    pub async fn set_event_callback(&self, callback: TcpRtpEventCallback) {
        *self.event_callback.write().await = Some(callback);
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        tracing::info!("[TcpRtspRtpServer] 启动 {}", self.config.bind_addr);

        let cleanup_handle = self.start_cleanup_task();

        let mut connections_join: JoinSet<()> = JoinSet::new();

        loop {
            tokio::select! {
                biased;

                accept = self.listener.accept() => {
                    match accept {
                        Ok((stream, addr)) => {
                            if self.connections.len() >= self.config.max_connections {
                                tracing::warn!(
                                    "[TcpRtspRtpServer] 连接数达上限，拒绝 {}",
                                    addr
                                );
                                drop(stream);
                                continue;
                            }

                            if let Some(cb) = self.event_callback.read().await.as_ref() {
                                cb(TcpRtpEvent::NewConnection { addr });
                            }

                            self.metrics.connection_opened();

                            let this = self.clone();
                            let shutdown_token = self.shutdown_token.clone();
                            connections_join.spawn(async move {
                                if let Err(e) = this
                                    .handle_connection(stream, addr, shutdown_token)
                                    .await
                                {
                                    tracing::error!(
                                        "[TcpRtspRtpServer] 连接 {} 出错: {}",
                                        addr,
                                        e
                                    );
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("[TcpRtspRtpServer] accept 失败: {}", e);
                        }
                    }
                }

                Some(_res) = connections_join.join_next(), if !connections_join.is_empty() => {
                }

                _ = self.shutdown_token.cancelled() => {
                    tracing::info!("[TcpRtspRtpServer] 收到关闭信号");
                    break;
                }
            }
        }

        self.shutdown().await;
        cleanup_handle.abort();
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            async {
                while connections_join.join_next().await.is_some() {}
            },
        )
        .await;

        Ok(())
    }

    async fn shutdown(&self) {
        tracing::info!("[TcpRtspRtpServer] 关闭中...");
        self.shutdown_token.cancel();

        let entries: Vec<(String, Arc<ConnectionEntry>)> = self
            .connections
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();

        for (_, entry) in &entries {
            entry.abort.cancel();
        }

        for (_, entry) in entries {
            if !entry.try_mark_disconnected() {
                continue;
            }
            Self::finalize_connection(&entry, &self.event_bus, &self.event_callback, "server shutdown")
                .await;
            self.metrics.connection_closed();
        }

        self.connections.clear();
        tracing::info!("[TcpRtspRtpServer] 关闭完成");
    }

    fn start_cleanup_task(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(KEEPALIVE_TICK_SECS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        this.cleanup_idle_connections().await;
                    }
                    _ = this.shutdown_token.cancelled() => {
                        break;
                    }
                }
            }
        })
    }

    async fn cleanup_idle_connections(&self) {
        let now = chrono::Utc::now().timestamp();

        let candidates: Vec<(String, Arc<ConnectionEntry>)> = self
            .connections
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();

        for (key, entry) in candidates {
            let idle_secs = entry.idle_timeout_secs;
            let is_idle = now - entry.last_message_at.load(Ordering::Relaxed) > idle_secs;

            if is_idle {
                if !entry.try_mark_disconnected() {
                    self.connections.remove(&key);
                    continue;
                }
                tracing::info!(
                    "[TcpRtspRtpServer] 连接 {} 空闲超时 ({}s)，清理",
                    key,
                    idle_secs
                );
                self.connections.remove(&key);
                entry.abort.cancel();

                Self::finalize_connection(
                    &entry,
                    &self.event_bus,
                    &self.event_callback,
                    "idle timeout",
                )
                .await;
                self.metrics.connection_closed();
            }
        }
    }

    async fn finalize_connection(
        entry: &Arc<ConnectionEntry>,
        event_bus: &Arc<EventBus>,
        event_callback: &RwLock<Option<TcpRtpEventCallback>>,
        reason: &str,
    ) {
        {
            let mut guard = entry.adapter.lock().await;
            if let Err(e) = guard.on_disconnected().await {
                tracing::warn!(
                    "[TcpRtspRtpServer] on_disconnected 出错 ({}): {}",
                    entry.remote_addr,
                    e
                );
            }
        }

        if let Some(tag) = entry.get_device_tag().await {
            let event = SignalEvent::DeviceOffline {
                device_id: entry.get_device_id(),
                device_tag: Some(tag),
                reason: Some(format!("{} from {}", reason, entry.remote_addr)),
            };
            if let Err(e) = event_bus.publish(event).await {
                tracing::error!("[TcpRtspRtpServer] 发布 DeviceOffline 失败: {}", e);
            }
        }

        if let Some(cb) = event_callback.read().await.as_ref() {
            cb(TcpRtpEvent::Disconnected {
                addr: entry.remote_addr,
            });
        }
    }

    async fn handle_connection(
        self: &Arc<Self>,
        stream: TcpStream,
        addr: SocketAddr,
        shutdown_token: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!("[TcpRtspRtpServer] 新连接来自 {}", addr);

        let _ = stream.set_nodelay(true);

        let (reader, write_half) = stream.into_split();

        let mut adapter: Box<dyn SignalAdapter + Send + Sync> =
            Box::new(RtspServerAdapter::new(self.protocol_deps.clone()));
        adapter.set_tcp_write(write_half);
        adapter.on_connected(addr).await?;

        let adapter_arc: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>> =
            Arc::new(Mutex::new(adapter));

        let idle_secs = {
            let g = adapter_arc.lock().await;
            g.idle_timeout().unwrap_or(300) as i64
        };

        let entry = Arc::new(ConnectionEntry::new(adapter_arc.clone(), addr, idle_secs));
        let key = addr.to_string();
        self.connections.insert(key.clone(), entry.clone());

        let result = Self::run_connection_loop(
            &entry,
            adapter_arc,
            reader,
            addr,
            &self.event_bus,
            &self.metrics,
            &shutdown_token,
        )
        .await;

        self.connections.remove(&key);
        if entry.try_mark_disconnected() {
            Self::finalize_connection(
                &entry,
                &self.event_bus,
                &self.event_callback,
                "connection closed",
            )
            .await;
            self.metrics.connection_closed();
        }

        result
    }

    async fn run_connection_loop(
        entry: &Arc<ConnectionEntry>,
        adapter_arc: Arc<Mutex<Box<dyn SignalAdapter + Send + Sync>>>,
        reader: tokio::net::tcp::OwnedReadHalf,
        addr: SocketAddr,
        event_bus: &Arc<EventBus>,
        metrics: &Arc<Metrics>,
        shutdown_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        let mut reader = BufReader::with_capacity(READ_CHUNK, reader);
        let mut buffer = vec![0u8; READ_CHUNK];

        let mut keepalive_tick =
            tokio::time::interval(Duration::from_secs(KEEPALIVE_TICK_SECS));
        keepalive_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        keepalive_tick.tick().await;

        let mut event_poll =
            tokio::time::interval(Duration::from_millis(EVENT_POLL_INTERVAL_MS));
        event_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        event_poll.tick().await;

        loop {
            tokio::select! {
                biased;

                _ = entry.abort.cancelled() => {
                    tracing::info!("[TcpRtspRtpServer] 连接 {} 被主动关闭", addr);
                    break;
                }

                _ = shutdown_token.cancelled() => {
                    tracing::info!("[TcpRtspRtpServer] 连接 {} 收到服务关闭信号", addr);
                    break;
                }

                read = reader.read(&mut buffer) => {
                    match read {
                        Ok(0) => {
                            tracing::debug!("[TcpRtspRtpServer] 连接 {} 被对端关闭", addr);
                            break;
                        }
                        Ok(n) => {
                            metrics.bytes_received(n);
                            entry.update_activity();

                            if tracing::enabled!(tracing::Level::TRACE) {
                                if let Ok(s) = std::str::from_utf8(&buffer[..n]) {
                                    tracing::trace!(
                                        "[TcpRtspRtpServer] RECV {} bytes from {}: {}",
                                        n, addr, s
                                    );
                                } else {
                                    tracing::trace!(
                                        "[TcpRtspRtpServer] RECV {} bytes from {} (binary)",
                                        n, addr
                                    );
                                }
                            }

                            let parse_result = {
                                let mut guard = adapter_arc.lock().await;
                                guard.parse(&buffer[..n]).await
                            };

                            match parse_result {
                                Ok(events) => {
                                    for event in &events {
                                        if let SignalEvent::DeviceRegister {
                                            device_id,
                                            ref device_tag,
                                            ..
                                        } = event
                                        {
                                            entry.update_device_id(*device_id);
                                            if let Some(tag) = device_tag {
                                                entry.update_device_tag(tag.clone()).await;
                                            }
                                        }
                                    }

                                    for event in events {
                                        metrics.event_received();
                                        if let Err(e) = event_bus.publish(event).await {
                                            tracing::error!(
                                                "[TcpRtspRtpServer] 发布事件失败: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "[TcpRtspRtpServer] 解析错误 ({}): {}",
                                        addr,
                                        e
                                    );
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "[TcpRtspRtpServer] 读取错误 ({}): {}",
                                addr,
                                e
                            );
                            break;
                        }
                    }
                }

                _ = event_poll.tick() => {
                    let events = {
                        let mut guard = adapter_arc.lock().await;
                        guard.parse(&[]).await.unwrap_or_default()
                    };
                    for event in events {
                        metrics.event_received();
                        if let Err(e) = event_bus.publish(event).await {
                            tracing::error!("[TcpRtspRtpServer] 发布事件失败: {}", e);
                        }
                    }
                }

                _ = keepalive_tick.tick() => {
                    let idle_limit = {
                        let guard = adapter_arc.lock().await;
                        guard.idle_timeout().unwrap_or(300) as i64
                    };
                    if entry.is_idle(idle_limit) {
                        tracing::warn!(
                            "[TcpRtspRtpServer] 连接 {} 空闲超过 {}s，断开",
                            addr,
                            idle_limit
                        );
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}