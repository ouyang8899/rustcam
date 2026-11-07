use crate::telemetry::Telemetry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DroneEvent {
    Telemetry(Telemetry),
    Alert {
        drone_tag: String,
        rule: String,
        level: String,
        message: String,
    },
    Control {
        drone_tag: String,
        control_type: String,
        holder: String,
        priority: i32,
        expire_at_ms: i64,
        action: String,
    },
    CommandReply {
        cmd_id: String,
        drone_tag: String,
        status: String,
        message: String,
    },
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
            DroneEvent::Telemetry(t) => {
                serde_json::to_value(t).unwrap_or_else(|_| serde_json::json!({}))
            }
            DroneEvent::Alert {
                drone_tag,
                rule,
                level,
                message,
            } => serde_json::json!({
                "drone_tag": drone_tag,
                "rule": rule,
                "level": level,
                "message": message,
            }),
            DroneEvent::Control {
                drone_tag,
                control_type,
                holder,
                priority,
                expire_at_ms,
                action,
            } => {
                serde_json::json!({
                    "drone_tag": drone_tag,
                    "control_type": control_type,
                    "holder": holder,
                    "priority": priority,
                    "expire_at_ms": expire_at_ms,
                    "action": action,
                })
            }
            DroneEvent::CommandReply {
                cmd_id,
                drone_tag,
                status,
                message,
            } => serde_json::json!({
                "cmd_id": cmd_id,
                "drone_tag": drone_tag,
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
        assert_eq!(
            DroneEvent::Telemetry(Telemetry::default()).msg_type(),
            "telemetry"
        );
        assert_eq!(
            DroneEvent::Alert {
                drone_tag: "d".into(),
                rule: "geofence".into(),
                level: "high".into(),
                message: "m".into()
            }
            .msg_type(),
            "alert"
        );
        assert_eq!(
            DroneEvent::Control {
                drone_tag: "d".into(),
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
                drone_tag: "d".into(),
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
            drone_tag: "drone-001".into(),
            ts_ms: 7,
            ..Default::default()
        });
        let v = evt.payload_value();
        assert_eq!(v["drone_tag"], "drone-001");
        assert_eq!(v["ts_ms"], 7);
    }

    #[test]
    fn alert_payload_has_contract_fields() {
        let evt = DroneEvent::Alert {
            drone_tag: "drone-001".into(),
            rule: "geofence".into(),
            level: "high".into(),
            message: "entered no_fly zone".into(),
        };
        let v = evt.payload_value();
        assert_eq!(v["rule"], "geofence");
        assert_eq!(v["level"], "high");
    }
}
