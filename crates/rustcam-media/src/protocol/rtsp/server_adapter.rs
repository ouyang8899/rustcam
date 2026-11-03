use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::domain::SessionType;
use crate::error::{AppError, Result};
use crate::media::rtp_server::{UDPRtpServer, RtpServerActive};
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::event::{ProtocolType, PtzCommand, SignalEvent, TransportType};
use crate::protocol::rtsp::auth::{
    build_www_authenticate, generate_nonce, RtspAuthContext,
};
use crate::protocol::rtsp::common::{
    parse_interleaved_channels, InterleavedBuilder, InterleavedFrame, InterleavedStats, RtcpPacket, RTSP_USER_AGENT,
};
use crate::protocol::rtsp::response::RtspResponse;
use crate::protocol::rtsp::sdp::SdpParser;
use crate::protocol::rtsp::session::{
    create_session, get_session, get_session_by_stream_key, remove_session, RtspSessionState,
    TrackTransport,
};
use crate::protocol::rtsp::RtpTransportMode;
use crate::protocol::traits::ProtocolDeps;

const UDP_EVENT_QUEUE_CAPACITY: usize = 4096;
const RTCP_RR_INTERVAL_SECS: u64 = 50;
const UDP_RTP_BUF: usize = 2048;
const UDP_RTCP_BUF: usize = 512;

struct UdpTrack {
    server: Arc<UDPRtpServer>,
    stats: Arc<Mutex<InterleavedStats>>,
    client_addr: Arc<Mutex<Option<SocketAddr>>>,
    receive_handle: JoinHandle<()>,
    rr_handle: JoinHandle<()>,
}

pub struct RtspServerAdapter {
    recv_buffer: Vec<u8>,
    session: Option<String>,
    cseq: u32,
    remote_addr: Option<SocketAddr>,
    write: Option<Arc<RwLock<OwnedWriteHalf>>>,

    tcp_stats: HashMap<u8, Arc<Mutex<InterleavedStats>>>,

    deps: ProtocolDeps,

    device_id: Option<i64>,
    device_tag: Option<String>,

    udp_tracks: HashMap<String, UdpTrack>,

    udp_event_tx: mpsc::Sender<SignalEvent>,
    udp_event_rx: mpsc::Receiver<SignalEvent>,

    auth_nonce: Option<String>,
    auth_ctx: Option<RtspAuthContext>,
}

impl RtspServerAdapter {
    pub fn new(deps: ProtocolDeps) -> Self {
        let (udp_event_tx, udp_event_rx) = mpsc::channel(UDP_EVENT_QUEUE_CAPACITY);
        Self {
            recv_buffer: Vec::with_capacity(4096),
            session: None,
            cseq: 0,
            remote_addr: None,
            write: None,
            tcp_stats: HashMap::new(),
            deps,
            device_id: None,
            device_tag: None,
            udp_tracks: HashMap::new(),
            udp_event_tx,
            udp_event_rx,
            auth_nonce: None,
            auth_ctx: None,
        }
    }

    fn parse_request_line(line: &str) -> Option<(String, String, String)> {
        let mut it = line.split_whitespace();
        Some((it.next()?.to_string(), it.next()?.to_string(), it.next()?.to_string()))
    }

    fn parse_headers(lines: &[&str]) -> HashMap<String, String> {
        let mut headers = HashMap::with_capacity(lines.len());
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        headers
    }

    pub fn extract_stream_key(url: &str) -> String {
        let url = url.strip_prefix("rtsp://").unwrap_or(url);
        let url = url.rsplit_once('@').map(|(_, h)| h).unwrap_or(url);
        let url = url.split_once('?').map(|(p, _)| p).unwrap_or(url);
        match url.split_once('/') {
            Some((_, path)) => {
                let path = path.trim_end_matches('/');
                if path.is_empty() {
                    url.split_once('/').unwrap().0.to_string()
                } else {
                    path.to_string()
                }
            }
            None => url.to_string(),
        }
    }

    fn extract_track_id(url: &str) -> Option<String> {
        let key = Self::extract_stream_key(url);
        let (_, last) = key.rsplit_once('/')?;
        if last.starts_with("trackID=") {
            Some(last.to_string())
        } else {
            None
        }
    }

