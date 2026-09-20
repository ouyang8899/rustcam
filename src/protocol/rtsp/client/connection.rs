use crate::protocol::rtsp::common::{
    InterleavedBuilder, ReceivedData, RtcpPacket, RtcpReportType, RtspError,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex};

const MAX_READ_BUF_SIZE: usize = 10 * 1024 * 1024;
const MAX_INTERLEAVED_LEN: usize = u16::MAX as usize;
const READ_IDLE_TIMEOUT_SECS: u64 = 60;
const WRITE_TIMEOUT_SECS: u64 = 10;
const RESPONSE_TIMEOUT_SECS: u64 = 10;

pub struct TcpConnection {
    read_half: Arc<tokio::sync::Mutex<OwnedReadHalf>>,
    write_half: Arc<tokio::sync::Mutex<OwnedWriteHalf>>,
    pending_responses: Mutex<HashMap<u32, oneshot::Sender<String>>>,
    closed: AtomicBool,
}

#[derive(Debug)]
enum ParsedMessage {
    Interleaved { channel: u8, payload: Vec<u8> },
    Rtsp(String),
}

impl TcpConnection {
    pub async fn connect(addr: SocketAddr) -> Result<Self, RtspError> {
        tracing::info!("[RTSP Connection] Connecting to {}", addr);
        let stream =
            tokio::time::timeout(std::time::Duration::from_secs(5), TcpStream::connect(addr))
                .await
                .map_err(|_| RtspError::Timeout("连接超时".into()))?
                .map_err(|e| RtspError::ConnectionFailed(e.to_string()))?;

        let _ = stream.set_nodelay(true);
        tracing::info!("[RTSP Connection] TCP connected to {}", addr);

        let (read_half, write_half) = stream.into_split();
        Ok(Self {
            read_half: Arc::new(tokio::sync::Mutex::new(read_half)),
            write_half: Arc::new(tokio::sync::Mutex::new(write_half)),
            pending_responses: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub fn mark_closed(&self) {
        eprintln!(
            "[Connection] mark_closed called, thread_id={:?}",
            std::thread::current().id()
        );
        self.closed.store(true, Ordering::Release);
    }

    pub async fn send(&self, data: &[u8]) -> Result<(), RtspError> {
        if self.is_closed() {
            return Err(RtspError::ConnectionFailed("连接已关闭".into()));
        }
        let data_str = String::from_utf8_lossy(data);
        eprintln!("[Connection] send {} bytes:\n{}", data.len(), data_str);
        let mut write_half = self.write_half.lock().await;
        write_half
            .write_all(data)
            .await
            .map_err(|e| RtspError::ConnectionFailed(e.to_string()))
    }

    pub async fn send_interleaved(&self, channel: u8, data: &[u8]) -> Result<(), RtspError> {
        if self.is_closed() {
            return Err(RtspError::ConnectionFailed("连接已关闭".into()));
        }
        let frame = InterleavedBuilder::build_frame(channel, data);
        let mut write_half = self.write_half.lock().await;
        write_half
            .write_all(&frame)
            .await
            .map_err(|e| RtspError::ConnectionFailed(e.to_string()))
    }

    pub async fn send_request(&self, data: &[u8], cseq: u32) -> Result<String, RtspError> {
        if self.is_closed() {
            eprintln!("[Connection] send_request: connection is closed!");
            return Err(RtspError::ConnectionFailed("连接已关闭".into()));
        }

        eprintln!(
            "[Connection] send_request: sending cseq={}, is_closed={}",
            cseq,
            self.is_closed()
        );
        let (tx, rx) = oneshot::channel::<String>();
        {
            let mut pending = self.pending_responses.lock().await;
            if pending.contains_key(&cseq) {
                return Err(RtspError::ProtocolError(format!(
                    "CSeq {} 已有未完成请求",
                    cseq
                )));
            }
            pending.insert(cseq, tx);
        }

        let data_str = String::from_utf8_lossy(data);
        eprintln!(
            "[Connection] send_request: cseq={}, sending {} bytes",
            cseq,
            data.len()
        );

        if let Err(e) = self.send(data).await {
            self.pending_responses.lock().await.remove(&cseq);
            eprintln!(
                "[Connection] send_request: cseq={} send FAILED: {}",
                cseq, e
            );
            return Err(e);
        }

        eprintln!(
            "[Connection] send_request: cseq={} sent successfully, waiting for response...",
            cseq
        );

        match tokio::time::timeout(std::time::Duration::from_secs(RESPONSE_TIMEOUT_SECS), rx).await
        {
            Ok(Ok(response)) => {
                eprintln!(
                    "[Connection] send_request: cseq={} got response, len={}",
                    cseq,
                    response.len()
                );
                Ok(response)
            }
            Ok(Err(_)) => {
                self.pending_responses.lock().await.remove(&cseq);
                eprintln!(
                    "[Connection] send_request: cseq={} channel CLOSED (rx returned Err)",
                    cseq
                );
                Err(RtspError::ConnectionFailed("连接已关闭".into()))
            }
            Err(_) => {
                self.pending_responses.lock().await.remove(&cseq);
                eprintln!(
                    "[Connection] send_request: cseq={} TIMEOUT after {}s",
                    cseq, RESPONSE_TIMEOUT_SECS
                );
                Err(RtspError::Timeout(format!("等待 CSeq {} 响应超时", cseq)))
            }
        }
    }

    /// 唯一的 TCP 读循环。从连接到断开，永不切换。
    pub async fn run_receive_loop(
        self: Arc<Self>,
        on_data: Arc<dyn Fn(ReceivedData) + Send + Sync>,
        on_closed: Arc<dyn Fn() + Send + Sync>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        let mut read_buf: Vec<u8> = Vec::with_capacity(8192);
        let mut chunk = [0u8; 16 * 1024];

        'outer: loop {
            loop {
                match Self::try_parse_message(&mut read_buf) {
                    Some(ParsedMessage::Interleaved { channel, payload }) => {
                        match Self::build_interleaved_message(channel, payload) {
                            Ok(data) => on_data(data),
                            Err(e) => {
                                tracing::warn!("[TCP Receiver] 解析 interleaved 帧失败: {}", e)
                            }
                        }
                    }
                    Some(ParsedMessage::Rtsp(response)) => {
                        let cseq = Self::extract_cseq(&response);
                        eprintln!("[TCP Receiver] 收到 RTSP 响应, cseq={:?}", cseq);
                        match cseq {
                            Some(cseq) => {
                                let tx = {
                                    let mut pending = self.pending_responses.lock().await;
                                    pending.remove(&cseq)
                                };
                                match tx {
                                    Some(tx) => {
                                        eprintln!(
                                            "[TCP Receiver] 发送响应到等待的请求 cseq={}",
                                            cseq
                                        );
                                        let _ = tx.send(response);
                                    }
                                    None => {
                                        eprintln!("[TCP Receiver] 收到未知 CSeq={} 的响应", cseq)
                                    }
                                }
                            }
                            None => eprintln!("[TCP Receiver] RTSP 响应缺少 CSeq 头"),
                        }
                    }
                    None => {
                        tracing::debug!(
                            "[TCP Receiver] 数据不完整，需要更多数据, read_buf.len={}",
                            read_buf.len()
                        );
                        break;
                    }
                }
            }

            if read_buf.len() > MAX_READ_BUF_SIZE {
                tracing::error!("[TCP Receiver] 缓冲区超限 ({} 字节)，断开", read_buf.len());
                self.mark_closed();
                break 'outer;
            }

            let mut read_half = self.read_half.lock().await;
            tracing::debug!(
                "[TCP Receiver] 等待读取数据... read_buf.len={}",
                read_buf.len()
            );

            tokio::select! {
                readable = read_half.readable() => {
                    match readable {
                        Ok(()) => {
                            match read_half.read(&mut chunk).await {
                                Ok(0) => {
                                    eprintln!("[TCP Receiver] 服务器关闭连接 (EOF)");
                                    self.mark_closed();
                                    break 'outer;
                                }
                                Ok(n) => {
                                    eprintln!("[TCP Receiver] 读取 {} 字节, read_buf.len={}", n, read_buf.len());
                                    read_buf.extend_from_slice(&chunk[..n]);
                                }
                                Err(e) => {
                                    eprintln!("[TCP Receiver] 读错误: {}", e);
                                    self.mark_closed();
                                    break 'outer;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[TCP Receiver] readable 错误: {}", e);
                            self.mark_closed();
                            break 'outer;
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    eprintln!("[TCP Receiver] 收到关闭信号");
                    break 'outer;
                }
            }
        }

        self.mark_closed();
        let cancelled = {
            let mut pending = self.pending_responses.lock().await;
            let n = pending.len();
            pending.clear();
            n
        };
        if cancelled > 0 {
            tracing::warn!("[TCP Receiver] 连接断开，取消 {} 个等待中的请求", cancelled);
        }
        on_closed();
        tracing::info!("[TCP Receiver] Stopped");
    }

    fn try_parse_message(buf: &mut Vec<u8>) -> Option<ParsedMessage> {
        if buf.is_empty() {
            return None;
        }

        let first_byte_display = if buf[0] == 0x24 {
            format!("0x24 (RTP)")
        } else {
            format!("0x{:02x} ('{}')", buf[0], buf[0] as char)
        };
        eprintln!(
            "[TCP Receiver] try_parse_message: first byte={}, buf.len()={}",
            first_byte_display,
            buf.len()
        );

        if buf[0] == 0x24 {
            if buf.len() < 4 {
                return None;
            }
            let channel = buf[1];
            let len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
            if len > MAX_INTERLEAVED_LEN {
                tracing::error!(
                    "[TCP Receiver] interleaved 帧长超限 ({} > {})，清空缓冲",
                    len,
                    MAX_INTERLEAVED_LEN
                );
                buf.clear();
                return None;
            }
            let total = 4 + len;
            if buf.len() < total {
                return None;
            }
            let mut frame: Vec<u8> = buf.drain(..total).collect();
            let payload = frame.split_off(4);
            Some(ParsedMessage::Interleaved { channel, payload })
        } else {
            Self::try_parse_response(buf).map(ParsedMessage::Rtsp)
        }
    }

    fn try_parse_response(buf: &mut Vec<u8>) -> Option<String> {
        let header_end = match buf.windows(4).position(|w| w == b"\r\n\r\n") {
            Some(pos) => pos + 4,
            None => {
                eprintln!("[TCP Receiver] try_parse_response: no header end found");
                return None;
            }
        };

        eprintln!(
            "[TCP Receiver] try_parse_response: header_end={}, buf.len()={}",
            header_end,
            buf.len()
        );

        let header_str = std::str::from_utf8(&buf[..header_end]).ok()?;
        eprintln!("[TCP Receiver] header:\n{}", header_str);
        let content_length = Self::find_content_length(header_str);

        let total_len = header_end + content_length;
        if buf.len() < total_len {
            eprintln!(
                "[TCP Receiver] try_parse_response: buf.len()={} < total_len={}",
                buf.len(),
                total_len
            );
            return None;
        }

        let bytes: Vec<u8> = buf.drain(..total_len).collect();
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn find_content_length(header: &str) -> usize {
        for line in header.lines() {
            if let Some(rest) = line
                .strip_prefix("Content-Length:")
                .or_else(|| line.strip_prefix("content-length:"))
            {
                return rest.trim().parse().unwrap_or(0);
            }
        }
        0
    }

    fn extract_cseq(response: &str) -> Option<u32> {
        for line in response.lines() {
            if line.len() < 5 {
                continue;
            }
            let (key, val) = line.split_at(5);
            if key.eq_ignore_ascii_case("CSeq:") {
                if let Ok(n) = val.trim().parse::<u32>() {
                    return Some(n);
                }
            }
        }
        None
    }

    fn build_interleaved_message(channel: u8, payload: Vec<u8>) -> Result<ReceivedData, RtspError> {
        let track_id = format!("trackID={}", channel / 2);
        if channel % 2 == 0 {
            Ok(ReceivedData::RtpData {
                track_id,
                data: payload,
                ssrc: 0,
            })
        } else {
            let report_type = RtcpPacket::parse(&payload)
                .map(|(_, rt)| rt)
                .unwrap_or(RtcpReportType::Unknown);
            Ok(ReceivedData::RtcpData {
                track_id,
                data: payload,
                ssrc: 0,
                report_type,
            })
        }
    }
}
