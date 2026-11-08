use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustcam_drone_proto::common::CommandReply;
use rustcam_drone_proto::link::adapter_link_client::AdapterLinkClient;
use rustcam_drone_proto::link::adapter_message::Payload;
use rustcam_drone_proto::link::service_message;
use rustcam_drone_proto::link::{
    AdapterMessage, DeviceIdentity, Heartbeat, IdentityReport, Register, ServiceMessage,
};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Endpoint;

use crate::config::AdapterConfig;
use crate::fc::{self, FcPool, FlightController};

/// 已完成指令的幂等记录保留时长。
const COMPLETED_TTL: Duration = Duration::from_secs(3_600);
/// 未确认回执最多保留条数。
const MAX_PENDING_REPLIES: usize = 64;

/// 跨会话保留的指令台账。
///
/// - `completed`：已执行的 `cmd_id`（规则 §16「指令重复下发 → 飞控重复执行」）；
/// - `pending`：已产生但未确认的回执，重连后补发（规则 §7.5）。
#[derive(Default)]
pub struct CommandLedger {
    inner: Mutex<LedgerInner>,
}

#[derive(Default)]
struct LedgerInner {
    completed: HashSet<String>,
    completed_at: HashMap<String, Instant>,
    pending: Vec<CommandReply>,
}

impl CommandLedger {
    /// 该 cmd_id 是否已经执行过。
    pub fn is_completed(&self, cmd_id: &str) -> bool {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.prune();
        guard.completed.contains(cmd_id)
    }

    /// 标记 cmd_id 已执行。
    pub fn mark_completed(&self, cmd_id: &str) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.prune();
        guard.completed.insert(cmd_id.to_string());
        guard
            .completed_at
            .insert(cmd_id.to_string(), Instant::now());
    }

    /// 记录一条回执（待重连补发）。
    pub fn record_reply(&self, reply: CommandReply) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.pending.push(reply);
        if guard.pending.len() > MAX_PENDING_REPLIES {
            let excess = guard.pending.len() - MAX_PENDING_REPLIES;
            guard.pending.drain(0..excess);
        }
    }

    /// 取出待补发的回执。
    pub fn take_pending(&self) -> Vec<CommandReply> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut guard.pending)
    }
}

impl LedgerInner {
    fn prune(&mut self) {
        let now = Instant::now();
        self.completed_at
            .retain(|_, at| now.duration_since(*at) < COMPLETED_TTL);
        self.completed
            .retain(|id| self.completed_at.contains_key(id));
    }
}

/// 唯一标识上报器：把飞控实测 uid 报给 drone-service。
///
/// 规则：
/// - **空值不上报**：飞控未提供 uid 时上报空值会被判"不一致"并撤销设备分配
///   （`registry.rs` 的 `apply_identity_report`），而撤销后不会自动恢复分配。
/// - **相同值不重复上报**：飞控会周期性重发版本消息，值变化才重新上报。
struct IdentityReporter {
    drone_tag: String,
    reported: Mutex<Option<String>>,
}

impl IdentityReporter {
    fn new(drone_tag: String) -> IdentityReporter {
        IdentityReporter {
            drone_tag,
            reported: Mutex::new(None),
        }
    }

    /// 上报实测唯一标识；空值跳过，相同值跳过。
    async fn report(&self, tx: &mpsc::Sender<AdapterMessage>, unique_id: &str) {
        let unique_id = unique_id.trim();
        if unique_id.is_empty() {
            return;
        }
        {
            let mut guard = self.reported.lock().unwrap_or_else(|e| e.into_inner());
            if guard.as_deref() == Some(unique_id) {
                return;
            }
            *guard = Some(unique_id.to_string());
        }
        let msg = AdapterMessage {
            payload: Some(Payload::IdentityReport(IdentityReport {
                identities: vec![DeviceIdentity {
                    drone_tag: self.drone_tag.clone(),
                    unique_id: unique_id.to_string(),
                }],
            })),
        };
        if let Err(e) = tx.send(msg).await {
            // 发送失败时撤销标记，等下一次事件再报。
            let mut guard = self.reported.lock().unwrap_or_else(|e| e.into_inner());
            *guard = None;
            tracing::warn!(error = %e, "[Adapter] failed to send identity report");
            return;
        }
        tracing::info!(unique_id, "[Adapter] reported flight controller unique_id");
    }
}

