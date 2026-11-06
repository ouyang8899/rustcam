use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustcam_drone_proto::common::CommandReply;
use rustcam_drone_proto::link::adapter_link_client::AdapterLinkClient;
use rustcam_drone_proto::link::adapter_message::Payload;
use rustcam_drone_proto::link::service_message;
use rustcam_drone_proto::link::{AdapterMessage, Heartbeat, Register, ServiceMessage};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Endpoint;

use crate::config::AdapterConfig;
use crate::fc::{self, FlightController};

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
    let fc = FlightController::connect(cfg).await?;
    tracing::info!(device = %fc.device_id, "flight controller link established");
    // 会话结束后必须停掉后台任务，否则重连会重复占用同一 UDP 端口。
    let result = run_session(cfg, fc.clone(), ledger.clone()).await;
    fc.close();
    result
}

async fn run_session(
    cfg: &AdapterConfig,
    fc: FlightController,
    ledger: Arc<CommandLedger>,
) -> anyhow::Result<()> {
    let endpoint = Endpoint::from_shared(cfg.service_addr.clone())
        .map_err(|e| anyhow::anyhow!("invalid service_addr: {e}"))?
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(30));
    let channel = endpoint.connect().await?;
    let mut client = AdapterLinkClient::new(channel);

    let capacity = cfg.buffer_size.clamp(64, 65_536);
    let (tx, rx) = mpsc::channel::<AdapterMessage>(capacity);

    let devices = cfg.registration_devices();
    tx.send(AdapterMessage {
        payload: Some(Payload::Register(Register {
            adapter_id: cfg.adapter_id.clone(),
            protocol: cfg.protocol.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            devices,
            identities: Vec::new(),
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

    loop {
        tokio::select! {
            message = inbound.message() => {
                match message? {
                    Some(msg) => handle_service_message(msg, &fc, &tx, &ledger, cfg.heartbeat_interval_secs).await?,
                    None => return Ok(()),
                }
            }
            _ = fc.wait_closed() => {
                return Err(anyhow::anyhow!("flight controller link lost"));
            }
        }
    }
}

async fn handle_service_message(
    msg: ServiceMessage,
    fc: &FlightController,
    tx: &mpsc::Sender<AdapterMessage>,
    ledger: &Arc<CommandLedger>,
    fallback_heartbeat_secs: u64,
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
                assigned = ?ack.assigned_devices,
                heartbeat_interval_sec = ack.heartbeat_interval_sec,
                "[Adapter] registered with drone-service"
            );
            // 服务端只分配了部分设备时，未被分配的设备不再上报（规则 §6.5 独占）。
            let assigned = ack.assigned_devices.is_empty()
                || ack.assigned_devices.iter().any(|d| d == &fc.device_id);
            if !assigned {
                tracing::warn!(
                    device = %fc.device_id,
                    "[Adapter] device not assigned by drone-service, telemetry suppressed"
                );
            }
            // 心跳周期以服务端为准（规则 §5.7：adapter 不得写死）。
            let interval_secs =
                cfg_heartbeat_fallback(ack.heartbeat_interval_sec, fallback_heartbeat_secs);
            spawn_heartbeat_forwarder(tx.clone(), fc.clone(), interval_secs);
            if assigned {
                spawn_telemetry_forwarder(tx.clone(), fc.clone());
                spawn_event_forwarder(tx.clone(), fc.clone());
            }
        }
        Some(service_message::Payload::Command(cmd)) => {
            // 已执行过的 cmd_id 不再重复执行（规则 §16）。
            if ledger.is_completed(&cmd.cmd_id) {
                tracing::warn!(cmd_id = %cmd.cmd_id, "[Adapter] duplicate cmd_id ignored");
                return Ok(());
            }
            let fc = fc.clone();
            let tx = tx.clone();
            let ledger = ledger.clone();
            tokio::spawn(async move {
                let cmd_id = cmd.cmd_id.clone();
                let kind = cmd.r#type.clone();
                let reply = fc.execute(cmd).await;
                tracing::info!(
                    cmd_id = %cmd_id,
                    kind = %kind,
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
                device = %state.device_id,
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
    fc: FlightController,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(interval_secs.max(1));
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let online = fc.is_online().await;
            let msg = AdapterMessage {
                payload: Some(Payload::Heartbeat(Heartbeat {
                    ts_ms: fc::now_ms(),
                    device_count: 1,
                    inflight_commands: fc.inflight_commands() as i32,
                    adapter_state: if online {
                        "ok".into()
                    } else {
                        "fc_offline".into()
                    },
                })),
            };
            if tx.send(msg).await.is_err() {
                break;
            }
        }
    });
}

fn spawn_telemetry_forwarder(tx: mpsc::Sender<AdapterMessage>, fc: FlightController) {
    tokio::spawn(async move {
        let mut telemetry_rx = fc.subscribe_telemetry();
        loop {
            match telemetry_rx.recv().await {
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
            }
        }
    });
}

fn spawn_event_forwarder(tx: mpsc::Sender<AdapterMessage>, fc: FlightController) {
    tokio::spawn(async move {
        let mut event_rx = fc.subscribe_events();
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    let msg = AdapterMessage {
                        payload: Some(Payload::Event(rustcam_drone_proto::common::DeviceEvent {
                            device_id: fc.device_id.clone(),
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

    #[test]
    fn ledger_dedups_completed_commands() {
        let ledger = CommandLedger::default();
        assert!(!ledger.is_completed("cmd-1"));
        ledger.mark_completed("cmd-1");
        assert!(ledger.is_completed("cmd-1"));
        assert!(!ledger.is_completed("cmd-2"));
    }

    #[test]
    fn ledger_keeps_pending_replies_for_resend() {
        let ledger = CommandLedger::default();
        ledger.record_reply(CommandReply {
            cmd_id: "cmd-1".into(),
            device_id: "d1".into(),
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
