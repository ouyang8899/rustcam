//! 单个 `udpin` 端口共享给多架飞控。
//!
//! `mavlink` crate 的 `AsyncUdpConnection` 在 `udpin` 模式下只记住**最后一个**发送者，
//! 多架飞控打同一个端口时回包会发错对象。这里直接接管 socket：
//!
//! - 收：`recv_from` 拿到 `(帧, 对端地址)`，按帧头 `system_id` 记下对端并分派给订阅者；
//! - 发：按目标 `system_id` 查对端地址，自己序列化 MAVLink 2 帧后 `send_to`。
//!
//! 一个端口一个 [`SharedEndpoint`]，由 [`EndpointRegistry`] 按归一化地址去重。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::Duration;

use ::mavlink::dialects::ardupilotmega::MavMessage;
use ::mavlink::{MAVLinkV2MessageRaw, MavFrame, MavHeader, MavlinkVersion};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch};

/// MAVLink 1 起始字节。
const STX_V1: u8 = 0xFE;
/// MAVLink 2 起始字节。
const STX_V2: u8 = 0xFD;
/// 收包缓冲；MAVLink 2 最大帧 280 字节，留足余量。
const RECV_BUF: usize = 2048;

type RouteSender = mpsc::UnboundedSender<(MavHeader, MavMessage)>;

/// 一个共享 UDP 监听端口。
pub struct SharedEndpoint {
    key: String,
    socket: Arc<UdpSocket>,
    peers: StdMutex<HashMap<u8, SocketAddr>>,
    routes: RwLock<HashMap<u8, RouteSender>>,
    sequence: AtomicU8,
    shutdown: watch::Sender<bool>,
    closed: AtomicBool,
}

impl SharedEndpoint {
    /// 绑定 `target`（如 `0.0.0.0:14540`）并启动分派任务。
    pub async fn bind(key: String, target: &str) -> anyhow::Result<Arc<SharedEndpoint>> {
        let socket = Arc::new(UdpSocket::bind(target).await?);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let endpoint = Arc::new(SharedEndpoint {
            key,
            socket,
            peers: StdMutex::new(HashMap::new()),
            routes: RwLock::new(HashMap::new()),
            sequence: AtomicU8::new(0),
            shutdown: shutdown_tx,
            closed: AtomicBool::new(false),
        });
        tokio::spawn(dispatch(endpoint.clone(), shutdown_rx));
        Ok(endpoint)
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    #[allow(dead_code)]
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// 该端点是否已因无订阅者而关闭。
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// 订阅某个飞控 `sysid` 的入站帧。返回接收端与用于注销的句柄。
    pub fn subscribe(&self, sysid: u8) -> (mpsc::UnboundedReceiver<(MavHeader, MavMessage)>, RouteSender) {
        let (tx, rx) = mpsc::unbounded_channel();
        self.routes
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(sysid, tx.clone());
        (rx, tx)
    }

    /// 注销订阅；仅当当前登记的就是 `sender` 本身时移除（避免重连时误删新订阅）。
    pub fn release(&self, sysid: u8, sender: &RouteSender) {
        let empty = {
            let mut routes = self.routes.write().unwrap_or_else(|e| e.into_inner());
            if let Some(current) = routes.get(&sysid) {
                if current.same_channel(sender) {
                    routes.remove(&sysid);
                }
            }
            routes.is_empty()
        };
        if empty && !self.closed.swap(true, Ordering::SeqCst) {
            let _ = self.shutdown.send(true);
        }
    }

    #[allow(dead_code)]
    pub fn subscriber_count(&self) -> usize {
        self.routes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// 已记录的对端地址（测试与诊断用）。
    #[allow(dead_code)]
    pub fn peer_addr(&self, sysid: u8) -> Option<SocketAddr> {
        self.peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&sysid)
            .copied()
    }

    /// 向 `sysid` 对应的对端发送一帧。
    pub async fn send(
        &self,
        header: &MavHeader,
        msg: &MavMessage,
        sysid: u8,
    ) -> anyhow::Result<()> {
        let peer = self
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&sysid)
            .copied();
        // 飞控还没发过帧（地址未知）时没有可发送的对象；丢弃这一帧即可，
        // 不能返回错误，否则会打断心跳任务。
        let Some(peer) = peer else {
            tracing::debug!(
                key = %self.key,
                sysid,
                "shared endpoint has no peer address yet; dropping outbound frame"
            );
            return Ok(());
        };

        let mut raw = MAVLinkV2MessageRaw::new();
        let mut frame_header = *header;
        frame_header.sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        raw.serialize_message(frame_header, msg);
        self.socket.send_to(raw.raw_bytes(), peer).await?;
        Ok(())
    }
}

