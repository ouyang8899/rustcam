use async_trait::async_trait;
use bytes::BytesMut;
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::RwLock;

use crate::media::codec::AudioCodec;
use crate::media::codec::G711;
use crate::media::errors::{MediaError, Result};
use crate::media::rtp::RtpHeader;

static NODE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaNodeType {
    GB28181_MEDIA,
    GB28181_BROADCAST_VOICE,
    ONVIF_BROADCAST_VOICE,
    WebSocket,
    RTSP,
    RTMP,
    RTMPPush,
    UDP,
    TCP,
    TCP_ACTIVE,
    Unknown,
}

impl MediaNodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaNodeType::GB28181_MEDIA => "gb28181_media",
            MediaNodeType::GB28181_BROADCAST_VOICE => "gb28181_broadcast_voice",
            MediaNodeType::ONVIF_BROADCAST_VOICE => "onvif_broadcast_voice",
            MediaNodeType::WebSocket => "websocket",
            MediaNodeType::RTSP => "rtsp",
            MediaNodeType::RTMP => "rtmp",
            MediaNodeType::RTMPPush => "rtmp_push",
            MediaNodeType::UDP => "udp",
            MediaNodeType::TCP => "tcp",
            MediaNodeType::TCP_ACTIVE => "tcp_active",
            MediaNodeType::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for MediaNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaNodeId {
    pub id: u64,
    pub node_type: MediaNodeType,
    pub device_tag: String,
    pub channel_tag: Option<String>,
}

impl MediaNodeId {
    pub fn new(node_type: MediaNodeType, device_tag: &str, channel_tag: Option<String>) -> Self {
        Self {
            id: NODE_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            node_type,
            device_tag: device_tag.to_string(),
            channel_tag,
        }
    }

    pub fn with_channel(mut self, channel_tag: Option<String>) -> Self {
        self.channel_tag = channel_tag;
        self
    }

    pub fn as_str(&self) -> String {
        match &self.channel_tag {
            Some(ch) => format!("{}:{}/{}", self.node_type, self.device_tag, ch),
            None => format!("{}:{}", self.node_type, self.device_tag),
        }
    }
}

impl Hash for MediaNodeId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for MediaNodeId {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for MediaNodeId {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    OrgData,
    Video,
    Audio,
    Rtcp,
    VideoAudio,
}

impl DataType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataType::OrgData => "org_data",
            DataType::Video => "video",
            DataType::Audio => "audio",
            DataType::Rtcp => "rtcp",
            DataType::VideoAudio => "video_audio",
        }
    }
}

#[async_trait]
pub trait MediaTransport: Send + Sync {
    fn local_addr(&self) -> Result<SocketAddr>;
    fn peer_addr(&self) -> Result<SocketAddr>;
    async fn send(&self, data: &[u8], data_type: DataType) -> Result<()>;
    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>>;
    async fn close(&self) -> Result<()>;
}

#[async_trait]
pub trait MediaTransportCallback: Send + Sync {
    async fn on_media_data(&self, data: HashMap<DataType, BytesMut>);
}

#[derive(Debug, Clone)]
pub struct MediaRtpPacket {
    pub data: Vec<u8>,
    pub ssrc: u32,
    pub payload_type: u8,
}

pub trait RtpPacketCallback: Send + Sync {
    fn on_rtp_packet(&self, packet: MediaRtpPacket);
}

#[derive(Debug, Clone)]
pub struct NodeMediaContext {
    pub target_type: MediaNodeType,
    pub data: HashMap<DataType, BytesMut>,
}

impl NodeMediaContext {
    pub fn new(data: HashMap<DataType, BytesMut>) -> Self {
        Self {
            target_type: MediaNodeType::Unknown,
            data,
        }
    }

    pub fn get_data(&self, data_type: DataType) -> Option<&BytesMut> {
        self.data.get(&data_type)
    }

    pub fn get_data_mut(&mut self, data_type: DataType) -> Option<&mut BytesMut> {
        self.data.get_mut(&data_type)
    }

    pub fn put_data(&mut self, data_type: DataType, buf: BytesMut) {
        self.data.insert(data_type, buf);
    }
}

