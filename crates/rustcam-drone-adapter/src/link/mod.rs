use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustcam_drone_core::reconnect::Backoff;
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
use crate::fc::addr::{MavAddress, Transport};
use crate::fc::{self, EndpointRegistry, FcPool, FlightController};

mod assignments;

use self::assignments::{ApplyReport, AssignmentTracker};

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
    let mut backoff = Backoff::from_secs_jittered(
        cfg.reconnect_min_secs,
        cfg.reconnect_max_secs,
        0.2,
    );
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
    // 共享端点按会话存活：同一 `udpin` 端口只绑定一次，供多架飞控复用。
    let endpoints = EndpointRegistry::new();
    // 当前生效的分配：平台下发全量快照后与它做差量。
    let mut tracker = AssignmentTracker::new(&[]);
    let result = run_session(cfg, &mut inbound, tx.clone(), &pool, &endpoints, &mut tracker, ledger).await;
    pool.close();
    result
}

async fn run_session(
    cfg: &AdapterConfig,
    inbound: &mut tonic::Streaming<ServiceMessage>,
    tx: mpsc::Sender<AdapterMessage>,
    pool: &Arc<FcPool>,
    endpoints: &EndpointRegistry,
    tracker: &mut AssignmentTracker,
    ledger: Arc<CommandLedger>,
) -> anyhow::Result<()> {
    loop {
        let message = inbound.message().await?;
        let Some(msg) = message else {
            return Ok(());
        };
        handle_service_message(msg, cfg, &tx, pool, endpoints, tracker, &ledger).await?;
    }
}