    fn base_stream_key(url: &str) -> String {
        let tid = match Self::extract_track_id(url) {
            Some(t) => t,
            None => return Self::extract_stream_key(url),
        };
        let key = Self::extract_stream_key(url);
        let stripped = key
            .strip_suffix(&format!("/{}", tid))
            .or_else(|| key.strip_suffix(&tid))
            .unwrap_or(&key);
        stripped.trim_end_matches('/').to_string()
    }

    fn parse_client_ports(transport: &str) -> Option<(u16, u16)> {
        for part in transport.split(';') {
            let part = part.trim();
            if let Some(v) = part.strip_prefix("client_port=") {
                let (a, b) = v.split_once('-')?;
                return Some((a.trim().parse().ok()?, b.trim().parse().ok()?));
            }
        }
        None
    }

    fn extract_rtsp_message(buffer: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
        let header_end = buffer.windows(4).position(|w| w == b"\r\n\r\n")? + 4;
        let header = std::str::from_utf8(&buffer[..header_end]).ok()?;
        let content_length = Self::find_content_length(header);
        let total = header_end + content_length;
        if buffer.len() < total {
            return None;
        }
        Some((buffer[..total].to_vec(), buffer[total..].to_vec()))
    }

    fn find_content_length(header: &str) -> usize {
        for line in header.lines() {
            if let Some(v) = line
                .strip_prefix("Content-Length:")
                .or_else(|| line.strip_prefix("content-length:"))
            {
                return v.trim().parse().unwrap_or(0);
            }
        }
        0
    }

    fn parse_interleaved_frames(buffer: &[u8]) -> (Vec<(u8, Vec<u8>)>, usize) {
        let mut frames = Vec::new();
        let mut pos = 0;
        while pos + 4 <= buffer.len() {
            if buffer[pos] != 0x24 {
                break;
            }
            let channel = buffer[pos + 1];
            let len = u16::from_be_bytes([buffer[pos + 2], buffer[pos + 3]]) as usize;
            if pos + 4 + len > buffer.len() {
                break;
            }
            frames.push((channel, buffer[pos + 4..pos + 4 + len].to_vec()));
            pos += 4 + len;
        }
        (frames, pos)
    }