pub trait NodeMiddleware: Send + Sync {
    fn name(&self) -> &str;
    fn as_middleware(&self) -> &dyn Any;
    fn process(
        &self,
        ctx: &mut NodeMediaContext,
        chain: &[Arc<dyn NodeMiddleware>],
        next_index: usize,
    ) -> bool;
}

pub struct BasicMiddleware {
    name: String,
    func: Box<dyn Fn(&mut BytesMut) -> bool + Send + Sync>,
}

impl BasicMiddleware {
    pub fn new<F>(name: &str, f: F) -> Self
    where
        F: Fn(&mut BytesMut) -> bool + Send + Sync + 'static,
    {
        Self {
            name: name.to_string(),
            func: Box::new(f),
        }
    }
}

impl NodeMiddleware for BasicMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    fn as_middleware(&self) -> &dyn Any {
        self
    }

    fn process(
        &self,
        ctx: &mut NodeMediaContext,
        chain: &[Arc<dyn NodeMiddleware>],
        next_index: usize,
    ) -> bool {
        if ctx.data.contains_key(&DataType::Audio) {
            let buf = ctx.data.get_mut(&DataType::Audio).unwrap();
            let stop = (self.func)(buf);
            if stop {
                return true;
            }
            if next_index < chain.len() {
                return chain[next_index].process(ctx, chain, next_index + 1);
            }
            return true;
        }

        if let Some(org) = ctx.data.get(&DataType::OrgData) {
            let mut new_buf = org.clone();
            let stop = (self.func)(&mut new_buf);
            ctx.data.insert(DataType::Audio, new_buf);
            ctx.data.remove(&DataType::OrgData);
            if stop {
                return true;
            }
            if next_index < chain.len() {
                return chain[next_index].process(ctx, chain, next_index + 1);
            }
            return true;
        }

        true
    }
}

pub struct G711ToLinearPcmMiddleware {
    name: String,
    codec: AtomicU8,
}

impl G711ToLinearPcmMiddleware {
    pub fn new(codec: AudioCodec) -> Self {
        Self {
            name: "g711_to_pcm".to_string(),
            codec: AtomicU8::new(codec as u8),
        }
    }

    pub fn set_codec(&self, codec: AudioCodec) {
        self.codec.store(codec as u8, Ordering::Relaxed);
    }
}

impl NodeMiddleware for G711ToLinearPcmMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    fn as_middleware(&self) -> &dyn Any {
        self
    }

    fn process(
        &self,
        ctx: &mut NodeMediaContext,
        chain: &[Arc<dyn NodeMiddleware>],
        next_index: usize,
    ) -> bool {
        let input_data = match ctx.data.get(&DataType::OrgData) {
            Some(d) => d.clone(),
            None => return true,
        };

        let codec_val = self.codec.load(Ordering::Relaxed);
        let samples = if codec_val == AudioCodec::Pcma as u8 {
            G711::alaw_decode_slice(&input_data)
        } else if codec_val == AudioCodec::Pcmu as u8 {
            G711::ulaw_decode_slice(&input_data)
        } else {
            return true;
        };

        let mut pcm_data = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            pcm_data.extend_from_slice(&sample.to_le_bytes());
        }
        ctx.data
            .insert(DataType::Audio, BytesMut::from(pcm_data.as_slice()));
        ctx.data.remove(&DataType::OrgData);
        if next_index < chain.len() {
            return chain[next_index].process(ctx, chain, next_index + 1);
        }
        true
    }
}

pub struct LinearPcmToG711Middleware {
    name: String,
    codec: AtomicU8,
}

impl LinearPcmToG711Middleware {
    pub fn new(codec: AudioCodec) -> Self {
        Self {
            name: "pcm_to_g711".to_string(),
            codec: AtomicU8::new(codec as u8),
        }
    }

    pub fn set_codec(&self, codec: AudioCodec) {
        self.codec.store(codec as u8, Ordering::Relaxed);
    }
}

impl NodeMiddleware for LinearPcmToG711Middleware {
    fn name(&self) -> &str {
        &self.name
    }

