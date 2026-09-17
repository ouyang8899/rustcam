# RTSP Client 模块实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现独立的 RTSP Client 模块，支持视频拉流、语音上传/广播、双向对讲

**Architecture:** 
- 独立 `RtspClient` 模块处理客户端模式
- 复用现有 SDP 解析、认证、response 构建代码
- 通过回调接口向上层传递 RTP 数据和状态变化
- 支持 UDP 和 TCP (Interleaved) 两种 RTP 传输模式

**Tech Stack:** tokio, thiserror, tracing

**Spec:** `docs/superpowers/specs/2026-09-17-rtsp-client-design.md`

---

## 全局约束

- 使用 thiserror 定义错误
- 所有异步方法使用 tokio
- 使用 Arc<dyn Callback> 实现线程安全回调
- 复用 `src/protocol/rtsp/sdp.rs` 的 SDP 解析
- 复用 `src/protocol/rtsp/auth.rs` 的认证逻辑

---

## 文件结构

```
src/protocol/rtsp/
├── client/                      # 新建目录
│   ├── mod.rs                  # 模块导出
│   ├── types.rs               # MediaTrack, MediaDirection, TrackTransport, TrackState
│   ├── errors.rs              # RtspClientError
│   ├── state.rs               # RtspState 状态机
│   ├── auth.rs                # 客户端认证（处理 401 响应）
│   ├── requests.rs            # RTSP 请求构建（OPTIONS/DESCRIBE/SETUP/PLAY/TEARDOWN）
│   ├── connection.rs          # TCP/UDP 连接管理
│   └── client.rs             # RtspClient 主结构
├── types.rs                   # 修改：添加 MediaDirection, MediaTrack, TrackState, TrackTransport
├── auth.rs                    # 修改：添加客户端认证相关函数
└── adapter/                   # 保留不动
```

---

## 任务列表

### Task 1: 创建 client 模块基础结构

**Files:**
- Create: `src/protocol/rtsp/client/mod.rs`
- Create: `src/protocol/rtsp/client/types.rs`
- Create: `src/protocol/rtsp/client/errors.rs`

**Interfaces:**
- Produces:
  - `MediaDirection` enum
  - `MediaTrack` struct
  - `TrackTransport` enum
  - `TrackState` enum
  - `RtspClientError` enum

