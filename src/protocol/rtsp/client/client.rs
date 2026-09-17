//! RTSP 客户端：修复版
//!
//! 关键变更：
//! - `new` 返回 `Result`，不再 panic
//! - `send_request` 内部处理 401 认证重试（一次）
//! - `start_rtcp_auto_report` / `start_session_keepalive` 使用 `self: &Arc<Self>`
//! - TCP/UDP 接收彻底避免在异步上下文调用 `blocking_*`
//! - TCP interleaved 通道按 track 索引分配，并解析 SETUP 响应里的 `interleaved=N-M`
//! - UDP `client_port` 按 RTP 惯例 `[port, port+1]`
//! - UDP 接收按 `track_id` 定位，收到第一包学习远端 SSRC
//! - `send_rtcp_rr` UDP 分支修复（原 `connect().ok()` 类型错误）
//! - `parse_session` / 头部解析统一用 `splitn(2, ':')`
//! - 删除 `teardown` 中残留的伪请求
//! - `parse_url` 支持 IPv6 与 userinfo

use super::auth::{AuthCredentials, AuthHandler};
use super::connection::TcpConnection;
use super::state::RtspState;
use super::types::{
    MediaDirection, MediaTrack, MediaType, ReceivedData, RtpTransportMode,
    TrackState, TrackTransport,
};
use crate::media::rtp_server::UDPRtpServer;
use crate::protocol::rtsp::common::{
    RtspError, RequestBuilder, RtcpPacket, RtcpReportType, RtpUpdateResult,
};
use crate::protocol::sdp::{SdpInfo, SdpParser};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

const RTCP_REPORT_INTERVAL_SECS: u64 = 50;
const SESSION_KEEPALIVE_INTERVAL_SECS: u64 = 60;
const TCP_RECV_CHANNEL_CAPACITY: usize = 512;

pub trait RtspClientCallback: Send + Sync {
    fn on_state_change(&self, state: &RtspState);
    fn on_rtp_packet(&self, track_id: &str, data: &[u8], ssrc: u32);
    fn on_rtcp_packet(&self, track_id: &str, data: &[u8], ssrc: u32, report_type: &RtcpReportType);
    fn on_error(&self, error: &RtspError);
    fn on_teardown_complete(&self);
}

pub struct RtspClient {
    host: String,
    port: u16,
    path: String,
    callback: Arc<dyn RtspClientCallback>,
    auth_handler: RwLock<AuthHandler>,
    connection: RwLock<Option<Arc<TcpConnection>>>,
    state: RwLock<RtspState>,
    cseq: RwLock<u32>,
    session_id: RwLock<Option<String>>,
    session_timeout: RwLock<u64>,
    tracks: Arc<RwLock<Vec<MediaTrack>>>,
    rtp_server: RwLock<Option<Arc<UDPRtpServer>>>,
    transport_mode: RtpTransportMode,
    running: Arc<RwLock<bool>>,
}

impl RtspClient {
    pub fn new(
        url: String,
        credentials: AuthCredentials,
        transport_mode: RtpTransportMode,
        callback: Arc<dyn RtspClientCallback>,
    ) -> Result<Arc<Self>, RtspError> {
        let (host, port, path) = Self::parse_url(&url)?;

        Ok(Arc::new(Self {
            host,
            port,
            path,
            callback,
            auth_handler: RwLock::new(AuthHandler::new(credentials)),
            connection: RwLock::new(None),
            state: RwLock::new(RtspState::Disconnected),
            cseq: RwLock::new(0),
            session_id: RwLock::new(None),
            session_timeout: RwLock::new(SESSION_KEEPALIVE_INTERVAL_SECS),
            tracks: Arc::new(RwLock::new(Vec::new())),
            rtp_server: RwLock::new(None),
            transport_mode,
            running: Arc::new(RwLock::new(false)),
        }))
    }

