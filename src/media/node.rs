use std::net::SocketAddr;
use std::sync::Arc;
use std::any::Any;
use dashmap::DashMap;
use async_trait::async_trait;
use bytes::BytesMut;
use once_cell::sync::OnceCell;

use crate::media::errors::{MediaError, Result};
use crate::media::codec::AudioCodec;
use crate::media::codec::G711;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaNodeType {
    GB28181,
    WebSocket,
    RTSP,
    RTMP,
    RTMPPush,
    UDP,
    TCP,
    Unknown,
}

impl MediaNodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaNodeType::GB28181 => "gb28181",
            MediaNodeType::WebSocket => "websocket",
            MediaNodeType::RTSP => "rtsp",
            MediaNodeType::RTMP => "rtmp",
            MediaNodeType::RTMPPush => "rtmp_push",
            MediaNodeType::UDP => "udp",
            MediaNodeType::TCP => "tcp",
            MediaNodeType::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for MediaNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct MediaNodeId {
    pub node_type: MediaNodeType,
    pub device_id: String,
    pub channel_id: Option<String>,
}

impl MediaNodeId {
    pub fn new(node_type: MediaNodeType, device_id: &str) -> Self {
        Self {
            node_type,
            device_id: device_id.to_string(),
            channel_id: None,
        }
    }
    
    pub fn with_channel(mut self, channel_id: &str) -> Self {
        self.channel_id = Some(channel_id.to_string());
        self
    }
    
    pub fn as_str(&self) -> String {
        match &self.channel_id {
            Some(ch) => format!("{}:{}/{}", self.node_type, self.device_id, ch),
            None => format!("{}:{}", self.node_type, self.device_id),
        }
    }
}

#[async_trait]
pub trait MediaTransport: Send + Sync {
    fn local_addr(&self) -> Result<SocketAddr>;
    fn peer_addr(&self) -> Result<SocketAddr>;
    async fn send(&self, data: &[u8]) -> Result<()>;
    async fn recv(&self) -> Result<BytesMut>;
    async fn close(&self) -> Result<()>;
}

#[async_trait]
pub trait NodeMiddleware: Send + Sync {
    fn name(&self) -> &str;
    async fn process(&self, data: &mut BytesMut) -> bool;
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

#[async_trait]
impl NodeMiddleware for BasicMiddleware {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn process(&self, data: &mut BytesMut) -> bool {
        (self.func)(data)
    }
}

use std::sync::atomic::{AtomicU8, Ordering};

pub struct TalkNodeMiddleware {
    name: String,
    codec: AtomicU8,
}

impl TalkNodeMiddleware {
    pub fn new(codec: AudioCodec) -> Self {
        Self {
            name: "talk".to_string(),
            codec: AtomicU8::new(codec as u8),
        }
    }
    
    pub fn set_codec(&self, codec: AudioCodec) {
        self.codec.store(codec as u8, Ordering::Relaxed);
    }
}

#[async_trait]
impl NodeMiddleware for TalkNodeMiddleware {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn process(&self, data: &mut BytesMut) -> bool {
        let codec_val = self.codec.load(Ordering::Relaxed);
        let samples = if codec_val == AudioCodec::Pcma as u8 {
            G711::alaw_decode_slice(data)
        } else if codec_val == AudioCodec::Pcmu as u8 {
            G711::ulaw_decode_slice(data)
        } else {
            return true;
        };
        
        let mut pcm_data = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            pcm_data.extend_from_slice(&sample.to_le_bytes());
        }
        *data = BytesMut::from(pcm_data.as_slice());
        false
    }
}

pub struct SendChainNodeMiddleware {
    name: String,
    codec: AtomicU8,
}

impl SendChainNodeMiddleware {
    pub fn new(codec: AudioCodec) -> Self {
        Self {
            name: "send_chain".to_string(),
            codec: AtomicU8::new(codec as u8),
        }
    }
    
    pub fn set_codec(&self, codec: AudioCodec) {
        self.codec.store(codec as u8, Ordering::Relaxed);
    }
}

#[async_trait]
impl NodeMiddleware for SendChainNodeMiddleware {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn process(&self, data: &mut BytesMut) -> bool {
        let samples: Vec<i16> = data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        
        let codec_val = self.codec.load(Ordering::Relaxed);
        let encoded = if codec_val == AudioCodec::Pcma as u8 {
            G711::alaw_encode_slice(&samples)
        } else if codec_val == AudioCodec::Pcmu as u8 {
            G711::ulaw_encode_slice(&samples)
        } else {
            return true;
        };
        
