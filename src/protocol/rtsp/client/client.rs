use super::auth::{AuthCredentials, AuthHandler};
use super::connection::TcpConnection;
use super::state::RtspState;
use super::types::{
    MediaDirection, MediaTrack, MediaType, RtpTransportMode, TrackState, TrackTransport,
};
use crate::media::rtp_server::UDPRtpServer;
use crate::protocol::rtsp::common::{
    ReceivedData, RequestBuilder, RtcpPacket, RtcpReportType, RtspError, RtpUpdateResult, RTSP_USER_AGENT,
};
use crate::protocol::rtsp::sdp::{SdpDirection, SdpInfo, SdpParser};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::sync::{mpsc, oneshot};

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
    /// 完整的 RTSP URI，如 `rtsp://192.168.1.211:554/Streaming/Channels/101`
    full_uri: String,
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
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    /// TCP 接收 consumer 的发送端。teardown 时 take 掉，关闭 channel。
    tcp_data_tx: Mutex<Option<mpsc::Sender<ReceivedData>>>,
}

impl RtspClient {
    pub fn new(
        url: String,
        credentials: AuthCredentials,
        transport_mode: RtpTransportMode,
        callback: Arc<dyn RtspClientCallback>,
    ) -> Result<Arc<Self>, RtspError> {
        let (host, port, path, full_uri) = Self::parse_url(&url)?;

        Ok(Arc::new(Self {
            host,
            port,
            path,
            full_uri,
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
            shutdown_tx: Mutex::new(None),
            tcp_data_tx: Mutex::new(None),
        }))
    }

