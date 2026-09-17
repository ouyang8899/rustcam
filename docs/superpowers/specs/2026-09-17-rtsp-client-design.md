# RTSP Client 模块设计

## 目标

实现独立的 RTSP Client 模块，用于连接 RTSP 设备，支持：
- 视频拉流（RecvOnly）
- 语音上传/广播（SendOnly）
- 双向对讲（SendRecv）

## 模块结构

```
src/protocol/rtsp/
├── client/
│   ├── mod.rs              # 模块导出
│   ├── client.rs           # RtspClient 主结构
│   ├── connection.rs       # TCP/UDP 连接管理
│   ├── state.rs            # 客户端状态机
│   ├── requests.rs         # RTSP 请求构建
│   ├── auth.rs             # 认证处理（Basic/Digest）
│   ├── errors.rs           # 错误定义
│   └── types.rs            # 公共类型
├── adapter/                # 服务端适配器（保留不动）
├── server_adapter.rs
├── session.rs
├── sdp.rs                 # SDP 解析（共享）
├── auth.rs                # 认证（共享）
├── response.rs
└── types.rs               # 公共类型（MediaTrack, MediaDirection 等）
```

## 核心类型

### MediaDirection

```rust
pub enum MediaDirection {
    RecvOnly,   // 拉流
    SendOnly,   // 语音上传/广播
    SendRecv,   // 双向对讲
}
```

### MediaTrack

```rust
pub struct MediaTrack {
    pub track_id: String,
    pub media_type: MediaType,     // Video / Audio
    pub codec: String,
    pub direction: MediaDirection,
    pub fmtp: Option<String>,
    pub transport: TrackTransport,
    pub state: TrackState,
    pub ssrc: Option<u32>,
}

pub enum TrackState {
    Pending,
    Setup,
    Playing,
    Teardown,
}
```

### TrackTransport

```rust
pub enum TrackTransport {
    Udp {
        rtp_port: u16,
        rtcp_port: u16,
        local_rtp_port: u16,
        local_rtcp_port: u16,
        server: Option<Arc<UDPRtpServer>>,
    },
    Tcp {
        interleaved: (u8, u8),  // (rtp_channel, rtcp_channel)
    },
}
```

### RtspState

```rust
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
```

## 回调接口

```rust
pub trait RtspClientCallback: Send + Sync {
    fn on_state_change(&self, state: &RtspState);
    fn on_rtp_packet(&self, track_id: &str, data: &[u8]);
    fn on_error(&self, error: &RtspClientError);
    fn on_teardown_complete(&self);
}
```

## 公共接口

```rust
impl RtspClient {
    pub fn new(
        url: String,
        credentials: AuthCredentials,
        callback: Arc<dyn RtspClientCallback>,
    ) -> Arc<Self>;

    pub async fn connect(&self) -> Result<()>;
    pub async fn teardown(&self) -> Result<()>;

    pub fn get_tracks(&self) -> Vec<MediaTrack>;
    pub async fn send_rtp(&self, track_id: &str, data: &[u8]) -> Result<()>;
}
```

## 认证流程

1. 首次请求不带认证
2. 服务器返回 401 + WWW-Authenticate header
3. 解析 header 判断是 Basic 还是 Digest
4. 重新发送请求，带 Authorization header

## RTP 数据处理

### UDP 模式
- 向 RTP Server Manager 申请 UDP 端口
- UDPRtpServer 接收 RTP，通过回调通知

### TCP 模式 (Interleaved)
- 在 RTSP TCP 连接上处理 $ (0x24) 开头的 interleaved 数据
- 格式: `$ channel length payload`

## 错误处理

使用 thiserror 定义错误枚举：
- ConnectionFailed
- AuthFailed
- ServerError
- Timeout
- ProtocolError
- SessionNotFound
- TrackNotFound
- RtpSendFailed

## 实现原则

1. **简单健壮** - 不做过度设计
2. **产品级** - 完整的错误处理、日志、重连
3. **共享基础代码** - SDP 解析、认证、response 构建复用现有代码
4. **线程安全** - 使用 Arc + Mutex
5. **异步设计** - 使用 tokio 异步运行时