    /// 解析 `rtsp://[user:pass@]host[:port][/path]`，支持 IPv6 字面量。
    fn parse_url(url: &str) -> Result<(String, u16, String), RtspError> {
        let rest = url
            .strip_prefix("rtsp://")
            .ok_or_else(|| RtspError::ProtocolError("URL 必须以 rtsp:// 开头".into()))?;

        let rest = match rest.find('@') {
            Some(at) => &rest[at + 1..],
            None => rest,
        };

        let (host_port, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };

        let (host, port) = if host_port.starts_with('[') {
            let close = host_port
                .find(']')
                .ok_or_else(|| RtspError::ProtocolError("IPv6 主机格式错误".into()))?;
            let host = host_port[1..close].to_string();
            let port = if let Some(rest) = host_port[close + 1..].strip_prefix(':') {
                rest.parse::<u16>()
                    .map_err(|_| RtspError::ProtocolError("端口无效".into()))?
            } else {
                554
            };
            (host, port)
        } else if let Some((h, p)) = host_port.rsplit_once(':') {
            let port = p
                .parse::<u16>()
                .map_err(|_| RtspError::ProtocolError("端口无效".into()))?;
            (h.to_string(), port)
        } else {
            (host_port.to_string(), 554)
        };

        if host.is_empty() {
            return Err(RtspError::ProtocolError("URL 中主机为空".into()));
        }

        Ok((host, port, path.to_string()))
    }

    async fn update_state(&self, state: RtspState) {
        *self.state.write().await = state.clone();
        self.callback.on_state_change(&state);
    }

    async fn next_cseq(&self) -> u32 {
        let mut cseq = self.cseq.write().await;
        *cseq = cseq.wrapping_add(1);
        if *cseq == 0 {
            *cseq = 1;
        }
        *cseq
    }

    /// 发送 RTSP 请求；若收到 401 则解析 `WWW-Authenticate` 后重试一次。
    async fn send_request(&self, request: RequestBuilder) -> Result<String, RtspError> {
        let mut last_err: Option<RtspError> = None;

        for attempt in 0..2 {
            let cseq = self.next_cseq().await;

            let auth_header = {
                let auth = self.auth_handler.read().await;
                if auth.has_auth_method() {
                    auth.build_authorization(request.method(), request.uri()).ok()
                } else {
                    None
                }
            };

            let mut req = request.clone();
            if let Some(ref session) = *self.session_id.read().await {
                req = req.header("Session", session);
            }
            if let Some(ref auth) = auth_header {
                req = req.header("Authorization", auth);
            }

            let data = req.build(cseq).into_bytes();

            let response = {
                let conn_guard = self.connection.read().await;
                let conn = conn_guard
                    .as_ref()
                    .ok_or_else(|| RtspError::ConnectionFailed("未连接".into()))?;
                conn.send_request(&data, cseq).await?
            };

            match self.handle_response(&response).await {
                Ok(()) => return Ok(response),
                Err(RtspError::AuthFailed(_)) if attempt == 0 => {
                    tracing::debug!("RTSP 401，带认证信息重试");
                    last_err = Some(RtspError::AuthFailed("认证重试".into()));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or_else(|| RtspError::AuthFailed("认证失败".into())))
    }

    async fn handle_response(&self, response: &str) -> Result<(), RtspError> {
        let status_line = response
            .lines()
            .next()
            .ok_or_else(|| RtspError::ProtocolError("响应为空".into()))?;

        let parts: Vec<&str> = status_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(RtspError::ProtocolError("状态行无效".into()));
        }

        let status: u16 = parts[1]
            .parse()
            .map_err(|_| RtspError::ProtocolError("状态码无效".into()))?;

        match status {
            200..=299 => Ok(()),
            401 => {
                for line in response.lines() {
                    if line.to_lowercase().starts_with("www-authenticate:") {
                        let value = line
                            .splitn(2, ':')
                            .nth(1)
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let mut auth = self.auth_handler.write().await;
                        auth.parse_www_authenticate(&value)?;
                        return Err(RtspError::AuthFailed(
                            "需要带认证信息重试".into(),
                        ));
                    }
                }
                Err(RtspError::AuthFailed(
                    "401 响应缺少 WWW-Authenticate".into(),
                ))
            }
            454 => Err(RtspError::SessionNotFound),
            _ => Err(RtspError::ServerError(format!("服务器返回 {}", status))),
        }
    }

    pub async fn connect(&self) -> Result<(), RtspError> {
        {
            let state = self.state.read().await;
            if state.is_connected() {
                return Ok(());
            }
        }

        self.update_state(RtspState::Connecting).await;

        let addr_str = if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        };
        let addr: SocketAddr = addr_str
            .parse()
            .map_err(|e| RtspError::ConnectionFailed(format!("无效地址 {}: {}", addr_str, e)))?;

        let connection = TcpConnection::connect(addr).await?;
        *self.connection.write().await = Some(Arc::new(connection));

        self.update_state(RtspState::Connected).await;
        Ok(())
    }

