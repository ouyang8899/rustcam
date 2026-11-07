use rustcam_drone_core::metrics::names;
use rustcam_drone_proto::link::adapter_link_server::AdapterLink;
use rustcam_drone_proto::link::{
    adapter_message, service_message, AdapterMessage, RegisterAck, ServiceMessage,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};

use crate::commands;
use crate::presence;
use crate::registry::Registry;
use crate::shadow;
use crate::state::DroneState;

pub struct AdapterLinkService {
    pub state: DroneState,
    pub registry: Registry,
}

impl AdapterLinkService {
    pub fn new(state: DroneState) -> AdapterLinkService {
        let registry = Registry::new(state.clone(), None);
        AdapterLinkService { state, registry }
    }
}

#[tonic::async_trait]
impl AdapterLink for AdapterLinkService {
    type OpenStream = std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = Result<ServiceMessage, Status>> + Send + 'static>,
    >;

    async fn open(
        &self,
        request: Request<Streaming<AdapterMessage>>,
    ) -> Result<Response<Self::OpenStream>, Status> {
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel::<service_message::Payload>(1024);

        let reg = match tokio::time::timeout(std::time::Duration::from_secs(10), inbound.message())
            .await
        {
            Ok(Ok(Some(AdapterMessage {
                payload: Some(adapter_message::Payload::Register(r)),
            }))) => r,
            Ok(Ok(_)) => return Err(Status::invalid_argument("expected Register as first frame")),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(Status::deadline_exceeded("Register frame timeout")),
        };

        let ack = self.registry.register(&reg, tx.clone()).await;
        let generation = self
            .state
            .generation_of(&reg.adapter_id)
            .unwrap_or_default();

        tx.send(service_message::Payload::RegisterAck(RegisterAck {
            accepted: ack.accepted,
            reason: ack.reason,
            assigned_drone_tags: ack.assigned_drone_tags,
            heartbeat_interval_sec: ack.heartbeat_interval_sec,
            assignments: ack.assignments,
        }))
        .await
        .map_err(|_| Status::internal("failed to send RegisterAck"))?;

        let registry = self.registry.clone();
        let state = self.state.clone();
        let adapter_id = reg.adapter_id.clone();
        tokio::spawn(async move {
            // 上行循环受监管：处理单条消息时 panic 不会连带跳过 unregister。
            crate::supervisor::guard("adapter-inbound", async {
                loop {
                    match inbound.message().await {
                        Ok(Some(msg)) => {
                            if !registry.is_current(&adapter_id, generation) {
                                tracing::warn!(
                                    adapter = %adapter_id,
                                    generation,
                                    "[AdapterLink] connection superseded, closing inbound loop"
                                );
                                break;
                            }
                            dispatch(&registry, &state, &adapter_id, msg).await;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(adapter = %adapter_id, error = %e, "[AdapterLink] stream error");
                            break;
                        }
                    }
                }
            })
            .await;
            registry.unregister(&adapter_id, generation).await;
        });

        let stream = ReceiverStream::new(rx).map(|p| Ok(ServiceMessage { payload: Some(p) }));
        Ok(Response::new(Box::pin(stream)))
    }
}

async fn dispatch(registry: &Registry, state: &DroneState, adapter_id: &str, msg: AdapterMessage) {
    match msg.payload {
        Some(adapter_message::Payload::Telemetry(telemetry)) => {
            let core = shadow::from_proto(&telemetry);
            let result = shadow::ingest(state, core);
            if !result.accepted() {
                tracing::debug!(?result, device = %telemetry.drone_tag, "telemetry not applied");
            }
        }
        Some(adapter_message::Payload::CommandReply(reply)) => {
            commands::on_reply(state, reply).await;
        }
        Some(adapter_message::Payload::Heartbeat(heartbeat)) => {
            let now = chrono::Utc::now().timestamp_millis();
            state.touch_adapter_heartbeat(adapter_id, now);
            // 心跳年龄：§14.2 用「> 3 个心跳周期」判断 adapter 掉线。
            if let Some(age_ms) = state.adapter_heartbeat_age_ms(adapter_id) {
                state.metrics_set_labeled(
                    names::ADAPTER_HEARTBEAT_AGE_SECONDS,
                    age_ms as f64 / 1000.0,
                    &[("adapter_id", adapter_id)],
                );
            }
            tracing::debug!(
                adapter = %adapter_id,
                device_count = heartbeat.device_count,
                inflight = heartbeat.inflight_commands,
                adapter_state = %heartbeat.adapter_state,
                "[AdapterLink] heartbeat"
            );
            // adapter 心跳携带飞控链路状态，用它兜住“遥测未断但飞控已掉”的情况。
            let online = heartbeat.adapter_state != "fc_offline";
            let devices: Vec<String> = state
                .device_to_adapter
                .iter()
                .filter(|e| e.value() == adapter_id)
                .map(|e| e.key().clone())
                .collect();
            for drone_tag in devices {
                presence::set_device_link_state(state, &drone_tag, online, "adapter heartbeat")
                    .await;
            }
        }
        Some(adapter_message::Payload::Event(event)) => {
            handle_device_event(state, event).await;
        }
        Some(adapter_message::Payload::IdentityReport(report)) => {
            let results = registry
                .apply_identity_report(adapter_id, &report.identities)
                .await;
            for (drone_tag, ok, detail) in results {
                if ok {
                    continue;
                }
                // 置离线 + 广播告警（沿用既有事件发布方式）。
                presence::set_device_link_state(state, &drone_tag, false, &detail).await;
                let _ = state
                    .event_tx
                    .send(rustcam_drone_core::event::DroneEvent::Alert {
                        drone_tag: drone_tag.clone(),
                        rule: "unique_id_mismatch".to_string(),
                        level: "error".to_string(),
                        message: detail,
                    });
            }
        }
        Some(adapter_message::Payload::Register(reg)) => {
            tracing::warn!(
                adapter = %reg.adapter_id,
                "[AdapterLink] unexpected Register after session established"
            );
        }
        None => {}
    }
}

