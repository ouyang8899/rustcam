use crate::media::errors::{MediaError, Result};
use crate::media::node::{ DataType, MediaNode };
use crate::protocol::rtsp::client::client::{RtspClient, RtspClientCallback};
use crate::protocol::rtsp::client::state::RtspState;
use crate::protocol::rtsp::client::types::{MediaDirection, RtpTransportMode};
use crate::protocol::rtsp::common::RtcpReportType;
use crate::protocol::rtsp::common::RtspError;
use async_trait::async_trait;
use bytes::BytesMut;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

const PACER_INTERVAL_MS: u64 = 20;
const PACER_CHUNK_BYTES: usize = 160;
const SILENCE_ALAW: u8 = 0xD5;
const MAX_BUFFER_BYTES: usize = 16000;
const AUDIO_CHANNEL_CAPACITY: usize = 1000;

pub struct OnvifBackchannelTransport {
    me: Weak<OnvifBackchannelTransport>,
    rtsp_url: String,
    username: String,
    password: String,
    rtsp_client: Mutex<Option<Arc<RtspClient>>>,
    backchannel_track_id: Mutex<Option<String>>,
    ssrc: u32,
    seq_num: AtomicU16,
    timestamp: AtomicU32,
    payload_type: u8,
    audio_tx: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    pacer_started: AtomicBool,
    node: Mutex<Option<Arc<dyn MediaNode>>>,
    send_errors: AtomicU32,
    reconnect_in_progress: AtomicBool,
    is_closing: AtomicBool,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}

