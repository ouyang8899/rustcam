use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_proto::control::event_service_server::EventService;
use rustcam_drone_proto::control::{EventMessage, SubscribeRequest};
use tokio::sync::mpsc;
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

    /// 内部使用：直接拿到事件流 channel，便于单测。
    pub async fn subscribe_to_channel(&self) -> mpsc::Receiver<EventMessage> {
        let (tx, rx) = mpsc::channel::<EventMessage>(1024);
        let mut events = self.state.event_tx.subscribe();
        tokio::spawn(async move {
            while let Ok(evt) = events.recv().await {
                let msg = to_event_message(&evt);
                if tx.send(msg).await.is_err() {
                    break;
                }
            }
        });
        rx
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
        let _filter = request.into_inner();
        let rx = self.subscribe_to_channel().await;
        let stream = ReceiverStream::new(rx).map(|m| Ok(m));
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

        let mut stream = svc.subscribe_to_channel().await;

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
}