/// 共享端点注册表：同一归一化地址只绑定一次。
///
/// 只保存 `Weak`：端点由各 `FlightController` 持有，全部释放后 socket 才会真正
/// 关闭（端口才能重新绑定），注册表本身不能成为“僵尸端口”的持有者。
#[derive(Clone, Default)]
pub struct EndpointRegistry {
    endpoints: Arc<tokio::sync::Mutex<HashMap<String, std::sync::Weak<SharedEndpoint>>>>,
}

impl EndpointRegistry {
    pub fn new() -> EndpointRegistry {
        EndpointRegistry::default()
    }

    /// 取已绑定且仍可用的端点，否则绑定新的。
    pub async fn get_or_bind(
        &self,
        key: String,
        target: &str,
    ) -> anyhow::Result<Arc<SharedEndpoint>> {
        let mut guard = self.endpoints.lock().await;
        if let Some(existing) = guard.get(&key).and_then(std::sync::Weak::upgrade) {
            if !existing.is_closed() {
                return Ok(existing);
            }
        }
        // 旧端点已关闭但 socket 可能还没释放，短暂重试等待端口回收。
        guard.remove(&key);
        let mut last_err = None;
        for _ in 0..50 {
            match SharedEndpoint::bind(key.clone(), target).await {
                Ok(endpoint) => {
                    guard.insert(key, Arc::downgrade(&endpoint));
                    return Ok(endpoint);
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("failed to bind shared endpoint {target}")))
    }
}

/// 分派循环：收帧 → 记对端 → 按 sysid 投递。
async fn dispatch(endpoint: Arc<SharedEndpoint>, mut shutdown_rx: watch::Receiver<bool>) {
    let mut buf = [0u8; RECV_BUF];
    loop {
        let received = tokio::select! {
            received = endpoint.socket.recv_from(&mut buf) => received,
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
                continue;
            }
        };

        let (len, peer) = match received {
            Ok(v) => v,
            Err(e) => {
                // Windows 上对端端口关闭后 `recv_from` 可能持续返回 ConnectionReset，
                // 短暂退避后继续，避免误杀整个端点。
                tracing::debug!(error = %e, key = %endpoint.key, "shared endpoint recv failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };

        let Some(frame) = decode_frame(&buf[..len]) else {
            tracing::debug!(key = %endpoint.key, bytes = len, "dropped undecodable mavlink frame");
            continue;
        };

        let sysid = frame.header.system_id as u8;
        endpoint
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(sysid, peer);

        let sender = endpoint
            .routes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&sysid)
            .cloned();
        if let Some(sender) = sender {
            let _ = sender.send((frame.header, frame.msg));
        } else {
            tracing::debug!(key = %endpoint.key, sysid, "no subscriber for shared endpoint frame");
        }
    }
}

/// 从完整帧（含 STX）解析出 `MavFrame`。
///
/// `deser_with_flags` 的入参必须从 sequence 字段开始、且只包含声明长度的 payload
/// （不含 CRC）。MAVLink 2 会裁掉 payload 末尾的 0，解析器再补零，因此这里必须
/// 按帧头的 `len` 精确切片，不能把 CRC 也当成 payload 传进去。
fn decode_frame(buf: &[u8]) -> Option<MavFrame<MavMessage>> {
    let payload_len = *buf.get(1)? as usize;
    match buf.first().copied()? {
        // STX(1) + len(1) + seq/sysid/compid/msgid(4) + payload + crc(2)
        STX_V1 => {
            let end = 2 + 4 + payload_len;
            if buf.len() < end + 2 {
                return None;
            }
            MavFrame::deser_with_flags(MavlinkVersion::V1, 0, &buf[2..end]).ok()
        }
        // STX(1) + len(1) + incompat(1) + compat(1) + seq/sysid/compid/msgid(6)
        // + payload + crc(2)；`IFLAG_SYSID32` 时 sysid 多 3 字节。
        STX_V2 => {
            let incompat = *buf.get(2)?;
            let sysid_extra = if incompat & ::mavlink::consts::v2::IFLAG_SYSID32 != 0 {
                3
            } else {
                0
            };
            let end = 4 + 6 + sysid_extra + payload_len;
            if buf.len() < end + 2 {
                return None;
            }
            MavFrame::deser_with_flags(MavlinkVersion::V2, incompat, &buf[4..end]).ok()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::mavlink::dialects::ardupilotmega::{HEARTBEAT_DATA, MavAutopilot, MavModeFlag, MavType};

    fn heartbeat() -> MavMessage {
        MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            custom_mode: 0,
            mavtype: MavType::MAV_TYPE_QUADROTOR,
            autopilot: MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA,
            base_mode: MavModeFlag::empty(),
            system_status: ::mavlink::dialects::ardupilotmega::MavState::MAV_STATE_ACTIVE,
            mavlink_version: 3,
        })
    }

    fn header(sysid: u32) -> MavHeader {
        MavHeader {
            system_id: sysid,
            component_id: 1,
            sequence: 0,
        }
    }

    fn encode(sysid: u32) -> Vec<u8> {
        let mut raw = MAVLinkV2MessageRaw::new();
        raw.serialize_message(header(sysid), &heartbeat());
        raw.raw_bytes().to_vec()
    }

    async fn fake_fc(sysid: u32, server: SocketAddr) -> UdpSocket {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.send_to(&encode(sysid), server).await.unwrap();
        socket
    }

    #[tokio::test]
    async fn routes_frames_by_system_id() {
        let endpoint = SharedEndpoint::bind("test".into(), "127.0.0.1:0").await.unwrap();
        let server = endpoint.local_addr().unwrap();

        let (mut rx1, _h1) = endpoint.subscribe(1);
        let (mut rx2, _h2) = endpoint.subscribe(2);

        let _fc1 = fake_fc(1, server).await;
        let _fc2 = fake_fc(2, server).await;

        let (h1, m1) = tokio::time::timeout(Duration::from_secs(2), rx1.recv())
            .await
            .expect("sysid 1 frame timed out")
            .expect("sysid 1 channel closed");
        let (h2, m2) = tokio::time::timeout(Duration::from_secs(2), rx2.recv())
            .await
            .expect("sysid 2 frame timed out")
            .expect("sysid 2 channel closed");

        assert_eq!(h1.system_id, 1);
        assert_eq!(h2.system_id, 2);
        assert!(matches!(m1, MavMessage::HEARTBEAT(_)));
        assert!(matches!(m2, MavMessage::HEARTBEAT(_)));
        assert!(endpoint.peer_addr(1).is_some());
        assert!(endpoint.peer_addr(2).is_some());
    }

    #[tokio::test]
    async fn sends_only_to_the_target_peer() {
        let endpoint = SharedEndpoint::bind("test".into(), "127.0.0.1:0").await.unwrap();
        let server = endpoint.local_addr().unwrap();

        let (mut rx1, _h1) = endpoint.subscribe(1);
        let (mut rx2, _h2) = endpoint.subscribe(2);
        let fc1 = fake_fc(1, server).await;
        let fc2 = fake_fc(2, server).await;
        // 等待分派器记录两个对端。
        let _ = tokio::time::timeout(Duration::from_secs(2), rx1.recv()).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), rx2.recv()).await;

        let msg = heartbeat();
        endpoint.send(&header(255), &msg, 1).await.unwrap();

        let mut buf = [0u8; RECV_BUF];
        let (len, _) = tokio::time::timeout(Duration::from_secs(2), fc1.recv_from(&mut buf))
            .await
            .expect("fc1 should receive the command")
            .unwrap();
        let frame = decode_frame(&buf[..len]).expect("valid frame");
        assert_eq!(frame.header.system_id, 255);

        let received = tokio::time::timeout(Duration::from_millis(300), fc2.recv_from(&mut buf)).await;
        assert!(received.is_err(), "fc2 must not receive fc1's command");
    }

    #[tokio::test]
    async fn endpoint_closes_when_last_subscriber_leaves() {
        let endpoint = SharedEndpoint::bind("test".into(), "127.0.0.1:0").await.unwrap();
        let (_rx, handle) = endpoint.subscribe(7);
        assert_eq!(endpoint.subscriber_count(), 1);
        endpoint.release(7, &handle);
        assert_eq!(endpoint.subscriber_count(), 0);
        assert!(endpoint.is_closed());
    }

    #[tokio::test]
    async fn release_keeps_newer_subscription() {
        let endpoint = SharedEndpoint::bind("test".into(), "127.0.0.1:0").await.unwrap();
        let (_old_rx, old_handle) = endpoint.subscribe(3);
        let (_new_rx, _new_handle) = endpoint.subscribe(3);
        endpoint.release(3, &old_handle);
        assert_eq!(endpoint.subscriber_count(), 1);
        assert!(!endpoint.is_closed());
    }
}