async fn handle_service_message(
    msg: ServiceMessage,
    cfg: &AdapterConfig,
    tx: &mpsc::Sender<AdapterMessage>,
    pool: &Arc<FcPool>,
    endpoints: &EndpointRegistry,
    tracker: &mut AssignmentTracker,
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

            // 注册时下发的分配就是当前生效配置：后续快照与它做差量。
            tracker.replace_all(&ack.assignments);

            // 按平台分配的端点逐条建链；单架建链失败只跳过该机（平台侧显示离线），
            // 不中断整场会话——一个 adapter 可能带多架机，一架失败不能让其余全部掉线。
            let connected = connect_assignments(cfg, tx, pool, endpoints, &ack.assignments).await;
            tracing::info!(
                assigned = ack.assignments.len(),
                connected,
                "[Adapter] flight controller links established"
            );
            if ack.assigned_drone_tags.is_empty() {
                tracing::warn!(
                    adapter = %cfg.adapter_id,
                    "[Adapter] platform assigned no drones; check drone_device rows for this adapter_id"
                );
            }

        }
        Some(service_message::Payload::AssignmentsUpdated(update)) => {
            // 平台重算分配后主动下发（全量快照）：按差量重建受影响的链路。
            tracing::info!(
                adapter = %cfg.adapter_id,
                revision = update.revision,
                count = update.assignments.len(),
                "[Adapter] assignment update received"
            );
            let report = apply_assignments(
                tracker,
                &update.assignments,
                cfg,
                tx,
                pool,
                endpoints,
            )
            .await;
            tracing::info!(
                connected = report.connected,
                reconnected = report.reconnected,
                disconnected = report.disconnected,
                failed = report.failed,
                "[Adapter] assignment update applied"
            );
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
/// 逐条建立平台分配的飞控链路，返回成功建链的架数。
///
/// 单架建链失败只跳过该机（该机在平台侧显示离线），不影响其余架；
/// 全部失败时不报错——平台随时可能补录新机，会话本身仍是健康的。
async fn connect_assignments(
    cfg: &AdapterConfig,
    tx: &mpsc::Sender<AdapterMessage>,
    pool: &Arc<FcPool>,
    endpoints: &EndpointRegistry,
    assignments: &[rustcam_drone_proto::link::DeviceAssignment],
) -> usize {
    let mut connected = 0usize;
    for assignment in assignments {
        match connect_assignment(cfg, tx, pool, endpoints, assignment).await {
            Ok(()) => connected += 1,
            Err(e) => {
                tracing::warn!(
                    drone_tag = %assignment.drone_tag,
                    error = %e,
                    "[Adapter] flight controller link failed, skipping this drone"
                );
            }
        }
    }
    connected
}

/// 判断一条链路是否应复用共享端口，返回 (注册键, 绑定目标)。
///
/// 只有 `udpin`（本地监听）且 sysid 非零时才共享：sysid 是分派依据。
fn shared_target(addr: &str) -> Option<(String, String)> {
    let address = MavAddress::parse(addr).ok()?;
    if address.transport() != Transport::UdpIn {
        return None;
    }
    let (_, target) = address.as_str().split_once(':')?;
    Some((address.as_str().to_string(), target.to_string()))
}

/// 平台 → adapter 的单架链路信息：描述一条机载链路身份与飞控寻址。
///
/// 平台 → adapter：一条链路身份 = 端点（协议族 + 端口/串口）+ 飞控系统号。
async fn connect_assignment(
    cfg: &AdapterConfig,
    tx: &mpsc::Sender<AdapterMessage>,
    pool: &Arc<FcPool>,
    endpoints: &EndpointRegistry,
    assignment: &rustcam_drone_proto::link::DeviceAssignment,
) -> anyhow::Result<()> {
    let fc = open_assignment(cfg, tx, endpoints, assignment).await?;
    pool.insert(&assignment.drone_tag, fc.clone());
    spawn_link_forwarders(tx.clone(), fc);
    Ok(())
}

/// 建立一条飞控链路（不入池、不起转发）。
///
/// 端点与机体标识由平台分配：`udpin` + 非零 sysid 走共享端口，
/// 其余走独占连接（tcp / serial / udpout）。
async fn open_assignment(
    cfg: &AdapterConfig,
    tx: &mpsc::Sender<AdapterMessage>,
    endpoints: &EndpointRegistry,
    assignment: &rustcam_drone_proto::link::DeviceAssignment,
) -> anyhow::Result<FlightController> {
    let drone_tag = assignment.drone_tag.clone();
    let sysid = assignment.mavlink_sysid as u8;
    let compid = assignment.mavlink_compid as u8;

    // `udpin` + 非零 sysid 走共享端口：同一端口按 sysid 分派给多架飞控。
    let connected = match shared_target(&assignment.mavlink_addr) {
        Some((key, target)) if sysid > 0 => {
            let endpoint = endpoints.get_or_bind(key, &target).await?;
            FlightController::connect_shared(endpoint, sysid, &drone_tag, compid, cfg).await
        }
        _ => {
            FlightController::connect(&assignment.mavlink_addr, &drone_tag, sysid, compid, cfg).await
        }
    };
    connected.map_err(|e| {
        tracing::warn!(
            drone_tag = %drone_tag,
            error = %e,
            "[Adapter] failed to connect flight controller"
        );
        e
    })
}

/// 起该机的三个转发任务（遥测 / 事件 / 唯一标识）。
fn spawn_link_forwarders(tx: mpsc::Sender<AdapterMessage>, fc: FlightController) {
    spawn_telemetry_forwarder(tx.clone(), fc.clone());
    spawn_event_forwarder(tx.clone(), fc.clone());
    // 每台机体一个上报器：实测 uid 归属到自己的 drone_tag。
    let reporter = Arc::new(IdentityReporter::new(fc.drone_tag.clone()));
    spawn_identity_forwarder(tx, fc, reporter);
}

/// 差量应用：新增建链、变更先建新链再关旧链（失败保留旧链）、移除关链。
///
/// - 新增 / 变更：**先建新链**，成功后才关旧链——端口变更时该机无空窗；
///   新链建不起来时保留旧链（该机继续在线工作），并上报失败。
/// - 移除：关链即可（该机被删除或换到了别的 adapter）。
async fn apply_assignments(
    tracker: &mut AssignmentTracker,
    next: &[rustcam_drone_proto::link::DeviceAssignment],
    cfg: &AdapterConfig,
    tx: &mpsc::Sender<AdapterMessage>,
    pool: &Arc<FcPool>,
    endpoints: &EndpointRegistry,
) -> ApplyReport {
    let diff = assignments::diff_assignments(tracker.applied(), next);
    let mut report = ApplyReport::default();
    let by_tag: HashMap<String, &rustcam_drone_proto::link::DeviceAssignment> =
        next.iter().map(|a| (a.drone_tag.clone(), a)).collect();

    for (drone_tag, action) in diff.actions {
        match action {
            assignments::AssignmentAction::Keep => {}
            assignments::AssignmentAction::Connect => {
                let Some(assignment) = by_tag.get(&drone_tag) else {
                    continue;
                };
                match open_assignment(cfg, tx, endpoints, assignment).await {
                    Ok(fc) => {
                        spawn_link_forwarders(tx.clone(), fc.clone());
                        pool.insert(&drone_tag, fc);
                        tracker.set_applied(&drone_tag, (*assignment).clone());
                        report.connected += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            drone_tag = %drone_tag,
                            error = %e,
                            "[Adapter] connect failed on assignment update"
                        );
                        report.failed += 1;
                        report_failure(tx, &drone_tag, "connect failed").await;
                    }
                }
            }
            assignments::AssignmentAction::Reconnect => {
                let Some(assignment) = by_tag.get(&drone_tag) else {
                    continue;
                };
                // 先建新链：成功后才关旧链；失败保留旧链（该机继续在线工作）。
                match open_assignment(cfg, tx, endpoints, assignment).await {
                    Ok(fc) => {
                        spawn_link_forwarders(tx.clone(), fc.clone());
                        pool.remove(&drone_tag);
                        pool.insert(&drone_tag, fc);
                        tracker.set_applied(&drone_tag, (*assignment).clone());
                        report.reconnected += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            drone_tag = %drone_tag,
                            error = %e,
                            "[Adapter] reconnect failed, keeping old link"
                        );
                        report.failed += 1;
                        report_failure(tx, &drone_tag, "reconnect failed").await;
                    }
                }
            }
            assignments::AssignmentAction::Disconnect => {
                pool.remove(&drone_tag);
                tracker.clear_applied(&drone_tag);
                report.disconnected += 1;
            }
        }
    }
    report
}

