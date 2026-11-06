//! 飞控链路：连接 MAVLink、驱动心跳、聚合遥测、执行指令。
//!
//! 生命周期：`link::run` 建连后本模块启动读 / 心跳 / 遥测三个任务；gRPC 会话
//! 订阅遥测与事件并向 `execute` 提交指令；飞控读循环出错时通过 `wait_closed`
//! 通知上层重建连接。

pub mod addr;
pub mod autopilot;
pub mod command;
pub mod state;
pub mod telemetry;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ::mavlink::dialects::ardupilotmega::{
    MavAutopilot, MavMessage, MavModeFlag, MavState, MavType, AUTOPILOT_VERSION_REQUEST_DATA,
    HEARTBEAT_DATA,
};
use ::mavlink::{connect_async, AsyncConnection, AsyncMavConnection, MavHeader};
use tokio::sync::{broadcast, watch};
use tokio::time::MissedTickBehavior;

use rustcam_drone_core::telemetry::Telemetry;
use rustcam_drone_proto::common as proto;

use crate::config::AdapterConfig;
use addr::MavAddress;
use autopilot::AutopilotHint;
use command::PendingCommands;
use state::{StateChange, VehicleState};

/// 飞控侧事件（上线 / 掉线 / STATUSTEXT 等），由 gRPC 会话转成 `DeviceEvent`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FcEvent {
    pub event_type: String,
    pub detail: String,
}

struct Shared {
    state: Mutex<VehicleState>,
    pending: PendingCommands,
    online: AtomicBool,
    target_system: AtomicU8,
    target_component: AtomicU8,
    seq: AtomicU32,
    /// 上次已上抛的固件版本：飞控可能周期性重发版本消息，只在变化时才上抛事件。
    last_version: Mutex<String>,
    /// 上次已上抛的飞控唯一标识。
    last_identity: Mutex<String>,
}

