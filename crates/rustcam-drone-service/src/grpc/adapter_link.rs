use rustcam_drone_proto::link::adapter_link_server::AdapterLink;
use rustcam_drone_proto::link::{
    adapter_message, service_message, AdapterMessage, RegisterAck, ServiceMessage,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};

use crate::registry::Registry;
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

        let reg = match inbound.message().await? {
            Some(AdapterMessage {
                payload: Some(adapter_message::Payload::Register(r)),
            }) => r,
            _ => return Err(Status::invalid_argument("expected Register as first frame")),
        };

        let ack = self.registry.register(&reg).await;
        let generation = self.state.generation_of(&reg.adapter_id).unwrap_or_default();

        tx.send(service_message::Payload::RegisterAck(RegisterAck {
            accepted: ack.accepted,
            reason: ack.reason,
            assigned_devices: ack.assigned_devices,
            heartbeat_interval_sec: ack.heartbeat_interval_sec,
        }))
        .await
        .map_err(|_| Status::internal("failed to send RegisterAck"))?;

        let registry = self.registry.clone();
        let adapter_id = reg.adapter_id.clone();
        tokio::spawn(async move {
            while let Ok(Some(msg)) = inbound.message().await {
                if !registry.is_current(&adapter_id, generation) {
                    break;
                }
                // 上行消息分发在后续任务实现；本步只做代次校验与丢弃。
                let _ = msg;
            }
            registry.unregister(&adapter_id, generation).await;
        });

        let stream = ReceiverStream::new(rx).map(|p| Ok(ServiceMessage { payload: Some(p) }));
        Ok(Response::new(Box::pin(stream)))
    }
}