    fn device_id_from_key(key: &str) -> i64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in key.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        ((hash & 0x7FFF_FFFF_FFFF_FFFF) | 1) as i64
    }

    fn default_h264_track() -> Vec<crate::protocol::rtsp::sdp::SdpTrack> {
        vec![crate::protocol::rtsp::sdp::SdpTrack {
            media: "video".to_string(),
            payload_type: 96,
            codec: "H264".to_string(),
            clock_rate: 90000,
            fmtp: Some("packetization-mode=1".to_string()),
            control: Some("trackID=0".to_string()),
            direction: crate::protocol::rtsp::sdp::SdpDirection::RecvOnly,
            attributes: std::collections::HashMap::new(),
        }]
    }

    async fn send_raw(&self, data: &[u8]) -> Result<()> {
        let Some(ref write_arc) = self.write else {
            tracing::warn!("[RTSP] 无 write 端，丢弃 {} 字节", data.len());
            return Ok(());
        };
        let mut write = write_arc.write().await;
        write
            .write_all(data)
            .await
            .map_err(|e| AppError::Internal(format!("TCP write error: {}", e)))?;
        Ok(())
    }

    async fn alloc_udp_for_track(
        &self,
        stream_key: &str,
        track_id: &str,
    ) -> Result<(u16, u16, u32, Arc<UDPRtpServer>, String)> {
        let manager = crate::media::get_rtp_server_manager().ok_or_else(|| {
            AppError::MediaServerError("RtpServerManager not initialized".to_string())
        })?;

        let bind_addr = self.deps.config.server.host.clone();
        let device_tag = self.device_tag.clone().unwrap_or_default();
        let actor = format!("rtsp_server_{}_{}", stream_key, track_id);

        let (rtp_port, local_ssrc, server) = manager
            .allocate_udp(&device_tag, stream_key, &actor)
            .await
            .map_err(|e| AppError::MediaServerError(format!("UDP 端口分配失败: {}", e)))?;

        Ok((rtp_port, rtp_port + 1, local_ssrc, server, bind_addr))
    }

    async fn stop_udp_track_tasks(&mut self, track_id: &str) {
        if let Some(track) = self.udp_tracks.remove(track_id) {
            track.receive_handle.abort();
            track.rr_handle.abort();
        }
    }

    async fn release_udp_track(&mut self, track_id: &str) {
        self.stop_udp_track_tasks(track_id).await;

        let port = if let Some(sess_id) = self.session.as_ref() {
            if let Some(sess) = get_session(sess_id) {
                let mut s = sess.write().await;
                let port = s.track_transports.get(track_id).and_then(|t| t.rtp_port);
                s.track_transports.remove(track_id);
                port
            } else {
                None
            }
        } else {
            None
        };

        if let Some(port) = port {
            if let Some(manager) = crate::media::get_rtp_server_manager() {
                manager.release(port, TransportType::Udp).await;
            }
        }
    }

    async fn release_all_udp_tracks(&mut self) {
        let mut ids: HashSet<String> = self.udp_tracks.keys().cloned().collect();
        if let Some(sess_id) = self.session.as_ref() {
            if let Some(sess) = get_session(sess_id) {
                let s = sess.read().await;
                for k in s.track_transports.keys() {
                    ids.insert(k.clone());
                }
            }
        }
        for id in ids {
            self.release_udp_track(&id).await;
        }
    }

    async fn start_udp_tracks(&mut self, device_id: i64) {
        if !self.udp_tracks.is_empty() {
            return;
        }

        let Some(sess_id) = self.session.clone() else {
            tracing::warn!("[RTSP] 无会话，无法启动 UDP");
            return;
        };
        let Some(sess) = get_session(&sess_id) else {
            tracing::warn!("[RTSP] 会话 {} 已不存在", sess_id);
            return;
        };

        let tracks: Vec<(String, TrackTransport)> = {
            let s = sess.read().await;
            if s.transport_mode != RtpTransportMode::Udp {
                return;
            }
            s.track_transports
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };

        if tracks.is_empty() {
            tracing::warn!("[RTSP] 会话无 UDP track");
            return;
        }

        for (track_id, tt) in tracks {
            let Some(server) = tt.server.clone() else {
                tracing::warn!("[RTSP] track {} 无 server，跳过", track_id);
                continue;
            };

            let rtp_channel = tt.channel_rtp;
            let rtcp_channel = tt.channel_rtcp;
            let client_rtcp = tt.client_rtcp.unwrap_or(0);
            let local_ssrc = tt.local_ssrc.unwrap_or(0);

            let stats = Arc::new(Mutex::new(InterleavedStats::new()));
            let client_addr = Arc::new(Mutex::new(None));

            let receive_handle = self.spawn_udp_receive(
                device_id,
                track_id.clone(),
                rtp_channel,
                rtcp_channel,
                Arc::clone(&server),
                Arc::clone(&stats),
                Arc::clone(&client_addr),
            );

            let rr_handle = self.spawn_rtcp_rr(
                track_id.clone(),
                local_ssrc,
                client_rtcp,
                Arc::clone(&stats),
                Arc::clone(&client_addr),
                Arc::clone(&server),
            );

            self.udp_tracks.insert(
                track_id.clone(),
                UdpTrack {
                    server,
                    stats,
                    client_addr,
                    receive_handle,
                    rr_handle,
                },
            );

        }
    }

    fn spawn_udp_receive(
        &self,
        device_id: i64,
        _track_id: String,
        rtp_channel: u8,
        rtcp_channel: u8,
        server: Arc<UDPRtpServer>,
        stats: Arc<Mutex<InterleavedStats>>,
        client_addr: Arc<Mutex<Option<SocketAddr>>>,
    ) -> JoinHandle<()> {
        let tx = self.udp_event_tx.clone();
        let device_tag = self.device_tag.clone();

        tokio::spawn(async move {
            let rtp_socket = server.rtp_socket();
            let rtcp_socket = server.rtcp_socket();
            let mut rtp_buf = [0u8; UDP_RTP_BUF];
            let mut rtcp_buf = [0u8; UDP_RTCP_BUF];

            const TOUCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
            let mut last_touch = std::time::Instant::now()
                .checked_sub(TOUCH_INTERVAL)
                .unwrap_or_else(std::time::Instant::now);

            loop {
                tokio::select! {
                    biased;

                    r = rtp_socket.recv_from(&mut rtp_buf) => {
                        let Ok((len, addr)) = r else { continue };
                        if len <= 12 || len > UDP_RTP_BUF { continue; }
                        *client_addr.lock().await = Some(addr);

                        if last_touch.elapsed() >= TOUCH_INTERVAL {
                            server.touch_active().await;
                            last_touch = std::time::Instant::now();
                        }

                        let ssrc = RtcpPacket::extract_ssrc_from_rtp(&rtp_buf[..len]).unwrap_or(0);

                        let frame = InterleavedFrame {
                            channel: rtp_channel,
                            payload: rtp_buf[..len].to_vec(),
                            is_rtp: true,
                        };
                        stats.lock().await.process_frame(&frame);

                        let event = SignalEvent::InterleavedMediaData {
                            device_id,
                            device_tag: device_tag.clone(),
                            channel: rtp_channel,
                            ssrc,
                            is_rtp: true,
                            data: rtp_buf[..len].to_vec(),
                        };
                        if tx.try_send(event).is_err() {
                        }
                    }

                    r = rtcp_socket.recv_from(&mut rtcp_buf) => {
                        let Ok((len, _)) = r else { continue };
                        if len == 0 || len > UDP_RTCP_BUF { continue; }

                        let Some((ssrc, _rt)) = RtcpPacket::parse(&rtcp_buf[..len]) else {
                            continue;
                        };

                        let frame = InterleavedFrame {
                            channel: rtcp_channel,
                            payload: rtcp_buf[..len].to_vec(),
                            is_rtp: false,
                        };
                        stats.lock().await.process_frame(&frame);

                        let event = SignalEvent::InterleavedMediaData {
                            device_id,
                            device_tag: device_tag.clone(),
                            channel: rtcp_channel,
                            ssrc,
                            is_rtp: false,
                            data: rtcp_buf[..len].to_vec(),
                        };
                        let _ = tx.try_send(event);
                    }

                    _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                        if last_touch.elapsed() >= TOUCH_INTERVAL {
                            server.touch_active().await;
                            last_touch = std::time::Instant::now();
                        }
                    }
                }
            }
        })
    }

    fn spawn_rtcp_rr(
        &self,
        track_id: String,
        local_ssrc: u32,
        client_rtcp_port: u16,
        stats: Arc<Mutex<InterleavedStats>>,
        client_addr: Arc<Mutex<Option<SocketAddr>>>,
        server: Arc<UDPRtpServer>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let rtcp_socket = server.rtcp_socket();
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(RTCP_RR_INTERVAL_SECS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;

            loop {
                interval.tick().await;

                let Some(rtp_src) = *client_addr.lock().await else { continue };
                let rtcp_target_port = if client_rtcp_port > 0 {
                    client_rtcp_port
                } else {
                    rtp_src.port().saturating_add(1)
                };
                let rtcp_addr = SocketAddr::new(rtp_src.ip(), rtcp_target_port);

                let (frac, lost, jitter, remote_ssrc) = {
                    let g = stats.lock().await;
                    (
                        g.rtcp_stats.fraction_lost,
                        g.rtcp_stats.lost_count,
                        g.rtcp_stats.jitter,
                        g.rtcp_stats.last_remote_ssrc.unwrap_or(0),
                    )
                };

                if frac == 0 && lost == 0 && jitter == 0 {
                    continue;
                }

                let rr = RtcpPacket::build_receiver_report(
                    local_ssrc,
                    remote_ssrc,
                    frac,
                    lost,
                    jitter,
                    0,
                    0,
                );

                  match rtcp_socket.send_to(&rr, rtcp_addr).await {
                      Ok(_) => {},
                    Err(e) => tracing::warn!(
                        "[RTSP] RTCP RR 发送失败 track={} target={}: {}",
                        track_id,
                        rtcp_addr,
                        e
                    ),
                }
            }
        })
    }

    fn check_auth(
        &mut self,
        cseq: u32,
        authorization: Option<&str>,
        method: &str,
        _uri: &str,
        stream_key: &str,
    ) -> Option<String> {
        let rtsp_auth = &self.deps.config.rtsp_auth;
        if !rtsp_auth.enabled {
            return None;
        }

        let nonce = match self.auth_nonce.as_ref() {
            Some(n) => n.clone(),
            None => {
                let (n, _) = generate_nonce();
                self.auth_nonce = Some(n.clone());
                n
            }
        };
        let www = build_www_authenticate(&rtsp_auth.realm, &nonce);

        let Some(_auth_header) = authorization else {
            return Some(RtspResponse::unauthorized(cseq, &www));
        };

        tracing::warn!("[RTSP] 认证暂未实现 method={} stream={}", method, stream_key);
        Some(RtspResponse::unauthorized(cseq, &www))
    }

    async fn handle_request(
        &mut self,
        method: &str,
        url: &str,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> (Vec<SignalEvent>, Option<String>) {
        let mut events = Vec::new();
        let Some(cseq) = headers.get("cseq").and_then(|v| v.parse::<u32>().ok()) else {
            tracing::warn!("[RTSP] 缺少 CSeq 头");
            return (events, Some(RtspResponse::error(0, 400, "Bad Request")));
        };
        self.cseq = cseq;
        let session_header = headers.get("session").cloned();
        let authorization = headers.get("authorization").map(|s| s.as_str());

        match method {
            "OPTIONS" => (events, Some(RtspResponse::options(cseq))),

            "ANNOUNCE" => {
                let stream_key = Self::extract_stream_key(url);
                if let Some(resp) =
                    self.check_auth(cseq, authorization, "ANNOUNCE", url, &stream_key)
                {
                    return (events, Some(resp));
                }

                let body_str = String::from_utf8_lossy(body);
                let sdp_info = SdpParser::parse(&body_str).ok();
                let tracks = sdp_info
                    .as_ref()
                    .map(|s| s.tracks.clone())
                    .unwrap_or_default();

                let addr = self
                    .remote_addr
                    .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
                let Some(sess) = create_session(stream_key.clone(), addr) else {
                    tracing::error!("[RTSP] ANNOUNCE 创建会话失败（已达上限）");
                    return (
                        events,
                        Some(RtspResponse::error(cseq, 503, "Service Unavailable")),
                    );
                };
                {
                    let mut s = sess.write().await;
                    s.device_sdp = sdp_info;
                    s.stream_key = stream_key.clone();
                    s.tracks = tracks;
                    s.state = RtspSessionState::Announced;
                }
                self.session = Some(sess.read().await.session_id.clone());

                let device_id = Self::device_id_from_key(&stream_key);
                self.device_id = Some(device_id);

                events.push(SignalEvent::DeviceRegister {
                    device_id,
                    device_tag: None,
                    name: stream_key.clone(),
                    stream_key: Some(stream_key.clone()),
                    manufacturer: None,
                    model: None,
                    protocol: ProtocolType::Rtsp,
                });

                (events, Some(RtspResponse::announce(cseq)))
            }

            "DESCRIBE" => {
                let stream_key = Self::extract_stream_key(url);
                if let Some(resp) =
                    self.check_auth(cseq, authorization, "DESCRIBE", url, &stream_key)
                {
                    return (events, Some(resp));
                }

                let sdp_tracks = if let Some(sess) = get_session_by_stream_key(&stream_key) {
                    let s = sess.read().await;
                    s.device_sdp
                        .as_ref()
                        .map(|d| d.tracks.clone())
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(Self::default_h264_track)
                } else {
                    Self::default_h264_track()
                };

                let sdp = SdpParser::build_sdp(
                    &stream_key,
                    &self.deps.config.server.host,
                    &sdp_tracks,
                );
                (events, Some(RtspResponse::describe(cseq, &sdp)))
            }

            "SETUP" => {
                let stream_key = Self::base_stream_key(url);
                let track_id =
                    Self::extract_track_id(url).unwrap_or_else(|| "trackID=0".to_string());
                let sess_id = session_header
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches(';');

                let transport_in = headers.get("transport").map(|s| s.as_str());
                let wants_interleaved = transport_in
                    .map(|t| t.contains("interleaved="))
                    .unwrap_or(false);

                let sess = if sess_id.is_empty() {
                    let addr = self
                        .remote_addr
                        .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
                    let Some(s) = create_session(stream_key.clone(), addr) else {
                        tracing::error!("[RTSP] SETUP 创建会话失败（已达上限）");
                        return (
                            events,
                            Some(RtspResponse::error(cseq, 503, "Service Unavailable")),
                        );
                    };
                    self.session = Some(s.read().await.session_id.clone());
                    s
                } else if let Some(s) = get_session(sess_id) {
                    self.session = Some(sess_id.to_string());
                    s
                } else {
                    return (
                        events,
                        Some(RtspResponse::error(cseq, 454, "Session Not Found")),
                    );
                };

                let (client_rtp, client_rtcp) = transport_in
                    .and_then(Self::parse_client_ports)
                    .unwrap_or((0, 0));

                self.release_udp_track(&track_id).await;

                let transport_str = if wants_interleaved {
                    let (ch_rtp, ch_rtcp) = transport_in
                        .and_then(parse_interleaved_channels)
                        .unwrap_or((0, 1));

                    {
                        let mut s = sess.write().await;
                        s.update_activity();
                        s.state = RtspSessionState::Ready;
                        s.transport_mode = RtpTransportMode::Tcp;
                        s.track_transports.insert(
                            track_id.clone(),
                            TrackTransport::new_tcp(track_id.clone(), ch_rtp, ch_rtcp),
                        );
                    }

                    format!("RTP/AVP/TCP;unicast;interleaved={}-{}", ch_rtp, ch_rtcp)
                } else {
                    let (rtp_port, rtcp_port, local_ssrc, server, server_ip) =
                        match self.alloc_udp_for_track(&stream_key, &track_id).await {
                            Ok(r) => r,
                            Err(e) => {
                                tracing::error!("[RTSP] SETUP UDP 端口分配失败: {}", e);
                                return (
                                    events,
                                    Some(RtspResponse::error(cseq, 500, "RTP allocation failed")),
                                );
                            }
                        };

                    {
                        let mut s = sess.write().await;
                        s.update_activity();
                        s.state = RtspSessionState::Ready;
                        s.transport_mode = RtpTransportMode::Udp;
                        s.rtp_port = Some(rtp_port);

                        let mut tt =
                            TrackTransport::new_udp(track_id.clone(), rtp_port, rtcp_port);
                        tt.server = Some(Arc::clone(&server));
                        tt.local_ssrc = Some(local_ssrc);
                        tt.client_rtp = Some(client_rtp);
                        tt.client_rtcp = Some(client_rtcp);
                        s.track_transports.insert(track_id.clone(), tt);
                    }

                    format!(
                        "RTP/AVP;unicast;client_port={}-{};server_port={}-{};source={}",
                        client_rtp, client_rtcp, rtp_port, rtcp_port, server_ip
                    )
                };

                let sess_id_str = self.session.as_deref().unwrap_or("");
                (
                    events,
                    Some(RtspResponse::setup(cseq, sess_id_str, &transport_str, None)),
                )
            }

            "PLAY" => {
                let stream_key = Self::base_stream_key(url);
                let sess_id = session_header
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches(';');

                let Some(sess) = get_session(sess_id) else {
                    tracing::warn!("[RTSP] PLAY 会话不存在: {}", sess_id);
                    return (
                        events,
                        Some(RtspResponse::error(cseq, 454, "Session Not Found")),
                    );
                };

                let transport_mode = {
                    let mut s = sess.write().await;
                    s.update_activity();
                    s.state = RtspSessionState::Playing;
                    s.transport_mode.clone()
                };

                if self.device_id.is_none() {
                    self.device_id = Some(Self::device_id_from_key(&stream_key));
                }

                if transport_mode == RtpTransportMode::Udp {
                    self.start_udp_tracks(self.device_id.unwrap_or(0)).await;
                }

                let rtp_info = format!(
                    "url=rtsp://{}/{};seq=0;rtptime=0",
                    self.deps.config.server.host, stream_key
                );

                let session_id = self
                    .session
                    .clone()
                    .unwrap_or_else(|| format!("rtsp_{}", chrono::Utc::now().timestamp_millis()));

                events.push(SignalEvent::StartPlay {
                    device_tag: self.device_tag.clone(),
                    session_id,
                    channel_id: Some(stream_key.clone()),
                    transport: match transport_mode {
                        RtpTransportMode::Tcp => TransportType::Tcp,
                        RtpTransportMode::Udp => TransportType::Udp,
                    },
                    media_server_name: None,
                    session_type: SessionType::Play,
                });

                (
                    events,
                    Some(RtspResponse::play(
                        cseq,
                        self.session.as_deref().unwrap_or(""),
                        None,
                        Some(&rtp_info),
                    )),
                )
            }

            "PAUSE" => {
                if let Some(sess_id) = session_header.as_ref() {
                    if let Some(sess) = get_session(sess_id.trim_start_matches(';')) {
                        let mut s = sess.write().await;
                        s.update_activity();
                        s.state = RtspSessionState::Paused;
                    }
                }
                let ids: Vec<String> = self.udp_tracks.keys().cloned().collect();
                for id in ids {
                    self.stop_udp_track_tasks(&id).await;
                }
                (events, Some(RtspResponse::ok(cseq)))
            }

            "GET_PARAMETER" => {
                if let Some(sess_id) = session_header.as_ref() {
                    if let Some(sess) = get_session(sess_id.trim_start_matches(';')) {
                        sess.write().await.update_activity();
                    }
                }
                (
                    events,
                    Some(RtspResponse::get_parameter(
                        cseq,
                        self.session.as_deref().unwrap_or(""),
                    )),
                )
            }

            "SET_PARAMETER" => {
                if let Some(sess_id) = session_header.as_ref() {
                    if let Some(sess) = get_session(sess_id.trim_start_matches(';')) {
                        sess.write().await.update_activity();
                    }
                }
                (events, Some(RtspResponse::ok(cseq)))
            }

            "TEARDOWN" => {
                let sess_id = session_header
                    .as_deref()
                    .unwrap_or("")
                    .trim_start_matches(';');

                let (_stream_key, _device_id) = if let Some(sess) = get_session(sess_id) {
                    let s = sess.read().await;
                    (s.stream_key.clone(), self.device_id.unwrap_or(0))
                } else {
                    (sess_id.to_string(), self.device_id.unwrap_or(0))
                };

                self.release_all_udp_tracks().await;

                let session_id = self
                    .session
                    .clone()
                    .unwrap_or_else(|| sess_id.to_string());

                events.push(SignalEvent::StopPlay {
                    device_tag: self.device_tag.clone(),
                    session_id,
                    session_type: SessionType::Play,
                });

                remove_session(sess_id);
                self.session = None;

                (events, Some(RtspResponse::ok(cseq)))
            }

            _ => {
                tracing::warn!("[RTSP] 不支持的方法: {}", method);
                (
                    events,
                    Some(RtspResponse::error(cseq, 405, "Method Not Allowed")),
                )
            }
        }
    }

    async fn process_message(&mut self, data: &[u8]) -> (Vec<SignalEvent>, Option<String>) {
        if data.is_empty() || data[0] == 0x24 {
            return (Vec::new(), None);
        }

        let header_end = match data.windows(4).position(|w| w == b"\r\n\r\n") {
            Some(p) => p,
            None => return (Vec::new(), None),
        };

        let header_str = match std::str::from_utf8(&data[..header_end]) {
            Ok(s) => s,
            Err(_) => {
                return (
                    Vec::new(),
                    Some(RtspResponse::error(0, 400, "Bad Request")),
                )
            }
        };

        let lines: Vec<&str> = header_str.split("\r\n").collect();
        if lines.is_empty() {
            return (
                Vec::new(),
                Some(RtspResponse::error(0, 400, "Bad Request")),
            );
        }
        let Some((method, url, _version)) = Self::parse_request_line(lines[0]) else {
            return (
                Vec::new(),
                Some(RtspResponse::error(0, 400, "Bad Request")),
            );
        };
        let headers = Self::parse_headers(&lines[1..]);

        let content_length = Self::find_content_length(header_str);
        let body_start = header_end + 4;
        let body_end = (body_start + content_length).min(data.len());
        let body = &data[body_start..body_end];

        self.handle_request(&method, &url, &headers, body).await
    }

    async fn handle_tcp_interleaved(&mut self, payloads: Vec<(u8, Vec<u8>)>) -> Vec<SignalEvent> {
        let mut events = Vec::with_capacity(payloads.len());

        for (channel, payload) in payloads {
            let is_rtp = channel % 2 == 0;

            let stats = self
                .tcp_stats
                .entry(channel)
                .or_insert_with(|| Arc::new(Mutex::new(InterleavedStats::new())))
                .clone();
            let frame = InterleavedFrame {
                channel,
                payload: payload.clone(),
                is_rtp,
            };
            stats.lock().await.process_frame(&frame);

            let ssrc = if is_rtp {
                RtcpPacket::extract_ssrc_from_rtp(&payload).unwrap_or(0)
            } else {
                RtcpPacket::parse(&payload).map(|(s, _)| s).unwrap_or(0)
            };

            events.push(SignalEvent::InterleavedMediaData {
                device_id: self.device_id.unwrap_or(0),
                device_tag: self.device_tag.clone(),
                channel,
                ssrc,
                is_rtp,
                data: payload,
            });
        }

        events
    }
}