impl Shared {
    fn new(hint: AutopilotHint) -> Shared {
        Shared {
            state: Mutex::new(VehicleState::new(hint)),
            pending: PendingCommands::new(),
            online: AtomicBool::new(false),
            target_system: AtomicU8::new(1),
            target_component: AtomicU8::new(1),
            seq: AtomicU32::new(0),
            last_version: Mutex::new(String::new()),
            last_identity: Mutex::new(String::new()),
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, VehicleState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 记录固件版本；返回 true 表示这是新版本（需要上抛事件）。
    fn note_version(&self, version: &str) -> bool {
        let mut guard = self.last_version.lock().unwrap_or_else(|e| e.into_inner());
        if *guard == version {
            return false;
        }
        *guard = version.to_string();
        true
    }

    /// 记录唯一标识；返回 true 表示这是新的唯一标识（需要上抛事件）。
    fn note_identity(&self, unique_id: &str) -> bool {
        let mut guard = self.last_identity.lock().unwrap_or_else(|e| e.into_inner());
        if *guard == unique_id {
            return false;
        }
        *guard = unique_id.to_string();
        true
    }
}

#[derive(Clone)]
struct Settings {
    device_id: String,
    header: MavHeader,
    heartbeat_timeout: Duration,
    command_timeout: Duration,
    telemetry_period: Duration,
}

#[derive(Clone)]
pub struct FlightController {
    pub device_id: String,
    conn: Arc<AsyncConnection<MavMessage>>,
    shared: Arc<Shared>,
    telemetry_tx: broadcast::Sender<Telemetry>,
    event_tx: broadcast::Sender<FcEvent>,
    closed_tx: watch::Sender<bool>,
    shutdown_tx: watch::Sender<bool>,
    settings: Settings,
}

impl FlightController {
    /// 建立 MAVLink 连接并启动后台任务。
    pub async fn connect(cfg: &AdapterConfig) -> anyhow::Result<FlightController> {
        let address = MavAddress::parse(&cfg.mavlink_addr)
            .map_err(|e| anyhow::anyhow!("invalid mavlink_addr: {e}"))?;
        let hint = AutopilotHint::parse(&cfg.autopilot)?;
        let device_id = cfg.effective_device_id();

        tracing::info!(
            address = %address,
            transport = address.transport().as_str(),
            autopilot = %cfg.autopilot,
            device = %device_id,
            "connecting to flight controller"
        );

        let mut conn = connect_async::<MavMessage>(address.as_str())
            .await
            .map_err(|e| anyhow::anyhow!("mavlink connect {address} failed: {e}"))?;
        // PX4 / ArduPilot 新旧固件可能同时存在 MAVLink1 与 MAVLink2 帧。
        conn.set_allow_recv_any_version(true);

        let conn = Arc::new(conn);
        let shared = Arc::new(Shared::new(hint));
        let header = MavHeader {
            system_id: cfg.source_system as u32,
            component_id: cfg.source_component,
            sequence: 0,
        };
        let settings = Settings {
            device_id,
            header,
            heartbeat_timeout: Duration::from_millis(cfg.heartbeat_timeout_ms),
            command_timeout: Duration::from_millis(cfg.command_timeout_ms),
            telemetry_period: cfg.telemetry_period(),
        };

        let telemetry_capacity = cfg.buffer_size.clamp(64, 65_536);
        let (telemetry_tx, _) = broadcast::channel(telemetry_capacity);
        let (event_tx, _) = broadcast::channel(256);
        let (closed_tx, _) = watch::channel(false);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let fc = FlightController {
            device_id: settings.device_id.clone(),
            conn: Arc::clone(&conn),
            shared: Arc::clone(&shared),
            telemetry_tx,
            event_tx,
            closed_tx,
            shutdown_tx,
            settings: settings.clone(),
        };

        spawn_reader(
            Arc::clone(&conn),
            Arc::clone(&shared),
            fc.event_tx.clone(),
            settings.clone(),
            fc.closed_tx.clone(),
            shutdown_rx.clone(),
        );
        spawn_heartbeat(
            Arc::clone(&conn),
            Arc::clone(&shared),
            fc.event_tx.clone(),
            settings.clone(),
            shutdown_rx.clone(),
        );
        spawn_telemetry(
            fc.telemetry_tx.clone(),
            Arc::clone(&shared),
            settings,
            shutdown_rx,
        );

        Ok(fc)
    }

    /// 停止读 / 心跳 / 遥测任务；重连前必须调用，避免重复占用同一 UDP 端口。
    pub fn close(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    pub fn subscribe_telemetry(&self) -> broadcast::Receiver<Telemetry> {
        self.telemetry_tx.subscribe()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<FcEvent> {
        self.event_tx.subscribe()
    }

    /// 等待飞控读循环结束（链路断开）。
    pub async fn wait_closed(&self) {
        let mut rx = self.closed_tx.subscribe();
        if *rx.borrow() {
            return;
        }
        let _ = rx.changed().await;
    }

    pub async fn is_online(&self) -> bool {
        let now = now_ms();
        let timeout = self.settings.heartbeat_timeout;
        let state = self.shared.lock_state();
        state.is_online(now, timeout.as_millis() as u64)
    }

    /// 飞控实测唯一标识（小写十六进制）；空 = 飞控未提供。
    ///
    /// gRPC 会话建立时飞控链路可能已经读到 uid（连接顺序：先连飞控后注册），
    /// 因此上报 IdentityReport 时直接读当前值，而不是只等事件。
    pub fn unique_id(&self) -> String {
        self.shared.lock_state().unique_id.clone()
    }

    pub fn inflight_commands(&self) -> usize {
        self.shared.pending.len()
    }

    /// 执行一条下行指令；飞控离线时直接失败，不排队。
    pub async fn execute(&self, cmd: proto::Command) -> proto::CommandReply {
        let now = now_ms();
        let timeout_ms = self.settings.heartbeat_timeout.as_millis() as u64;
        let (autopilot, vehicle_type, online, home_alt) = {
            let state = self.shared.lock_state();
            (
                state.autopilot,
                state.vehicle_type,
                state.is_online(now, timeout_ms),
                state.home_alt,
            )
        };
        if !online {
            return rejected_reply(&cmd, 503, "flight controller offline (no HEARTBEAT)");
        }

        let target_system = self.shared.target_system.load(Ordering::Relaxed);
        let target_component = self.shared.target_component.load(Ordering::Relaxed);

        tracing::info!(
            cmd_id = %cmd.cmd_id,
            kind = %cmd.r#type,
            device = %cmd.device_id,
            autopilot = autopilot.as_str(),
            target_system,
            "executing command"
        );

        command::execute(
            &self.conn,
            self.settings.header,
            &self.shared.pending,
            autopilot,
            vehicle_type,
            target_system,
            target_component,
            home_alt,
            self.settings.command_timeout,
            &cmd,
        )
        .await
    }
}

fn spawn_reader(
    conn: Arc<AsyncConnection<MavMessage>>,
    shared: Arc<Shared>,
    event_tx: broadcast::Sender<FcEvent>,
    settings: Settings,
    closed_tx: watch::Sender<bool>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        loop {
            let received = tokio::select! {
                received = conn.recv() => received,
                _ = wait_shutdown(&mut shutdown_rx) => break,
            };
            let (header, msg) = match received {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "mavlink receive loop stopped");
                    break;
                }
            };

            let now = now_ms();
            let changes = {
                let mut state = shared.lock_state();
                state.apply(msg, now)
            };

            for change in changes {
                match change {
                    StateChange::Heartbeat {
                        autopilot,
                        armed,
                        mode,
                    } => {
                        // HEARTBEAT 头里的 system/component 才是指令应发往的地址。
                        shared
                            .target_system
                            .store(header.system_id as u8, Ordering::Relaxed);
                        shared
                            .target_component
                            .store(header.component_id, Ordering::Relaxed);
                        let first = !shared.online.swap(true, Ordering::SeqCst);
                        if first {
                            tracing::info!(
                                autopilot = autopilot.as_str(),
                                system_id = header.system_id,
                                component_id = header.component_id,
                                mode = %mode,
                                armed,
                                "flight controller online"
                            );
                            let _ = event_tx.send(FcEvent {
                                event_type: "fc_online".into(),
                                detail: format!("{} {} armed={}", autopilot.as_str(), mode, armed),
                            });
                            // 主动索取固件版本，便于日志与前端展示。
                            let req = MavMessage::AUTOPILOT_VERSION_REQUEST(
                                AUTOPILOT_VERSION_REQUEST_DATA {
                                    target_system: header.system_id as u8,
                                    target_component: header.component_id,
                                },
                            );
                            if let Err(e) = conn.send(&settings.header, &req).await {
                                tracing::debug!(error = %e, "failed to request autopilot version");
                            }
                        }
                    }
                    StateChange::Ack { command, result } => {
                        if !shared.pending.resolve(command, result) {
                            tracing::debug!(
                                command,
                                result = ?result,
                                "received COMMAND_ACK without a pending command"
                            );
                        }
                    }
                    StateChange::StatusText { severity, text } => {
                        tracing::info!(severity, text = %text, "flight controller status text");
                        let _ = event_tx.send(FcEvent {
                            event_type: "statustext".into(),
                            detail: text,
                        });
                    }
                    StateChange::AutopilotVersion { version, unique_id } => {
                        tracing::info!(version = %version, unique_id = %unique_id, "autopilot firmware version");
                        // 版本与唯一标识只在变化时上抛：飞控会周期性重发版本消息。
                        if shared.note_version(&version) {
                            let _ = event_tx.send(FcEvent {
                                event_type: "autopilot_version".into(),
                                detail: version,
                            });
                        }
                        // 唯一标识单独上抛，供 gRPC 会话上报 IdentityReport
                        // （drone.md §12.4 L3）。
                        if !unique_id.is_empty() && shared.note_identity(&unique_id) {
                            let _ = event_tx.send(FcEvent {
                                event_type: "identity".into(),
                                detail: unique_id,
                            });
                        }
                    }
                    StateChange::Landed { on_ground } => {
                        let _ = event_tx.send(FcEvent {
                            event_type: "landed_state".into(),
                            detail: if on_ground { "on_ground" } else { "in_air" }.to_string(),
                        });
                    }
                }
            }
        }

        let _ = shared.online.swap(false, Ordering::SeqCst);
        let _ = closed_tx.send(true);
    });
}