    fn as_middleware(&self) -> &dyn Any {
        self
    }

    fn process(
        &self,
        ctx: &mut NodeMediaContext,
        chain: &[Arc<dyn NodeMiddleware>],
        next_index: usize,
    ) -> bool {
        let input_data = if let Some(d) = ctx.data.get(&DataType::OrgData) {
            d.clone()
        } else if let Some(d) = ctx.data.get(&DataType::Audio) {
            d.clone()
        } else {
            return true;
        };

        let input_samples: Vec<i16> = input_data
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();

        let samples: Vec<i16> = input_samples
            .chunks_exact(6)
            .map(|c| {
                let sum: i32 = c.iter().map(|&s| s as i32).sum();
                (sum / 6) as i16
            })
            .collect();

        let codec_val = self.codec.load(Ordering::Relaxed);
        let encoded = if codec_val == AudioCodec::Pcma as u8 {
            G711::alaw_encode_slice(&samples)
        } else if codec_val == AudioCodec::Pcmu as u8 {
            G711::ulaw_encode_slice(&samples)
        } else {
            return true;
        };

        tracing::debug!(
            "[LinearPcmToG711] in_samples={}, resampled(6:1)={}, encoded={}",
            input_samples.len(),
            samples.len(),
            encoded.len()
        );

        ctx.data
            .insert(DataType::Audio, BytesMut::from(encoded.as_slice()));
        ctx.data.remove(&DataType::OrgData);
        if next_index < chain.len() {
            return chain[next_index].process(ctx, chain, next_index + 1);
        }
        true
    }
}

pub struct RtpSendMiddleware {
    name: String,
    ssrc: u32,
    seq_num: AtomicU16,
    timestamp: AtomicU32,
    payload_type: u8,
}

// impl RtpSendMiddleware {
//     pub fn new(ssrc: u32, payload_type: u8) -> Self {
//         Self {
//             name: "rtp_send".to_string(),
//             ssrc,
//             seq_num: AtomicU16::new(0),
//             timestamp: AtomicU32::new(0),
//             payload_type,
//         }
//     }

//     pub fn set_timestamp(&self, timestamp: u32) {
//         self.timestamp.store(timestamp, Ordering::Relaxed);
//     }
// }

#[async_trait]
impl NodeMiddleware for RtpSendMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    fn as_middleware(&self) -> &dyn Any {
        self
    }

    fn process(
        &self,
        ctx: &mut NodeMediaContext,
        chain: &[Arc<dyn NodeMiddleware>],
        next_index: usize,
    ) -> bool {
        let input_data = match ctx.data.get(&DataType::Audio) {
            Some(d) => d.clone(),
            None => return true,
        };

        let seq = self.seq_num.fetch_add(1, Ordering::Relaxed);
        let ts = self
            .timestamp
            .fetch_add(input_data.len() as u32, Ordering::Relaxed);

        let header = RtpHeader {
            version: 2,
            padding_flag: 0,
            extension_flag: 0,
            csrc_count: 0,
            marker: 1,
            payload_type: self.payload_type,
            sequence_number: seq,
            timestamp: ts,
            ssrc: self.ssrc,
            csrcs: Vec::new(),
        };

        let mut header_bytes = match header.build() {
            Ok(h) => h,
            Err(_) => return true,
        };

        header_bytes.extend_from_slice(&input_data);
        ctx.data.insert(DataType::Audio, header_bytes);
        if next_index < chain.len() {
            return chain[next_index].process(ctx, chain, next_index + 1);
        }
        true
    }
}

pub struct SendToReceiversMiddleware {
    name: String,
    node: Weak<dyn MediaNode>,
    target_type: MediaNodeType,
}

impl SendToReceiversMiddleware {
    pub fn new(node: Weak<dyn MediaNode>, target_type: MediaNodeType) -> Self {
        Self {
            name: format!("send_to_receivers_{}", target_type.as_str()),
            node,
            target_type,
        }
    }
}

