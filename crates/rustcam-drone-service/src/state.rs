use std::sync::Arc;

use dashmap::DashMap;
use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_proto::link::service_message::Payload;
use tokio::sync::{broadcast, mpsc};

use crate::config::DroneConfig;

pub type AdapterTx = mpsc::Sender<Payload>;

#[derive(Clone)]
pub struct AdapterEntry {
    pub tx: AdapterTx,
    pub generation: u64,
    pub protocol: String,
    pub connected_at_ms: i64,
}

#[derive(Clone)]
pub struct DroneState {
    pub config: Arc<DroneConfig>,
    pub adapters: Arc<DashMap<String, AdapterEntry>>,
    pub device_to_adapter: Arc<DashMap<String, String>>,
    pub adapter_generation: Arc<DashMap<String, u64>>,
    pub event_tx: broadcast::Sender<DroneEvent>,
}

impl DroneState {
    pub fn new(config: DroneConfig) -> DroneState {
        let (event_tx, _) = broadcast::channel(4096);
        DroneState {
            config: Arc::new(config),
            adapters: Arc::new(DashMap::new()),
            device_to_adapter: Arc::new(DashMap::new()),
            adapter_generation: Arc::new(DashMap::new()),
            event_tx,
        }
    }

    /// 自增并返回该 adapter 的连接代次；用于踢掉旧连接。
    pub fn next_generation(&self, adapter_id: &str) -> u64 {
        let mut entry = self.adapter_generation.entry(adapter_id.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// 当前 adapter 的代次；未注册返回 None。
    pub fn generation_of(&self, adapter_id: &str) -> Option<u64> {
        self.adapter_generation.get(adapter_id).map(|v| *v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_increases_per_registration() {
        let state = DroneState::new(DroneConfig::default());
        assert_eq!(state.next_generation("a-1"), 1);
        assert_eq!(state.next_generation("a-1"), 2);
        assert_eq!(state.next_generation("a-2"), 1);
        assert_eq!(state.generation_of("a-1"), Some(2));
        assert_eq!(state.generation_of("nope"), None);
    }
}