#[async_trait]
impl SignalAdapter for RtspServerAdapter {
    async fn parse(&mut self, data: &[u8]) -> Result<Vec<SignalEvent>> {
        if !data.is_empty() {
            self.recv_buffer.extend_from_slice(data);
        }
        let mut events = Vec::new();

        loop {
            if self.recv_buffer.is_empty() || self.recv_buffer[0] != 0x24 {
                break;
            }
            let (frames, consumed) = Self::parse_interleaved_frames(&self.recv_buffer);
            if consumed == 0 {
                break;
            }
            self.recv_buffer.drain(..consumed);
            events.extend(self.handle_tcp_interleaved(frames).await);
        }

        loop {
            if self.recv_buffer.is_empty() || self.recv_buffer[0] == 0x24 {
                break;
            }
            let Some((msg, remainder)) = Self::extract_rtsp_message(&self.recv_buffer) else {
                break;
            };
            self.recv_buffer = remainder;

            let (msg_events, response) = self.process_message(&msg).await;
            events.extend(msg_events);

            if let Some(resp) = response {
                if let Err(e) = self.send_raw(resp.as_bytes()).await {
                    tracing::warn!("[RTSP] 发送响应失败: {}", e);
                }
            }
        }

        while let Ok(evt) = self.udp_event_rx.try_recv() {
            events.push(evt);
        }

        Ok(events)
    }

