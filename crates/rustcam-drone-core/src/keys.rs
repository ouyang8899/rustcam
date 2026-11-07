pub const CONTROL_KEY_PREFIX: &str = "control:";
pub const ADAPTER_INSTANCE_PREFIX: &str = "adapter:instance:";
pub const ADAPTER_DEVICE_PREFIX: &str = "adapter:device:";
pub const SHADOW_PREFIX: &str = "shadow:";
pub const CMD_IDEM_PREFIX: &str = "cmd:idem:";
pub const GEOFENCE_VERSION_KEY: &str = "geofence:version";

pub const STREAM_TELEMETRY: &str = "telemetry:default";
pub const STREAM_EVENTS: &str = "drone:events";
pub const STREAM_CMD_REPLY: &str = "cmd:reply";
pub const STREAM_RISK_ALERTS: &str = "risk:alerts";
pub const STREAM_ARCHIVE_TELEMETRY: &str = "archive:telemetry";
pub const STREAM_GEOFENCE_UPDATED: &str = "geofence:updated";

pub fn control_key(drone_tag: &str, control_type: &str) -> String {
    format!("{CONTROL_KEY_PREFIX}{drone_tag}:{control_type}")
}

pub fn adapter_instance_key(adapter_id: &str) -> String {
    format!("{ADAPTER_INSTANCE_PREFIX}{adapter_id}")
}

pub fn adapter_device_key(drone_tag: &str) -> String {
    format!("{ADAPTER_DEVICE_PREFIX}{drone_tag}")
}

pub fn shadow_key(drone_tag: &str) -> String {
    format!("{SHADOW_PREFIX}{drone_tag}")
}

pub fn cmd_idem_key(cmd_id: &str) -> String {
    format!("{CMD_IDEM_PREFIX}{cmd_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_built_from_drone_tag() {
        assert_eq!(control_key("drone-a", "flight"), "control:drone-a:flight");
        assert_eq!(shadow_key("drone-a"), "shadow:drone-a");
        assert_eq!(adapter_device_key("drone-a"), "adapter:device:drone-a");
    }

    #[test]
    fn keys_are_namespaced() {
        assert_eq!(
            control_key("drone-001", "flight"),
            "control:drone-001:flight"
        );
        assert_eq!(adapter_instance_key("a-1"), "adapter:instance:a-1");
        assert_eq!(adapter_device_key("drone-001"), "adapter:device:drone-001");
        assert_eq!(shadow_key("drone-001"), "shadow:drone-001");
        assert_eq!(cmd_idem_key("c-1"), "cmd:idem:c-1");
        assert_eq!(GEOFENCE_VERSION_KEY, "geofence:version");
    }

    #[test]
    fn stream_names_are_stable() {
        assert_eq!(STREAM_TELEMETRY, "telemetry:default");
        assert_eq!(STREAM_ARCHIVE_TELEMETRY, "archive:telemetry");
        assert_eq!(STREAM_GEOFENCE_UPDATED, "geofence:updated");
    }
}