/// 上报重配失败：平台据此产生告警（该机仍用旧链工作）。
async fn report_failure(tx: &mpsc::Sender<AdapterMessage>, drone_tag: &str, detail: &str) {
    let _ = tx
        .send(AdapterMessage {
            payload: Some(Payload::Event(rustcam_drone_proto::common::DeviceEvent {
                drone_tag: drone_tag.to_string(),
                r#type: "link_reconfig_failed".into(),
                detail: detail.to_string(),
                ts_ms: fc::now_ms(),
            })),
        })
        .await;
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let endpoints = EndpointRegistry::new();
        let result = connect_assignment(&cfg, &tx, &pool, &endpoints, &assignment).await;

        assert!(
            result.is_err(),
            "单架建链失败必须上报，供平台把该机标为离线"
        );
        assert!(pool.get("drone-test").is_none());
    }

    /// 一架机建链失败不能连累同一 adapter 上的其它机（端口被占即典型场景）。
    #[tokio::test]
    async fn one_failed_link_does_not_block_other_drones() {
        // 占用一个端口，让指向它的链路必然 bind 失败。
        let blocker = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind blocker");
        let blocked_port = blocker.local_addr().expect("local addr").port();

        // 另一个可用端口，供正常链路使用。
        let free = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind free");
        let free_port = free.local_addr().expect("local addr").port();
        drop(free);

        let cfg = AdapterConfig::default();
        let (tx, _guard) = mpsc::channel(8);
        let pool = Arc::new(FcPool::default());
        let assignments = vec![
            rustcam_drone_proto::link::DeviceAssignment {
                drone_tag: "drone-bad".into(),
                mavlink_addr: format!("udpin:0.0.0.0:{blocked_port}"),
                ..Default::default()
            },
            rustcam_drone_proto::link::DeviceAssignment {
                drone_tag: "drone-ok".into(),
                mavlink_addr: format!("udpin:0.0.0.0:{free_port}"),
                ..Default::default()
            },
        ];

        let endpoints = EndpointRegistry::new();
        let connected = connect_assignments(&cfg, &tx, &pool, &endpoints, &assignments).await;

        assert_eq!(connected, 1, "失败的那架应被跳过，其余照常建链");
        assert!(pool.get("drone-bad").is_none());
        assert!(pool.get("drone-ok").is_some());
    }

    #[test]
    fn ledger_keeps_pending_replies_for_resend() {        let ledger = CommandLedger::default();
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

    /// 取一个空闲 UDP 端口（建链测试用）。
    fn free_udp_port() -> u16 {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.local_addr().unwrap().port()
    }

    /// 参数变化：旧链关闭、新链建立，池里只剩新链，生效配置同步更新。
    #[tokio::test]
    async fn apply_assignments_rebuilds_changed_link() {
        let port_a = free_udp_port();
        let port_b = free_udp_port();
        let cfg = AdapterConfig::default();
        let (tx, _guard) = mpsc::channel(8);
        let pool = Arc::new(FcPool::default());
        let endpoints = EndpointRegistry::new();

        let old = rustcam_drone_proto::link::DeviceAssignment {
            drone_tag: "drone-x".into(),
            mavlink_addr: format!("udpin:127.0.0.1:{port_a}"),
            mavlink_sysid: 1,
            mavlink_compid: 1,
            ..Default::default()
        };
        let new = rustcam_drone_proto::link::DeviceAssignment {
            mavlink_addr: format!("udpin:127.0.0.1:{port_b}"),
            ..old.clone()
        };

        let mut tracker = AssignmentTracker::new(&[old.clone()]);
        connect_assignment(&cfg, &tx, &pool, &endpoints, &old)
            .await
            .expect("初始建链");
        assert!(pool.get("drone-x").is_some());

        let report = apply_assignments(
            &mut tracker,
            &[new.clone()],
            &cfg,
            &tx,
            &pool,
            &endpoints,
        )
        .await;

        assert_eq!(report.reconnected, 1, "应重建一条链路");
        assert_eq!(report.failed, 0);
        assert!(pool.get("drone-x").is_some(), "新链应已入池");
        assert_eq!(
            tracker
                .applied()
                .get("drone-x")
                .map(|a| a.mavlink_addr.clone()),
            Some(new.mavlink_addr.clone())
        );
    }

    /// 新链建不起来：旧链保留（该机继续在线工作），失败计数 +1。
    #[tokio::test]
    async fn apply_assignments_keeps_old_link_on_failure() {
        let port_a = free_udp_port();
        // 占用目标端口，让新链必然 bind 失败。
        let blocker = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind blocker");
        let blocked_port = blocker.local_addr().expect("local addr").port();

        let cfg = AdapterConfig::default();
        let (tx, _guard) = mpsc::channel(8);
        let pool = Arc::new(FcPool::default());
        let endpoints = EndpointRegistry::new();

        let old = rustcam_drone_proto::link::DeviceAssignment {
            drone_tag: "drone-y".into(),
            mavlink_addr: format!("udpin:127.0.0.1:{port_a}"),
            mavlink_sysid: 1,
            mavlink_compid: 1,
            ..Default::default()
        };
        let new = rustcam_drone_proto::link::DeviceAssignment {
            mavlink_addr: format!("udpin:127.0.0.1:{blocked_port}"),
            ..old.clone()
        };

        let mut tracker = AssignmentTracker::new(&[old.clone()]);
        connect_assignment(&cfg, &tx, &pool, &endpoints, &old)
            .await
            .expect("初始建链");

        let report = apply_assignments(&mut tracker, &[new], &cfg, &tx, &pool, &endpoints).await;

        assert_eq!(report.failed, 1, "应记一次失败");
        assert_eq!(report.reconnected, 0);
        assert!(pool.get("drone-y").is_some(), "旧链必须保留");
        assert_eq!(
            tracker
                .applied()
                .get("drone-y")
                .map(|a| a.mavlink_addr.clone()),
            Some(old.mavlink_addr.clone()),
            "生效配置仍应是旧地址"
        );
    }

    /// 参数没变：不重建（不闪断）。
    #[tokio::test]
    async fn apply_assignments_keeps_unchanged_link() {
        let port_a = free_udp_port();
        let cfg = AdapterConfig::default();
        let (tx, _guard) = mpsc::channel(8);
        let pool = Arc::new(FcPool::default());
        let endpoints = EndpointRegistry::new();

        let assignment = rustcam_drone_proto::link::DeviceAssignment {
            drone_tag: "drone-z".into(),
            mavlink_addr: format!("udpin:127.0.0.1:{port_a}"),
            mavlink_sysid: 1,
            mavlink_compid: 1,
            ..Default::default()
        };

        let mut tracker = AssignmentTracker::new(&[assignment.clone()]);
        connect_assignment(&cfg, &tx, &pool, &endpoints, &assignment)
            .await
            .expect("初始建链");

        let report =
            apply_assignments(&mut tracker, &[assignment], &cfg, &tx, &pool, &endpoints).await;

        assert_eq!(report.reconnected, 0);
        assert_eq!(report.connected, 0);
        assert_eq!(report.disconnected, 0);
        assert_eq!(report.failed, 0);
        assert!(pool.get("drone-z").is_some());
    }
}