impl NodeMiddleware for SendToReceiversMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    fn as_middleware(&self) -> &dyn Any {
        self
    }

    fn process(
        &self,
        ctx: &mut NodeMediaContext,
        chain: &[Arc<dyn NodeMiddleware>],
        next_index: usize,
    ) -> bool {
        let node = match self.node.upgrade() {
            Some(n) => n,
            None => return true,
        };

        ctx.target_type = self.target_type;

        let receivers = node.receivers();
        let node_type = node.node_type();

        if let Some(recv_list) = receivers.get(&self.target_type) {
            let mut ordered: Vec<(DataType, BytesMut)> = Vec::with_capacity(3);
            for dt in [DataType::Video, DataType::Audio, DataType::Rtcp] {
                if let Some(d) = ctx.data.get(&dt) {
                    ordered.push((dt, d.clone()));
                }
            }

            if !ordered.is_empty() {
                for receiver in recv_list.iter() {
                    if let Some(rx_node) = receiver.upgrade() {
                        let ordered = ordered.clone();
                        let rx_node = rx_node.clone();
                        tokio::spawn(async move {
                            for (data_type, data) in ordered {
                                if let Err(e) = rx_node
                                    .send_data_to_remote(&data, node_type, data_type)
                                    .await
                                {
                                    tracing::warn!(
                                        "[SendToReceiversMiddleware] send_data_to_remote failed: {}",
                                        e
                                    );
                                }
                            }
                        });
                    }
                }
            }
        }
        if next_index < chain.len() {
            return chain[next_index].process(ctx, chain, next_index + 1);
        }
        true
    }
}

pub type SendChainsMap = Arc<DashMap<MediaNodeType, Vec<Arc<dyn NodeMiddleware>>>>;
pub type ReceiverListMap = Arc<DashMap<MediaNodeType, Vec<Weak<dyn MediaNode>>>>;
pub type PostProcessChain = Arc<RwLock<Vec<Arc<dyn NodeMiddleware>>>>;

#[async_trait]
pub trait MediaNode: MediaTransportCallback + Send + Sync {
    fn id(&self) -> &MediaNodeId;
    fn node_type(&self) -> MediaNodeType;

    fn send_chains(&self) -> SendChainsMap;
    fn receivers(&self) -> ReceiverListMap;
    fn post_process_chain(&self) -> PostProcessChain;

    fn datatransport(&self) -> Arc<dyn MediaTransport>;

    async fn send_data_to_remote(
        &self,
        data: &[u8],
        from_type: MediaNodeType,
        data_type: DataType,
    ) -> Result<()>;

    async fn sendto_receivers(&self, data: HashMap<DataType, BytesMut>) -> Result<()>;

    async fn recv(&self) -> Result<()>;

    async fn run(self: Arc<Self>);

    fn add_receiver(&self, target_type: MediaNodeType, receiver: Weak<dyn MediaNode>);
    fn remove_receiver(&self, target_type: MediaNodeType, node_id: &str);