/// 连接 drone-service 并注册；断线后指数退避重连，进程永不退出。
pub async fn run(cfg: AdapterConfig) -> anyhow::Result<()> {
    let mut backoff = Backoff::new(cfg.reconnect_min_secs, cfg.reconnect_max_secs);
    let ledger = Arc::new(CommandLedger::default());
    loop {
        match serve(&cfg, ledger.clone()).await {
            Ok(()) => {
                tracing::info!("[Adapter] session closed, reconnecting");
                backoff.reset();
            }
            Err(e) => {
                tracing::warn!(error = %e, "[Adapter] session ended with error");
            }
        }
        let delay = backoff.next_delay();
        tracing::info!(
            delay_ms = delay.as_millis() as u64,
            "[Adapter] reconnect scheduled"
        );
        tokio::time::sleep(delay).await;
    }
}

async fn serve(cfg: &AdapterConfig, ledger: Arc<CommandLedger>) -> anyhow::Result<()> {
    // 先连平台、再按平台分配的 assignment 建链：
    // 端点与设备标识都不在本地配置里。
    let endpoint = Endpoint::from_shared(cfg.service_addr.clone())
        .map_err(|e| anyhow::anyhow!("invalid service_addr: {e}"))?
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(30));
    let channel = endpoint.connect().await?;
    let mut client = AdapterLinkClient::new(channel);

    let capacity = cfg.buffer_size.clamp(64, 65_536);
    let (tx, rx) = mpsc::channel::<AdapterMessage>(capacity);

    tx.send(AdapterMessage {
        payload: Some(Payload::Register(Register {
            adapter_id: cfg.adapter_id.clone(),
            protocol: cfg.protocol.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        })),
    })
    .await
    .map_err(|_| anyhow::anyhow!("failed to send Register"))?;

    // 补发上一会话未确认的回执（规则 §7.5）。
    for reply in ledger.take_pending() {
        tx.send(AdapterMessage {
            payload: Some(Payload::CommandReply(reply)),
        })
        .await
        .map_err(|_| anyhow::anyhow!("failed to resend pending reply"))?;
    }

    let mut inbound = client.open(ReceiverStream::new(rx)).await?.into_inner();

    let pool = Arc::new(FcPool::default());
    let result = run_session(cfg, &mut inbound, tx.clone(), &pool, ledger).await;
    pool.close();
    result
}

async fn run_session(
    cfg: &AdapterConfig,
    inbound: &mut tonic::Streaming<ServiceMessage>,
    tx: mpsc::Sender<AdapterMessage>,
    pool: &Arc<FcPool>,
    ledger: Arc<CommandLedger>,
) -> anyhow::Result<()> {
    loop {
        let message = inbound.message().await?;
        let Some(msg) = message else {
            return Ok(());
        };
        handle_service_message(msg, cfg, &tx, pool, &ledger).await?;
    }
}