        *data = BytesMut::from(encoded.as_slice());
        false
    }
}

pub type SendChainsMap = Arc<DashMap<MediaNodeType, Vec<Box<dyn NodeMiddleware + 'static>>>>;
pub type ReceiverListMap = Arc<DashMap<MediaNodeType, Vec<Weak<dyn MediaNode>>>>;
pub type PostProcessChain = Arc<RwLock<Vec<Box<dyn NodeMiddleware + 'static>>>>;

#[async_trait]
pub trait MediaNode: Send + Sync {
    fn id(&self) -> &MediaNodeId;
    fn node_type(&self) -> MediaNodeType;
    
    fn send_chains(&self) -> SendChainsMap;
    fn receivers(&self) -> ReceiverListMap;
    fn post_process_chain(&self) -> PostProcessChain;
    fn recv_chain(&self) -> Arc<RwLock<Vec<Box<dyn NodeMiddleware + 'static>>>>;
    
    fn datatransport(&self) -> Arc<dyn MediaTransport>;
    
    async fn send_data_to_remote(&self, data: &[u8], from_type: MediaNodeType) -> Result<()>;
    
    async fn send(&self, data: &[u8]) -> Result<()>;
    
    async fn recv(&self) -> Result<BytesMut>;
    
    fn add_receiver(&self, target_type: MediaNodeType, receiver: Weak<dyn MediaNode>);
    fn remove_receiver(&self, target_type: MediaNodeType, node_id: &str);
    
