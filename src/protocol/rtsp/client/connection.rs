//! RTSP/TCP 连接层
//!
//! 设计要点：
//! - 单个 `TcpStream`，读由唯一后台任务负责，写通过 `write_lock` 串行化。
//! - RTSP 请求响应通过 `cseq → oneshot` 路由，避免"请求/响应"与"RTP 数据"抢流。
//! - 接收侧维护一个 `read_buf`，按字节流边界解析 interleaved 帧与 RTSP 响应，
//!   不做 lossy 字符串切片，避免非 UTF-8 body 导致越界。
//! - 断开时清空 `pending_responses`，唤醒所有等待者。

use crate::protocol::rtsp::client::types::ReceivedData;
use crate::protocol::rtsp::common::{
    InterleavedBuilder, RtcpPacket, RtcpReportType, RtspError,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex};

const MAX_READ_BUF_SIZE: usize = 10 * 1024 * 1024;
const MAX_INTERLEAVED_LEN: usize = u16::MAX as usize;
const READ_IDLE_TIMEOUT_SECS: u64 = 60;
const WRITE_TIMEOUT_SECS: u64 = 10;
const RESPONSE_TIMEOUT_SECS: u64 = 10;

pub struct TcpConnection {
    stream: Arc<TcpStream>,
    write_lock: Mutex<()>,
    pending_responses: Mutex<HashMap<u32, oneshot::Sender<String>>>,
}

#[derive(Debug)]
enum ParsedMessage {
    Interleaved { channel: u8, payload: Vec<u8> },
    Rtsp(String),
}

impl TcpConnection {
    pub async fn connect(addr: SocketAddr) -> Result<Self, RtspError> {
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            TcpStream::connect(addr),
        )
        .await
        .map_err(|_| RtspError::Timeout("连接超时".into()))?
        .map_err(|e| RtspError::ConnectionFailed(e.to_string()))?;

        let _ = stream.set_nodelay(true);

        Ok(Self {
            stream: Arc::new(stream),
            write_lock: Mutex::new(()),
            pending_responses: Mutex::new(HashMap::new()),
        })
    }

    pub async fn send(&self, data: &[u8]) -> Result<(), RtspError> {
        let _guard = self.write_lock.lock().await;
        let mut stream: &TcpStream = &*self.stream;
        tokio::time::timeout(
            std::time::Duration::from_secs(WRITE_TIMEOUT_SECS),
            stream.write_all(data),
        )
        .await
        .map_err(|_| RtspError::Timeout("写入超时".into()))?
        .map_err(|e| RtspError::ConnectionFailed(e.to_string()))
    }

    pub async fn send_interleaved(&self, channel: u8, data: &[u8]) -> Result<(), RtspError> {
        let frame = InterleavedBuilder::build_frame(channel, data);
        let _guard = self.write_lock.lock().await;
        let mut stream: &TcpStream = &*self.stream;
        tokio::time::timeout(
            std::time::Duration::from_secs(WRITE_TIMEOUT_SECS),
            stream.write_all(&frame),
        )
        .await
        .map_err(|_| RtspError::Timeout("写入 interleaved 超时".into()))?
        .map_err(|e| RtspError::ConnectionFailed(e.to_string()))
    }

    pub async fn send_request(&self, data: &[u8], cseq: u32) -> Result<String, RtspError> {
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

        if let Err(e) = self.send(data).await {
            self.pending_responses.lock().await.remove(&cseq);
            return Err(e);
        }

        match tokio::time::timeout(
            std::time::Duration::from_secs(RESPONSE_TIMEOUT_SECS),
            rx,
        )
        .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                self.pending_responses.lock().await.remove(&cseq);
                Err(RtspError::ConnectionFailed("连接已关闭".into()))
            }
            Err(_) => {
                self.pending_responses.lock().await.remove(&cseq);
                Err(RtspError::Timeout(format!(
                    "等待 CSeq {} 响应超时",
                    cseq
                )))
            }
        }
    }

    pub async fn start_receive_loop(
        self: Arc<Self>,
        on_data: Arc<dyn Fn(ReceivedData) + Send + Sync>,
    ) {
        let mut read_buf: Vec<u8> = Vec::with_capacity(8192);
        let mut chunk = [0u8; 16 * 1024];

        'outer: loop {
            loop {
                match Self::try_parse_message(&mut read_buf) {
                    Some(ParsedMessage::Interleaved { channel, payload }) => {
                        match Self::build_interleaved_message(channel, payload) {
                            Ok(data) => on_data(data),
                            Err(e) => tracing::warn!(
                                "[TCP Receiver] 解析 interleaved 帧失败: {}",
                                e
                            ),
                        }
                    }
                    Some(ParsedMessage::Rtsp(response)) => {
                        let cseq = Self::extract_cseq(&response);
                        match cseq {
                            Some(cseq) => {
                                let tx = {
                                    let mut pending = self.pending_responses.lock().await;
                                    pending.remove(&cseq)
                                };
                                match tx {
                                    Some(tx) => {
                                        let _ = tx.send(response);
                                    }
                                    None => tracing::warn!(
                                        "[TCP Receiver] 收到未知 CSeq={} 的响应",
                                        cseq
                                    ),
                                }
                            }
                            None => tracing::warn!(
                                "[TCP Receiver] RTSP 响应缺少 CSeq 头"
                            ),
                        }
                    }
                    None => break,
                }
            }

            if read_buf.len() > MAX_READ_BUF_SIZE {
                tracing::error!(
                    "[TCP Receiver] 缓冲区超限 ({} 字节)，断开",
                    read_buf.len()
                );
                break 'outer;
            }

            let mut stream: &TcpStream = &*self.stream;
            let read_result = tokio::time::timeout(
                std::time::Duration::from_secs(READ_IDLE_TIMEOUT_SECS),
                stream.read(&mut chunk),
            )
            .await;

            match read_result {
                Ok(Ok(0)) => {
                    tracing::info!("[TCP Receiver] 对端关闭连接");
                    break 'outer;
                }
                Ok(Ok(n)) => read_buf.extend_from_slice(&chunk[..n]),
                Ok(Err(e)) => {
                    tracing::error!("[TCP Receiver] 读错误: {}", e);
                    break 'outer;
                }
                Err(_) => {
                    tracing::trace!("[TCP Receiver] 读空闲");
                    continue;
                }
            }
        }

        let cancelled = {
            let mut pending = self.pending_responses.lock().await;
            let n = pending.len();
            pending.clear();
            n
        };
        if cancelled > 0 {
            tracing::warn!(
                "[TCP Receiver] 连接断开，取消 {} 个等待中的请求",
                cancelled
            );
        }
        tracing::info!("[TCP Receiver] Stopped");
    }

    fn try_parse_message(buf: &mut Vec<u8>) -> Option<ParsedMessage> {
        if buf.is_empty() {
            return None;
        }

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
        let header_end = buf.windows(4).position(|w| w == b"\r\n\r\n")? + 4;

        let header_str = std::str::from_utf8(&buf[..header_end]).ok()?;
        let content_length = Self::find_content_length(header_str);

        let total_len = header_end + content_length;
        if buf.len() < total_len {
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

    fn build_interleaved_message(
        channel: u8,
        payload: Vec<u8>,
    ) -> Result<ReceivedData, RtspError> {
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
