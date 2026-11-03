use std::sync::Arc;

use rustcam_drone_core::keys;
use rustcam_drone_proto::link::{Register, RegisterAck};

use crate::infra::redis::DroneRedis;
use crate::state::{AdapterEntry, DroneState};

#[derive(Clone)]
pub struct Registry {
    pub state: DroneState,
    redis: Option<Arc<DroneRedis>>,
}

impl Registry {
    pub fn new(state: DroneState, redis: Option<Arc<DroneRedis>>) -> Registry {
        Registry { state, redis }
    }

    /// adapter 注册：分配设备、记录代次、返回 RegisterAck。
    /// 同一 adapter_id 重复注册时新连接胜出（代次自增，旧连接失效）。
    pub async fn register(&self, reg: &Register) -> RegisterAck {
        let generation = self.state.next_generation(&reg.adapter_id);
        let (tx, _rx) = tokio::sync::mpsc::channel::<
            rustcam_drone_proto::link::service_message::Payload,
        >(1024);

        let mut assigned = Vec::new();
        for device_id in &reg.devices {
            if self.state.device_to_adapter.contains_key(device_id) {
                continue;
            }
            self.state
                .device_to_adapter
                .insert(device_id.clone(), reg.adapter_id.clone());
            assigned.push(device_id.clone());

            if let Some(redis) = &self.redis {
                let key = keys::adapter_device_key(device_id);
                match redis.set_nx_ex(&key, &reg.adapter_id, 30).await {
                    Ok(true) => {}
                    Ok(false) => {
                        self.state.device_to_adapter.remove(device_id);
                        assigned.pop();
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, device = %device_id, "[Registry] redis claim failed");
                    }
                }
            }
        }

        self.state.adapters.insert(
            reg.adapter_id.clone(),
            AdapterEntry {
                tx,
                generation,
                protocol: reg.protocol.clone(),
                connected_at_ms: chrono::Utc::now().timestamp_millis(),
            },
        );

        RegisterAck {
            accepted: true,
            reason: String::new(),
            assigned_devices: assigned,
            heartbeat_interval_sec: self.state.config.adapter_heartbeat_sec as i32,
        }
    }

    /// 断开清理：仅当仍是当前代次时才清理。
    pub async fn unregister(&self, adapter_id: &str, generation: u64) {
        if !self.is_current(adapter_id, generation) {
            return;
        }
        let devices: Vec<String> = self
            .state
            .device_to_adapter
            .iter()
            .filter(|e| e.value() == adapter_id)
            .map(|e| e.key().clone())
            .collect();
        for d in devices {
            self.state.device_to_adapter.remove(&d);
            if let Some(redis) = &self.redis {
                let _ = redis.expire(&keys::adapter_device_key(&d), 1).await;
            }
        }
        self.state.adapters.remove(adapter_id);
    }

    /// 代次校验：旧连接的上行消息必须被丢弃。
    pub fn is_current(&self, adapter_id: &str, generation: u64) -> bool {
        matches!(self.state.generation_of(adapter_id), Some(g) if g == generation)
    }

    pub fn devices_of(&self, adapter_id: &str) -> Vec<String> {
        self.state
            .device_to_adapter
            .iter()
            .filter(|e| e.value() == adapter_id)
            .map(|e| e.key().clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use rustcam_drone_proto::link::Register;

    fn registry() -> Registry {
        Registry::new(DroneState::new(DroneConfig::default()), None)
    }

    fn register_msg(adapter_id: &str, devices: Vec<&str>) -> Register {
        Register {
            adapter_id: adapter_id.into(),
            protocol: "mavlink".into(),
            version: "0.1.0".into(),
            devices: devices.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn register_assigns_devices() {
        let r = registry();
        let ack = r
            .register(&register_msg("a-1", vec!["drone-001", "drone-002"]))
            .await;
        assert!(ack.accepted);
        assert_eq!(ack.assigned_devices.len(), 2);
        assert_eq!(r.devices_of("a-1").len(), 2);
        assert!(ack.heartbeat_interval_sec > 0);
    }

    #[tokio::test]
    async fn re_register_invalidates_old_generation() {
        let r = registry();
        let reg = register_msg("a-1", vec!["drone-001"]);
        r.register(&reg).await;
        let gen1 = r.state.generation_of("a-1").unwrap();
        r.register(&reg).await;
        let gen2 = r.state.generation_of("a-1").unwrap();
        assert!(gen2 > gen1);
        assert!(r.is_current("a-1", gen2));
        assert!(!r.is_current("a-1", gen1));
    }

    #[tokio::test]
    async fn unregister_clears_device_mapping() {
        let r = registry();
        r.register(&register_msg("a-1", vec!["drone-001"])).await;
        let gen = r.state.generation_of("a-1").unwrap();
        r.unregister("a-1", gen).await;
        assert!(r.devices_of("a-1").is_empty());
    }

    #[tokio::test]
    async fn device_owned_by_other_adapter_is_not_assigned() {
        let r = registry();
        r.register(&register_msg("a-1", vec!["drone-001"])).await;
        let ack = r.register(&register_msg("a-2", vec!["drone-001"])).await;
        assert!(ack.assigned_devices.is_empty());
        assert_eq!(r.devices_of("a-2").len(), 0);
    }
}