    fn add_send_chain(&self, target_type: MediaNodeType, mw: Box<dyn NodeMiddleware + 'static>);

    async fn add_post_process(&self, mw: Box<dyn NodeMiddleware + 'static>);

    async fn stop(&self) -> Result<()>;

    fn as_any(&self) -> &dyn Any;
}

pub struct BasicMediaNode {
    id: MediaNodeId,
    send_chains: SendChainsMap,
    receivers: ReceiverListMap,
    post_process_chain: PostProcessChain,
    datatransport: Arc<dyn MediaTransport>,
    ref_count: AtomicU32,
}

#[async_trait]
impl MediaTransportCallback for BasicMediaNode {
    async fn on_media_data(&self, data: HashMap<DataType, BytesMut>) {
        tracing::debug!("[MediaNode] on_media_data called, data.len={}", data.len());
        let _ = self.sendto_receivers(data).await;
    }
}

impl BasicMediaNode {
    pub fn new(id: MediaNodeId, datatransport: Arc<dyn MediaTransport>) -> Self {
        Self {
            id,
            send_chains: Arc::new(DashMap::new()),
            receivers: Arc::new(DashMap::new()),
            post_process_chain: Arc::new(RwLock::new(Vec::new())),
            datatransport,
            ref_count: AtomicU32::new(0),
        }
    }

    pub fn inc_ref(&self) {
        self.ref_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn dec_ref(&self) -> bool {
        self.ref_count.fetch_sub(1, Ordering::SeqCst) == 1
    }

    pub fn ref_count(&self) -> u32 {
        self.ref_count.load(Ordering::SeqCst)
    }

    pub fn add_send_chain(
        &self,
        target_type: MediaNodeType,
        mw: Box<dyn NodeMiddleware + 'static>,
    ) {
        let mut chain = self.send_chains.entry(target_type).or_default();
        let arc_mw: Arc<dyn NodeMiddleware> = mw.into();
        chain.push(arc_mw);
    }

    pub fn add_receiver(&self, target_type: MediaNodeType, receiver: Weak<dyn MediaNode>) {
        let mut list = self.receivers.entry(target_type).or_default();
        list.push(receiver);
    }

    pub fn remove_receiver(&self, target_type: MediaNodeType, node_id: &str) {
        if let Some(mut list) = self.receivers.get_mut(&target_type) {
            list.retain(|r| {
                if let Some(n) = r.upgrade() {
                    n.id().as_str() != node_id
                } else {
                    true
                }
            });
        }
    }

    pub async fn stop(&self) -> Result<()> {
        tracing::info!("[BasicMediaNode] stop() called for node {}", self.id.id);
        self.datatransport.close().await
    }

    async fn do_send_data_to_remote(
        &self,
        data: &[u8],
        from_type: MediaNodeType,
        data_type: DataType,
    ) -> Result<()> {
        let post_chain = self.post_process_chain.read().await;
        if !post_chain.is_empty() {
            let mut data_map = HashMap::new();
            data_map.insert(data_type, BytesMut::from(data));
            let mut ctx = NodeMediaContext::new(data_map);
            ctx.target_type = from_type;
            post_chain[0].process(&mut ctx, &post_chain, 1);
            Ok(())
        } else {
            self.datatransport.send(data, data_type).await
        }
    }

    async fn do_recv_once(&self) -> Result<()> {
        let data = self.datatransport.recv().await?;
        self.on_media_data(data).await;
        Ok(())
    }
}

#[async_trait]
impl MediaNode for BasicMediaNode {
    fn id(&self) -> &MediaNodeId {
        &self.id
    }

    fn node_type(&self) -> MediaNodeType {
        self.id.node_type
    }

    fn send_chains(&self) -> SendChainsMap {
        self.send_chains.clone()
    }

    fn receivers(&self) -> ReceiverListMap {
        self.receivers.clone()
    }

    fn post_process_chain(&self) -> PostProcessChain {
        self.post_process_chain.clone()
    }

    fn datatransport(&self) -> Arc<dyn MediaTransport> {
        self.datatransport.clone()
    }

    async fn add_post_process(&self, mw: Box<dyn NodeMiddleware + 'static>) {
        let mut chain = self.post_process_chain.write().await;
        let arc_mw: Arc<dyn NodeMiddleware> = mw.into();
        chain.push(arc_mw);
    }

    async fn send_data_to_remote(
        &self,
        data: &[u8],
        from_type: MediaNodeType,
        data_type: DataType,
    ) -> Result<()> {
        self.do_send_data_to_remote(data, from_type, data_type)
            .await
    }

    async fn sendto_receivers(&self, data: HashMap<DataType, BytesMut>) -> Result<()> {
        let chains_snapshot: Vec<(MediaNodeType, Vec<Arc<dyn NodeMiddleware>>)> = self
            .send_chains
            .iter()
            .map(|item| (*item.key(), item.value().clone()))
            .collect();

        for (target_type, chain) in chains_snapshot {
            if !chain.is_empty() {
                let mut ctx = NodeMediaContext::new(data.clone());
                ctx.target_type = target_type;
                chain[0].process(&mut ctx, &chain, 1);
            }
        }
        Ok(())
    }

    async fn recv(&self) -> Result<()> {
        self.do_recv_once().await
    }

    fn add_receiver(&self, target_type: MediaNodeType, receiver: Weak<dyn MediaNode>) {
        BasicMediaNode::add_receiver(self, target_type, receiver);
    }

    fn remove_receiver(&self, target_type: MediaNodeType, node_id: &str) {
        BasicMediaNode::remove_receiver(self, target_type, node_id);
    }

    fn add_send_chain(&self, target_type: MediaNodeType, mw: Box<dyn NodeMiddleware + 'static>) {
        BasicMediaNode::add_send_chain(self, target_type, mw);
    }

    async fn run(self: Arc<Self>) {
        tracing::debug!(
            "[MediaNode] run() no-op (transport handles recv internally), node={}",
            self.id.as_str()
        );
    }

    async fn stop(&self) -> Result<()> {
        BasicMediaNode::stop(self).await
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct MediaSession {
    pub id: String,
    nodes: Arc<DashMap<MediaNodeId, Arc<dyn MediaNode>>>,
}

impl MediaSession {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            nodes: Arc::new(DashMap::new()),
        }
    }

    pub fn get_broadcast_nodes(&self) -> Vec<Arc<dyn MediaNode>> {
        self.nodes
            .iter()
            .filter(|n| n.value().node_type() == MediaNodeType::GB28181_BROADCAST_VOICE)
            .map(|n| n.value().clone())
            .collect()
    }

    pub fn get_broadcast_node(&self, _channel_tag: &str) -> Option<Arc<dyn MediaNode>> {
        self.nodes
            .iter()
            .find(|n| n.value().node_type() == MediaNodeType::GB28181_BROADCAST_VOICE)
            .map(|n| n.value().clone())
    }

    pub fn has_broadcast_session(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| n.value().node_type() == MediaNodeType::GB28181_BROADCAST_VOICE)
    }

    pub fn start_broadcast(&self, channel_tag: &str, node: Arc<dyn MediaNode>) {
        let node_id = node.id().clone();
        self.nodes.insert(node_id.clone(), node.clone());
        if let Some(bn) = node.as_any().downcast_ref::<BasicMediaNode>() {
            bn.inc_ref();
            tracing::info!(
                "[MediaSession] broadcast {} session created, node_id={}",
                channel_tag,
                node_id.id
            );
        }
    }

    pub async fn end_broadcast(&self, channel_tag: &str) -> bool {
        let node_id = self
            .nodes
            .iter()
            .find(|n| n.value().node_type() == MediaNodeType::GB28181_BROADCAST_VOICE)
            .map(|n| n.key().clone());

        if let Some(node_id) = node_id {
            if let Some((_, removed)) = self.nodes.remove(&node_id) {
                tracing::info!("[MediaSession] broadcast {} session removed", channel_tag);
                if let Err(e) = removed.stop().await {
                    tracing::warn!("[MediaSession] Failed to stop broadcast node: {}", e);
                }
                return true;
            }
        }
        false
    }

    pub async fn end_all_broadcasts(&self) -> usize {
        tracing::info!("[MediaSession] end_all_broadcasts called, checking {} nodes", self.nodes.len());
        let broadcast_node_ids: Vec<MediaNodeId> = self
            .nodes
            .iter()
            .filter(|n| n.value().node_type() == MediaNodeType::GB28181_BROADCAST_VOICE)
            .map(|n| n.key().clone())
            .collect();

        tracing::info!("[MediaSession] found {} broadcast nodes to remove", broadcast_node_ids.len());

        let mut removed = 0;
        for node_id in &broadcast_node_ids {
            if let Some((_, removed_node)) = self.nodes.remove(node_id) {
                tracing::info!("[MediaSession] broadcast node {} removed, calling stop()", node_id.id);
                if let Err(e) = removed_node.stop().await {
                    tracing::warn!("[MediaSession] Failed to stop broadcast node: {}", e);
                }
                removed += 1;
            }
        }
        removed
    }

    pub fn add_node(&self, node: Arc<dyn MediaNode>) -> Result<()> {
        let node_id = node.id().clone();
        let new_type = node.node_type();

        for item in self.nodes.iter() {
            let existing_node = item.value();
            let existing_type = existing_node.node_type();

            if let Some(chain) = node.send_chains().get(&existing_type) {
                if !chain.is_empty() {
                    node.add_receiver(existing_type, Arc::downgrade(existing_node));
                }
            }

            if let Some(chain) = existing_node.send_chains().get(&new_type) {
                if !chain.is_empty() {
                    existing_node.add_receiver(new_type, Arc::downgrade(&node));
                }
            }
        }

        self.nodes.insert(node_id.clone(), node);
        Ok(())
    }

    pub fn remove_node(&self, node_id: &MediaNodeId) -> Option<Arc<dyn MediaNode>> {
        if let Some((_, node)) = self.nodes.remove(node_id) {
            let node_id_str = node.id().as_str();
            for item in self.nodes.iter() {
                let existing_node = item.value();
                let keys: Vec<MediaNodeType> =
                    existing_node.receivers().iter().map(|i| *i.key()).collect();
                for key in keys {
                    existing_node.remove_receiver(key, &node_id_str);
                }
            }
            Some(node)
        } else {
            None
        }
    }

    pub async fn remove_and_stop_node(&self, node_id: &MediaNodeId) -> bool {
        if let Some(node) = self.remove_node(node_id) {
            tracing::info!("[MediaSession] remove_and_stop_node: {} removed, calling stop()", node_id.id);
            if let Err(e) = node.stop().await {
                tracing::warn!("[MediaSession] Failed to stop node {}: {}", node_id.id, e);
            }
            true
        } else {
            tracing::warn!("[MediaSession] remove_and_stop_node: node {} not found", node_id.id);
            false
        }
    }

    pub fn get_node(&self, node_id: &MediaNodeId) -> Option<Arc<dyn MediaNode>> {
        self.nodes.get(node_id).map(|r| r.value().clone())
    }

    pub fn nodes_iter(&self) -> impl Iterator<Item = Arc<dyn MediaNode>> + '_ {
        self.nodes.iter().map(|r| r.value().clone())
    }

    pub fn clear_nodes(&self) {
        self.nodes.retain(|_, _| false);
        tracing::debug!("[MediaSession] All nodes cleared for session {}", self.id);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaSessionKey {
    pub device_tag: String,
}

impl MediaSessionKey {
    pub fn new(device_tag: &str) -> Self {
        Self {
            device_tag: device_tag.to_string(),
        }
    }
}

impl std::fmt::Display for MediaSessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.device_tag)
    }
}