    pub async fn options(&self) -> Result<(), RtspError> {
        let request = RequestBuilder::new("OPTIONS", &self.path);
        self.send_request(request).await?;
        Ok(())
    }

    pub async fn describe(&self) -> Result<Vec<MediaTrack>, RtspError> {
        let request =
            RequestBuilder::new("DESCRIBE", &self.path).header("Accept", "application/sdp");

        let response = self.send_request(request).await?;
        let sdp_content = self.extract_sdp_content(&response)?;
        let sdp = SdpParser::parse(&sdp_content)
            .map_err(|e| RtspError::ProtocolError(format!("SDP 解析失败: {}", e)))?;

        let tracks = self.parse_tracks_from_sdp(&sdp);
        *self.tracks.write().await = tracks.clone();

        self.update_state(RtspState::Described {
            tracks: tracks.clone(),
        })
        .await;

        Ok(tracks)
    }

    fn extract_sdp_content(&self, response: &str) -> Result<String, RtspError> {
        let blank = response
            .find("\r\n\r\n")
            .ok_or_else(|| RtspError::ProtocolError("响应缺少 SDP 正文".into()))?;
        Ok(response[blank + 4..].to_string())
    }

    fn parse_tracks_from_sdp(&self, sdp: &SdpInfo) -> Vec<MediaTrack> {
        let mut tracks = Vec::new();
        for (idx, track) in sdp.tracks.iter().enumerate() {
            let track_id = format!("trackID={}", idx);
            let media_type = match track.media.as_str() {
                "video" => MediaType::Video,
                "audio" => MediaType::Audio,
                _ => MediaType::Application,
            };
            tracks.push(MediaTrack::new(track_id, media_type, track.codec.clone()));
        }
        tracks
    }

    pub async fn setup(&self, track_id: &str, direction: MediaDirection) -> Result<(), RtspError> {
        let track_idx = track_id
            .strip_prefix("trackID=")
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| RtspError::TrackNotFound(track_id.to_string()))?;

        {
            let tracks = self.tracks.read().await;
            if track_idx >= tracks.len() {
                return Err(RtspError::TrackNotFound(track_id.to_string()));
            }
        }

        let udp_alloc = if self.transport_mode == RtpTransportMode::Udp {
            match crate::media::get_rtp_server_manager() {
                Some(manager) => {
                    match manager
                        .allocate_udp("rtsp_client", track_id, "rtsp_client")
                        .await
                    {
                        Ok((port, ssrc, server)) => {
                            tracing::info!(
                                "[RtspClient] 为 {} 分配 UDP 端口 RTP={} RTCP={}",
                                track_id,
                                port,
                                port + 1
                            );
                            Some((port, ssrc, server))
                        }
                        Err(e) => {
                            tracing::warn!(
                                "[RtspClient] UDP 端口分配失败: {}，回退 TCP",
                                e
                            );
                            None
                        }
                    }
                }
                None => {
                    tracing::warn!("[RtspClient] 无 RTP server manager，回退 TCP");
                    None
                }
            }
        } else {
            None
        };

