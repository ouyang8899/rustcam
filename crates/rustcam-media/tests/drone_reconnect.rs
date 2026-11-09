//! drone-service 连接韧性：启动顺序无关、单独重启后自愈。

use std::time::Duration;

use futures::StreamExt;
use rustcam_drone_proto::common as proto;
use rustcam_drone_proto::control::event_service_server::{EventService, EventServiceServer};
use rustcam_drone_proto::control::query_service_server::{QueryService, QueryServiceServer};
use rustcam_drone_proto::control::{
    AlertList, AlertListRequest, CommandStatus, CommandStatusRequest, DeviceInfo, DeviceList,
    DeviceRequest, EventMessage, SubscribeRequest, TelemetryHistory, TelemetryHistoryRequest,
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use rustcam_media::config::DroneClientConfig;
use rustcam_media::drone_gateway::client::{DroneApi, DroneClient};

#[derive(Debug, Default)]
struct MockQueryService;

#[tonic::async_trait]
impl QueryService for MockQueryService {
    async fn get_device(
        &self,
        _request: Request<DeviceRequest>,
    ) -> Result<Response<DeviceInfo>, Status> {
        Err(Status::not_found("mock"))
    }

    async fn list_devices(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<DeviceList>, Status> {
        Ok(Response::new(DeviceList::default()))
    }

    async fn get_telemetry(
        &self,
        _request: Request<DeviceRequest>,
    ) -> Result<Response<proto::Telemetry>, Status> {
        Ok(Response::new(proto::Telemetry::default()))
    }

    async fn get_command_status(
        &self,
        _request: Request<CommandStatusRequest>,
    ) -> Result<Response<CommandStatus>, Status> {
        Err(Status::not_found("mock"))
    }

    async fn list_alerts(
        &self,
        _request: Request<AlertListRequest>,
    ) -> Result<Response<AlertList>, Status> {
        Ok(Response::new(AlertList::default()))
    }

    async fn get_telemetry_history(
        &self,
        _request: Request<TelemetryHistoryRequest>,
    ) -> Result<Response<TelemetryHistory>, Status> {
        Ok(Response::new(TelemetryHistory::default()))
    }
}

async fn spawn_query_service(listener: TcpListener) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _ = Server::builder()
            .add_service(QueryServiceServer::new(MockQueryService))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await;
    })
}

/// 每次订阅推一条事件，便于观察断线后是否恢复。
#[derive(Debug, Default)]
struct MockEventService;

#[tonic::async_trait]
impl EventService for MockEventService {
    type SubscribeStream =
        std::pin::Pin<Box<dyn futures::Stream<Item = Result<EventMessage, Status>> + Send>>;

    async fn subscribe(
        &self,
        _request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel::<EventMessage>(4);
        tokio::spawn(async move {
            let mut n = 0u64;
            loop {
                n += 1;
                let msg = EventMessage {
                    msg_type: "telemetry".into(),
                    payload_json: format!("{{\"seq\":{n}}}"),
                    ts_ms: n as i64,
                };
                if tx.send(msg).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok);
        Ok(Response::new(Box::pin(stream)))
    }
}

async fn spawn_event_service(listener: TcpListener) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _ = Server::builder()
            .add_service(EventServiceServer::new(MockEventService))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await;
    })
}

/// 占用一个端口后立即释放，得到一个"当前无人监听"的端口号。
async fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    port
}

fn config_for(port: u16) -> DroneClientConfig {
    DroneClientConfig {
        service_addr: format!("http://127.0.0.1:{port}"),
        ..Default::default()
    }
}

/// drone-service 还没启动时，主服务不能因此启动失败。
#[tokio::test]
async fn connect_succeeds_even_when_drone_service_is_not_running() {
    let port = free_port().await;
    let client = DroneClient::connect(&config_for(port)).await;
    assert!(client.is_ok(), "连接不应因 drone-service 未启动而失败");
}

/// drone-service 未启动时，drone 接口要返回 503（可重试），不是 500。
#[tokio::test]
async fn requests_report_unavailable_while_drone_service_is_down() {
    let port = free_port().await;
    let client = DroneClient::connect(&config_for(port)).await.expect("connect");
    let err = client.list_drones().await.expect_err("应当失败");
    assert_eq!(err.status, 503, "错误码应为 503，实际 {:?}", err);
}

/// drone-service 启动后，同一个客户端无需重启即可正常工作。
#[tokio::test]
async fn requests_succeed_once_drone_service_is_running() {
    let port = free_port().await;
    let client = DroneClient::connect(&config_for(port)).await.expect("connect");

    let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
    let _server = spawn_query_service(listener).await;

    client.list_drones().await.expect("drone-service 已就绪");
}

/// drone-service 单独重启后，客户端要能自动恢复，不需要重启主服务。
#[tokio::test]
async fn client_recovers_after_drone_service_restarts() {
    let port = free_port().await;
    let client = DroneClient::connect(&config_for(port)).await.expect("connect");

    let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
    let server = spawn_query_service(listener).await;
    client.list_drones().await.expect("首次调用");

    server.abort();
    let _ = server.await;

    let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("rebind");
    let _server = spawn_query_service(listener).await;

    // 重启窗口内的第一次调用可能失败（连接被重置），随后必须自愈。
    let mut ok = false;
    for _ in 0..10 {
        if client.list_drones().await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(ok, "drone-service 重启后客户端应自动恢复");
}

/// 事件流在 drone-service 重启后要自动重订阅，浏览器连接不中断。
#[tokio::test]
async fn event_stream_resubscribes_after_drone_service_restarts() {
    let port = free_port().await;
    let client = DroneClient::connect(&config_for(port)).await.expect("connect");

    let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
    let server = spawn_event_service(listener).await;

    let mut stream = client.event_stream(Vec::new()).await.expect("事件流");
    let first = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("首次事件超时")
        .expect("事件流结束")
        .expect("事件错误");
    assert_eq!(first.msg_type, "telemetry");

    server.abort();
    let _ = server.await;

    let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("rebind");
    let _server = spawn_event_service(listener).await;

    // 重订阅后应继续收到事件，而不是就此结束。
    let mut received = None;
    for _ in 0..50 {
        match tokio::time::timeout(Duration::from_secs(2), stream.next()).await {
            Ok(Some(Ok(event))) => {
                received = Some(event);
                break;
            }
            Ok(Some(Err(_))) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(received.is_some(), "drone-service 重启后事件流应自动重订阅");
}
