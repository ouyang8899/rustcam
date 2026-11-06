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
            payload: Some(link::adapter_message::Payload::Heartbeat(link::Heartbeat {
                ts_ms: 1,
                device_count: 2,
                inflight_commands: 0,
                adapter_state: "ok".into(),
            })),
        };
        assert!(matches!(
            m.payload,
            Some(link::adapter_message::Payload::Heartbeat(_))
        ));
    }

    #[test]
    fn telemetry_carries_unique_id() {
        let t = common::Telemetry {
            unique_id: "0123456789abcdef".into(),
            ..Default::default()
        };
        assert_eq!(t.unique_id, "0123456789abcdef");
    }

    #[test]
    fn register_carries_device_identities() {
        let reg = link::Register {
            adapter_id: "a-1".into(),
            protocol: "mavlink".into(),
            version: "0.1.0".into(),
            devices: vec!["drone-001".into()],
            identities: vec![link::DeviceIdentity {
                device_id: "drone-001".into(),
                unique_id: "0123456789abcdef".into(),
            }],
        };
        assert_eq!(reg.identities.len(), 1);
        assert_eq!(reg.identities[0].device_id, "drone-001");
        assert_eq!(reg.identities[0].unique_id, "0123456789abcdef");
    }

    #[test]
    fn register_ack_carries_assignments() {
        let ack = link::RegisterAck {
            accepted: true,
            reason: String::new(),
            assigned_devices: vec!["drone-001".into()],
            heartbeat_interval_sec: 10,
            assignments: vec![link::DeviceAssignment {
                device_id: "drone-001".into(),
                mavlink_addr: "udpin:10.88.0.2:14540".into(),
                signing_secret: String::new(),
                allow_unsigned: true,
                mavlink_version: 2,
                mavlink_sysid: 1,
                mavlink_compid: 1,
            }],
        };
        assert_eq!(ack.assignments[0].mavlink_addr, "udpin:10.88.0.2:14540");
        assert!(ack.assignments[0].allow_unsigned);
    }

    #[test]
    fn identity_report_is_reachable_as_oneof_branch() {
        let m = link::AdapterMessage {
            payload: Some(link::adapter_message::Payload::IdentityReport(
                link::IdentityReport {
                    identities: vec![link::DeviceIdentity {
                        device_id: "drone-001".into(),
                        unique_id: "0123456789abcdef".into(),
                    }],
                },
            )),
        };
        match m.payload {
            Some(link::adapter_message::Payload::IdentityReport(r)) => {
                assert_eq!(r.identities[0].unique_id, "0123456789abcdef")
            }
            _ => panic!("expected identity report"),
        }
    }

    #[test]
    fn drone_profile_carries_link_identity_fields() {
        let p = control::DroneProfile {
            unique_id: "0123456789abcdef".into(),
            mavlink_addr: "udpin:10.88.0.2:14540".into(),
            signing_secret: String::new(),
            signing_allow_unsigned: true,
            ..Default::default()
        };
        assert_eq!(p.unique_id, "0123456789abcdef");
        assert!(p.signing_allow_unsigned);
    }
}