        let (transport_str, udp_info, tcp_channels) =
            if let Some((port, _, _)) = udp_alloc.as_ref() {
                let s = match direction {
                    MediaDirection::RecvOnly => format!(
                        "RTP/AVP/UDP;unicast;client_port={}-{}",
                        port,
                        port + 1
                    ),
                    MediaDirection::SendOnly => format!(
                        "RTP/AVP/UDP;unicast;client_port={}-{};mode=record",
                        port,
                        port + 1
                    ),
                    MediaDirection::SendRecv => format!(
                        "RTP/AVP/UDP;unicast;client_port={}-{};mode=play",
                        port,
                        port + 1
                    ),
                };
                (s, Some((*port, *port + 1)), None)
            } else {
                let rtp_ch = (track_idx * 2) as u8;
                let rtcp_ch = rtp_ch + 1;
                let s = match direction {
                    MediaDirection::RecvOnly => format!(
                        "RTP/AVP/TCP;unicast;interleaved={}-{}",
                        rtp_ch, rtcp_ch
                    ),
                    MediaDirection::SendOnly => format!(
                        "RTP/AVP/TCP;unicast;interleaved={}-{};mode=record",
                        rtp_ch, rtcp_ch
                    ),
                    MediaDirection::SendRecv => format!(
                        "RTP/AVP/TCP;unicast;interleaved={}-{};mode=play",
                        rtp_ch, rtcp_ch
                    ),
                };
                (s, None, Some((rtp_ch, rtcp_ch)))
            };

        let setup_uri = format!("{}/{}", self.path, track_id);
        let request = RequestBuilder::new("SETUP", &setup_uri)
            .header("Transport", &transport_str)
            .header("User-Agent", "RustCam-RTSP-Client/1.0");

        let response = self.send_request(request).await?;
        self.parse_session(&response)?;

        let server_ports = Self::parse_transport_server_port(&response);
        let server_interleaved = Self::parse_transport_interleaved(&response);

        let udp_server_addr = if let Some((rtp_port, _)) = server_ports {
            if rtp_port > 0 {
                tokio::net::lookup_host((self.host.as_str(), rtp_port))
                    .await
                    .ok()
                    .and_then(|mut it| it.next())
            } else {
                None
            }
        } else {
            None
        };

        {
            let mut tracks = self.tracks.write().await;
            if track_idx >= tracks.len() {
                return Err(RtspError::TrackNotFound(track_id.to_string()));
            }
            let track = &mut tracks[track_idx];
            track.direction = direction;
            track.state = TrackState::Setup;

            if let Some((port, ssrc, server)) = udp_alloc {
                let (rtp_port, rtcp_port) = server_ports.unwrap_or((0, 0));
                track.transport = TrackTransport::Udp {
                    rtp_port,
                    rtcp_port,
                    local_rtp_port: port,
                    local_rtcp_port: port + 1,
                    server_addr: udp_server_addr,
                    server: Some(Arc::clone(&server)),
                };
                track.ssrc = Some(ssrc);

                *self.rtp_server.write().await = Some(server);
            } else {
                let channels = server_interleaved.or(tcp_channels).unwrap_or((0, 1));
                track.transport = TrackTransport::Tcp {
                    interleaved: channels,
                };
            }
        }