pub struct MediaSessionManager {
    sessions: Arc<DashMap<MediaSessionKey, Arc<MediaSession>>>,
}

impl MediaSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }

    pub fn get_or_create(&self, device_tag: &str) -> Arc<MediaSession> {
        let key = MediaSessionKey::new(device_tag);

        match self.sessions.entry(key) {
            Entry::Occupied(e) => e.get().clone(),
            Entry::Vacant(e) => {
                let session = Arc::new(MediaSession::new(device_tag));
                e.insert(session.clone());
                session
            }
        }
    }

    pub fn get(&self, device_tag: &str) -> Option<Arc<MediaSession>> {
        let key = MediaSessionKey::new(device_tag);
        self.sessions.get(&key).map(|r| r.value().clone())
    }

    pub fn remove(&self, device_tag: &str) -> Option<Arc<MediaSession>> {
        let key = MediaSessionKey::new(device_tag);
        self.sessions.remove(&key).map(|(_, session)| session)
    }

    pub fn session_keys(&self) -> Vec<MediaSessionKey> {
        self.sessions.iter().map(|r| r.key().clone()).collect()
    }

    pub fn sessions_count(&self) -> usize {
        self.sessions.len()
    }
}

impl Default for MediaSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

static MEDIA_SESSION_MANAGER: OnceCell<Arc<MediaSessionManager>> = OnceCell::new();

pub async fn init_media_session_manager() {
    let manager = Arc::new(MediaSessionManager::new());
    MEDIA_SESSION_MANAGER.set(manager.clone()).ok();
    tracing::info!("[MediaSessionManager] Initialized");
}

pub fn get_media_session_manager() -> Option<&'static Arc<MediaSessionManager>> {
    MEDIA_SESSION_MANAGER.get()
}
