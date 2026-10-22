use std::net::SocketAddr;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::net::tcp::OwnedWriteHalf;
use crate::error::Result;
use crate::media::MediaNodeId;
use crate::protocol::event::{SignalEvent, ProtocolType};
use crate::protocol::traits::ProtocolDeps;
use crate::protocol::matcher::ProtocolMatcher;
use crate::protocol::adapter_manager::AdapterEntry;
use crate::domain::EventPublisher;

#[async_trait]
pub trait SignalAdapter: Send + Sync {
    async fn parse(&mut self, data: &[u8]) -> Result<Vec<SignalEvent>>;

    async fn on_connected(&mut self, addr: SocketAddr) -> Result<()>;

    async fn on_disconnected(&mut self) -> Result<()>;

    async fn send(&mut self, data: &[u8]) -> Result<()>;

    async fn send_interleaved(&mut self, _channel: u8, _payload: &[u8]) -> Result<()> {
        Ok(())
    }

    fn protocol_type(&self) -> ProtocolType;

    fn name(&self) -> &'static str;

    fn keepalive(&self) -> bool { true }

    fn idle_timeout(&self) -> Option<u64> { Some(60) }

    fn set_tcp_write(&mut self, _write: OwnedWriteHalf) {}

    async fn set_udp_peer(&mut self, _addr: SocketAddr) -> Result<()> { Ok(()) }

    async fn set_local_addr(&mut self, _addr: SocketAddr) -> Result<()> { Ok(()) }

    fn set_adapter_entry(&mut self, _entry: crate::protocol::adapter_manager::AdapterEntry) {}

    async fn start(&mut self, device_tag: &str, channel_tag: &str) -> Result<()>;

    async fn start_playback(&mut self, _device_tag: &str, _start_time: chrono::DateTime<chrono::Utc>, _end_time: chrono::DateTime<chrono::Utc>) -> Result<()> {
        Ok(())
    }

    async fn send_notify(&mut self, _device_tag: &str, _event_type: &str, _content: &str) -> Result<()> {
        Ok(())
    }

    async fn ptz_control(&mut self, channel_id: &str, command: &crate::protocol::event::PtzCommand, speed: Option<u8>) -> Result<()>;

    async fn cleanup_expired_subscriptions(&self) -> usize { 0 }

    async fn send_device_config_query(&self, _config_type: &str) -> Result<String> {
        Ok(String::new())
    }

    async fn start_audio_talk_v2(
        &mut self,
        _device_tag: &str,
        _channel_tag: &str,
        _device_ip: &str,
        _device_audio_port: u16,
        _platform_audio_port: u16,
        _transport: crate::protocol::event::TransportType,
    ) -> Result<()> {
        Ok(())
    }

    async fn stop_audio_talk_v2(&mut self, _device_tag: &str) -> Result<()> {
        Ok(())
    }

    async fn send_audio_to_device(&mut self, _device_tag: &str, _pcm_data: &[i16]) -> Result<()> {
        Ok(())
    }

    async fn start_audio_talk_internal(&mut self, _device_tag: &str, _node_id: MediaNodeId) -> Result<()> {
        Ok(())
    }

    async fn stop_audio_talk_internal(&mut self, _node_id: MediaNodeId) -> Result<()> {
        Ok(())
    }

    async fn handle_event(&mut self, _event: SignalEvent, _event_bus: Arc<dyn EventPublisher>) {}

    async fn publish_event(&mut self, _event: SignalEvent) {}

    async fn recv_audio_from_device_v2(&mut self, _device_tag: &str) -> Result<Option<Vec<i16>>> {
        Ok(None)
    }

    async fn is_audio_talk_active(&self, _device_tag: &str) -> bool {
        false
    }

    async fn set_tcp_stream_for_talk(&mut self, _device_tag: &str, _stream: Arc<tokio::net::TcpStream>) -> Result<()> {
        Ok(())
    }

    fn needs_timeout_check(&self) -> bool { false }

    async fn handle_registration_timeout(&mut self) {}
}

pub trait AdapterFactory: Send + Sync {

    fn create(
        &self,
        deps: ProtocolDeps,
        register_fn: Arc<dyn Fn(String, AdapterEntry) + Send + Sync>,
        unregister_fn: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Box<dyn SignalAdapter>;

    fn protocol_type(&self) -> ProtocolType;

    fn name(&self) -> &'static str;
}

struct ClosureAdapterFactory {
    create_fn: Box<dyn Fn(ProtocolDeps, Arc<dyn Fn(String, AdapterEntry) + Send + Sync>, Arc<dyn Fn(String) + Send + Sync>) -> Box<dyn SignalAdapter> + Send + Sync>,
    protocol_type: ProtocolType,
    name: &'static str,
}

impl AdapterFactory for ClosureAdapterFactory {
    fn create(
        &self,
        deps: ProtocolDeps,
        register_fn: Arc<dyn Fn(String, AdapterEntry) + Send + Sync>,
        unregister_fn: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Box<dyn SignalAdapter> {
        (self.create_fn)(deps, register_fn, unregister_fn)
    }
    fn protocol_type(&self) -> ProtocolType {
        self.protocol_type.clone()
    }
    fn name(&self) -> &'static str {
        self.name
    }
}

pub struct AdapterRegistry {
    factories: Vec<Box<dyn AdapterFactory>>,
    matchers: Vec<(ProtocolMatcher, ProtocolType)>,
}

impl AdapterRegistry {

    pub fn new() -> Self {
        Self {
            factories: Vec::new(),
            matchers: Vec::new(),
        }
    }

    pub fn register_with_deps<F>(
        &mut self,
        factory: F,
        protocol_type: ProtocolType,
        name: &'static str,
    )
    where
        F: Fn(ProtocolDeps, Arc<dyn Fn(String, AdapterEntry) + Send + Sync>, Arc<dyn Fn(String) + Send + Sync>) -> Box<dyn SignalAdapter> + Send + Sync + 'static,
    {
        self.factories.push(Box::new(ClosureAdapterFactory {
            create_fn: Box::new(factory),
            protocol_type,
            name,
        }));
    }

    pub fn register_matcher(
        &mut self,
        matcher: ProtocolMatcher,
        protocol: ProtocolType,
    ) {
        self.matchers.push((matcher, protocol));
    }

    pub fn match_protocol(&self, first_packet: &[u8]) -> Option<ProtocolType> {
        for (matcher, protocol) in &self.matchers {
            if matcher.matches(first_packet) {
                return Some(protocol.clone());
            }
        }
        None
    }

    pub fn create_adapter(
        &self,
        protocol: &ProtocolType,
        deps: ProtocolDeps
    ) -> Option<Box<dyn SignalAdapter>> {
        let register_fn = deps.register_fn.clone();
        let unregister_fn = deps.unregister_fn.clone();
        for factory in &self.factories {
            if &factory.protocol_type() == protocol {
                return Some(factory.create(deps, register_fn, unregister_fn));
            }
        }
        None
    }

    pub fn registered_protocols(&self) -> Vec<ProtocolType> {
        self.factories.iter().map(|f| f.protocol_type()).collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AdapterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterRegistry")
            .field("factories_count", &self.factories.len())
            .field("matchers_count", &self.matchers.len())
            .finish()
    }
}