        Ok(())
    }

    fn parse_session(&self, response: &str) -> Result<(), RtspError> {
        for line in response.lines() {
            if line.to_lowercase().starts_with("session:") {
                let value = line.splitn(2, ':').nth(1).unwrap_or("").trim();
                let mut parts = value.split(';');
                let session_id = parts.next().unwrap_or("").trim().to_string();
                let mut timeout = SESSION_KEEPALIVE_INTERVAL_SECS;

                for part in parts {
                    if let Some(t) = part.trim().strip_prefix("timeout=") {
                        if let Ok(v) = t.trim().parse::<u64>() {
                            timeout = v;
                        }
                    }
                }

                if session_id.is_empty() {
                    return Err(RtspError::SessionNotFound);
                }

                *self.session_id.write().await = Some(session_id);
                *self.session_timeout.write().await = timeout;
                tracing::debug!("Session timeout: {} 秒", timeout);
                return Ok(());
            }
        }
        Err(RtspError::SessionNotFound)
    }

    fn parse_transport_server_port(response: &str) -> Option<(u16, u16)> {
        for line in response.lines() {
            if line.to_lowercase().starts_with("transport:") {
                let value = line.splitn(2, ':').nth(1)?.trim();
                for part in value.split(';') {
                    let part = part.trim();
                    if let Some(ports) = part.strip_prefix("server_port=") {
                        if let Some((p1, p2)) = ports.split_once('-') {
                            let rtp = p1.trim().parse::<u16>().ok()?;
                            let rtcp = p2.trim().parse::<u16>().ok()?;
                            return Some((rtp, rtcp));
                        }
                    }
                }
            }
        }
        None
    }

    fn parse_transport_interleaved(response: &str) -> Option<(u8, u8)> {
        for line in response.lines() {
            if line.to_lowercase().starts_with("transport:") {
                let value = line.splitn(2, ':').nth(1)?.trim();
                for part in value.split(';') {
                    let part = part.trim();
                    if let Some(v) = part.strip_prefix("interleaved=") {
                        if let Some((a, b)) = v.split_once('-') {
                            let c1 = a.trim().parse::<u8>().ok()?;
                            let c2 = b.trim().parse::<u8>().ok()?;
                            return Some((c1, c2));
                        }
                    }
                }
            }
        }
        None
    }

    pub async fn play(self: &Arc<Self>) -> Result<(), RtspError> {
        let request =
            RequestBuilder::new("PLAY", &self.path).header("Range", "npt=0.000-");
        let _ = self.send_request(request).await?;

        {
            let mut tracks = self.tracks.write().await;
            for track in tracks.iter_mut() {
                track.state = TrackState::Playing;
            }
        }

        let tracks_snapshot = self.tracks.read().await.clone();
        self.update_state(RtspState::Playing {
            tracks: tracks_snapshot,
        })
        .await;

        *self.running.write().await = true;

        self.start_receive_loop().await;
        Ok(())
    }

    async fn start_receive_loop(self: &Arc<Self>) {
        let udp_targets: Vec<(String, Arc<UDPRtpServer>)> = {
            let tracks = self.tracks.read().await;
            tracks
                .iter()
                .filter_map(|t| {
                    if let TrackTransport::Udp {
                        server: Some(s), ..
                    } = &t.transport
                    {
                        Some((t.track_id.clone(), Arc::clone(s)))
                    } else {
                        None
                    }
                })
                .collect()
        };

        for (track_id, server) in udp_targets {
            Self::spawn_udp_receive(
                track_id,
                server,
                Arc::clone(&self.tracks),
                Arc::clone(&self.callback),
                Arc::clone(&self.running),
            );
        }

        let has_tcp = {
            let tracks = self.tracks.read().await;
            tracks
                .iter()
                .any(|t| matches!(t.transport, TrackTransport::Tcp { .. }))
        };
        if has_tcp {
            self.start_tcp_receive(Arc::clone(&self.tracks)).await;
        }

        self.start_rtcp_auto_report(Arc::clone(&self.running), Arc::clone(&self.tracks))
            .await;
        self.start_session_keepalive(Arc::clone(&self.running)).await;
    }

    fn spawn_udp_receive(
        track_id: String,
        server: Arc<UDPRtpServer>,
        tracks: Arc<RwLock<Vec<MediaTrack>>>,
        callback: Arc<dyn RtspClientCallback>,
        running: Arc<RwLock<bool>>,
    ) {
        tokio::spawn(async move {
            let rtp_socket = server.rtp_socket();
            let rtcp_socket = server.rtcp_socket();
            let mut buf = [0u8; 2048];

            loop {
                if !*running.read().await {
                    tracing::info!("UDP 接收循环已停止 (track {})", track_id);
                    break;
                }

                tokio::select! {
                    r = rtp_socket.recv_from(&mut buf) => {
                        match r {
                            Ok((len, _)) if len > 12 => {
                                let ssrc = RtcpPacket::extract_ssrc_from_rtp(&buf[..len]).unwrap_or(0);
                                let seq = u16::from_be_bytes([buf[2], buf[3]]);
                                let ts = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
                                let arrival = std::time::Instant::now();

                                {
                                    let mut guard = tracks.write().await;
                                    if let Some(track) = guard.iter_mut().find(|t| t.track_id == track_id) {
                                        if track.remote_ssrc.is_none() && ssrc != 0 {
                                            track.remote_ssrc = Some(ssrc);
                                            tracing::info!(
                                                "UDP 学习远端 SSRC {} (track {})",
                                                ssrc, track_id
                                            );
                                        }
                                        if track.remote_ssrc == Some(ssrc) {
                                            if let Some(result) = track.rtcp_stats.update_from_rtp(seq, ts, arrival) {
                                                match result {
                                                    RtpUpdateResult::SequenceWraparound =>
                                                        tracing::debug!("RTP 序列号回绕 (track {})", track_id),
                                                    RtpUpdateResult::LargeJump(j) =>
                                                        tracing::debug!("RTP 序列号大跳变 {} (track {})", j, track_id),
                                                    RtpUpdateResult::SSRCConflict => {}
                                                }
                                            }
                                        }
                                    }
                                }

                                callback.on_rtp_packet(&track_id, &buf[..len], ssrc);
                            }
                            Ok(_) => {}
                            Err(e) => tracing::error!("UDP RTP 接收错误: {}", e),
                        }
                    }
                    r = rtcp_socket.recv_from(&mut buf) => {
                        match r {
                            Ok((len, _)) if len <= 2048 => {
                                if let Some((ssrc, report_type)) = RtcpPacket::parse(&buf[..len]) {
                                    if let RtcpReportType::SenderReport {
                                        ntp_timestamp,
                                        rtp_timestamp,
                                        sender_packet_count,
                                        sender_octet_count,
                                    } = &report_type
                                    {
                                        let mut guard = tracks.write().await;
                                        if let Some(track) = guard.iter_mut().find(|t| t.track_id == track_id) {
                                            track.rtcp_stats.update_from_sr(
                                                *ntp_timestamp,
                                                *rtp_timestamp,
                                                *sender_packet_count,
                                                *sender_octet_count,
                                            );
                                        }
                                    }
                                    callback.on_rtcp_packet(&track_id, &buf[..len], ssrc, &report_type);
                                }
                            }
                            Ok(_) => {}
                            Err(e) => tracing::error!("UDP RTCP 接收错误: {}", e),
                        }
                    }
                }
            }
        });
    }

    async fn start_tcp_receive(
        self: &Arc<Self>,
        tracks: Arc<RwLock<Vec<MediaTrack>>>,
    ) {
        let connection = {
            let guard = self.connection.read().await;
            match guard.as_ref() {
                Some(c) => c.clone(),
                None => {
                    tracing::error!("启动 TCP 接收失败: 未连接");
                    return;
                }
            }
        };

        let callback = Arc::clone(&self.callback);
        let running = Arc::clone(&self.running);

        let (tx, mut rx) = tokio::sync::mpsc::channel::<ReceivedData>(TCP_RECV_CHANNEL_CAPACITY);

        {
            let tracks = Arc::clone(&tracks);
            let callback = Arc::clone(&callback);
            let running = Arc::clone(&running);
            tokio::spawn(async move {
                while let Some(data) = rx.recv().await {
                    if !*running.read().await {
                        break;
                    }
                    Self::handle_tcp_data(&tracks, &callback, data).await;
                }
                tracing::info!("TCP 接收 consumer 已停止");
            });
        }

        let on_data: Arc<dyn Fn(ReceivedData) + Send + Sync> = Arc::new(move |data| {
            if let Err(e) = tx.try_send(data) {
                tracing::warn!("TCP 接收通道已满/关闭，丢弃数据: {}", e);
            }
        });

        connection.start_receive_loop(on_data).await;
    }

    async fn handle_tcp_data(
        tracks: &RwLock<Vec<MediaTrack>>,
        callback: &Arc<dyn RtspClientCallback>,
        data: ReceivedData,
    ) {
        match data {
            ReceivedData::RtspResponse(resp) => {
                tracing::warn!(
                    "TCP 接收循环收到意外的 RTSP 响应: {:?}",
                    resp.lines().next()
                );
            }
            ReceivedData::RtpData {
                track_id,
                data,
                ssrc,
            } => {
                if data.len() > 12 {
                    let seq = u16::from_be_bytes([data[2], data[3]]);
                    let ts = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                    let arrival = std::time::Instant::now();

                    let mut guard = tracks.write().await;
                    if let Some(track) = guard.iter_mut().find(|t| t.track_id == track_id) {
                        if track.remote_ssrc.is_none() && ssrc != 0 {
                            track.remote_ssrc = Some(ssrc);
                            tracing::info!(
                                "TCP 学习远端 SSRC {} (track {})",
                                ssrc,
                                track_id
                            );
                        }
                        if track.remote_ssrc == Some(ssrc) {
                            if let Some(result) =
                                track.rtcp_stats.update_from_rtp(seq, ts, arrival)
                            {
                                match result {
                                    RtpUpdateResult::SequenceWraparound => {
                                        tracing::debug!("RTP 序列号回绕 (track {})", track_id)
                                    }
                                    RtpUpdateResult::LargeJump(j) => tracing::debug!(
                                        "RTP 序列号大跳变 {} (track {})",
                                        j,
                                        track_id
                                    ),
                                    RtpUpdateResult::SSRCConflict => {}
                                }
                            }
                        }
                    }
                }
                callback.on_rtp_packet(&track_id, &data, ssrc);
            }
            ReceivedData::RtcpData {
                track_id,
                data,
                ssrc,
                report_type,
            } => {
                if let RtcpReportType::SenderReport {
                    ntp_timestamp,
                    rtp_timestamp,
                    sender_packet_count,
                    sender_octet_count,
                } = &report_type
                {
                    let mut guard = tracks.write().await;
                    if let Some(track) = guard
                        .iter_mut()
                        .find(|t| t.remote_ssrc == Some(ssrc) || t.track_id == track_id)
                    {
                        track.rtcp_stats.update_from_sr(
                            *ntp_timestamp,
                            *rtp_timestamp,
                            *sender_packet_count,
                            *sender_octet_count,
                        );
                    }
                }
                callback.on_rtcp_packet(&track_id, &data, ssrc, &report_type);
            }
        }
    }

    async fn start_rtcp_auto_report(
        self: &Arc<Self>,
        running: Arc<RwLock<bool>>,
        tracks: Arc<RwLock<Vec<MediaTrack>>>,
    ) {
        let this = Arc::clone(self);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                RTCP_REPORT_INTERVAL_SECS,
            ));
            interval.tick().await;

            loop {
                interval.tick().await;

                if !*running.read().await {
                    tracing::info!("RTCP 自动报告已停止");
                    break;
                }

                let snapshot: Vec<(String, u32, u8, u32, u32, u32, u32)> = {
                    let guard = tracks.read().await;
                    guard
                        .iter()
                        .filter(|t| t.state == TrackState::Playing)
                        .filter_map(|t| {
                            let ssrc = t.remote_ssrc.or(t.ssrc)?;
                            let s = &t.rtcp_stats;
                            Some((
                                t.track_id.clone(),
                                ssrc,
                                s.fraction_lost,
                                s.lost_count,
                                s.jitter,
                                s.last_sr_ntp as u32,
                                s.calc_delay_since_last_sr(),
                            ))
                        })
                        .collect()
                };

                for (track_id, ssrc, frac, lost, jitter, last_sr, delay) in snapshot {
                    if let Err(e) = this
                        .send_rtcp_rr(&track_id, ssrc, frac, lost, jitter, last_sr, delay)
                        .await
                    {
                        tracing::warn!("RTCP RR 发送失败 ({}): {}", track_id, e);
                    }
                }
            }
        });
    }

    async fn start_session_keepalive(
        self: &Arc<Self>,
        running: Arc<RwLock<bool>>,
    ) {
        let this = Arc::clone(self);

        tokio::spawn(async move {
            let timeout = *this.session_timeout.read().await;
            let interval_secs = if timeout > 10 { timeout - 10 } else { timeout.max(5) };

            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            interval.tick().await;

            loop {
                interval.tick().await;

                if !*running.read().await {
                    tracing::info!("Session keepalive 已停止");
                    break;
                }

                let request = RequestBuilder::new("OPTIONS", &this.path);
                if let Err(e) = this.send_request(request).await {
                    tracing::warn!("Keepalive OPTIONS 失败: {}", e);
                }
            }
        });
    }

    pub async fn teardown(&self) -> Result<(), RtspError> {
        *self.running.write().await = false;

        if self.session_id.read().await.is_some() {
            let request = RequestBuilder::new("TEARDOWN", &self.path);
            if let Err(e) = self.send_request(request).await {
                tracing::warn!("TEARDOWN 失败: {}", e);
            }
        }

        {
            let mut tracks = self.tracks.write().await;
            for track in tracks.iter_mut() {
                track.state = TrackState::Teardown;
            }
        }

        *self.connection.write().await = None;
        *self.rtp_server.write().await = None;

        self.update_state(RtspState::Teardown).await;
        self.callback.on_teardown_complete();
        Ok(())
    }

    pub async fn get_tracks(&self) -> Vec<MediaTrack> {
        self.tracks.read().await.clone()
    }

    pub async fn send_rtp(&self, track_id: &str, data: &[u8]) -> Result<(), RtspError> {
        {
            let state = self.state.read().await;
            if !state.can_send_rtp() {
                return Err(RtspError::ProtocolError(
                    "当前状态不允许发送 RTP".into(),
                ));
            }
        }

        let track_idx = track_id
            .strip_prefix("trackID=")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let transport = {
            let tracks = self.tracks.read().await;
            tracks.get(track_idx).map(|t| t.transport.clone())
        };

        match transport {
            Some(TrackTransport::Tcp { interleaved }) => {
                let conn_guard = self.connection.read().await;
                let conn = conn_guard
                    .as_ref()
                    .ok_or_else(|| RtspError::ConnectionFailed("未连接".into()))?;
                conn.send_interleaved(interleaved.0, data).await
            }
            Some(TrackTransport::Udp {
                server,
                server_addr,
                ..
            }) => {
                let server = server
                    .ok_or_else(|| RtspError::RtpSendFailed("UDP server 不可用".into()))?;
                let addr = server_addr
                    .ok_or_else(|| RtspError::RtpSendFailed("服务器地址未知".into()))?;
                let socket = server.rtp_socket();
                socket
                    .send_to(data, addr)
                    .await
                    .map_err(|e| RtspError::RtpSendFailed(format!("UDP 发送失败: {}", e)))?;
                Ok(())
            }
            None => Err(RtspError::TrackNotFound(track_id.to_string())),
        }
    }

    pub async fn send_rtcp_rr(
        &self,
        track_id: &str,
        ssrc: u32,
        fraction_lost: u8,
        lost_count: u32,
        jitter: u32,
        last_sr: u32,
        delay_since_last_sr: u32,
    ) -> Result<(), RtspError> {
        let track_idx = track_id
            .strip_prefix("trackID=")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let transport = {
            let tracks = self.tracks.read().await;
            tracks.get(track_idx).map(|t| t.transport.clone())
        };

        let rr_packet = RtcpPacket::build_receiver_report(
            0,
            ssrc,
            fraction_lost,
            lost_count,
            jitter,
            last_sr,
            delay_since_last_sr,
        );

        match transport {
            Some(TrackTransport::Tcp { interleaved }) => {
                let channel = interleaved.1;
                let conn_guard = self.connection.read().await;
                let conn = conn_guard
                    .as_ref()
                    .ok_or_else(|| RtspError::ConnectionFailed("未连接".into()))?;
                conn.send_interleaved(channel, &rr_packet).await
            }
            Some(TrackTransport::Udp {
                server,
                server_addr,
                rtcp_port,
                ..
            }) => {
                let server = server
                    .ok_or_else(|| RtspError::RtpSendFailed("UDP server 不可用".into()))?;
                let remote = server_addr
                    .ok_or_else(|| RtspError::ConnectionFailed("服务器地址未知".into()))?;
                let rtcp_addr = SocketAddr::new(remote.ip(), rtcp_port);
                let socket = server.rtcp_socket();
                socket
                    .send_to(&rr_packet, rtcp_addr)
                    .await
                    .map_err(|e| RtspError::RtpSendFailed(format!("RTCP 发送失败: {}", e)))?;
                Ok(())
            }
            None => Err(RtspError::TrackNotFound(track_id.to_string())),
        }
    }
}