    /// 解析 `rtsp://[user:pass@]host[:port][/path]`，支持 IPv6 字面量。
    /// 返回 (host, port, path, full_uri)
    fn parse_url(url: &str) -> Result<(String, u16, String, String), RtspError> {
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

        let full_uri = format!("rtsp://{}:{}/{}", host, port, path.trim_start_matches('/'));

        Ok((host, port, path.to_string(), full_uri))
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
                    let header = auth.build_authorization(request.method(), request.uri());
                    if let Ok(ref h) = header {
                        tracing::debug!("[RtspClient] Using existing auth method, header: {}", h);
                    }
                    header.ok()
                } else {
                    tracing::warn!("[RtspClient] No auth method available");
                    None
                }
            };

            let mut req = RequestBuilder::new(request.method(), request.uri());
            if let Some(ref auth) = auth_header {
                req = req.header("Authorization", auth);
            }
            if let Some(ref session) = *self.session_id.read().await {
                req = req.header("Session", session);
            }
            for (key, value) in request.headers() {
                if key != "Authorization" && key != "Session" {
                    req = req.header(key, value);
                }
            }

            let data = req.build(cseq).into_bytes();
            eprintln!(
                "[Client] Sending {}:\n{}",
                request.method(),
                String::from_utf8_lossy(&data)
            );

            let response = {
                let conn_guard = self.connection.read().await;
                let conn = conn_guard
                    .as_ref()
                    .ok_or_else(|| RtspError::ConnectionFailed("未连接".into()))?;
                conn.send_request(&data, cseq).await?
            };

            match self.handle_response(&response).await {
                Ok(()) => return Ok(response),
                Err(RtspError::AuthFailed(msg)) if attempt == 0 => {
                    eprintln!(
                        "[Client] Received 401, preparing retry. Connection state: is_closed={}",
                        self.connection
                            .read()
                            .await
                            .as_ref()
                            .map(|c| c.is_closed())
                            .unwrap_or(true)
                    );
                    last_err = Some(RtspError::AuthFailed("认证重试".into()));
                    continue;
                }
                Err(RtspError::AuthFailed(msg)) => {
                    tracing::error!("RTSP 认证失败: {}", msg);
                    return Err(RtspError::AuthFailed(msg));
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

        tracing::debug!("[RtspClient] handle_response: {}", status_line);

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
                tracing::debug!("[RtspClient] Received 401, checking WWW-Authenticate");
                for line in response.lines() {
                    if line.to_lowercase().starts_with("www-authenticate:") {
                        let value = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
                        tracing::debug!("[RtspClient] WWW-Authenticate: {}", value);
                        let mut auth = self.auth_handler.write().await;
                        if let Err(e) = auth.parse_www_authenticate(&value) {
                            tracing::error!("[RtspClient] Failed to parse WWW-Authenticate: {}", e);
                            return Err(e);
                        }
                        tracing::info!("[RtspClient] Parsed auth, retrying with credentials");
                        return Err(RtspError::AuthFailed("需要带认证信息重试".into()));
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

    /// 启动唯一的 TCP 读循环。
    ///
    /// - RTSP 响应由 `TcpConnection` 内部按 CSeq 路由。
    /// - interleaved 帧通过 channel 交给 consumer。
    /// - 连接断开时触发 `on_tcp_closed`。
    pub async fn start_rtsp_receive(self: &Arc<Self>) -> Result<(), RtspError> {
        tracing::info!("[RtspClient] start_rtsp_receive: called");

        let connection = {
            let guard = self.connection.read().await;
            guard
                .as_ref()
                .cloned()
                .ok_or_else(|| RtspError::ConnectionFailed("未连接".into()))?
        };

        let (started_tx, started_rx) = oneshot::channel::<()>();

        // 1. 创建 channel，consumer 处理 RTP/RTCP
        let (tx, mut rx) = mpsc::channel::<ReceivedData>(TCP_RECV_CHANNEL_CAPACITY);
        {
            let mut guard = self.tcp_data_tx.lock().unwrap();
            *guard = Some(tx.clone());
        }

        let tracks = Arc::clone(&self.tracks);
        let callback = Arc::clone(&self.callback);
        let running = Arc::clone(&self.running);
        let conn_for_consumer = connection.clone();

        tokio::spawn(async move {
            tracing::info!("[RtspClient] TCP consumer 任务启动");
            while let Some(data) = rx.recv().await {
                if !*running.read().await {
                    break;
                }
                if conn_for_consumer.is_closed() {
                    break;
                }
                Self::handle_tcp_data(&tracks, &callback, data).await;
            }
            tracing::info!("[RtspClient] TCP consumer 已停止");
        });

        // 2. on_data：把 interleaved 数据发给 consumer（非阻塞）
        let tx_for_callback = tx.clone();
        let on_data: Arc<dyn Fn(ReceivedData) + Send + Sync> = Arc::new(move |data| {
            tracing::debug!("[RtspClient] on_data: 收到 interleaved 数据");
            if let Err(e) = tx_for_callback.try_send(data) {
                tracing::warn!("TCP 接收通道已满/关闭，丢弃数据: {}", e);
            }
        });

        // 3. on_closed：连接断开时通知上层
        let client_weak = Arc::downgrade(self);
        let on_closed: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(client) = client_weak.upgrade() {
                tokio::spawn(async move {
                    client.on_tcp_closed().await;
                });
            }
        });

        // 4. 启动唯一的读循环
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        {
            let mut guard = self.shutdown_tx.lock().unwrap();
            *guard = Some(shutdown_tx);
        }

        let started_tx_for_loop = started_tx;
        let handle = tokio::spawn(async move {
            tracing::info!("[RtspClient] receive loop 任务启动");
            let send_result = started_tx_for_loop.send(());
            tracing::info!("[RtspClient] started_tx.send() 结果: {:?}", send_result);
            connection
                .run_receive_loop(on_data, on_closed, shutdown_rx)
                .await;
            tracing::info!("[RtspClient] receive loop run_receive_loop 结束");
        });
        tracing::info!(
            "[RtspClient] tokio::spawn 完成, handle.is_finished={}",
            handle.is_finished()
        );

        let rx_result = started_rx.await;
        tracing::info!("[RtspClient] started_rx.await 结果: {:?}", rx_result);
        tracing::info!("[RtspClient] start_rtsp_receive: 完成，receive loop 已启动");
        Ok(())
    }

    /// 连接断开时的清理。
    async fn on_tcp_closed(self: &Arc<Self>) {
        tracing::warn!("[RtspClient] TCP 连接已关闭");
        *self.running.write().await = false;

        {
            let mut guard = self.tcp_data_tx.lock().unwrap();
            guard.take();
        }

        self.callback
            .on_error(&RtspError::ConnectionFailed("TCP 连接已关闭".into()));
    }

    pub async fn start(self: &Arc<Self>) -> Result<(), RtspError> {
        self.start_receive_loop().await;
        Ok(())
    }

    pub async fn options(&self) -> Result<String, RtspError> {
        let request = RequestBuilder::new("OPTIONS", &self.full_uri)
            .header("User-Agent", RTSP_USER_AGENT);
        let ret = self.send_request(request).await?;
        Ok(ret)
    }

    pub async fn describe(&self) -> Result<Vec<MediaTrack>, RtspError> {
        let request = RequestBuilder::new("DESCRIBE", &self.full_uri)
            .header("User-Agent", RTSP_USER_AGENT)
            .header("Accept", "application/sdp");

        let response = self.send_request(request).await?;
        tracing::info!("[ONVIF Backchannel] describe success,{}", response);
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

    pub async fn describe_with_backchannel(&self) -> Result<Vec<MediaTrack>, RtspError> {
        let request = RequestBuilder::new("DESCRIBE", &self.full_uri)
            .header("Accept", "application/sdp")
            .header("Require", "www.onvif.org/ver20/backchannel");

        let response = match self.send_request(request).await {
            Ok(resp) => resp,
            Err(RtspError::ServerError(code)) => {
                let code_lower = code.to_lowercase();
                if code_lower.contains("551") || code_lower.contains("option") {
                    return Err(RtspError::BackchannelNotSupported);
                }
                return Err(RtspError::ServerError(code));
            }
            Err(e) => return Err(e),
        };
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

    pub async fn get_backchannel_track(&self) -> Option<MediaTrack> {
        let tracks = self.tracks.read().await;
        tracks
            .iter()
            .find(|t| t.media_type == MediaType::Audio && t.direction == MediaDirection::SendOnly)
            .cloned()
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
            let direction = match track.direction {
                SdpDirection::SendOnly => MediaDirection::SendOnly,
                SdpDirection::RecvOnly => MediaDirection::RecvOnly,
                SdpDirection::SendRecv => MediaDirection::SendRecv,
                SdpDirection::Inactive => MediaDirection::RecvOnly,
            };
            let mut media_track = MediaTrack::new(track_id, media_type, track.codec.clone());
            media_track.control_url = track.control.clone();
            media_track.fmtp = track.fmtp.clone();
            media_track.payload_type = Some(track.payload_type);
            media_track.clock_rate = Some(track.clock_rate);
            media_track.direction = direction;
            media_track.attributes = track.attributes.clone();
            tracks.push(media_track);
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
                            tracing::warn!("[RtspClient] UDP 端口分配失败: {}，回退 TCP", e);
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

        let (transport_str, udp_info, tcp_channels) = if let Some((port, _, _)) = udp_alloc.as_ref()
        {
            let s = match direction {
                MediaDirection::RecvOnly => {
                    format!("RTP/AVP/UDP;unicast;client_port={}-{}", port, port + 1)
                }
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
                MediaDirection::RecvOnly => {
                    format!("RTP/AVP/TCP;unicast;interleaved={}-{}", rtp_ch, rtcp_ch)
                }
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

        let setup_uri = {
            let tracks = self.tracks.read().await;
            if let Some(track) = tracks.get(track_idx) {
                if let Some(ref ctrl) = track.control_url {
                    if ctrl.starts_with("rtsp://") {
                        ctrl.clone()
                    } else {
                        format!(
                            "{}/{}",
                            self.full_uri.trim_end_matches('/'),
                            ctrl.trim_start_matches('/')
                        )
                    }
                } else if let Some(query_pos) = self.path.find('?') {
                    let base_path = &self.path[..query_pos];
                    format!("{}/{}?{}", base_path, track_id, &self.path[query_pos + 1..])
                } else {
                    format!("{}/{}", self.path, track_id)
                }
            } else if let Some(query_pos) = self.path.find('?') {
                let base_path = &self.path[..query_pos];
                format!("{}/{}?{}", base_path, track_id, &self.path[query_pos + 1..])
            } else {
                format!("{}/{}", self.path, track_id)
            }
        };
        let request = RequestBuilder::new("SETUP", &setup_uri)
            .header("Transport", &transport_str)
            .header("User-Agent", RTSP_USER_AGENT);

        let response = self.send_request(request).await?;
        self.parse_session(&response).await?;

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

    async fn parse_session(&self, response: &str) -> Result<(), RtspError> {
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
        let request = RequestBuilder::new("PLAY", &self.full_uri).header("Range", "npt=0.000-");
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
        Ok(())
    }

    pub async fn play_with_backchannel(self: &Arc<Self>) -> Result<(), RtspError> {
        let request = RequestBuilder::new("PLAY", &self.full_uri).header("Range", "npt=0.000-");
        let _ = self.send_request(request).await?;

        {
            let mut tracks = self.tracks.write().await;
            for track in tracks.iter_mut() {
                if track.state != TrackState::Teardown {
                    track.state = TrackState::Playing;
                }
            }
        }

        let tracks_snapshot = self.tracks.read().await.clone();
        self.update_state(RtspState::Playing {
            tracks: tracks_snapshot,
        })
        .await;

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

        self.start_rtcp_auto_report(Arc::clone(&self.running), Arc::clone(&self.tracks))
            .await;
        self.start_session_keepalive(Arc::clone(&self.running))
            .await;

        // 注意：TCP 读循环已经在 start_rtsp_receive 里启动，这里不再启动。
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
            let mut rtp_buf = [0u8; 2048];
            let mut rtcp_buf = [0u8; 2048];

            loop {
                if !*running.read().await {
                    tracing::info!("UDP 接收循环已停止 (track {})", track_id);
                    break;
                }

                tokio::select! {
                    r = rtp_socket.recv_from(&mut rtp_buf) => {
                        match r {
                            Ok((len, _)) if len > 12 => {
                                let ssrc = RtcpPacket::extract_ssrc_from_rtp(&rtp_buf[..len]).unwrap_or(0);
                                let seq = u16::from_be_bytes([rtp_buf[2], rtp_buf[3]]);
                                let ts = u32::from_be_bytes([rtp_buf[4], rtp_buf[5], rtp_buf[6], rtp_buf[7]]);
                                let arrival = std::time::Instant::now();

                                {
                                    let mut guard = tracks.write().await;
                                    if let Some(track) = guard.iter_mut().find(|t| t.track_id == track_id) {
                                        if track.remote_ssrc.is_none() && ssrc != 0 {
                                            track.remote_ssrc = Some(ssrc);
                                            tracing::info!("UDP 学习远端 SSRC {} (track {})", ssrc, track_id);
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

                                callback.on_rtp_packet(&track_id, &rtp_buf[..len], ssrc);
                            }
                            Ok(_) => {}
                            Err(e) => tracing::error!("UDP RTP 接收错误: {}", e),
                        }
                    }
                    r = rtcp_socket.recv_from(&mut rtcp_buf) => {
                        match r {
                            Ok((len, _)) if len <= 2048 => {
                                if let Some((ssrc, report_type)) = RtcpPacket::parse(&rtcp_buf[..len]) {
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
                                    callback.on_rtcp_packet(&track_id, &rtcp_buf[..len], ssrc, &report_type);
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

    async fn handle_tcp_data(
        tracks: &RwLock<Vec<MediaTrack>>,
        callback: &Arc<dyn RtspClientCallback>,
        data: ReceivedData,
    ) {
        match data {
            ReceivedData::RtspResponse(resp) => {
                // 不会发生：RTSP 响应由 TcpConnection 内部消化
                tracing::warn!(
                    "TCP consumer 收到意外的 RTSP 响应: {:?}",
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
                            tracing::info!("TCP 学习远端 SSRC {} (track {})", ssrc, track_id);
                        }
                        if track.remote_ssrc == Some(ssrc) {
                            if let Some(result) = track.rtcp_stats.update_from_rtp(seq, ts, arrival)
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
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(RTCP_REPORT_INTERVAL_SECS));
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

    async fn start_session_keepalive(self: &Arc<Self>, running: Arc<RwLock<bool>>) {
        let this = Arc::clone(self);

        tokio::spawn(async move {
            let timeout = *this.session_timeout.read().await;
            let interval_secs = if timeout > 10 {
                timeout - 10
            } else {
                timeout.max(5)
            };

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
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
            let request = RequestBuilder::new("TEARDOWN", &self.full_uri);
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

        {
            let mut guard = self.shutdown_tx.lock().unwrap();
            guard.take();
        }
        {
            let mut guard = self.tcp_data_tx.lock().unwrap();
            guard.take();
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
                return Err(RtspError::ProtocolError("当前状态不允许发送 RTP".into()));
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
                let server =
                    server.ok_or_else(|| RtspError::RtpSendFailed("UDP server 不可用".into()))?;
                let addr =
                    server_addr.ok_or_else(|| RtspError::RtpSendFailed("服务器地址未知".into()))?;
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
                let server =
                    server.ok_or_else(|| RtspError::RtpSendFailed("UDP server 不可用".into()))?;
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
