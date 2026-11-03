use crate::telemetry::Telemetry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DroneEvent {
    Telemetry(Telemetry),
    Alert { device_id: String, rule: String, level: String, message: String },
    Control {
        device_id: String,
        control_type: String,
        holder: String,
        priority: i32,
        expire_at_ms: i64,
        action: String,
    },
    CommandReply { cmd_id: String, device_id: String, status: String, message: String },
}

impl DroneEvent {
    pub fn msg_type(&self) -> &'static str {
        match self {
            DroneEvent::Telemetry(_) => "telemetry",
            DroneEvent::Alert { .. } => "alert",
            DroneEvent::Control { .. } => "control",
            DroneEvent::CommandReply { .. } => "command",
        }
    }

    /// 浏览器契约里的 `data` 内容：与 msg_type 对应的扁平对象。
    pub fn payload_value(&self) -> serde_json::Value {
        match self {
            DroneEvent::Telemetry(t) => serde_json::to_value(t).unwrap_or_else(|_| serde_json::json!({})),
            DroneEvent::Alert { device_id, rule, level, message } => serde_json::json!({
                "device_id": device_id,
                "rule": rule,
                "level": level,
                "message": message,
            }),
            DroneEvent::Control { device_id, control_type, holder, priority, expire_at_ms, action } => {
                serde_json::json!({
                    "device_id": device_id,
                    "control_type": control_type,
                    "holder": holder,
                    "priority": priority,
                    "expire_at_ms": expire_at_ms,
                    "action": action,
                })
            }
            DroneEvent::CommandReply { cmd_id, device_id, status, message } => serde_json::json!({
                "cmd_id": cmd_id,
                "device_id": device_id,
                "status": status,
                "message": message,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_types_match_browser_contract() {
        assert_eq!(DroneEvent::Telemetry(Telemetry::default()).msg_type(), "telemetry");
        assert_eq!(
            DroneEvent::Alert {
                device_id: "d".into(),
                rule: "geofence".into(),
                level: "high".into(),
                message: "m".into()
            }
            .msg_type(),
            "alert"
        );
        assert_eq!(
            DroneEvent::Control {
                device_id: "d".into(),
                control_type: "flight".into(),
                holder: "alice".into(),
                priority: 10,
                expire_at_ms: 0,
                action: "acquired".into(),
            }
            .msg_type(),
            "control"
        );
        assert_eq!(
            DroneEvent::CommandReply {
                cmd_id: "c".into(),
                device_id: "d".into(),
                status: "acked".into(),
                message: String::new()
            }
            .msg_type(),
            "command"
        );
    }

    #[test]
    fn telemetry_payload_is_flat_object() {
        let evt = DroneEvent::Telemetry(Telemetry {
            device_id: "drone-001".into(),
            ts_ms: 7,
            ..Default::default()
        });
        let v = evt.payload_value();
        assert_eq!(v["device_id"], "drone-001");
        assert_eq!(v["ts_ms"], 7);
    }

    #[test]
    fn alert_payload_has_contract_fields() {
        let evt = DroneEvent::Alert {
            device_id: "drone-001".into(),
            rule: "geofence".into(),
            level: "high".into(),
            message: "entered no_fly zone".into(),
        };
        let v = evt.payload_value();
        assert_eq!(v["rule"], "geofence");
        assert_eq!(v["level"], "high");
    }
}