async fn handle_service_message(
    msg: ServiceMessage,
    cfg: &AdapterConfig,
    tx: &mpsc::Sender<AdapterMessage>,
    pool: &Arc<FcPool>,
    ledger: &Arc<CommandLedger>,
) -> anyhow::Result<()> {
    match msg.payload {
        Some(service_message::Payload::RegisterAck(ack)) => {
            if !ack.accepted {
                return Err(anyhow::anyhow!(
                    "registration rejected: {}",
                    if ack.reason.is_empty() {
                        "no reason given"
                    } else {
                        ack.reason.as_str()
                    }
                ));
            }
            tracing::info!(
                assigned = ?ack.assigned_drone_tags,
                heartbeat_interval_sec = ack.heartbeat_interval_sec,
                "[Adapter] registered with drone-service"
            );
            // 心跳周期以服务端为准（规则 §5.7：adapter 不得写死）。
            let interval_secs = cfg_heartbeat_fallback(
                ack.heartbeat_interval_sec,
                cfg.heartbeat_interval_secs,
            );
            spawn_heartbeat_forwarder(tx.clone(), pool.clone(), interval_secs);

            // 按平台分配的端点逐条建链；建链失败必须让本次会话失败并重连，
            // 否则会以「gRPC 在线但无飞控链路」的半死状态一直挂着。
            for assignment in ack.assignments.iter() {
                connect_assignment(cfg, &tx, pool, assignment).await?;
            }
            if ack.assigned_drone_tags.is_empty() {
                tracing::warn!(
                    adapter = %cfg.adapter_id,
                    "[Adapter] platform assigned no drones; check drone_device rows for this adapter_id"
                );
            }
        }
        Some(service_message::Payload::Command(cmd)) => {
            // 已执行过的 cmd_id 不再重复执行（规则 §16）。
            if ledger.is_completed(&cmd.cmd_id) {
                tracing::warn!(cmd_id = %cmd.cmd_id, "[Adapter] duplicate cmd_id ignored");
                return Ok(());
            }
            let pool = pool.clone();
            let tx = tx.clone();
            let ledger = ledger.clone();
            let drone_tag = cmd.drone_tag.clone();
            tokio::spawn(async move {
                let cmd_id = cmd.cmd_id.clone();
                let kind = cmd.r#type.clone();
                let reply = pool.execute(&drone_tag, cmd).await;
                tracing::info!(
                    cmd_id = %cmd_id,
                    kind = %kind,
                    drone_tag = %drone_tag,
                    status = %reply.status,
                    code = reply.code,
                    message = %reply.message,
                    "command finished"
                );
                ledger.mark_completed(&cmd_id);
                ledger.record_reply(reply.clone());
                let _ = tx
                    .send(AdapterMessage {
                        payload: Some(Payload::CommandReply(reply)),
                    })
                    .await;
            });
        }
        Some(service_message::Payload::ControlState(state)) => {
            tracing::info!(
                drone_tag = %state.drone_tag,
                control_type = %state.control_type,
                action = %state.action,
                holder = %state.holder,
                "[Adapter] control state updated"
            );
        }
        None => {}
    }
    Ok(())
}

/// 按平台下发的 assignment 建一条飞控链路，并挂上遥测 / 事件 / 唯一标识转发。
async fn connect_assignment(
    cfg: &AdapterConfig,
    tx: &mpsc::Sender<AdapterMessage>,
    pool: &Arc<FcPool>,
    assignment: &rustcam_drone_proto::link::DeviceAssignment,
) -> anyhow::Result<()> {
    let drone_tag = assignment.drone_tag.clone();
    let fc = match FlightController::connect(
        &assignment.mavlink_addr,
        &drone_tag,
        assignment.mavlink_sysid as u8,
        assignment.mavlink_compid as u8,
        cfg,
    )
    .await
    {
        Ok(fc) => fc,
        Err(e) => {
            tracing::warn!(
                drone_tag = %drone_tag,
                error = %e,
                "[Adapter] failed to connect flight controller"
            );
            return Err(e);
        }
    };
    pool.insert(&drone_tag, fc.clone());
    spawn_telemetry_forwarder(tx.clone(), fc.clone());
    spawn_event_forwarder(tx.clone(), fc.clone());
    // 每台机体一个上报器：实测 uid 归属到自己的 drone_tag。
    let reporter = Arc::new(IdentityReporter::new(drone_tag.clone()));
    spawn_identity_forwarder(tx.clone(), fc.clone(), reporter);
    Ok(())
}

/// 心跳周期以服务端为准；服务端未给出时用本地配置兜底（规则 §5.7）。
fn cfg_heartbeat_fallback(server_value: i32, local_secs: u64) -> u64 {
    if server_value > 0 {
        server_value as u64
    } else {
        local_secs.max(1)
    }
}

fn spawn_heartbeat_forwarder(
    tx: mpsc::Sender<AdapterMessage>,
    pool: Arc<FcPool>,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(interval_secs.max(1));
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let tags = pool.drone_tags();
            let inflight: usize = tags
                .iter()
                .filter_map(|t| pool.get(t))
                .map(|fc| fc.inflight_commands())
                .sum();
            let mut online = false;
            for tag in &tags {
                if let Some(fc) = pool.get(tag) {
                    if fc.is_online().await {
                        online = true;
                        break;
                    }
                }
            }
            let msg = AdapterMessage {
                payload: Some(Payload::Heartbeat(Heartbeat {
                    ts_ms: fc::now_ms(),
                    device_count: pool.len() as i32,
                    inflight_commands: inflight as i32,
                    adapter_state: if !online {
                        "fc_offline".into()
                    } else if pool.is_empty() {
                        "no_drone".into()
                    } else {
                        "ok".into()
                    },
                })),
            };
            if tx.send(msg).await.is_err() {
                break;
            }
        }
    });
}