fn spawn_heartbeat(
    conn: Arc<AsyncConnection<MavMessage>>,
    shared: Arc<Shared>,
    event_tx: broadcast::Sender<FcEvent>,
    settings: Settings,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let heartbeat = MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            custom_mode: 0,
            mavtype: MavType::MAV_TYPE_GCS,
            autopilot: MavAutopilot::MAV_AUTOPILOT_INVALID,
            base_mode: MavModeFlag::empty(),
            system_status: MavState::MAV_STATE_ACTIVE,
            mavlink_version: 3,
        });
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                _ = wait_shutdown(&mut shutdown_rx) => break,
            }
            if let Err(e) = conn.send(&settings.header, &heartbeat).await {
                tracing::warn!(error = %e, "failed to send GCS heartbeat");
                break;
            }

            let now = now_ms();
            let online = {
                let state = shared.lock_state();
                state.is_online(now, settings.heartbeat_timeout.as_millis() as u64)
            };
            if !online && shared.online.swap(false, Ordering::SeqCst) {
                tracing::warn!(
                    timeout_ms = settings.heartbeat_timeout.as_millis() as u64,
                    "flight controller heartbeat lost"
                );
                let _ = event_tx.send(FcEvent {
                    event_type: "fc_offline".into(),
                    detail: "heartbeat timeout".into(),
                });
            }
        }
    });
}