impl OnvifBackchannelTransport {
    pub fn new(rtsp_url: String, username: String, password: String) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            me: weak.clone(),
            rtsp_url,
            username,
            password,
            rtsp_client: Mutex::new(None),
            backchannel_track_id: Mutex::new(None),
            ssrc: rand::random(),
            seq_num: AtomicU16::new(rand::random()),
            timestamp: AtomicU32::new(rand::random()),
            payload_type: 8,
            audio_tx: Mutex::new(None),
            pacer_started: AtomicBool::new(false),
            node: Mutex::new(None),
            send_errors: AtomicU32::new(0),
            reconnect_in_progress: AtomicBool::new(false),
            is_closing: AtomicBool::new(false),
            shutdown_tx: Mutex::new(None),
        })
    }

    pub fn set_node(self: &Arc<Self>, node: Arc<dyn MediaNode>) {
        *self.node.lock().unwrap() = Some(node);
    }

    pub fn send_error_count(&self) -> u32 {
        self.send_errors.load(Ordering::Relaxed)
    }

    pub fn is_connected(&self) -> bool {
        self.rtsp_client.lock().unwrap().is_some()
            && self.backchannel_track_id.lock().unwrap().is_some()
    }

    async fn reconnect(self: &Arc<Self>) -> Result<()> {

        {
            let mut guard = self.audio_tx.lock().unwrap();
            guard.take();
        }

        {
            let mut guard = self.shutdown_tx.lock().unwrap();
            guard.take();
        }

        let client_to_teardown = self.rtsp_client.lock().unwrap().take();
        if let Some(client) = client_to_teardown {
            let _ = client.teardown().await;
        }
        *self.backchannel_track_id.lock().unwrap() = None;
        self.pacer_started.store(false, Ordering::SeqCst);

        self.connect().await
    }

    pub async fn connect(self: &Arc<Self>) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }

        let callback = Arc::new(OnvifBackchannelCallback {
            transport: Arc::downgrade(self),
        });

        let client = RtspClient::new(
            self.rtsp_url.clone(),
            crate::protocol::rtsp::client::auth::AuthCredentials {
                username: self.username.clone(),
                password: self.password.clone(),
            },
            RtpTransportMode::Tcp,
            callback,
        )
        .map_err(|e| MediaError::Transport(format!("Failed to create RTSP client: {}", e)))?;

        let client_arc = client.clone();

        client
            .connect()
            .await
            .map_err(|e| MediaError::Transport(format!("RTSP connect failed: {}", e)))?;

        client
            .start_rtsp_receive()
            .await
            .map_err(|e| MediaError::Transport(format!("Failed to start RTSP client: {}", e)))?;

        match client.options().await {
            Ok(_resp) => {
            }
            Err(e) => {
                tracing::error!("[ONVIF Backchannel] OPTIONS failed: {:?}", e);
                return Err(MediaError::Transport(format!("OPTIONS failed: {:?}", e)));
            }
        };

        let describe_result = client.describe_with_backchannel().await;

        match &describe_result {
            Ok(_tracks) => {
            }
            Err(e) => {
                tracing::error!(
                    "[ONVIF Backchannel] DESCRIBE with backchannel failed: {:?}",
                    e
                );
                if matches!(e, RtspError::BackchannelNotSupported) {
                    return Err(MediaError::Transport(
                        "Device does not support backchannel".to_string(),
                    ));
                }
                return Err(MediaError::Transport(format!(
                    "DESCRIBE with backchannel failed: {:?}",
                    e
                )));
            }
        }

        let _tracks = describe_result.map_err(|e| {
            if matches!(e, RtspError::BackchannelNotSupported) {
                MediaError::Transport("Device does not support backchannel (551)".to_string())
            } else {
                MediaError::Transport(format!("DESCRIBE failed: {}", e))
            }
        })?;

        let bc_track = client.get_backchannel_track().await;
        let track = bc_track.ok_or_else(|| {
            MediaError::Transport("No backchannel track found in SDP".to_string())
        })?;
        *self.backchannel_track_id.lock().unwrap() = Some(track.track_id.clone());

        client
            .setup(&track.track_id, MediaDirection::SendOnly)
            .await
            .map_err(|e| MediaError::Transport(format!("SETUP backchannel failed: {}", e)))?;

        client
            .play_with_backchannel()
            .await
            .map_err(|e| MediaError::Transport(format!("PLAY failed: {}", e)))?;

        *self.rtsp_client.lock().unwrap() = Some(client_arc);

        client
            .start()
            .await
            .map_err(|e| MediaError::Transport(format!("Failed to start RTSP client: {}", e)))?;

        Ok(())
    }

    fn start_pacer(&self) {
        let rx = {
            let mut guard = self.audio_tx.lock().unwrap();
            if guard.is_some() {
                return;
            }
            let (tx, rx) = mpsc::channel::<Vec<u8>>(AUDIO_CHANNEL_CAPACITY);
            *guard = Some(tx);
            rx
        };

        if self.pacer_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        {
            let mut guard = self.shutdown_tx.lock().unwrap();
            *guard = Some(shutdown_tx);
        }

        let weak = self.me.clone();
        tokio::spawn(Self::pacer_loop(weak, rx, shutdown_rx));
    }

    async fn pacer_loop(
        me: Weak<Self>,
        mut rx: mpsc::Receiver<Vec<u8>>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_millis(PACER_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut buffer: Vec<u8> = Vec::with_capacity(2048);

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = &mut shutdown_rx => {
                    break;
                }
            }

            loop {
                match rx.try_recv() {
                    Ok(data) => {
                        if buffer.len() + data.len() > MAX_BUFFER_BYTES {
                            tracing::warn!(
                                "[ONVIF Backchannel] buffer full, dropping {} incoming bytes",
                                data.len()
                            );
                            continue;
                        }
                        buffer.extend_from_slice(&data);
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        return;
                    }
                }
            }

            if buffer.len() > MAX_BUFFER_BYTES {
                let drop = buffer.len() - MAX_BUFFER_BYTES;
                tracing::warn!(
                    "[ONVIF Backchannel] pacer buffer overflow, dropping {} bytes",
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

            if let Err(e) = this.send_rtp_packet(&chunk, ts, seq).await {
                this.send_errors.fetch_add(1, Ordering::Relaxed);
                tracing::warn!("[ONVIF Backchannel] send RTP failed: {}", e);
            }
        }
    }

    async fn send_rtp_packet(&self, payload: &[u8], timestamp: u32, seq: u16) -> Result<()> {
        if self.is_closing.load(Ordering::Relaxed) {
            return Ok(());
        }

        let rtp_header = self.build_rtp_header(timestamp, seq);
        let mut packet = Vec::with_capacity(12 + payload.len());
        packet.extend_from_slice(&rtp_header);
        packet.extend_from_slice(payload);

        let (client, track_id) = {
            let client_guard = self.rtsp_client.lock().unwrap();
            let track_id_guard = self.backchannel_track_id.lock().unwrap();
            let client = client_guard.clone();
            let track_id = track_id_guard.clone();
            (client, track_id)
        };

        if let (Some(c), Some(tid)) = (client, track_id) {
            c.send_rtp(&tid, &packet)
                .await
                .map_err(|e| MediaError::TransportSend(format!("RTP send failed: {}", e)))?;
        }
        Ok(())
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
}

struct OnvifBackchannelCallback {
    transport: Weak<OnvifBackchannelTransport>,
}

impl RtspClientCallback for OnvifBackchannelCallback {
    fn on_state_change(&self, _state: &RtspState) {
    }

    fn on_rtp_packet(&self, _track_id: &str, _data: &[u8], _ssrc: u32) {}

    fn on_rtcp_packet(
        &self,
        _track_id: &str,
        _data: &[u8],
        _ssrc: u32,
        report_type: &RtcpReportType,
    ) {
        match report_type {
            RtcpReportType::ReceiverReport { .. } => {
            }
            RtcpReportType::SenderReport { .. } => {
            }
            RtcpReportType::Bye { .. } => {
            }
            RtcpReportType::Unknown => {
            }
        }
    }

    fn on_error(&self, error: &RtspError) {
        tracing::error!("[ONVIF Backchannel] RTSP error: {}", error);

        let Some(transport) = self.transport.upgrade() else {
            return;
        };

        if transport.reconnect_in_progress.swap(true, Ordering::SeqCst) {
            return;
        }

        let transport_clone = transport.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;

            if let Err(e) = transport_clone.reconnect().await {
                tracing::error!("[ONVIF Backchannel] reconnect failed: {}", e);
            } else {
            }

            transport_clone
                .reconnect_in_progress
                .store(false, Ordering::SeqCst);
        });
    }

    fn on_teardown_complete(&self) {
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for OnvifBackchannelTransport {
    fn local_addr(&self) -> Result<SocketAddr> {
        Err(MediaError::Transport("Not implemented".into()))
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        Err(MediaError::Transport("Not implemented".into()))
    }

    async fn send(&self, data: &[u8], data_type: DataType) -> Result<()> {
        if !matches!(data_type, DataType::Audio) {
            return Ok(());
        }

        if self.is_closing.load(Ordering::Relaxed) {
            return Err(MediaError::TransportSend("transport closing".into()));
        }

        self.start_pacer();

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
                        "[ONVIF Backchannel] audio channel full, dropping {} bytes",
                        data.len()
                    );
                    return Err(MediaError::TransportSend("audio buffer full".into()));
                }
                mpsc::error::TrySendError::Closed(_) => {
                    return Err(MediaError::TransportSend("audio channel closed".into()));
                }
            }
        }
        Ok(())
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        Ok(HashMap::new())
    }

    async fn close(&self) -> Result<()> {

        self.is_closing.store(true, Ordering::SeqCst);

        {
            let mut guard = self.audio_tx.lock().unwrap();
            guard.take();
        }

        {
            let mut guard = self.shutdown_tx.lock().unwrap();
            guard.take();
        }

        let client = self.rtsp_client.lock().unwrap().take();
        if let Some(c) = client {
            let _ = c.teardown().await;
        }

        Ok(())
    }
}
