use crate::media::errors::{MediaError, Result};
use crate::media::node::{ DataType, MediaNode };
use crate::media::rtp_server::{TcpRtpServer, UDPRtpServer, RtpServerActive};
use async_trait::async_trait;
use bytes::BytesMut;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

const PACER_INTERVAL_MS: u64 = 20;
const PACER_CHUNK_BYTES: usize = 160;
const SILENCE_ALAW: u8 = 0xD5;
const MAX_BUFFER_BYTES: usize = 16000;
const AUDIO_CHANNEL_CAPACITY: usize = 1000;

#[derive(Clone)]
pub enum BroadcastMode {
    Udp {
        server: Arc<UDPRtpServer>,
        remote: SocketAddr,
    },
    TcpPassive {
        server: Arc<TcpRtpServer>,
        stream: Arc<tokio::sync::Mutex<Option<TcpStream>>>,
    },
    TcpActive {
        server: Arc<TcpRtpServer>,
        stream: Arc<tokio::sync::Mutex<Option<TcpStream>>>,
        remote: SocketAddr,
    },
}

pub struct GB28181BroadcastVoiceTransport {
    me: Weak<GB28181BroadcastVoiceTransport>,
    mode: BroadcastMode,
    ssrc: u32,
    seq_num: AtomicU16,
    timestamp: AtomicU32,
    payload_type: u8,
    audio_tx: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    pacer_started: AtomicBool,
}