    fn add_send_chain(&self, target_type: MediaNodeType, mw: Box<dyn NodeMiddleware + 'static>);
    async fn add_recv_chain(&self, mw: Box<dyn NodeMiddleware + 'static>);
    
    async fn stop(&self) -> Result<()>;
    
    fn as_any(&self) -> &dyn Any;
}

use std::sync::Weak;
use tokio::sync::RwLock;

pub struct BasicMediaNode {
    id: MediaNodeId,
    send_chains: SendChainsMap,
    receivers: ReceiverListMap,
    post_process_chain: PostProcessChain,
    recv_chain: Arc<RwLock<Vec<Box<dyn NodeMiddleware + 'static>>>>,
    datatransport: Arc<dyn MediaTransport>,
}

impl BasicMediaNode {
    pub fn new(
        id: MediaNodeId,
        datatransport: Arc<dyn MediaTransport>,
    ) -> Self {
        Self {
            id,
            send_chains: Arc::new(DashMap::new()),
            receivers: Arc::new(DashMap::new()),
            post_process_chain: Arc::new(RwLock::new(Vec::new())),
            recv_chain: Arc::new(RwLock::new(Vec::new())),
            datatransport,
        }
    }
    
    pub fn add_send_chain(&self, target_type: MediaNodeType, mw: Box<dyn NodeMiddleware + 'static>) {
        let mut chain = self.send_chains.entry(target_type).or_default();
        chain.push(mw);
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
    
    pub async fn add_post_process(&self, mw: Box<dyn NodeMiddleware + 'static>) {
        let mut chain = self.post_process_chain.write().await;
        chain.push(mw);
    }
    
    pub async fn add_recv_chain(&self, mw: Box<dyn NodeMiddleware + 'static>) {
        let mut chain = self.recv_chain.write().await;
        chain.push(mw);
    }
    
    pub async fn stop(&self) -> Result<()> {
        self.datatransport.close().await
    }
    
    pub async fn recv(&self) -> Result<BytesMut> {
        let mut data = self.datatransport.recv().await?;
        
        {
            let chain = self.recv_chain.read().await;
            for mw in chain.iter() {
                if !mw.process(&mut data).await {
                    return Ok(data);
                }
            }
        }
        
        Ok(data)
    }
    
    async fn do_send_data_to_remote(&self, data: &[u8], from_type: MediaNodeType) -> Result<()> {
        let mut data = BytesMut::from(data);
        
        if let Some(chain) = self.send_chains.get(&from_type) {
            for mw in chain.iter() {
                if !mw.process(&mut data).await {
                    return Ok(());
                }
            }
        }
        
        let chain = self.post_process_chain.read().await;
        for mw in chain.iter() {
            if !mw.process(&mut data).await {
                return Ok(());
            }
        }
        
        self.datatransport.send(&data).await
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
    
    fn recv_chain(&self) -> Arc<RwLock<Vec<Box<dyn NodeMiddleware + 'static>>>> {
        self.recv_chain.clone()
    }
    
    fn datatransport(&self) -> Arc<dyn MediaTransport> {
        self.datatransport.clone()
    }
    
    async fn send_data_to_remote(&self, data: &[u8], from_type: MediaNodeType) -> Result<()> {
        self.do_send_data_to_remote(data, from_type).await
    }
    
    async fn send(&self, data: &[u8]) -> Result<()> {
        let chains = self.send_chains.clone();
        let receivers = self.receivers.clone();
        let node_type = self.node_type();
        
        for item in chains.iter() {
            let target_type = *item.key();
            if let Some(recv_list) = receivers.get(&target_type) {
                for receiver in recv_list.iter() {
                    if let Some(node) = receiver.upgrade() {
                        let data = data.to_vec();
                        let node = node.clone();
                        tokio::spawn(async move {
                            if let Err(e) = node.send_data_to_remote(&data, node_type).await {
                                tracing::warn!("[MediaNode] send_data_to_remote failed: {}", e);
                            }
                        });
                    }
                }
            }
        }
        Ok(())
    }
    
    async fn recv(&self) -> Result<BytesMut> {
        BasicMediaNode::recv(self).await
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
    
    async fn add_recv_chain(&self, mw: Box<dyn NodeMiddleware + 'static>) {
        BasicMediaNode::add_recv_chain(self, mw).await;
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
    nodes: Arc<DashMap<String, Arc<dyn MediaNode>>>,
}

impl MediaSession {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            nodes: Arc::new(DashMap::new()),
        }
    }
    
    pub fn add_node(&self, node: Arc<dyn MediaNode>) -> Result<()> {
        let node_id = node.id().as_str();
        let new_type = node.node_type();
        
        for item in self.nodes.iter() {
            let existing_node = item.value();
            let existing_type = existing_node.node_type();
            
            if let Some(chain) = existing_node.send_chains().get(&new_type) {
                if !chain.is_empty() {
                    existing_node.add_receiver(new_type, Arc::downgrade(&node));
                    node.add_receiver(existing_type, Arc::downgrade(existing_node));
                }
            }
            
            if let Some(chain) = node.send_chains().get(&existing_type) {
                if !chain.is_empty() {
                    node.add_receiver(existing_type, Arc::downgrade(existing_node));
                    existing_node.add_receiver(new_type, Arc::downgrade(&node));
                }
            }
        }
        
        self.nodes.insert(node_id.to_string(), node);
        Ok(())
    }
    
    pub fn remove_node(&self, node_id: &str) -> Option<Arc<dyn MediaNode>> {
        if let Some((_, node)) = self.nodes.remove(node_id) {
            let node_type = node.node_type();
            for item in self.nodes.iter() {
                let existing_node = item.value();
                let existing_type = existing_node.node_type();
                existing_node.remove_receiver(node_type, node_id);
                existing_node.remove_receiver(existing_type, node_id);
            }
            Some(node)
        } else {
            None
        }
    }
    
    pub fn get_node(&self, node_id: &str) -> Option<Arc<dyn MediaNode>> {
        self.nodes.get(node_id).map(|r| r.value().clone())
    }
    
    pub fn nodes_iter(&self) -> impl Iterator<Item = Arc<dyn MediaNode>> + '_ {
        self.nodes.iter().map(|r| r.value().clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaSessionKey {
    pub device_tag: String,
    pub channel_tag: String,
}

impl MediaSessionKey {
    pub fn new(device_tag: &str, channel_tag: &str) -> Self {
        Self {
            device_tag: device_tag.to_string(),
            channel_tag: channel_tag.to_string(),
        }
    }
}

impl std::fmt::Display for MediaSessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}", self.device_tag, self.channel_tag)
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
    
    pub fn get_or_create(&self, device_tag: &str, channel_tag: &str) -> Arc<MediaSession> {
        let key = MediaSessionKey::new(device_tag, channel_tag);
        
        if let Some(session) = self.sessions.get(&key) {
            return session.value().clone();
        }
        
        let session = Arc::new(MediaSession::new(&format!("{}_{}", device_tag, channel_tag)));
        self.sessions.insert(key, session.clone());
        session
    }
    
    pub fn get(&self, device_tag: &str, channel_tag: &str) -> Option<Arc<MediaSession>> {
        let key = MediaSessionKey::new(device_tag, channel_tag);
        self.sessions.get(&key).map(|r| r.value().clone())
    }
    
    pub fn remove(&self, device_tag: &str, channel_tag: &str) -> Option<Arc<MediaSession>> {
        let key = MediaSessionKey::new(device_tag, channel_tag);
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