/// forwarder 只保留 receiver + shutdown 信号，不长期持有 `FlightController`；
/// 否则 broadcast Sender 被任务自己握着，`recv` 永远等不到 Closed，任务泄漏并占住 UDP 端口。
fn spawn_telemetry_forwarder(tx: mpsc::Sender<AdapterMessage>, fc: FlightController) {
    let mut telemetry_rx = fc.subscribe_telemetry();
    let mut shutdown_rx = fc.subscribe_shutdown();
    drop(fc);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = fc::wait_shutdown(&mut shutdown_rx) => break,
                received = telemetry_rx.recv() => match received {
                    Ok(telemetry) => {
                        let msg = AdapterMessage {
                            payload: Some(Payload::Telemetry(fc::telemetry::to_proto(&telemetry))),
                        };
                        if tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "telemetry subscriber lagged, dropping old frames");
                    }
                    Err(RecvError::Closed) => break,
                },
            }
        }
    });
}

fn spawn_event_forwarder(tx: mpsc::Sender<AdapterMessage>, fc: FlightController) {
    let mut event_rx = fc.subscribe_events();
    let mut shutdown_rx = fc.subscribe_shutdown();
    let drone_tag = fc.drone_tag.clone();
    drop(fc);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = fc::wait_shutdown(&mut shutdown_rx) => break,
                received = event_rx.recv() => match received {
                    Ok(event) => {
                        // 唯一标识走 IdentityReport 通道，不重复作为设备事件上报。
                        if event.event_type == IDENTITY_EVENT {
                            continue;
                        }
                        let msg = AdapterMessage {
                            payload: Some(Payload::Event(rustcam_drone_proto::common::DeviceEvent {
                                drone_tag: drone_tag.clone(),
                                r#type: event.event_type,
                                detail: event.detail,
                                ts_ms: fc::now_ms(),
                            })),
                        };
                        if tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "adapter event subscriber lagged");
                    }
                    Err(RecvError::Closed) => break,
                },
            }
        }
    });
}

/// 唯一标识事件名（飞控层 → gRPC 会话）。
const IDENTITY_EVENT: &str = "identity";

/// 订阅飞控事件，把唯一标识变化上报给服务端。
fn spawn_identity_forwarder(
    tx: mpsc::Sender<AdapterMessage>,
    fc: FlightController,
    reporter: Arc<IdentityReporter>,
) {
    let mut event_rx = fc.subscribe_events();
    let mut shutdown_rx = fc.subscribe_shutdown();
    let unique_id = fc.unique_id();
    drop(fc);
    tokio::spawn(async move {
        // 建链瞬间可能已经读到 uid（事件先于订阅者产生），先补报一次。
        reporter.report(&tx, &unique_id).await;
        loop {
            tokio::select! {
                _ = fc::wait_shutdown(&mut shutdown_rx) => break,
                received = event_rx.recv() => match received {
                    Ok(event) => {
                        if event.event_type != IDENTITY_EVENT {
                            continue;
                        }
                        reporter.report(&tx, &event.detail).await;
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "adapter identity subscriber lagged");
                    }
                    Err(RecvError::Closed) => break,
                },
            }
        }
    });
}

struct Backoff {
    min: Duration,
    max: Duration,
    next: Duration,
}

impl Backoff {
    fn new(min_secs: u64, max_secs: u64) -> Backoff {
        let min = Duration::from_secs(min_secs.max(1));
        let max = Duration::from_secs(max_secs.max(min_secs.max(1)));
        Backoff {
            min,
            max,
            next: min,
        }
    }

    fn reset(&mut self) {
        self.next = self.min;
    }

    fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = self.next.saturating_mul(2).min(self.max);
        delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        let mut b = Backoff::new(1, 4);
        assert_eq!(b.next_delay(), Duration::from_secs(1));
        assert_eq!(b.next_delay(), Duration::from_secs(2));
        assert_eq!(b.next_delay(), Duration::from_secs(4));
        assert_eq!(b.next_delay(), Duration::from_secs(4));
        b.reset();
        assert_eq!(b.next_delay(), Duration::from_secs(1));
    }

    #[test]
    fn backoff_handles_zero_config() {
        let mut b = Backoff::new(0, 0);
        assert_eq!(b.next_delay(), Duration::from_secs(1));
        assert_eq!(b.next_delay(), Duration::from_secs(1));
    }

    #[tokio::test]
    async fn identity_reporter_skips_empty_unique_id() {
        let (tx, mut rx) = mpsc::channel::<AdapterMessage>(8);
        let reporter = IdentityReporter::new("drone-001".into());
        reporter.report(&tx, "").await;
        reporter.report(&tx, "   ").await;
        assert!(rx.try_recv().is_err(), "空唯一标识不应上报");
    }

    #[tokio::test]
    async fn identity_reporter_reports_once_per_unique_id() {
        let (tx, mut rx) = mpsc::channel::<AdapterMessage>(8);
        let reporter = IdentityReporter::new("drone-001".into());
        reporter.report(&tx, "0011223344556677").await;
        reporter.report(&tx, "0011223344556677").await;
        reporter.report(&tx, "0011223344556678").await;

        let first = rx.try_recv().expect("第一次应上报");
        let second = rx.try_recv().expect("值变化后应再次上报");
        assert!(rx.try_recv().is_err(), "相同唯一标识不应重复上报");

        for msg in [first, second] {
            match msg.payload {
                Some(Payload::IdentityReport(report)) => {
                    assert_eq!(report.identities.len(), 1);
                    assert_eq!(report.identities[0].drone_tag, "drone-001");
                }
                other => panic!("expected IdentityReport, got {other:?}"),
            }
        }
    }

    #[test]
    fn ledger_dedups_completed_commands() {
        let ledger = CommandLedger::default();
        assert!(!ledger.is_completed("cmd-1"));
        ledger.mark_completed("cmd-1");
        assert!(ledger.is_completed("cmd-1"));
        assert!(!ledger.is_completed("cmd-2"));
    }

    #[tokio::test]
    async fn closing_session_releases_mavlink_udp_port() {
        let port = {
            let socket = std::net::UdpSocket::bind("0.0.0.0:0").expect("pick free port");
            socket.local_addr().expect("local addr").port()
        };
        let cfg = AdapterConfig::default();
        let fc = FlightController::connect(
            &format!("udpin:0.0.0.0:{port}"),
            "drone-test",
            0,
            0,
            &cfg,
        )
        .await
        .expect("connect udpin");

        // 按生产路径起三个 forwarder；接收端必须存活，否则它们会因发送失败提前退出。
        let (tx, _guard) = mpsc::channel(64);
        spawn_telemetry_forwarder(tx.clone(), fc.clone());
        spawn_event_forwarder(tx.clone(), fc.clone());
        spawn_identity_forwarder(
            tx,
            fc.clone(),
            Arc::new(IdentityReporter::new("drone-test".into())),
        );

        fc.close();
        drop(fc);

        // 会话结束后，旧链路（含 forwarder 持有的 FC 克隆）必须释放 UDP 端口，
        // 否则下个会话 connect_assignment 会 bind 失败（Windows os error 10048）。
        let mut rebound = None;
        for _ in 0..40 {
            if let Ok(socket) = std::net::UdpSocket::bind(("0.0.0.0", port)) {
                rebound = Some(socket);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(
            rebound.is_some(),
            "MAVLink UDP port {port} was not released after session close"
        );
    }

    #[tokio::test]
    async fn assignment_connect_failure_is_reported() {
        let blocker = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind blocker");
        let port = blocker.local_addr().expect("local addr").port();
        let cfg = AdapterConfig::default();
        let (tx, _guard) = mpsc::channel(8);
        let pool = Arc::new(FcPool::default());
        let assignment = rustcam_drone_proto::link::DeviceAssignment {
            drone_tag: "drone-test".into(),
            mavlink_addr: format!("udpin:0.0.0.0:{port}"),
            ..Default::default()
        };

        let result = connect_assignment(&cfg, &tx, &pool, &assignment).await;

        assert!(
            result.is_err(),
            "connect failure must end the session instead of running half-dead"
        );
        assert!(pool.get("drone-test").is_none());
    }

    #[test]
    fn ledger_keeps_pending_replies_for_resend() {
        let ledger = CommandLedger::default();
        ledger.record_reply(CommandReply {
            cmd_id: "cmd-1".into(),
            drone_tag: "d1".into(),
            status: "acked".into(),
            code: 200,
            message: String::new(),
            ts_ms: 1,
        });
        let pending = ledger.take_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].cmd_id, "cmd-1");
        assert!(ledger.take_pending().is_empty(), "取出后不应重复补发");
    }
}