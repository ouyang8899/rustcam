pub mod common {
    tonic::include_proto!("drone.v1.common");
}

pub mod link {
    tonic::include_proto!("drone.v1.link");
}

pub mod control {
    tonic::include_proto!("drone.v1.control");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_message_has_expected_fields() {
        let t = common::Telemetry::default();
        assert_eq!(t.device_id, "");
        assert_eq!(t.ts_ms, 0);
        assert_eq!(t.seq, 0);
    }

    #[test]
    fn oneof_payload_paths_are_reachable() {
        let m = link::AdapterMessage {
            payload: Some(link::adapter_message::Payload::Heartbeat(
                link::Heartbeat {
                    ts_ms: 1,
                    device_count: 2,
                    inflight_commands: 0,
                    adapter_state: "ok".into(),
                },
            )),
        };
        assert!(matches!(
            m.payload,
            Some(link::adapter_message::Payload::Heartbeat(_))
        ));
    }
}