    async fn on_connected(&mut self, addr: SocketAddr) -> Result<()> {
        self.remote_addr = Some(addr);
        Ok(())
    }

    async fn on_disconnected(&mut self) -> Result<()> {
        self.release_all_udp_tracks().await;

        if let Some(sess_id) = self.session.take() {
            remove_session(&sess_id);
        }

        self.auth_nonce = None;
        self.auth_ctx = None;
        self.tcp_stats.clear();

        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.send_raw(data).await
    }

    async fn send_interleaved(&mut self, channel: u8, payload: &[u8]) -> Result<()> {
        let frame = InterleavedBuilder::build_frame(channel, payload);
        self.send_raw(&frame).await
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::Rtsp
    }

    fn name(&self) -> &'static str {
        RTSP_USER_AGENT
    }

    fn keepalive(&self) -> bool {
        true
    }

    fn idle_timeout(&self) -> Option<u64> {
        Some(60)
    }

    fn set_tcp_write(&mut self, write: OwnedWriteHalf) {
        self.write = Some(Arc::new(RwLock::new(write)));
    }

    async fn start(&mut self, _device_tag: &str, _channel_tag: &str) -> Result<()> {
        Ok(())
    }

    async fn ptz_control(
        &mut self,
        _channel_id: &str,
        _command: &PtzCommand,
        _speed: Option<u8>,
    ) -> Result<()> {
        Ok(())
    }
}