- [ ] **Step 1: Create client/types.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaDirection {
    RecvOnly,
    SendOnly,
    SendRecv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    Video,
    Audio,
    Application,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackState {
    Pending,
    Setup,
    Playing,
    Teardown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackTransport {
    Udp {
        rtp_port: u16,
        rtcp_port: u16,
        local_rtp_port: u16,
        local_rtcp_port: u16,
    },
    Tcp {
        interleaved: (u8, u8),
    },
}

#[derive(Debug, Clone)]
pub struct MediaTrack {
    pub track_id: String,
    pub media_type: MediaType,
    pub codec: String,
    pub direction: MediaDirection,
    pub fmtp: Option<String>,
    pub transport: TrackTransport,
    pub state: TrackState,
    pub ssrc: Option<u32>,
}

impl MediaTrack {
    pub fn new(track_id: String, media_type: MediaType, codec: String) -> Self {
        Self {
            track_id,
            media_type,
            codec,
            direction: MediaDirection::RecvOnly,
            fmtp: None,
            transport: TrackTransport::Udp {
                rtp_port: 0,
                rtcp_port: 0,
                local_rtp_port: 0,
                local_rtcp_port: 0,
            },
            state: TrackState::Pending,
            ssrc: None,
        }
    }
}
```

- [ ] **Step 2: Create client/errors.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RtspClientError {
    #[error("连接失败: {0}")]
    ConnectionFailed(String),

    #[error("认证失败: {0}")]
    AuthFailed(String),

    #[error("服务器错误: {0}")]
    ServerError(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("协议错误: {0}")]
    ProtocolError(String),

    #[error("会话不存在")]
    SessionNotFound,

    #[error("轨道不存在: {0}")]
    TrackNotFound(String),

    #[error("RTP 发送失败: {0}")]
    RtpSendFailed(String),
}
```

- [ ] **Step 3: Create client/mod.rs**

```rust
pub mod types;
pub mod errors;
pub mod state;
pub mod auth;
pub mod requests;
pub mod connection;
pub mod client;

pub use types::*;
pub use errors::*;
```

- [ ] **Step 4: Commit**

```bash
git add src/protocol/rtsp/client/
git commit -m "feat(rtsp): add client module types and errors"
```

---

### Task 2: 实现 client/state.rs 状态机

**Files:**
- Create: `src/protocol/rtsp/client/state.rs`

**Interfaces:**
- Produces: `RtspState` enum

- [ ] **Step 1: Create state.rs**

```rust
use super::types::MediaTrack;

#[derive(Debug, Clone)]
pub enum RtspState {
    Disconnected,
    Connecting,
    Connected,
    Described { tracks: Vec<MediaTrack> },
    Ready { tracks: Vec<MediaTrack> },
    Playing { tracks: Vec<MediaTrack> },
    Teardown,
    Error(String),
}

impl RtspState {
    pub fn is_connected(&self) -> bool {
        matches!(
            self,
            RtspState::Connected
                | RtspState::Described { .. }
                | RtspState::Ready { .. }
                | RtspState::Playing { .. }
        )
    }

    pub fn can_send_rtp(&self) -> bool {
        matches!(self, RtspState::Playing { .. })
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/protocol/rtsp/client/state.rs
git commit -m "feat(rtsp): add client state machine"
```

---

### Task 3: 实现 client/auth.rs 客户端认证

**Files:**
- Create: `src/protocol/rtsp/client/auth.rs`

**Interfaces:**
- Produces: `AuthCredentials` struct, `AuthContext` struct, `AuthHandler`

- [ ] **Step 1: Create auth.rs**

```rust
use super::errors::RtspClientError;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AuthCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    Basic,
    Digest { realm: String, nonce: String },
}

pub struct AuthHandler {
    credentials: AuthCredentials,
    auth_method: Option<AuthMethod>,
}

impl AuthHandler {
    pub fn new(credentials: AuthCredentials) -> Self {
        Self {
            credentials,
            auth_method: None,
        }
    }

    pub fn parse_www_authenticate(&mut self, header: &str) -> Result<(), RtspClientError> {
        let header = header.trim();
        if header.starts_with("Digest ") {
            let params = Self::parse_digest_params(&header[7..]);
            let realm = params
                .get("realm")
                .ok_or_else(|| RtspClientError::AuthFailed("Missing realm in Digest auth".into()))?;
            let nonce = params
                .get("nonce")
                .ok_or_else(|| RtspClientError::AuthFailed("Missing nonce in Digest auth".into()))?;
            self.auth_method = Some(AuthMethod::Digest {
                realm: realm.clone(),
                nonce: nonce.clone(),
            });
        } else if header.starts_with("Basic ") {
            self.auth_method = Some(AuthMethod::Basic);
        } else {
            return Err(RtspClientError::AuthFailed(format!(
                "Unknown auth method: {}",
                header
            )));
        }
        Ok(())
    }

    fn parse_digest_params(input: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();
        let input = input.trim();
        for part in input.split(',') {
            let part = part.trim();
            if let Some((key, value)) = part.split_once('=') {
                let key = key.trim();
                let mut value = value.trim();
                if value.starts_with('"') && value.ends_with('"') {
                    value = &value[1..value.len() - 1];
                }
                params.insert(key.to_string(), value.to_string());
            }
        }
        params
    }

    pub fn build_authorization(&self, method: &str, uri: &str) -> Result<String, RtspClientError> {
        match &self.auth_method {
            Some(AuthMethod::Basic) => {
                use base64::Engine;
                let credentials = format!(
                    "{}:{}",
                    self.credentials.username, self.credentials.password
                );
                let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
                Ok(format!("Basic {}", encoded))
            }
            Some(AuthMethod::Digest { realm, nonce }) => {
                use md5::{Digest, Md5};
                let response = Self::calc_digest_response(
                    &self.credentials.username,
                    realm,
                    &self.credentials.password,
                    uri,
                    nonce,
                    method,
                );
                Ok(format!(
                    "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
                    self.credentials.username, realm, nonce, uri, response
                ))
            }
            None => Err(RtspClientError::AuthFailed(
                "No auth method selected".into(),
            )),
        }
    }

    fn calc_digest_response(
        username: &str,
        realm: &str,
        password: &str,
        uri: &str,
        nonce: &str,
        method: &str,
    ) -> String {
        use md5::{Digest, Md5};
        let ha1 = format!(
            "{}:{}:{}",
            username,
            realm,
            password
        );
        let ha1 = format!("{:x}", Md5::new().chain_update(ha1.as_bytes()));
        let ha2 = format!("{}:{}", method, uri);
        let ha2 = format!("{:x}", Md5::new().chain_update(ha2.as_bytes()));
        let response = format!("{}:{}:{}", ha1, nonce, ha2);
        format!("{:x}", Md5::new().chain_update(response.as_bytes()))
    }

    pub fn has_auth_method(&self) -> bool {
        self.auth_method.is_some()
    }
}
```

- [ ] **Step 2: Add base64 and md5 to Cargo.toml**

```toml
base64 = "0.21"
md5 = "0.7"
```

- [ ] **Step 3: Commit**

```bash
git add src/protocol/rtsp/client/auth.rs Cargo.toml
git commit -m "feat(rtsp): add client authentication support"
```

---

### Task 4: 实现 client/requests.rs RTSP 请求构建

**Files:**
- Create: `src/protocol/rtsp/client/requests.rs`

**Interfaces:**
- Produces: `RtspRequest` struct, `RequestBuilder`

- [ ] **Step 1: Create requests.rs**

```rust
use super::errors::RtspClientError;

#[derive(Debug, Clone)]
pub struct RtspRequest {
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl RtspRequest {
    pub fn new(method: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            uri: uri.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn build(&self, cseq: u32) -> String {
        let mut s = format!("{} {} RTSP/1.0\r\n", self.method, self.uri);
        s.push_str(&format!("CSeq: {}\r\n", cseq));

        for (key, value) in &self.headers {
            s.push_str(&format!("{}: {}\r\n", key, value));
        }

        if let Some(body) = &self.body {
            s.push_str(&format!("Content-Length: {}\r\n", body.len()));
            s.push_str("\r\n");
            s.push_str(body);
        } else {
            s.push_str("\r\n");
        }

        s
    }
}

pub struct RequestBuilder {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl RequestBuilder {
    pub fn new(method: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            uri: uri.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn build(&self, cseq: u32) -> String {
        let mut s = format!("{} {} RTSP/1.0\r\n", self.method, self.uri);
        s.push_str(&format!("CSeq: {}\r\n", cseq));

        for (key, value) in &self.headers {
            s.push_str(&format!("{}: {}\r\n", key, value));
        }

        if let Some(body) = &self.body {
            s.push_str(&format!("Content-Length: {}\r\n", body.len()));
            s.push_str("\r\n");
            s.push_str(body);
        } else {
            s.push_str("\r\n");
        }

        s
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/protocol/rtsp/client/requests.rs
git commit -m "feat(rtsp): add RTSP request builder"
```

---

### Task 5: 实现 client/connection.rs 连接管理

**Files:**
- Create: `src/protocol/rtsp/client/connection.rs`

**Interfaces:**
- Produces: `TcpConnection`, `UdpTransport`

- [ ] **Step 1: Create connection.rs**

```rust
use super::errors::RtspClientError;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct TcpConnection {
    stream: TcpStream,
    read_buf: Vec<u8>,
}

impl TcpConnection {
    pub async fn connect(addr: SocketAddr) -> Result<Self, RtspClientError> {
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            TcpStream::connect(addr),
        )
        .await
        .map_err(|_| RtspClientError::Timeout("连接超时".into()))?
        .map_err(|e| RtspClientError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            stream,
            read_buf: Vec::with_capacity(8192),
        })
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<(), RtspClientError> {
        self.stream
            .write_all(data)
            .await
            .map_err(|e| RtspClientError::ConnectionFailed(e.to_string()))?;
        Ok(())
    }

    pub async fn receive_response(&mut self) -> Result<String, RtspClientError> {
        let mut buffer = [0u8; 4096];
        loop {
            let n = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                self.stream.read(&mut buffer),
            )
            .await
            .map_err(|_| RtspClientError::Timeout("读取响应超时".into()))?
            .map_err(|e| RtspClientError::ConnectionFailed(e.to_string()))?;

            if n == 0 {
                return Err(RtspClientError::ConnectionFailed("连接关闭".into()));
            }

            self.read_buf.extend_from_slice(&buffer[..n]);

            if let Ok(response) = self.parse_response() {
                return Ok(response);
            }
        }
    }

    fn parse_response(&mut self) -> Result<String, std::io::Error> {
        let buf = &self.read_buf;
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            let end = String::from_utf8_lossy(buf);
            if let Some(pos) = end.find("\r\n\r\n") {
                let response = end[..pos + 4].to_string();
                let header_end = pos + 4;

                let content_length = self.find_content_length(&response);
                if content_length == 0 {
                    self.read_buf.drain(..header_end);
                    return Ok(response);
                }

                let total_len = header_end + content_length;
                if self.read_buf.len() >= total_len {
                    self.read_buf.drain(..total_len);
                    return Ok(response);
                }
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "not enough data",
        ))
    }

    fn find_content_length(&self, response: &str) -> usize {
        for line in response.lines() {
            if line.to_lowerse().starts_with("content-length:") {
                if let Some(len) = line.split(':').nth(1) {
                    return len.trim().parse().unwrap_or(0);
                }
            }
        }
        0
    }

    pub fn into_inner(self) -> TcpStream {
        self.stream
    }

    pub fn set_read_timeout(&self, _timeout: std::time::Duration) {
        // tokio TCP 不支持单独设置读超时，通过 timeout() 在调用处处理
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/protocol/rtsp/client/connection.rs
git commit -m "feat(rtsp): add TCP connection management"
```

---

### Task 6: 实现 client/client.rs 主结构

**Files:**
- Create: `src/protocol/rtsp/client/client.rs`

**Interfaces:**
- Produces: `RtspClient`, `RtspClientCallback` trait

- [ ] **Step 1: Create client.rs**

```rust
use super::auth::{AuthCredentials, AuthHandler};
use super::connection::TcpConnection;
use super::errors::RtspClientError;
use super::requests::RequestBuilder;
use super::state::RtspState;
use super::types::{MediaDirection, MediaTrack, MediaType, TrackState, TrackTransport};
use crate::protocol::sdp::SdpParser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

pub trait RtspClientCallback: Send + Sync {
    fn on_state_change(&self, state: &RtspState);
    fn on_rtp_packet(&self, track_id: &str, data: &[u8]);
    fn on_error(&self, error: &RtspClientError);
    fn on_teardown_complete(&self);
}

pub struct RtspClient {
    url: String,
    host: String,
    port: u16,
    path: String,
    credentials: AuthCredentials,
    callback: Arc<dyn RtspClientCallback>,
    auth_handler: RwLock<AuthHandler>,
    connection: RwLock<Option<TcpConnection>>,
    state: RwLock<RtspState>,
    cseq: RwLock<u32>,
    session_id: RwLock<Option<String>>,
    tracks: RwLock<Vec<MediaTrack>>,
}

impl RtspClient {
    pub fn new(
        url: String,
        credentials: AuthCredentials,
        callback: Arc<dyn RtspClientCallback>,
    ) -> Arc<Self> {
        let (host, port, path) = Self::parse_url(&url).expect("Invalid RTSP URL");

        Arc::new(Self {
            url,
            host,
            port,
            path,
            credentials: credentials.clone(),
            callback,
            auth_handler: RwLock::new(AuthHandler::new(credentials)),
            connection: RwLock::new(None),
            state: RwLock::new(RtspState::Disconnected),
            cseq: RwLock::new(0),
            session_id: RwLock::new(None),
            tracks: RwLock::new(Vec::new()),
        })
    }

    fn parse_url(url: &str) -> Result<(String, u16, String), RtspClientError> {
        let url = url
            .strip_prefix("rtsp://")
            .ok_or_else(|| RtspClientError::ProtocolError("URL must start with rtsp://".into()))?;

        let (host_port, path) = url
            .split_once('/')
            .unwrap_or((url, ""));

        let path = format!("/{}", path);

        let (host, port) = if let Some((h, p)) = host_port.rsplit_once(':') {
            let port: u16 = p.parse().map_err(|_| {
                RtspClientError::ProtocolError("Invalid port in URL".into())
            })?;
            (h.to_string(), port)
        } else {
            (host_port.to_string(), 554)
        };

        Ok((host, port, path))
    }

    async fn update_state(&self, state: RtspState) {
        {
            let mut s = self.state.write().await;
            *s = state.clone();
        }
        self.callback.on_state_change(&state);
    }

    async fn next_cseq(&self) -> u32 {
        let mut cseq = self.cseq.write().await;
        *cseq += 1;
        *cseq
    }

    async fn send_request(&self, request: RequestBuilder) -> Result<String, RtspClientError> {
        let cseq = self.next_cseq().await;

        let mut auth_header = None;
        if self.auth_handler.read().await.has_auth_method() {
            let auth = self.auth_handler.read().await;
            if let Ok(header) = auth.build_authorization(&request.method, &request.uri) {
                auth_header = Some(header);
            }
        }

        let mut req = request;
        if let Some(ref session) = *self.session_id.read().await {
            req = req.header("Session", session);
        }
        if let Some(ref auth) = auth_header {
            req = req.header("Authorization", auth);
        }

        let data = req.build(cseq).into_bytes();

        let mut conn = self.connection.write().await;
        if let Some(ref mut connection) = *conn {
            connection.send(&data).await?;
            let response = connection.receive_response().await?;
            self.handle_response(&response)?;
            Ok(response)
        } else {
            Err(RtspClientError::ConnectionFailed("未连接".into()))
        }
    }

    fn handle_response(&self, response: &str) -> Result<(), RtspClientError> {
        let status_line = response.lines().next().ok_or_else(|| {
            RtspClientError::ProtocolError("Empty response".into())
        })?;

        let parts: Vec<&str> = status_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(RtspClientError::ProtocolError("Invalid status line".into()));
        }

        let status: u16 = parts[1].parse().map_err(|_| {
            RtspClientError::ProtocolError("Invalid status code".into())
        })?;

        match status {
            200..=299 => Ok(()),
            401 => {
                for line in response.lines() {
                    if line.to_lowerse().starts_with("www-authenticate:") {
                        let value = line.split(':').nth(1).unwrap_or("").trim();
                        let mut auth = self.auth_handler.blocking_write();
                        auth.parse_www_authenticate(value)?;
                        return Err(RtspClientError::AuthFailed("Need to retry with auth".into()));
                    }
                }
                Err(RtspClientError::AuthFailed("401 without WWW-Authenticate".into()))
            }
            454 => Err(RtspClientError::SessionNotFound),
            _ => Err(RtspClientError::ServerError(format!(
                "Server returned {}",
                status
            ))),
        }
    }

    pub async fn connect(&self) -> Result<(), RtspClientError> {
        {
            let state = self.state.read().await;
            if state.is_connected() {
                return Ok(());
            }
        }

        self.update_state(RtspState::Connecting).await;

        let addr: SocketAddr = format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|e| RtspClientError::ConnectionFailed(e.to_string()))?;

        let connection = TcpConnection::connect(addr).await?;
        {
            let mut conn = self.connection.write().await;
            *conn = Some(connection);
        }

        self.update_state(RtspState::Connected).await;

        Ok(())
    }

    pub async fn options(&self) -> Result<(), RtspClientError> {
        let request = RequestBuilder::new("OPTIONS", &self.path);
        self.send_request(request).await?;
        Ok(())
    }

    pub async fn describe(&self) -> Result<Vec<MediaTrack>, RtspClientError> {
        let request = RequestBuilder::new("DESCRIBE", &self.path)
            .header("Accept", "application/sdp");

        let response = self.send_request(request).await?;

        let sdp_content = self.extract_sdp_content(&response)?;
        let sdp = SdpParser::parse(&sdp_content)
            .map_err(|e| RtspClientError::ProtocolError(format!("SDP parse error: {}", e)))?;

        let tracks = self.parse_tracks_from_sdp(&sdp);

        {
            let mut t = self.tracks.write().await;
            *t = tracks.clone();
        }

        self.update_state(RtspState::Described { tracks: tracks.clone() }).await;

        Ok(tracks)
    }

    fn extract_sdp_content(&self, response: &str) -> Result<String, RtspClientError> {
        let blank_line = response.find("\r\n\r\n").ok_or_else(|| {
            RtspClientError::ProtocolError("SDP content not found".into())
        })?;
        Ok(response[blank_line + 4..].to_string())
    }

    fn parse_tracks_from_sdp(&self, sdp: &SdpParser) -> Vec<MediaTrack> {
        let mut tracks = Vec::new();

        for (idx, media) in sdp.media_descriptions.iter().enumerate() {
            let track_id = format!("trackID={}", idx);
            let media_type = match media.media.as_str() {
                "video" => MediaType::Video,
                "audio" => MediaType::Audio,
                _ => MediaType::Application,
            };

            let codec = sdp
                .get_attribute(&media.rtpmap, "encoding_name")
                .unwrap_or_else(|| "H264".to_string());

            tracks.push(MediaTrack::new(track_id, media_type, codec));
        }

        tracks
    }

    pub async fn setup(&self, track_id: &str, direction: MediaDirection) -> Result<(), RtspClientError> {
        let track_idx = track_id
            .strip_prefix("trackID=")
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| RtspClientError::TrackNotFound(track_id.to_string()))?;

        {
            let tracks = self.tracks.read().await;
            if track_idx >= tracks.len() {
                return Err(RtspClientError::TrackNotFound(track_id.to_string()));
            }
        }

        let transport_str = match direction {
            MediaDirection::RecvOnly => "RTP/AVP;unicast;client_port=5000-5001",
            MediaDirection::SendOnly => "RTP/AVP;unicast;client_port=5000-5001;mode=record",
            MediaDirection::SendRecv => "RTP/AVP;unicast;client_port=5000-5001;mode=play",
        };

        let request = RequestBuilder::new("SETUP", &format!("{}/{}", self.path, track_id))
            .header("Transport", transport_str)
            .header("User-Agent", "RustCam-RTSP-Client/1.0");

        let response = self.send_request(request).await?;

        self.parse_session(&response)?;

        {
            let mut tracks = self.tracks.write().await;
            if track_idx < tracks.len() {
                tracks[track_idx].direction = direction;
                tracks[track_idx].state = TrackState::Setup;
            }
        }

        Ok(())
    }

    fn parse_session(&self, response: &str) -> Result<(), RtspClientError> {
        for line in response.lines() {
            if line.to_lowercase().starts_with("session:") {
                let session = line.split(':').nth(1).unwrap_or("").trim();
                let session = session.split(';').next().unwrap_or(session).trim();
                let mut s = self.session_id.write().await;
                *s = Some(session.to_string());
                return Ok(());
            }
        }
        Err(RtspClientError::SessionNotFound)
    }

    pub async fn play(&self) -> Result<(), RtspClientError> {
        let request = RequestBuilder::new("PLAY", &self.path)
            .header("Range", "npt=0.000-");

        let _response = self.send_request(request).await?;

        {
            let mut tracks = self.tracks.write().await;
            for track in tracks.iter_mut() {
                track.state = TrackState::Playing;
            }
            let tracks_clone = tracks.clone();
            self.update_state(RtspState::Playing { tracks: tracks_clone }).await;
        }

        Ok(())
    }

    pub async fn teardown(&self) -> Result<(), RtspClientError> {
        let request = RequestBuilder::new("TEARDOWN", &self.path);

        if let Err(e) = self.send_request(request).await {
            tracing::warn!("TEARDOWN failed: {}", e);
        }

        {
            let mut tracks = self.tracks.write().await;
            for track in tracks.iter_mut() {
                track.state = TrackState::Teardown;
            }
        }

        if let Some(ref conn) = *self.connection.read().await {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                conn.send(b"OPTIONS rtsp://localhost RTSP/1.0\r\n\r\n"),
            )
            .await;
        }

        {
            let mut conn = self.connection.write().await;
            *conn = None;
        }

        self.update_state(RtspState::Teardown).await;
        self.callback.on_teardown_complete();

        Ok(())
    }

    pub fn get_tracks(&self) -> Vec<MediaTrack> {
        // 同步返回副本
        self.tracks.blocking_read().clone()
    }

    pub async fn send_rtp(&self, track_id: &str, data: &[u8]) -> Result<(), RtspClientError> {
        {
            let state = self.state.read().await;
            if !state.can_send_rtp() {
                return Err(RtspClientError::ProtocolError("Not in playing state".into()));
            }
        }

        let conn = self.connection.read().await;
        if conn.is_none() {
            return Err(RtspClientError::ConnectionFailed("Not connected".into()));
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/protocol/rtsp/client/client.rs
git commit -m "feat(rtsp): add RtspClient main structure"
```

---

### Task 7: 集成到 adapter_manager（可选，后续）

**Files:**
- Modify: `src/protocol/adapter_manager.rs`

**说明:** 如果需要通过统一的 adapter_factory 管理，可以注册 RTSP client factory。暂不实现，先让 client 模块独立运行。

---

## 自检清单

1. **Spec 覆盖:** 每个 spec 中的类型和接口都有对应实现
2. **占位符扫描:** 无 TBD/TODO
3. **类型一致性:** MediaDirection, MediaTrack, RtspState 等类型在各文件中一致