fn spawn_telemetry(
    telemetry_tx: broadcast::Sender<Telemetry>,
    shared: Arc<Shared>,
    settings: Settings,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        if *shutdown_rx.borrow() {
            return;
        }
        let mut ticker = tokio::time::interval(settings.telemetry_period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                _ = shutdown_rx.changed() => break,
            }
            if !shared.online.load(Ordering::SeqCst) {
                continue;
            }
            let now = now_ms();
            let seq = shared.seq.fetch_add(1, Ordering::Relaxed);
            let telemetry = {
                let state = shared.lock_state();
                state.to_telemetry(
                    &settings.device_id,
                    seq,
                    now,
                    settings.heartbeat_timeout.as_millis() as u64,
                )
            };
            // 没有订阅者（gRPC 断线）时只是丢弃这一帧，遥测允许丢旧。
            let _ = telemetry_tx.send(telemetry);
        }
    });
}

fn rejected_reply(cmd: &proto::Command, code: i32, message: &str) -> proto::CommandReply {
    proto::CommandReply {
        cmd_id: cmd.cmd_id.clone(),
        device_id: cmd.device_id.clone(),
        status: rustcam_drone_core::command::status::REJECTED.to_string(),
        code,
        message: message.to_string(),
        ts_ms: now_ms(),
    }
}