impl GB28181BroadcastVoiceTransport {
    fn make_arc(mode: BroadcastMode, ssrc: u32, payload_type: u8) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            me: weak.clone(),
            mode,
            ssrc,
            seq_num: AtomicU16::new(0),
            timestamp: AtomicU32::new(0),
            payload_type,
            audio_tx: Mutex::new(None),
            pacer_started: AtomicBool::new(false),
        })
    }

    pub fn new_udp(
        server: Arc<UDPRtpServer>,
        remote: SocketAddr,
        ssrc: u32,
        payload_type: u8,
    ) -> Arc<Self> {
        Self::make_arc(BroadcastMode::Udp { server, remote }, ssrc, payload_type)
    }

    pub fn new_tcp_passive(server: Arc<TcpRtpServer>, ssrc: u32, payload_type: u8) -> Arc<Self> {
        Self::make_arc(
            BroadcastMode::TcpPassive {
                server,
                stream: Arc::new(tokio::sync::Mutex::new(None)),
            },
            ssrc,
            payload_type,
        )
    }

    pub fn new_tcp_active(
        server: Arc<TcpRtpServer>,
        remote: SocketAddr,
        ssrc: u32,
        payload_type: u8,
    ) -> Arc<Self> {
        Self::make_arc(
            BroadcastMode::TcpActive {
                server,
                stream: Arc::new(tokio::sync::Mutex::new(None)),
                remote,
            },
            ssrc,
            payload_type,
        )
    }

    fn build_rtp_header(&self, ts: u32, seq: u16) -> [u8; 12] {
        [
            0x80,
            self.payload_type,
            (seq >> 8) as u8,
            seq as u8,
            (ts >> 24) as u8,
            (ts >> 16) as u8,
            (ts >> 8) as u8,
            ts as u8,
            (self.ssrc >> 24) as u8,
            (self.ssrc >> 16) as u8,
            (self.ssrc >> 8) as u8,
            self.ssrc as u8,
        ]
    }

    async fn pacer_loop(me: Weak<Self>, mut rx: mpsc::Receiver<Vec<u8>>) {
        let mut interval = tokio::time::interval(Duration::from_millis(PACER_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut buffer: Vec<u8> = Vec::with_capacity(2048);

        loop {
            interval.tick().await;

            loop {
                match rx.try_recv() {
                    Ok(data) => buffer.extend_from_slice(&data),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        return;
                    }
                }
            }

            if buffer.len() > MAX_BUFFER_BYTES {
                let drop = buffer.len() - MAX_BUFFER_BYTES;
                tracing::warn!(
                    "[GB28181Broadcast] pacer buffer overflow, dropping {} bytes",
                    drop
                );
                buffer.drain(..drop);
            }

            let chunk: Vec<u8> = if buffer.len() >= PACER_CHUNK_BYTES {
                buffer.drain(..PACER_CHUNK_BYTES).collect()
            } else {
                let mut c: Vec<u8> = buffer.drain(..).collect();
                c.resize(PACER_CHUNK_BYTES, SILENCE_ALAW);
                c
            };

            let Some(this) = me.upgrade() else {
                return;
            };

            let seq = this.seq_num.fetch_add(1, Ordering::Relaxed);
            let ts = this
                .timestamp
                .fetch_add(PACER_CHUNK_BYTES as u32, Ordering::Relaxed);
            let header = this.build_rtp_header(ts, seq);

            Self::send_packet(&this.mode, &header, &chunk).await;
        }
    }

    async fn send_packet(mode: &BroadcastMode, header: &[u8; 12], payload: &[u8]) {
        match mode {
            BroadcastMode::Udp { server, remote } => {
                server.touch_active().await;
                let socket = server.rtp_socket();

                let mut packet = Vec::with_capacity(12 + payload.len());
                packet.extend_from_slice(header);
                packet.extend_from_slice(payload);

                let _rtp_seq = u16::from_be_bytes([header[2], header[3]]);
                let _rtp_ts = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);

                if let Err(e) = socket.send_to(&packet, remote).await {
                    tracing::error!("[GB28181Broadcast] UDP send failed: {}", e);
                }
            }
            BroadcastMode::TcpPassive { server, stream }
            | BroadcastMode::TcpActive { server, stream, .. } => {
                server.touch_active().await;

                let payload_len = 12 + payload.len();
                let mut frame = Vec::with_capacity(4 + payload_len);
                frame.push(0x24);
                frame.push(0x00);
                frame.extend_from_slice(&(payload_len as u16).to_be_bytes());
                frame.extend_from_slice(header);
                frame.extend_from_slice(payload);

                let _rtp_seq = u16::from_be_bytes([header[2], header[3]]);
                let _rtp_ts = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);

                let mut guard = stream.lock().await;
                if let Some(s) = guard.as_mut() {
                    if let Err(e) = s.write_all(&frame).await {
                        tracing::error!("[GB28181Broadcast] TCP send failed: {}", e);
                    }
                }
            }
        }
    }

    pub async fn connect(&self) -> Result<()> {
        match &self.mode {
            BroadcastMode::TcpActive {
                server,
                stream,
                remote,
            } => {
                if stream.lock().await.is_some() {
                    return Ok(());
                }
                let client_stream = server
                    .connect(*remote)
                    .await
                    .map_err(|e| MediaError::Transport(format!("TCP connect failed: {}", e)))?;
                let mut guard = stream.lock().await;
                *guard = Some(client_stream);
                Ok(())
            }
            _ => Err(MediaError::Transport("Not in TCP active mode".into())),
        }
    }

    pub async fn accept(&self) -> Result<SocketAddr> {
        match &self.mode {
            BroadcastMode::TcpPassive { server, stream } => {
                let (client_stream, addr) = server
                    .accept()
                    .await
                    .map_err(|e| MediaError::Transport(format!("TCP accept failed: {}", e)))?;
                let mut guard = stream.lock().await;
                *guard = Some(client_stream);
                Ok(addr)
            }
            _ => Err(MediaError::Transport("Not in TCP passive mode".into())),
        }
    }

    pub fn activate(self: Arc<Self>, node: Arc<dyn MediaNode>) {
        tokio::spawn(async move {
            loop {
                let data = match &self.mode {
                    BroadcastMode::Udp { server, .. } => {
                        server.touch_active().await;
                        let socket = server.rtp_socket();
                        let mut buf = vec![0u8; 2048];
                        match socket.recv_from(&mut buf).await {
                            Ok((len, _from)) => {
                                buf.truncate(len);
                                BytesMut::from(&buf[..])
                            }
                            Err(e) => {
                                tracing::warn!("[GB28181Broadcast] UDP recv error: {}", e);
                                break;
                            }
                        }
                    }
                    BroadcastMode::TcpPassive { server, stream }
                    | BroadcastMode::TcpActive { server, stream, .. } => {
                        server.touch_active().await;
                        match self.recv_tcp_frame(stream.clone()).await {
                            Ok(data) => data,
                            Err(e) => {
                                tracing::warn!("[GB28181Broadcast] TCP recv error: {}", e);
                                break;
                            }
                        }
                    }
                };

                let mut result = HashMap::new();
                let data_type = Self::classify_rtp_packet(&data);
                result.insert(data_type, data);
                node.on_media_data(result).await;
            }
        });
    }

    async fn recv_tcp_frame(
        &self,
        stream: Arc<tokio::sync::Mutex<Option<TcpStream>>>,
    ) -> Result<BytesMut> {
        let mut guard = stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| MediaError::Transport("TCP stream not connected".into()))?;

        let mut header = [0u8; 4];
        stream
            .read_exact(&mut header)
            .await
            .map_err(|e| MediaError::TransportRecv(format!("TCP read header failed: {}", e)))?;

        if header[0] != 0x24 {
            return Err(MediaError::TransportRecv(format!(
                "Invalid RTP frame magic: 0x{:02x}",
                header[0]
            )));
        }

        let len = u16::from_be_bytes([header[2], header[3]]) as usize;
        if len > 2048 {
            return Err(MediaError::TransportRecv(format!(
                "RTP frame too large: {}",
                len
            )));
        }

        let mut data = vec![0u8; len];
        stream
            .read_exact(&mut data)
            .await
            .map_err(|e| MediaError::TransportRecv(format!("TCP read data failed: {}", e)))?;

        Ok(BytesMut::from(&data[..]))
    }

    fn start_pacer_if_needed(&self) {
        if self.pacer_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let (tx, rx) = mpsc::channel::<Vec<u8>>(AUDIO_CHANNEL_CAPACITY);
        {
            let mut guard = self.audio_tx.lock().unwrap();
            *guard = Some(tx);
        }
        let weak = self.me.clone();
        tokio::spawn(Self::pacer_loop(weak, rx));
    }

    fn classify_rtp_packet(data: &[u8]) -> DataType {
        if data.len() < 2 {
            return DataType::Audio;
        }

        let first = data[0];
        let second = data[1];
        let version = (first >> 6) & 0x03;
        let payload_type = second & 0x7F;

        if version != 2 {
            return DataType::Audio;
        }

        if (200..=204).contains(&payload_type) {
            return DataType::Rtcp;
        }

        match payload_type {
            0 | 8 => DataType::Audio,
            98 | 99 | 104 | 105 | 106 | 107 | 108 | 109 | 110 | 111 | 112 => DataType::Video,
            _ => DataType::Audio,
        }
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for GB28181BroadcastVoiceTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        match &self.mode {
            BroadcastMode::Udp { server, .. } => server
                .rtp_socket()
                .local_addr()
                .map_err(|e| MediaError::Transport(format!("Failed to get local addr: {}", e))),
            BroadcastMode::TcpPassive { stream, .. } => {
                let guard = stream.blocking_lock();
                if let Some(s) = guard.as_ref() {
                    s.local_addr().map_err(|e| {
                        MediaError::Transport(format!("Failed to get local addr: {}", e))
                    })
                } else {
                    Err(MediaError::Transport("TCP not connected".into()))
                }
            }
            BroadcastMode::TcpActive { stream, .. } => {
                let guard = stream.blocking_lock();
                if let Some(s) = guard.as_ref() {
                    s.local_addr().map_err(|e| {
                        MediaError::Transport(format!("Failed to get local addr: {}", e))
                    })
                } else {
                    Err(MediaError::Transport("TCP not connected".into()))
                }
            }
        }
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        match &self.mode {
            BroadcastMode::Udp { remote, .. } => Ok(*remote),
            BroadcastMode::TcpPassive { stream, .. } => {
                let guard = stream.blocking_lock();
                if let Some(s) = guard.as_ref() {
                    s.peer_addr().map_err(|e| {
                        MediaError::Transport(format!("Failed to get peer addr: {}", e))
                    })
                } else {
                    Err(MediaError::Transport("TCP not connected".into()))
                }
            }
            BroadcastMode::TcpActive { stream, .. } => {
                let guard = stream.blocking_lock();
                if let Some(s) = guard.as_ref() {
                    s.peer_addr().map_err(|e| {
                        MediaError::Transport(format!("Failed to get peer addr: {}", e))
                    })
                } else {
                    Err(MediaError::Transport("TCP not connected".into()))
                }
            }
        }
    }

    async fn send(&self, data: &[u8], data_type: DataType) -> Result<()> {
        if matches!(data_type, DataType::Audio) {
            self.start_pacer_if_needed();

            let tx = {
                let guard = self.audio_tx.lock().unwrap();
                guard.as_ref().cloned()
            };
            let Some(tx) = tx else {
                return Err(MediaError::TransportSend("transport closed".into()));
            };

            if let Err(e) = tx.try_send(data.to_vec()) {
                match e {
                    mpsc::error::TrySendError::Full(_) => {
                        tracing::warn!(
                            "[GB28181Broadcast] audio channel full, dropping {} bytes",
                            data.len()
                        );
                    }
                    mpsc::error::TrySendError::Closed(_) => {
                        return Err(MediaError::TransportSend("audio channel closed".into()));
                    }
                }
            }
        } else {
            let seq = self.seq_num.fetch_add(1, Ordering::Relaxed);
            let ts = self
                .timestamp
                .fetch_add(data.len() as u32, Ordering::Relaxed);
            let header = self.build_rtp_header(ts, seq);
            Self::send_packet(&self.mode, &header, data).await;
        }
        Ok(())
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        let data = match &self.mode {
            BroadcastMode::Udp { server, .. } => {
                server.touch_active().await;
                let socket = server.rtp_socket();
                let mut buf = vec![0u8; 2048];
                let (len, _) = socket
                    .recv_from(&mut buf)
                    .await
                    .map_err(|e| MediaError::TransportRecv(format!("UDP recv failed: {}", e)))?;
                buf.truncate(len);
                BytesMut::from(&buf[..len])
            }
            BroadcastMode::TcpPassive { server, stream } => {
                server.touch_active().await;
                self.recv_tcp_frame(stream.clone()).await?
            }
            BroadcastMode::TcpActive { server, stream, .. } => {
                server.touch_active().await;
                self.recv_tcp_frame(stream.clone()).await?
            }
        };

        let mut result = HashMap::new();
        let data_type = Self::classify_rtp_packet(&data);
        result.insert(data_type, data);
        Ok(result)
    }

    async fn close(&self) -> Result<()> {

        {
            let mut guard = self.audio_tx.lock().unwrap();
            guard.take();
        }

        match &self.mode {
            BroadcastMode::TcpPassive { stream, .. }
            | BroadcastMode::TcpActive { stream, .. } => {
                let mut guard = stream.lock().await;
                if let Some(mut s) = guard.take() {
                    let _ = s.shutdown().await;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
