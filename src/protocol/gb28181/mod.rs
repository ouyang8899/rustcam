mod adapter;
pub mod auth;
mod ptz;
pub mod audio;
pub mod rtp;
pub mod tcp_audio_pool;

pub use adapter::Gb28181Adapter;
pub use tcp_audio_pool::{TcpAudioServerPool, TcpAudioServerConfig, TcpAudioConnection};

use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

static UDP_SENDER: OnceCell<Arc<tokio::net::UdpSocket>> = OnceCell::new();

pub fn set_udp_sender(socket: Arc<tokio::net::UdpSocket>) { UDP_SENDER.set(socket).ok(); }
pub fn get_udp_sender() -> Option<&'static Arc<tokio::net::UdpSocket>> { UDP_SENDER.get() }

static GB28181_PLATFORM_CONFIG: OnceCell<(String, String, u16)> = OnceCell::new();

pub fn set_gb28181_platform_config(server_gb_id: String, ip: String, port: u16) {
    GB28181_PLATFORM_CONFIG.set((server_gb_id, ip, port)).ok();
}
pub fn get_gb28181_platform_config() -> &'static OnceCell<(String, String, u16)> {
    &GB28181_PLATFORM_CONFIG
}

static TCP_AUDIO_SERVER_POOL: OnceCell<Arc<TcpAudioServerPool>> = OnceCell::new();

pub fn init_tcp_audio_server_pool(config: TcpAudioServerConfig) {
    TCP_AUDIO_SERVER_POOL.set(Arc::new(TcpAudioServerPool::new(config))).ok();
}

pub fn get_tcp_audio_server_pool() -> Option<&'static Arc<TcpAudioServerPool>> {
    TCP_AUDIO_SERVER_POOL.get()
}
