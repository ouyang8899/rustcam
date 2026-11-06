use std::collections::HashSet;
use std::sync::atomic::Ordering;

use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_core::metrics::names;
use rustcam_drone_proto::control::event_service_server::EventService;
use rustcam_drone_proto::control::{EventMessage, SubscribeRequest};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use crate::state::DroneState;

pub struct EventServiceImpl {
    state: DroneState,
}

impl EventServiceImpl {
    pub fn new(state: DroneState) -> EventServiceImpl {
        EventServiceImpl { state }
    }

    /// 内部使用：按设备过滤的事件流，便于单测与网关复用。
    pub async fn subscribe_to_channel(
        &self,
        device_ids: Vec<String>,
    ) -> mpsc::Receiver<EventMessage> {
        let (tx, rx) = mpsc::channel::<EventMessage>(1024);
        let filter: HashSet<String> = device_ids
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let mut events = self.state.event_tx.subscribe();
        let state = self.state.clone();
        state.ws_subscribers.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(evt) => {
                        if !filter.is_empty() {
                            let keep = event_device_id(&evt)
                                .map(|id| filter.contains(id))
                                .unwrap_or(false);
                            if !keep {
                                continue;
                            }
                        }
                        let msg = to_event_message(&evt);
                        if tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    // 慢消费者：丢弃落后的旧消息后继续，不能因此终止订阅（§6.9 / §11.6）。
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "事件订阅者落后，已丢弃旧消息并继续接收");
                        state.metrics_inc_labeled(names::WS_LAGGED_TOTAL, &[("channel", "event")]);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            state.ws_subscribers.fetch_sub(1, Ordering::Relaxed);
        });
        rx
    }
}

pub fn event_device_id(evt: &DroneEvent) -> Option<&str> {
    match evt {
        DroneEvent::Telemetry(t) => Some(&t.device_id),
        DroneEvent::Alert { device_id, .. } => Some(device_id),
        DroneEvent::Control { device_id, .. } => Some(device_id),
        DroneEvent::CommandReply { device_id, .. } => Some(device_id),
    }
}

pub fn to_event_message(evt: &DroneEvent) -> EventMessage {
    EventMessage {
        msg_type: evt.msg_type().to_string(),
        payload_json: evt.payload_value().to_string(),
        ts_ms: chrono::Utc::now().timestamp_millis(),
    }
}

#[tonic::async_trait]
impl EventService for EventServiceImpl {
    type SubscribeStream = std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = Result<EventMessage, Status>> + Send + 'static>,
    >;

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let filter = request.into_inner().device_ids;
        let rx = self.subscribe_to_channel(filter).await;
        let stream = ReceiverStream::new(rx).map(Ok);
        Ok(Response::new(Box::pin(stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;
    use rustcam_drone_core::telemetry::Telemetry;

    #[tokio::test]
    async fn subscribe_streams_events_to_subscriber() {
        let state = DroneState::new(DroneConfig::default());
        let svc = EventServiceImpl::new(state.clone());

        let mut stream = svc.subscribe_to_channel(vec![]).await;

        let tx = state.event_tx.clone();
        let evt = DroneEvent::Telemetry(Telemetry {
            device_id: "drone-001".into(),
            ts_ms: 42,
            ..Default::default()
        });
        tx.send(evt).unwrap();

        let msg = tokio::time::timeout(std::time::Duration::from_secs(2), stream.recv())
            .await
            .expect("event should arrive")
            .expect("stream should not be closed");

        assert_eq!(msg.msg_type, "telemetry");
        assert!(msg.payload_json.contains("drone-001"));
    }

    #[tokio::test]
    async fn subscribe_filters_by_device_id() {
        let state = DroneState::new(DroneConfig::default());
        let svc = EventServiceImpl::new(state.clone());
        let mut stream = svc.subscribe_to_channel(vec!["drone-001".into()]).await;

        let tx = state.event_tx.clone();
        tx.send(DroneEvent::Telemetry(Telemetry {
            device_id: "drone-002".into(),
            ..Default::default()
        }))
        .unwrap();
        tx.send(DroneEvent::Telemetry(Telemetry {
            device_id: "drone-001".into(),
            ts_ms: 1,
            ..Default::default()
        }))
        .unwrap();

        let msg = tokio::time::timeout(std::time::Duration::from_secs(2), stream.recv())
            .await
            .expect("event should arrive")
            .expect("stream should not be closed");
        assert!(msg.payload_json.contains("drone-001"));
        assert!(!msg.payload_json.contains("drone-002"));
    }

    #[test]
    fn every_event_exposes_a_device_id() {
        let events = [
            DroneEvent::Telemetry(Telemetry {
                device_id: "d1".into(),
                ..Default::default()
            }),
            DroneEvent::Alert {
                device_id: "d2".into(),
                rule: "r".into(),
                level: "info".into(),
                message: "m".into(),
            },
            DroneEvent::CommandReply {
                cmd_id: "c".into(),
                device_id: "d3".into(),
                status: "accepted".into(),
                message: String::new(),
            },
        ];
        assert_eq!(event_device_id(&events[0]), Some("d1"));
        assert_eq!(event_device_id(&events[1]), Some("d2"));
        assert_eq!(event_device_id(&events[2]), Some("d3"));
    }

    /// 慢消费者触发 broadcast Lagged 后，订阅任务必须继续存活（规则 §6.9 / §11.6）。
    #[tokio::test]
    async fn lagging_subscriber_keeps_running() {
        let state = DroneState::new(DroneConfig::default());
        let svc = EventServiceImpl::new(state.clone());
        let mut stream = svc.subscribe_to_channel(vec![]).await;

        let tx = state.event_tx.clone();
        // 填满内部 mpsc(1024) 与 broadcast(4096)，逼出 Lagged。
        for i in 0..6_000i64 {
            let _ = tx.send(DroneEvent::Telemetry(Telemetry {
                device_id: format!("drone-{i}"),
                ts_ms: i,
                ..Default::default()
            }));
        }

        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            stream.recv().await
        })
        .await
        .expect("订阅者不应因 Lagged 而退出")
        .expect("事件流不应被关闭");

        assert_eq!(msg.msg_type, "telemetry");
        let text = state.metrics_text();
        assert!(
            text.contains("rustcam_drone_ws_lagged_total{channel=\"event\"}"),
            "Lagged 必须被计数: {text}"
        );
    }
}