async fn handle_device_event(state: &DroneState, event: rustcam_drone_proto::common::DeviceEvent) {
    use rustcam_drone_core::event::DroneEvent;

    match event.r#type.as_str() {
        "fc_online" => {
            presence::set_device_link_state(state, &event.drone_tag, true, &event.detail).await;
            return;
        }
        "fc_offline" => {
            presence::set_device_link_state(state, &event.drone_tag, false, &event.detail).await;
            return;
        }
        _ => {}
    }

    let (level, rule) = match event.r#type.as_str() {
        "statustext" => ("info", "statustext"),
        "autopilot_version" => ("info", "autopilot_version"),
        "landed_state" => ("info", "landed_state"),
        _ => ("info", "device_event"),
    };

    tracing::info!(
        device = %event.drone_tag,
        event_type = %event.r#type,
        detail = %event.detail,
        "[AdapterLink] device event"
    );

    let _ = state.event_tx.send(DroneEvent::Alert {
        drone_tag: event.drone_tag,
        rule: rule.to_string(),
        level: level.to_string(),
        message: event.detail,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use rustcam_drone_proto::common as proto;
    use rustcam_drone_proto::link::adapter_link_client::AdapterLinkClient;
    use rustcam_drone_proto::link::adapter_link_server::AdapterLinkServer;
    use rustcam_drone_proto::link::{AdapterMessage, Register};
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::Server;

    #[tokio::test]
    async fn adapter_registers_streams_telemetry_and_receives_commands() {
        let state = DroneState::new(DroneConfig::default());
        // 无数据库时注册不会下发 assignment；预置归属以验证遥测与指令的传输链路。
        state
            .device_to_adapter
            .insert("drone-001".into(), "a-1".into());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let service = AdapterLinkService::new(state.clone());
        tokio::spawn(async move {
            Server::builder()
                .add_service(AdapterLinkServer::new(service))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        let channel = tonic::transport::Endpoint::from_shared(format!("http://{addr}"))
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = AdapterLinkClient::new(channel);

        let (tx, rx) = mpsc::channel::<AdapterMessage>(16);
        tx.send(AdapterMessage {
            payload: Some(adapter_message::Payload::Register(Register {
                adapter_id: "a-1".into(),
                protocol: "mavlink".into(),
                version: "test".into(),
            })),
        })
        .await
        .unwrap();

        let mut inbound = client
            .open(ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();

        // 1) RegisterAck
        match inbound.message().await.unwrap().unwrap().payload {
            Some(service_message::Payload::RegisterAck(ack)) => {
                assert!(ack.accepted);
                assert!(
                    ack.assigned_drone_tags.is_empty(),
                    "无平台档案下发时不应分配设备"
                );
            }
            other => panic!("expected RegisterAck, got {other:?}"),
        }

        // 2) 上行遥测进入影子
        tx.send(AdapterMessage {
            payload: Some(adapter_message::Payload::Telemetry(proto::Telemetry {
                drone_tag: "drone-001".into(),
                ts_ms: 1_000,
                lat: 31.2304,
                lon: 121.4737,
                battery_pct: 88.0,
                autopilot: "px4".into(),
                armed: true,
                mode: "AUTO.LOITER".into(),
                seq: 1,
                ..Default::default()
            })),
        })
        .await
        .unwrap();

        let entry = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Some(entry) = state.devices.get("drone-001") {
                    if entry.last_telemetry.is_some() {
                        break entry.clone();
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("telemetry should be ingested");
        assert!(entry.online);
        assert_eq!(entry.last_seq, 1);

        // 3) 下行指令：先拿控制权，再 service → adapter
        crate::control::acquire(&state, "drone-001", "flight", "tester", 10, 30)
            .await
            .unwrap();
        let reply = commands::submit(
            &state,
            proto::Command {
                cmd_id: "cmd-1".into(),
                drone_tag: "drone-001".into(),
                r#type: "rtl".into(),
                params: Default::default(),
                deadline_ms: 0,
                operator: "tester".into(),
            },
        )
        .await;
        assert_eq!(reply.status, rustcam_drone_core::command::status::SENT);

        match inbound.message().await.unwrap().unwrap().payload {
            Some(service_message::Payload::Command(cmd)) => {
                assert_eq!(cmd.cmd_id, "cmd-1");
                assert_eq!(cmd.r#type, "rtl");
            }
            other => panic!("expected Command, got {other:?}"),
        }

        // 4) 回执闭环
        tx.send(AdapterMessage {
            payload: Some(adapter_message::Payload::CommandReply(
                proto::CommandReply {
                    cmd_id: "cmd-1".into(),
                    drone_tag: "drone-001".into(),
                    status: "accepted".into(),
                    code: 200,
                    message: "COMMAND_ACK accepted".into(),
                    ts_ms: 2_000,
                },
            )),
        })
        .await
        .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let status = state.commands.get("cmd-1").map(|r| r.status.clone());
                if status.as_deref() == Some("accepted") {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("command reply should be applied");
    }
}