async fn wait_shutdown(rx: &mut watch::Receiver<bool>) {
    if *rx.borrow() {
        return;
    }
    let _ = rx.changed().await;
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::mavlink::dialects::ardupilotmega::{
        MavCmd, MavResult, COMMAND_ACK_DATA, GLOBAL_POSITION_INT_DATA,
    };
    use std::time::Duration;

    fn free_udp_port() -> u16 {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.local_addr().unwrap().port()
    }

    fn test_config(port: u16) -> AdapterConfig {
        AdapterConfig {
            mavlink_addr: format!("udpin:127.0.0.1:{port}"),
            autopilot: "auto".into(),
            telemetry_hz: 20,
            heartbeat_timeout_ms: 2_000,
            command_timeout_ms: 1_000,
            ..Default::default()
        }
    }

    fn px4_heartbeat() -> MavMessage {
        MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            custom_mode: crate::fc::autopilot::px4_custom_mode(
                crate::fc::autopilot::PX4_MAIN_MODE_AUTO,
                crate::fc::autopilot::PX4_SUB_MODE_AUTO_LOITER,
            ),
            mavtype: MavType::MAV_TYPE_QUADROTOR,
            autopilot: MavAutopilot::MAV_AUTOPILOT_PX4,
            base_mode: MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED,
            system_status: MavState::MAV_STATE_ACTIVE,
            mavlink_version: 3,
        })
    }

    #[tokio::test]
    async fn real_mavlink_link_reports_telemetry_and_acks_commands() {
        let port = free_udp_port();
        let fc = FlightController::connect(&test_config(port)).await.unwrap();
        let mut telemetry_rx = fc.subscribe_telemetry();

        let vehicle = ::mavlink::connect_async::<MavMessage>(&format!("udpout:127.0.0.1:{port}"))
            .await
            .unwrap();
        let header = MavHeader {
            system_id: 1,
            component_id: 1,
            sequence: 0,
        };
        vehicle.send(&header, &px4_heartbeat()).await.unwrap();

        // 状态与心跳并发：先等到带 PX4 标识的遥测
        let telemetry = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match telemetry_rx.recv().await {
                    Ok(t) if t.autopilot == "px4" => break t,
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .expect("should receive telemetry over MAVLink");
        assert_eq!(telemetry.mode, "AUTO.LOITER");
        assert_eq!(telemetry.device_id, "drone-001");

        // 位置消息进入同一条遥测
        vehicle
            .send(
                &header,
                &MavMessage::GLOBAL_POSITION_INT(GLOBAL_POSITION_INT_DATA {
                    time_boot_ms: 1,
                    lat: 312_304_000,
                    lon: 1_214_737_000,
                    alt: 62_500,
                    relative_alt: 12_500,
                    vx: 0,
                    vy: 0,
                    vz: 0,
                    hdg: 9_000,
                }),
            )
            .await
            .unwrap();
        let telemetry = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match telemetry_rx.recv().await {
                    Ok(t) if t.lat > 0.0 => break t,
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .expect("position should be aggregated");
        assert!((telemetry.lat - 31.2304).abs() < 1e-6);
        assert!((telemetry.lon - 121.4737).abs() < 1e-6);

        // 指令 → COMMAND_LONG → ACK → 回执
        vehicle.send(&header, &px4_heartbeat()).await.unwrap();
        let fc_handle = fc.clone();
        let command = proto::Command {
            cmd_id: "cmd-it-1".into(),
            device_id: "drone-001".into(),
            r#type: "rtl".into(),
            params: Default::default(),
            deadline_ms: 0,
            operator: "tester".into(),
        };
        let execution = tokio::spawn(async move { fc_handle.execute(command).await });

        let command_long = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let (_, msg) = vehicle.recv().await.unwrap();
                if let MavMessage::COMMAND_LONG(c) = msg {
                    break c;
                }
            }
        })
        .await
        .expect("adapter should send COMMAND_LONG");
        assert_eq!(command_long.command, MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH);
        assert_eq!(command_long.target_system, 1);

        vehicle
            .send(
                &header,
                &MavMessage::COMMAND_ACK(COMMAND_ACK_DATA {
                    command: MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH,
                    result: MavResult::MAV_RESULT_ACCEPTED,
                }),
            )
            .await
            .unwrap();

        let reply = tokio::time::timeout(Duration::from_secs(3), execution)
            .await
            .expect("command should complete")
            .unwrap();
        assert_eq!(reply.status, rustcam_drone_core::command::status::ACCEPTED);
        assert_eq!(reply.code, 200);

        fc.close();
    }
}
