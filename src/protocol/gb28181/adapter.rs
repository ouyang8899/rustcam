use async_trait::async_trait;
use bytes::BytesMut;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;

use chrono::{DateTime, Utc};
use roxmltree::Document;

/// GB28181 播放错误类型
#[derive(Debug, Clone)]
pub enum PlaybackError {
    DeviceOffline,
    DeviceHasNoRecording,
    DeviceNotFound,
    AuthenticationFailed,
    ServerError(String),
    Busy,
    Timeout,
    Unknown(String),
}

impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaybackError::DeviceOffline => write!(f, "设备不在线"),
            PlaybackError::DeviceHasNoRecording => write!(f, "设备在请求的时间段内没有录像"),
            PlaybackError::DeviceNotFound => write!(f, "设备不存在"),
            PlaybackError::AuthenticationFailed => write!(f, "认证失败"),
            PlaybackError::ServerError(s) => write!(f, "服务器错误: {}", s),
            PlaybackError::Busy => write!(f, "设备忙"),
            PlaybackError::Timeout => write!(f, "请求超时"),
            PlaybackError::Unknown(s) => write!(f, "未知错误: {}", s),
        }
    }
}

use crate::common_types::TransportType;
use crate::context::registry;
use crate::media::node::{
    BasicMediaNode, DataType, MediaNode, MediaNodeId, MediaNodeType, RtpSendMiddleware,
    SendToReceiversMiddleware,
};
use crate::media::DataTransport;
use crate::media::AudioCodec;
use crate::protocol::event::{CatalogChannel, EventHandler, ProtocolType, PtzCommand, SignalEvent};
use crate::domain::SessionType;

use crate::adapter::media_server::RtpTransport;
use crate::error::{AppError, Result};
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::gb28181::auth::{
    consume_nonce, generate_nonce, parse_sip_authorization, verify_nonce, verify_sip_digest,
    VerifyResult,
};
use crate::protocol::gb28181::sip::{SipMessage, SipMessageBuffer, SipMethod, SipNameAddr, SipUri};
use crate::protocol::traits::ProtocolDeps;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gb28181Version {
    Gb2016,
    Gb2011,
}

impl Default for Gb28181Version {
    fn default() -> Self {
        Gb28181Version::Gb2016
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegistrationState {
    Init,        // 初始化
    Registered,  // 注册完成，等待发送 catalog
    CatalogSent, // 已发送 catalog，等待响应
    Failed,      // 失败状态，需要重置
}

impl Default for RegistrationState {
    fn default() -> Self {
        RegistrationState::Init
    }
}

#[derive(Debug, Clone)]
pub struct WaitTimer {
    pub start_at: DateTime<Utc>,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub retry_count: u32,
}

impl WaitTimer {
    pub fn new(timeout_secs: u64, max_retries: u32) -> Self {
        Self {
            start_at: Utc::now(),
            timeout_secs,
            max_retries,
            retry_count: 0,
        }
    }

    pub fn is_timed_out(&self) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.start_at);
        elapsed.num_seconds() as u64 >= self.timeout_secs
    }

    pub fn should_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    pub fn reset(&mut self) {
        self.start_at = Utc::now();
        self.retry_count = 0;
    }

    pub fn reset_with_time(&mut self) {
        self.start_at = Utc::now();
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

#[derive(Debug, Clone)]
pub struct RegistrationTimeout {
    pub catalog_secs: u64,
    pub invite_secs: u64,
    pub streaming_secs: u64,
    pub max_retries: u32,
}

impl Default for RegistrationTimeout {
    fn default() -> Self {
        Self {
            catalog_secs: 10,
            invite_secs: 30,
            streaming_secs: 60,
            max_retries: 3,
        }
    }
}

type GbWriteHalf = Arc<tokio::sync::RwLock<OwnedWriteHalf>>;

#[derive(Clone, PartialEq)]
pub enum SubSessionState {
    Pending,
    Ok,
    Failed,
}

#[derive(Clone)]
pub struct SubSession {
    pub call_id: String,
    pub session_type: SessionType,
    pub device_tag: String,
    pub seq: u32,
    pub channel_id: String,
    pub stream_key: String,
    pub media_server_name: String,
    /// SIP From URI - 格式: 平台ID@本地IP:端口 (无sip:前缀)
    /// 例如: 31011500001000000001@192.168.1.7:5061
    pub from_uri: String,
    /// SIP To URI - 格式: 设备ID@设备IP:设备端口 (无sip:前缀)
    /// 例如: 12010100001320000011@192.168.1.211:5060
    pub to_uri: String,
    pub from_tag: String,
    pub to_tag: Option<String>,
    pub via_branch: String,
    /// 媒体服务器IP (来自alloc_rtp_port返回的server_ip)
    pub server_ip: String,
    pub server_port: u16,
    pub transport: TransportType,
    pub device_host: String,
    pub device_port: u16,
    pub rtp_port: u16,
    pub state: SubSessionState,
    pub retry_count: u32,
    pub max_retries: u32,
    pub device_nated_ip: Option<String>,
    pub device_nated_port: Option<u16>,
    pub node_id: Option<MediaNodeId>,
}

pub struct PendingAudioTalkEntry {
    pub node_id: MediaNodeId,
    pub call_id: String,
    pub from_tag: String,
    pub to_tag: String,
    pub device_host: String,
    pub device_port: u16,
    pub transport: TransportType,
}

#[derive(Clone)]
pub struct Gb28181Adapter {
    pub version: Gb28181Version,
    pub device_tag: Option<String>,
    pub from_uri: Option<String>,
    pub device_id: Option<i64>,
    pub device_name: Option<String>,
    pub seq: Arc<AtomicU32>,
    pub remote_addr: Option<SocketAddr>,
    pub local_addr: Option<SocketAddr>,
    pub recv_buffer: SipMessageBuffer,
    pub write: Option<GbWriteHalf>,
    pub max_buffer_size: usize,
    pub transport: TransportType,
    pub call_id: Option<String>,
    pub from_tag: Option<String>,
    pub to_tag: Option<String>,
    pub udp_peer: Option<SocketAddr>,
    pub registration_expires: Option<u64>,
    pub deps: Arc<ProtocolDeps>,
    pub sessions: Arc<tokio::sync::RwLock<Vec<SubSession>>>,
    pub subscriptions: Arc<tokio::sync::RwLock<HashMap<String, Subscription>>>,
    pub pending_audio_talks:
        Arc<tokio::sync::RwLock<Vec<PendingAudioTalkEntry>>>,
    pub registration_state: RegistrationState,
    pub wait_timer: Option<WaitTimer>,
    pub event_handler_name: Option<String>,
    pub register_fn:
        Arc<dyn Fn(String, crate::protocol::adapter_manager::AdapterEntry) + Send + Sync>,
    pub unregister_fn: Arc<dyn Fn(String) + Send + Sync>,
    pub last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    pub adapter_entry: Option<crate::protocol::adapter_manager::AdapterEntry>,
    pub waiting_for_reregister: bool,
    pub pending_tcp_active: Arc<tokio::sync::RwLock<HashMap<String, (
        Arc<crate::media::GB28181BroadcastVoiceTransport>,
        Arc<dyn crate::media::MediaNode>
    )>>>,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub event_type: String,
    pub expires: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupMode {
    Udp,
    TcpPassive,
    TcpActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BroadcastDirection {
    PlatformToDevice,
    DeviceToPlatform,
}

impl Gb28181Adapter {
    async fn get_device_app(&self, channel_id: &str) -> Option<String> {
        self.deps
            .device_lookup
            .find_by_tag(channel_id)
            .await
            .and_then(|d| d.app)
    }

    pub fn set_local_addr(&mut self, addr: SocketAddr) {
        self.local_addr = Some(addr);
    }

    pub fn new(
        deps: ProtocolDeps,
        register_fn: Arc<
            dyn Fn(String, crate::protocol::adapter_manager::AdapterEntry) + Send + Sync,
        >,
        unregister_fn: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        Self {
            version: Gb28181Version::default(),
            device_tag: None,
            from_uri: None,
            device_id: None,
            device_name: None,
            seq: Arc::new(AtomicU32::new(1)),
            remote_addr: None,
            local_addr: None,
            recv_buffer: SipMessageBuffer::new(),
            write: None,
            max_buffer_size: 1024 * 1024,
            transport: TransportType::Tcp,
            call_id: None,
            from_tag: None,
            to_tag: None,
            udp_peer: None,
            registration_expires: None,
            deps: Arc::new(deps),
            sessions: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            subscriptions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            pending_audio_talks: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            last_broadcast_session: None,
            registration_state: RegistrationState::Init,
            wait_timer: None,
            event_handler_name: None,
            register_fn,
            unregister_fn,
            last_message_at: None,
            adapter_entry: None,
            waiting_for_reregister: false,
            pending_tcp_active: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    pub fn registration_expires(&self) -> u64 {
        self.registration_expires.unwrap_or(3600)
    }

    pub fn set_registration_state(&mut self, state: RegistrationState) {
        if self.registration_state != state {
            tracing::info!(
                "[GB28181] Registration state transition: {:?} -> {:?}",
                self.registration_state,
                state
            );
            self.registration_state = state;
            self.wait_timer = None;
        }
    }

    pub fn start_wait_timer(&mut self, timeout_secs: u64, max_retries: u32) {
        self.wait_timer = Some(WaitTimer::new(timeout_secs, max_retries));
    }

    pub fn is_registration_timed_out(&self) -> bool {
        match &self.wait_timer {
            Some(timer) => timer.is_timed_out(),
            None => false,
        }
    }

    pub async fn handle_registration_timeout(&mut self) {
        let device_tag = self.device_tag_str();
        if device_tag.is_empty() {
            return;
        }

        match self.registration_state {
            RegistrationState::Init | RegistrationState::Failed => {
                return;
            }
            RegistrationState::Registered | RegistrationState::CatalogSent => {
                let timer = match &mut self.wait_timer {
                    Some(t) => t,
                    None => return,
                };
                if timer.should_retry() {
                    tracing::info!(
                        "[GB28181] Catalog timeout, retry {}/{}",
                        timer.retry_count + 1,
                        timer.max_retries
                    );
                    timer.increment_retry();
                    timer.reset_with_time();
                    let query = self.build_catalog_query();
                    let _ = self.send(&query).await;
                } else {
                    tracing::warn!("[GB28181] Catalog timeout, max retries reached");
                    self.set_registration_state(RegistrationState::Failed);
                }
            }
        }
    }

    pub fn needs_timeout_check(&self) -> bool {
        match &self.wait_timer {
            Some(timer) => timer.is_timed_out(),
            None => false,
        }
    }

    fn truncate_20(s: &str) -> String {
        if s.len() <= 20 {
            s.to_string()
        } else {
            s[..20].to_string()
        }
    }

    fn truncate_32(s: &str) -> String {
        if s.len() <= 32 {
            s.to_string()
        } else {
            s[..32].to_string()
        }
    }

    fn is_valid_gb28181_device_id(id: &str) -> bool {
        let len = id.len();
        if len < 10 || len > 20 {
            return false;
        }
        id.chars().all(|c| c.is_ascii_digit())
    }

    fn parse_ssrc_from_sdp(sdp: &str) -> Option<u32> {
        for line in sdp.lines() {
            let line = line.trim();
            if line.starts_with("y=") {
                let ssrc_str = &line[2..];
                return ssrc_str.parse::<u32>().ok();
            }
        }
        None
    }

    fn parse_audio_codec_from_sdp(sdp: &str) -> Option<(u8, String)> {
        let mut audio_pt = None;
        let mut rtpmap: std::collections::HashMap<u8, String> = std::collections::HashMap::new();

        for line in sdp.lines() {
            let line = line.trim();

            if line.starts_with("m=audio") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    for pt_str in parts[3..].iter() {
                        if let Ok(pt) = pt_str.parse::<u8>() {
                            audio_pt = Some(pt);
                            break;
                        }
                    }
                }
            } else if line.starts_with("a=rtpmap:") {
                let rest = &line[10..];
                if let Some((pt_fmt, rest)) = rest.split_once(' ') {
                    if let Ok(pt) = pt_fmt.parse::<u8>() {
                        let encoding = rest.split('/').next().unwrap_or("").to_uppercase();
                        rtpmap.insert(pt, encoding);
                    }
                }
            }
        }

        if let Some(pt) = audio_pt {
            if let Some(encoding) = rtpmap.get(&pt) {
                return Some((pt, encoding.clone()));
            }
        }

        None
    }

    fn audio_pt_to_codec(pt: u8) -> Option<&'static str> {
        match pt {
            0 => Some("PCMU"),
            8 => Some("PCMA"),
            9 => Some("G722"),
            18 => Some("G729"),
            97 => Some("AAC"),
            _ => None,
        }
    }

    fn parse_subject_device_id(subject: &str) -> Option<String> {
        let parts: Vec<&str> = subject.split(':').collect();
        if parts.len() >= 2 {
            let device_id = parts[0];
            if Self::is_valid_gb28181_device_id(device_id) {
                return Some(device_id.to_string());
            }
        }
        None
    }

    fn parse_sip_message(buffer: &[u8]) -> Option<(String, Vec<(String, String)>, String)> {
        let msg_str = String::from_utf8_lossy(buffer);
        let header_end = msg_str.find("\r\n\r\n")?;
        let header_section = &msg_str[..header_end];

        let mut headers = Vec::new();
        for line in header_section.lines() {
            if let Some((k, v)) = line.split_once(':') {
                headers.push((k.trim().to_lowercase(), v.trim().to_string()));
            }
        }

        let content_length = Self::get_header(&headers, "Content-Length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let body_start = header_end + 4;
        let total_len = body_start + content_length;
        if buffer.len() < total_len {
            return None;
        }

        let first_line = header_section.lines().next()?;
        let body = if content_length > 0 {
            &msg_str[body_start..total_len]
        } else {
            ""
        };

        Some((first_line.to_string(), headers, body.to_string()))
    }

    fn get_header(headers: &[(String, String)], name: &str) -> Option<String> {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }

    fn parse_sip_uri(uri: &str) -> String {
        let uri = uri.trim();
        let uri = uri
            .strip_prefix('<')
            .unwrap_or(uri)
            .trim_end_matches('>')
            .trim();
        let uri = uri
            .strip_prefix("sip:")
            .or_else(|| uri.strip_prefix("SIP:"))
            .unwrap_or(uri);
        uri.split('@').next().unwrap_or(uri).trim().to_string()
    }

    fn parse_sip_uri_with_domain(uri: &str) -> String {
        let uri = uri.trim();
        let uri = uri
            .strip_prefix('<')
            .unwrap_or(uri)
            .trim_end_matches('>')
            .trim();
        if uri.to_lowercase().starts_with("sip:") {
            uri.to_string()
        } else {
            format!("sip:{}", uri)
        }
    }

    fn parse_sip_uri_with_domain_only(uri: &str) -> String {
        let uri = uri.trim();
        let uri = uri.strip_prefix('<').unwrap_or(uri);
        if let Some(angle_end) = uri.find('>') {
            let uri_part = &uri[..angle_end];
            if uri_part.to_lowercase().starts_with("sip:") {
                return uri_part.to_string();
            } else {
                return format!("sip:{}", uri_part);
            }
        }
        let uri = uri.split(';').next().unwrap_or(uri).trim();
        if uri.to_lowercase().starts_with("sip:") {
            uri.to_string()
        } else {
            format!("sip:{}", uri)
        }
    }

    fn parse_device_id_from_from_uri(from_uri: &str) -> Option<String> {
        let uri = from_uri.trim();
        let uri = uri.strip_prefix('<').unwrap_or(uri);
        let uri = uri.split('>').next().unwrap_or(uri);
        let uri = uri.split(';').next().unwrap_or(uri);
        let uri = uri.trim();
        let uri = uri.strip_prefix("sip:").unwrap_or(uri);
        if let Some(at_pos) = uri.find('@') {
            let device_id = &uri[..at_pos];
            if !device_id.is_empty() && device_id.len() >= 10 {
                return Some(device_id.to_string());
            }
        }
        None
    }

    fn parse_tag_from_header(header_value: &str) -> Option<String> {
        if let Some(tag_start) = header_value.find("tag=") {
            let after_tag = &header_value[tag_start + 4..];
            let end = after_tag
                .find(&[',', ';', '\r', '\n'][..])
                .unwrap_or(after_tag.len());
            if end > 0 {
                return Some(after_tag[..end].to_string());
            }
        }
        None
    }

    fn parse_from_to_uri_and_tag(header_value: &str) -> (String, Option<String>) {
        let uri = Self::parse_sip_uri_with_domain_only(header_value);
        let tag = Self::parse_tag_from_header(header_value);
        (uri, tag)
    }

    fn parse_contact_host_port(headers: &[(String, String)]) -> Option<(String, u16)> {
        let contact_value = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Contact"))
            .map(|(_, v)| v.trim())?;

        let uri = if contact_value.starts_with('<') && contact_value.ends_with('>') {
            &contact_value[1..contact_value.len() - 1]
        } else {
            contact_value
        };

        let uri = uri.strip_prefix("sip:").unwrap_or(uri);

        if let Some(at_pos) = uri.rfind('@') {
            let host_port = &uri[at_pos + 1..];

            let hp_clean = host_port.split(';').next().unwrap_or(host_port);
            let hp_clean = hp_clean.split('>').next().unwrap_or(hp_clean);

            if let Some(colon_pos) = hp_clean.rfind(':') {
                let host = &hp_clean[..colon_pos];
                let port_str = &hp_clean[colon_pos + 1..];
                if let Ok(port) = port_str.parse::<u16>() {
                    return Some((host.to_string(), port));
                }
            }

            let host =
                hp_clean.trim_end_matches(|c| c == '>' || c == ';' || c == '\r' || c == '\n');
            if !host.is_empty() {
                return Some((host.to_string(), 5060));
            }
        }
        None
    }

    fn parse_via_branch(headers: &[(String, String)]) -> String {
        Self::get_header(headers, "Via")
            .and_then(|v| {
                for part in v.split(';') {
                    let trimmed = part.trim();
                    if trimmed.starts_with("branch=") {
                        return Some(trimmed[7..].to_string());
                    }
                }
                None
            })
            .unwrap_or_else(|| "z9hG4bKdefault".to_string())
    }

    fn parse_via_received(headers: &[(String, String)]) -> Option<String> {
        Self::get_header(headers, "Via").and_then(|v| {
            for part in v.split(';') {
                let trimmed = part.trim();
                if trimmed.starts_with("received=") {
                    return Some(trimmed[9..].to_string());
                }
            }
            None
        })
    }

    fn parse_via_rport(headers: &[(String, String)]) -> Option<u16> {
        Self::get_header(headers, "Via").and_then(|v| {
            for part in v.split(';') {
                let trimmed = part.trim();
                if trimmed.starts_with("rport") {
                    if trimmed == "rport" {
                        return Some(0);
                    }
                    if let Some(val) = trimmed.strip_prefix("rport=") {
                        return val.parse().ok();
                    }
                }
            }
            None
        })
    }

    fn parse_cseq_line(headers: &[(String, String)]) -> String {
        Self::get_header(headers, "CSeq").unwrap_or_else(|| "1 REGISTER".to_string())
    }

    fn parse_xml_field(xml: &str, tag: &str) -> Option<String> {
        let doc = Document::parse(xml).ok()?;
        doc.root_element()
            .descendants()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text().map(|s| s.trim().to_string()))
    }

    fn parse_channel_id(body: &str) -> Option<String> {
        Self::parse_xml_field(body, "ChannelID").or_else(|| Self::parse_xml_field(body, "DeviceID"))
    }

    fn best_direction(pan: f64, tilt: f64, zoom: f64) -> &'static str {
        let p = pan.abs();
        let t = tilt.abs();
        let z = zoom.abs();
        if p >= t && p >= z {
            if pan > 0.0 {
                "right"
            } else {
                "left"
            }
        } else if t >= p && t >= z {
            if tilt > 0.0 {
                "up"
            } else {
                "down"
            }
        } else {
            if zoom > 0.0 {
                "zoom_in"
            } else {
                "zoom_out"
            }
        }
    }

    fn encode_ptz_cmd_compound(pan: f64, tilt: f64, zoom: f64) -> String {
        let pan_speed = ((pan.abs() * 255.0) as u8).max(1);
        let tilt_speed = ((tilt.abs() * 255.0) as u8).max(1);
        let zoom_speed = ((zoom.abs() * 255.0) as u8).max(1);

        let cmd1: u8 = 0x00;
        let cmd1 = if tilt > 0.01 { cmd1 | 0x08 } else { cmd1 };
        let cmd1 = if tilt < -0.01 { cmd1 | 0x10 } else { cmd1 };
        let cmd1 = if pan < -0.01 { cmd1 | 0x02 } else { cmd1 };
        let cmd1 = if pan > 0.01 { cmd1 | 0x20 } else { cmd1 };

        let cmd2: u8 = 0x00;
        let cmd2 = if zoom > 0.01 { cmd2 | 0x80 } else { cmd2 };
        let cmd2 = if zoom < -0.01 { cmd2 | 0x40 } else { cmd2 };

        let p_byte = if pan.abs() > 0.01 { pan_speed } else { 0 };
        let t_byte = if tilt.abs() > 0.01 { tilt_speed } else { 0 };
        let z_byte = if zoom.abs() > 0.01 { zoom_speed } else { 0 };

        format!(
            "80 01 {:02X} {:02X} {:02X} {:02X} {:02X} 00",
            cmd1, cmd2, p_byte, t_byte, z_byte
        )
    }

    fn encode_ptz_cmd(direction: &str, speed: u8) -> String {
        let spd = ((speed as f64 / 100.0) * 255.0) as u8;
        match direction {
            "up" => format!("80 01 04 08 00 {:02X} 00 {:02X} 00 00 00 00", spd, spd),
            "down" => format!("80 01 04 10 00 {:02X} 00 {:02X} 00 00 00 00", spd, spd),
            "left" => format!("80 01 04 20 00 {:02X} 00 {:02X} 00 00 00 00", spd, spd),
            "right" => format!("80 01 04 40 00 {:02X} 00 {:02X} 00 00 00 00", spd, spd),
            "zoom_in" => format!("80 01 04 80 00 {:02X} 00 00 00 00 00 00", spd),
            "zoom_out" => format!("80 01 04 00 01 {:02X} 00 00 00 00 00 00", spd),
            "focus_in" | "focus_near" => format!("80 01 47 00 00 {:02X} 00 00 00 00 00 00", spd),
            "focus_out" | "focus_far" => format!("80 01 46 00 00 {:02X} 00 00 00 00 00 00", spd),
            "iris_in" | "iris_open" => format!("80 01 49 00 00 {:02X} 00 00 00 00 00 00", spd),
            "iris_out" | "iris_close" => format!("80 01 48 00 00 {:02X} 00 00 00 00 00 00", spd),
            "stop" => "80 01 04 00 00 00 00 00 00 00 00 00".to_string(),
            _ => "80 01 04 00 00 00 00 00 00 00 00 00".to_string(),
        }
    }

    fn build_ptz_xml(channel_id: &str, direction: &str, speed: u8) -> String {
        let ptz_cmd = Self::encode_ptz_cmd(direction, speed);
        let sn = chrono::Utc::now().timestamp_millis() % 1000000;
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>PTZCmd</CmdType><SN>{}</SN><DeviceID>{}</DeviceID><PTZCmd>{}</PTZCmd></Notify>"#,
            sn, channel_id, ptz_cmd
        )
    }

    fn build_ptz_xml_compound(
        channel_id: &str,
        pan: f64,
        tilt: f64,
        zoom: f64,
    ) -> String {
        let ptz_cmd = Self::encode_ptz_cmd_compound(pan, tilt, zoom);
        let sn = chrono::Utc::now().timestamp_millis() % 1000000;
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>PTZCmd</CmdType><SN>{}</SN><DeviceID>{}</DeviceID><PTZCmd>{}</PTZCmd></Notify>"#,
            sn, channel_id, ptz_cmd
        )
    }

    fn build_ptz_message(&self, channel_id: &str, body: &str) -> String {
        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let server_port = self.deps.config.server.port;
        let server_ip = self
            .remote_addr
            .map(|a| a.ip().to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string());
        let branch = format!("z9hG4bK{}", &call_id[..7]);

        let via = if self.transport == TransportType::Tcp {
            format!(
                "SIP/2.0/TCP {}:{};branch={};rport",
                server_ip, server_port, branch
            )
        } else {
            format!(
                "SIP/2.0/UDP {}:{};branch={};rport",
                server_ip, server_port, branch
            )
        };

        let body_len = body.len();
        let to_uri = format!(
            "sip:{}@{}",
            channel_id,
            self.remote_addr
                .map(|a| a.to_string())
                .unwrap_or_else(|| "0.0.0.0:5060".to_string())
        );
        let from_uri = format!("sip:rustcam@{}:{}", server_ip, server_port);
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");

        format!(
            "MESSAGE {to_uri} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <{from_uri}>;tag={from_tag}\r\n\
             To: <{to_uri}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {sn} MESSAGE\r\n\
             Content-Type: Application/MANSCDP+xml\r\n\
             Max-Forwards: 70\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: {body_len}\r\n\
             \r\n\
             {body}",
            to_uri = to_uri,
            via = via,
            from_uri = from_uri,
            from_tag = from_tag,
            call_id = call_id,
            sn = sn,
            body_len = body_len,
            body = body
        )
    }

    fn build_catalog_query(&self) -> Vec<u8> {
        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let device_tag = self.device_tag_str();
        let (platform_id, platform_domain, platform_ip, _platform_port) =
            crate::protocol::gb28181::get_gb28181_platform_config()
                .get()
                .map(|(id, domain, ip, port)| (id.as_str(), domain.as_str(), ip.as_str(), *port))
                .unwrap_or(("00000000000000000000", "0000000000", "0.0.0.0", 5060));

        let server_port = match self.transport {
            TransportType::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportType::Udp => self.deps.config.signaling_server.udp_signaling_port,
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        };

        let local_ip = if platform_ip == "0.0.0.0" {
            if let Some(ref peer) = self.remote_addr {
                crate::protocol::gb28181::detect_local_ip(peer.ip().to_string().as_str())
                    .unwrap_or_else(|| "0.0.0.0".to_string())
            } else {
                platform_ip.to_string()
            }
        } else {
            platform_ip.to_string()
        };

        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Query><CmdType>Catalog</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<Info><DeviceID>{}</DeviceID></Info></Query>"#,
            sn, device_tag, device_tag
        );

        let ts = chrono::Utc::now().timestamp_millis() % 1000000;
        let call_id = format!("cat-{}", ts);
        let from_uri = SipUri {
            user: Some(platform_id.to_string()),
            host: format!("{}:{}", local_ip, server_port),
            port: None,
            params: HashMap::new(),
        };
        let device_ip = self
            .remote_addr
            .map(|p| p.ip().to_string())
            .unwrap_or_else(|| local_ip.clone());
        let to_uri = SipUri {
            user: Some(device_tag.to_string()),
            host: format!("{}:{}", device_ip, server_port),
            port: None,
            params: HashMap::new(),
        };

        let from_name_addr = SipNameAddr::new(from_uri)
            .with_param("domain", platform_domain)
            .with_param("tag", &format!("cat{}", ts));
        let to_name_addr = SipNameAddr::new(to_uri.clone());

        let via_str = match self.transport {
            TransportType::Tcp => format!(
                "SIP/2.0/TCP {}:{};branch=z9hG4bKcat{}",
                local_ip, server_port, ts
            ),
            TransportType::Udp => format!(
                "SIP/2.0/UDP {}:{};branch=z9hG4bKcat{}",
                local_ip, server_port, ts
            ),
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        };

        let msg = SipMessage::request(SipMethod::Message, to_uri)
            .header("via", &via_str)
            .header("from", &from_name_addr.to_string())
            .header("to", &to_name_addr.to_string())
            .header("call-id", &call_id)
            .header("CSeq", &format!("{} MESSAGE", sn))
            .header("content-type", "Application/MANSCDP+xml")
            .header("user-agent", "RustCam-Media/2.0")
            .set_body(&body);

        msg.to_string().into_bytes()
    }

    fn parse_catalog_response(&self, body: &str) -> Vec<CatalogChannel> {
        let doc = match Document::parse(body) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("[GB28181] Failed to parse catalog XML: {}", e);
                return Vec::new();
            }
        };

        let mut channels = Vec::new();
        for item in doc
            .root_element()
            .descendants()
            .filter(|n| n.has_tag_name("Item"))
        {
            let get_text = |tag: &str| -> Option<String> {
                item.children()
                    .find(|c| c.has_tag_name(tag))
                    .and_then(|c| c.text())
                    .map(|s| s.trim().to_string())
            };

            let get_info_block = || -> Option<crate::protocol::event::DeviceInfoBlock> {
                item.children()
                    .find(|c| c.has_tag_name("Info"))
                    .map(|info| {
                        let get_info_text = |tag: &str| -> Option<String> {
                            info.children()
                                .find(|c| c.has_tag_name(tag))
                                .and_then(|c| c.text())
                                .map(|s| s.trim().to_string())
                        };
                        crate::protocol::event::DeviceInfoBlock {
                            device_type: get_info_text("DeviceType"),
                            protocol: get_info_text("Protocol"),
                            ptz_type: get_info_text("PTZType").and_then(|v| v.parse::<u8>().ok()),
                            video_input_number: get_info_text("VideoInputNumber")
                                .and_then(|v| v.parse::<u8>().ok()),
                            audio_input_number: get_info_text("AudioInputNumber")
                                .and_then(|v| v.parse::<u8>().ok()),
                            alarm_output_number: get_info_text("AlarmOutputNumber")
                                .and_then(|v| v.parse::<u8>().ok()),
                        }
                    })
            };

            let device_id = get_text("DeviceID").unwrap_or_default();
            if device_id.is_empty() {
                continue;
            }

            let parental = get_text("Parental")
                .and_then(|v| v.parse::<u32>().ok())
                .map(|v| v > 0)
                .unwrap_or(false);

            channels.push(CatalogChannel {
                device_id,
                name: get_text("Name").unwrap_or_default(),
                manufacturer: get_text("Manufacturer"),
                model: get_text("Model"),
                status: get_text("Status").unwrap_or_else(|| "OFF".to_string()),
                parental,
                parent_id: get_text("ParentID"),
                civil_code: get_text("CivilCode"),
                address: get_text("Address"),
                ip_address: get_text("IPAddress"),
                port: get_text("Port").and_then(|v| v.parse::<u16>().ok()),
                owner: get_text("Owner"),
                secrecy: get_text("Secrecy").and_then(|v| v.parse::<u8>().ok()),
                device_type: get_text("DeviceType"),
                ptz_type: get_text("PTZType").and_then(|v| v.parse::<u8>().ok()),
                info: get_info_block(),
            });
        }

        channels
    }

    fn detect_version(body: &str) -> Gb28181Version {
        if body.contains("GB28181") || body.contains("28181") {
            Gb28181Version::Gb2016
        } else {
            Gb28181Version::Gb2016
        }
    }

    fn via_header(&self, branch: &str) -> String {
        let addr = self
            .remote_addr
            .map(|a| a.to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string());
        match self.transport {
            TransportType::Tcp => format!("SIP/2.0/TCP {};rport;branch={}", addr, branch),
            TransportType::Udp => format!("SIP/2.0/UDP {};rport;branch={}", addr, branch),
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        }
    }

    fn call_id_str(&self) -> String {
        self.call_id
            .clone()
            .unwrap_or_else(|| "rustcam-call-id".to_string())
    }

    fn from_tag_param(&self) -> String {
        self.from_tag
            .as_ref()
            .map(|t| format!(";tag={}", t))
            .unwrap_or_default()
    }

    fn to_tag_param(&self) -> String {
        self.to_tag
            .as_ref()
            .map(|t| format!(";tag={}", t))
            .unwrap_or_default()
    }

    fn build_sip_response_only_headers(
        &self,
        status_code: u16,
        status_text: &str,
        via_branch: &str,
        cseq_line: &str,
        call_id_override: Option<&str>,
    ) -> String {
        self.build_sip_response_only_headers_with_to(status_code, status_text, via_branch, cseq_line, None, None, None, None, None, call_id_override, None)
    }

    fn build_sip_response_only_headers_with_to(
        &self,
        status_code: u16,
        status_text: &str,
        via_branch: &str,
        cseq_line: &str,
        from_override: Option<&str>,
        from_tag_override: Option<&str>,
        to_override: Option<&str>,
        to_tag_override: Option<&str>,
        contact_override: Option<&str>,
        call_id_override: Option<&str>,
        content_length: Option<usize>,
    ) -> String {
        let expires_line = if let Some(exp) = self.registration_expires {
            format!("Expires: {}\r\n", exp)
        } else {
            String::new()
        };
        let from_uri = from_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.from_uri.clone().unwrap_or_else(|| format!("sip:{}", self.device_tag_str())));
        let from_tag = from_tag_override
            .map(|s| format!(";tag={}", s))
            .unwrap_or_else(|| self.from_tag.as_deref().map(|t| format!(";tag={}", t)).unwrap_or_default());
        let to_uri = to_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| from_uri.clone());
        let to_tag = to_tag_override
            .map(|s| format!(";tag={}", s))
            .unwrap_or_else(|| self.to_tag.as_deref().map(|t| format!(";tag={}", t)).unwrap_or_default());
        let contact_line = contact_override.map(|c| format!("Contact: <{}>\r\n", c)).unwrap_or_default();
        let call_id = call_id_override.unwrap_or_else(|| self.call_id.as_deref().unwrap_or("rustcam-call-id"));
        
        let (content_type_line, content_length_line) = if let Some(len) = content_length {
            (
                "Content-Type: application/sdp\r\n".to_string(),
                format!("Content-Length: {}\r\n", len)
            )
        } else {
            (
                String::new(),
                "Content-Length: 0\r\n".to_string()
            )
        };
        
        format!(
            "SIP/2.0 {} {}\r\n\
             Via: {}\r\n\
             From: <{}>{}\r\n\
             To: <{}>{}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             {}\
             {}User-Agent: RustCam-Media/2.0\r\n\
             {}{}",
            status_code,
            status_text,
            self.via_header(via_branch),
            from_uri,
            from_tag,
            to_uri,
            to_tag,
            cseq_line,
            call_id,
            contact_line,
            expires_line,
            content_type_line,
            content_length_line,
        )
    }

    fn build_401_response(&self, via_branch: &str, cseq_line: &str, realm: &str) -> String {
        let nonce = generate_nonce();
        let from_uri = self.from_uri.clone().unwrap_or_else(|| {
            let device = self.device_tag_str();
            if device.starts_with("sip:") || device.starts_with("SIP:") {
                device
            } else {
                format!("sip:{}", device)
            }
        });
        tracing::info!(
            "[GB28181] build_401_response: from_uri='{}', from_tag='{}', to_tag='{}'",
            from_uri,
            self.from_tag.as_deref().unwrap_or(""),
            self.to_tag.as_deref().unwrap_or("")
        );
        let www_auth = format!(r#"Digest realm="{}", nonce="{}""#, realm, nonce);
        let response = format!(
            "SIP/2.0 401 Unauthorized\r\n\
             Via: {}\r\n\
             From: <{}>{}\r\n\
             To: <{}>{}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             WWW-Authenticate: {}\r\n\
             Content-Length: 0\r\n\r\n",
            self.via_header(via_branch),
            from_uri,
            self.from_tag_param(),
            from_uri,
            self.to_tag_param(),
            cseq_line,
            self.call_id_str(),
            www_auth
        );
        tracing::info!("[GB28181] 401 response:\n{}", response);
        response
    }

    fn build_403_response(&self, via_branch: &str, cseq_line: &str) -> String {
        let from_uri = self.from_uri.clone().unwrap_or_else(|| {
            let device = self.device_tag_str();
            if device.starts_with("sip:") || device.starts_with("SIP:") {
                device
            } else {
                format!("sip:{}", device)
            }
        });
        format!(
            "SIP/2.0 403 Forbidden\r\n\
             Via: {}\r\n\
             From: <{}>{}\r\n\
             To: <{}>{}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            self.via_header(via_branch),
            from_uri,
            self.from_tag_param(),
            from_uri,
            self.to_tag_param(),
            cseq_line,
            self.call_id_str()
        )
    }

    fn build_403_response_with_headers(
        &self,
        via_branch: &str,
        cseq_line: &str,
        from_hdr: &str,
        to_hdr: &str,
        call_id_hdr: &str,
    ) -> String {
        format!(
            "SIP/2.0 403 Forbidden\r\n\
             Via: {}\r\n\
             From: {}\r\n\
             To: {}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            self.via_header(via_branch),
            from_hdr,
            to_hdr,
            cseq_line,
            call_id_hdr
        )
    }

    fn build_device_info_response(&self) -> String {
        let device_tag = self.device_tag_str();
        let device_name = self.device_name.as_deref().unwrap_or("IPC");
        let sn = self.seq.load(Ordering::SeqCst);
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>DeviceInfo</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<DeviceName>{}</DeviceName><Result>OK</Result><Manufacturer>RustCam</Manufacturer>
<Model>RustCam-GB28181</Model><Firmware>1.0.0</Firmware><Channel>1</Channel>
<Online>1</Online><Status>OK</Status></Notify>"#,
            sn, device_tag, device_name
        )
    }

    fn build_device_status_response(&self) -> String {
        let device_tag = self.device_tag_str();
        let sn = self.seq.load(Ordering::SeqCst);
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>DeviceStatus</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<Online>ON</Online><Status>OK</Status></Notify>"#,
            sn, device_tag
        )
    }

    fn build_device_config_response(&self, config_type: &str) -> String {
        let device_tag = self.device_tag_str();
        let sn = self.seq.load(Ordering::SeqCst);
        let config_item = match config_type {
            "BasicParam" => {
                r#"<BasicParam><VideoEnable>true</VideoEnable><AudioEnable>true</AudioEnable></BasicParam>"#
            }
            "NetworkParam" => {
                r#"<NetworkParam><IPAddress>0.0.0.0</IPAddress><Port>0</Port></NetworkParam>"#
            }
            "VideoParam" => {
                r#"<VideoParam><Resolution>1920x1080</Resolution><BitRate>2048</BitRate><FrameRate>25</FrameRate></VideoParam>"#
            }
            "VideoSrcParam" => {
                r#"<VideoSrcParam><Resolution>1920x1080</Resolution><Quality>85</Quality></VideoSrcParam>"#
            }
            _ => r#"<ConfigParam></ConfigParam>"#,
        };
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>ConfigDownload</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<Result>OK</Result>{}</Notify>"#,
            sn, device_tag, config_item
        )
    }

    fn build_preset_list_response(&self, channel_id: &str) -> String {
        let sn = self.seq.load(Ordering::SeqCst);
        let presets = vec![("1", "Preset 1"), ("2", "Preset 2"), ("3", "Preset 3")];
        let mut preset_items = String::new();
        for (idx, name) in presets {
            preset_items.push_str(&format!(
                r#"<PresetItem><id>{}</id><presetName>{}</presetName></PresetItem>"#,
                idx, name
            ));
        }
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>PresetList</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<PresetList>{}</PresetList></Notify>"#,
            sn, channel_id, preset_items
        )
    }

    fn build_preset_response(
        &self,
        channel_id: &str,
        preset_id: &str,
        preset_name: &str,
        cmd_type: &str,
    ) -> String {
        let sn = self.seq.load(Ordering::SeqCst);
        let result = if cmd_type == "SetPreset" {
            format!(
                r#"<PresetID>{}</PresetID><PresetName>{}</PresetName>"#,
                preset_id, preset_name
            )
        } else {
            String::new()
        };
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>{}</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<Result>OK</Result>{}</Notify>"#,
            cmd_type, sn, channel_id, result
        )
    }

    fn build_sdp(
        &self,
        server_gb_id: &str,
        server_ip: &str,
        rtp_port: u16,
        ssrc: &str,
        config: Option<&crate::domain::device::StreamConfig>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> String {
        let video_codec = config
            .and_then(|c| c.video_codec.clone())
            .unwrap_or_else(|| "PS".to_string());
        let audio_codec = config
            .and_then(|c| c.audio_codec.clone())
            .unwrap_or_else(|| "PCMA".to_string());
        let stream_mode = config
            .and_then(|c| c.stream_mode.clone())
            .unwrap_or_else(|| "recvonly".to_string());
        let profile_level_id = config.and_then(|c| c.profile_level_id.clone());
        let packaging_mode = config.and_then(|c| c.packaging_mode.clone());
        let sprop_parameter_sets = config.and_then(|c| c.sprop_parameter_sets.clone());

        let video_pt = config
            .and_then(|c| c.video_payload_type)
            .unwrap_or_else(|| Self::codec_to_video_pt(&video_codec));
        let audio_pt = config
            .and_then(|c| c.audio_payload_type)
            .unwrap_or_else(|| Self::codec_to_audio_pt(&audio_codec));

        let (video_encoding, video_clock) = Self::codec_to_rtp_map(&video_codec);
        let (audio_encoding, audio_clock) = Self::codec_to_audio_rtp_map(&audio_codec);

        let mut video_line = format!("m=video {} RTP/AVP {}\r\n", rtp_port, video_pt);
        video_line.push_str(&format!("a={}\r\n", stream_mode));
        video_line.push_str(&format!(
            "a=rtpmap:{} {}/{}\r\n",
            video_pt, video_encoding, video_clock
        ));
        video_line.push_str(&format!("a=rtcp:{}\r\n", rtp_port + 1));

        if let Some(ref pli) = profile_level_id {
            if video_codec == "PS" {
                if let Some(ref pm) = packaging_mode {
                    video_line.push_str(&format!(
                        "a=fmtp:{} profile-level-id={};packaging_mode={}\r\n",
                        video_pt, pli, pm
                    ));
                }
            } else if video_codec == "H264" || video_codec == "H265" {
                if let Some(ref sps) = sprop_parameter_sets {
                    video_line.push_str(&format!(
                        "a=fmtp:{} profile-level-id={};sprop-parameter-sets={}\r\n",
                        video_pt, pli, sps
                    ));
                } else {
                    video_line
                        .push_str(&format!("a=fmtp:{} profile-level-id={}\r\n", video_pt, pli));
                }
            } else {
                video_line.push_str(&format!("a=fmtp:{} profile-level-id={}\r\n", video_pt, pli));
            }
        }

        let audio_line = String::new();

        let (sdp_type, time_range) = if let (Some(start), Some(end)) = (start_time, end_time) {
            (
                "Playback",
                format!(
                    "{} {}",
                    Self::datetime_to_ntp_string(start),
                    Self::datetime_to_ntp_string(end)
                ),
            )
        } else {
            ("Play", "0 0".to_string())
        };

        let mut sdp = format!(
            "v=0\r\n\
             o={} 0 0 IN IP4 {}\r\n\
             s={}\r\n\
             c=IN IP4 {}\r\n\
             t={}\r\n",
            server_gb_id, server_ip, sdp_type, server_ip, time_range
        );

        sdp.push_str(&video_line);
        sdp.push_str(&audio_line);
        sdp.push_str(&format!("y={:010}\r\n", ssrc));

        // f字段格式: f=v/编码格式/分辨率/帧率/码率类型/码率
        // v=视频, 编码格式: PS/H264/H265等, 分辨率: 宽*高, 帧率: 数字, 码率类型: 1(可变码率)/0(固定码率), 码率: Kbps
        // 参考: f=v/264/1920*1080/25/1/2048
        let video_encoding_for_f = match video_codec.to_uppercase().as_str() {
            "H264" | "H.264" | "AVC" => "264",
            "H265" | "H.265" | "HEVC" => "265",
            "PS" => "PS",
            "MPEG4" | "MP4V" => "mp4v",
            _ => "264",
        };
        // 使用默认值: 1920*1080, 25fps, 可变码率, 2048Kbps
        let f_line = format!("f=v/{}/1920*1080/25/1/2048\r\n", video_encoding_for_f);
        sdp.push_str(&f_line);

        sdp
    }

    fn datetime_to_ntp_string(dt: DateTime<Utc>) -> String {
        dt.format("%Y%m%d%H%M%S").to_string()
    }

    fn codec_to_video_pt(codec: &str) -> u8 {
        match codec.to_uppercase().as_str() {
            "PS" => 96,
            "H264" | "H.264" => 98,
            "H265" | "H.265" | "HEVC" => 99,
            "MPEG4" | "MP4V" => 97,
            "AVC" => 98,
            _ => 96,
        }
    }

    fn codec_to_audio_pt(codec: &str) -> u8 {
        match codec.to_uppercase().as_str() {
            "PCMA" | "G711A" => 8,
            "PCMU" | "G711U" => 0,
            "AAC" => 97,
            "G722" => 9,
            "G728" => 15,
            "G729" => 18,
            _ => 8,
        }
    }

    fn codec_to_rtp_map(codec: &str) -> (&'static str, u32) {
        match codec.to_uppercase().as_str() {
            "PS" => ("PS", 90000),
            "H264" | "H.264" | "AVC" => ("H264", 90000),
            "H265" | "H.265" | "HEVC" => ("H265", 90000),
            "MPEG4" | "MP4V" => ("MPEG4", 90000),
            _ => ("PS", 90000),
        }
    }

    fn codec_to_audio_rtp_map(codec: &str) -> (&'static str, u32) {
        match codec.to_uppercase().as_str() {
            "PCMA" | "G711A" => ("PCMA", 8000),
            "PCMU" | "G711U" => ("PCMU", 8000),
            "AAC" => ("AAC", 8000),
            "G722" => ("G722", 8000),
            "G728" => ("G728", 16000),
            "G729" => ("G729", 8000),
            _ => ("PCMA", 8000),
        }
    }

    fn build_catalog_response(&self, channel_id: &str) -> String {
        let device_tag = self.device_tag_str();
        let device_name = self.device_name.as_deref().unwrap_or("IPC");
        let sn = self.seq.load(Ordering::SeqCst);
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>{}</SN><DeviceID>{}</DeviceID><SumNum>1</SumNum>
<DeviceList><Item><DeviceID>{}</DeviceID><Name>{}</Name><DeviceType>IPC</DeviceType>
<Manufacturer>RustCam</Manufacturer><Model>IPC</Model><Owner>Owner</Owner>
<CivilCode>CivilCode</CivilCode><Address>Address</Address><Parental>0</Parental>
<SafetyWay>0</SafetyWay><RegisterWay>1</RegisterWay><CertNum>CertNum</CertNum>
<ErrCode>0</ErrCode><Secrecy>0</Secrecy><IPAddress>0.0.0.0</IPAddress><Port>0</Port>
<Password></Password><Status>ON</Status><PTZType>3</PTZType>
<Info><DeviceType>IPC</DeviceType><Protocol>GB28181</Protocol><PTZType>3</PTZType>
<VideoInputNumber>1</VideoInputNumber><AudioInputNumber>1</AudioInputNumber>
<AlarmOutputNumber>0</AlarmOutputNumber></Info></Item></DeviceList></Response>"#,
            sn, device_tag, channel_id, device_name
        )
    }

    fn build_broadcast_notification(&self, target_id: &str) -> String {
        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let source_id = &self.deps.config.signaling_server.server_gb_id;
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify>
<CmdType>Broadcast</CmdType>
<SN>{}</SN>
<SourceID>{}</SourceID>
<TargetID>{}</TargetID>
</Notify>
"#,
            sn, source_id, target_id
        );
        tracing::info!("[GB28181] build_broadcast_notification:");
        tracing::info!("[GB28181]   source_id (platform): {}", source_id);
        tracing::info!("[GB28181]   target_id (device): {}", target_id);
        tracing::info!("[GB28181]   sn: {}", sn);
        tracing::info!("[GB28181]   XML:\n{}", xml);
        xml
    }

    fn parse_broadcast_answer(&self, body: &str) -> Option<(bool, Option<u16>)> {
        let result = Self::parse_xml_field(body, "Result")?;
        let accept = result.to_lowercase() == "accept" || result.to_lowercase() == "ok";
        let audio_port = Self::parse_xml_field(body, "AudioPort").and_then(|p| p.parse().ok());
        Some((accept, audio_port))
    }

    async fn alloc_rtp_port(
        &self,
        stream_key: &str,
        server_name: Option<&str>,
    ) -> Result<(u16, String, String)> {
        let adapter = if let Some(name) = server_name {
            self.deps.cluster.get_server(name)
        } else {
            None
        };

        let adapter = match adapter {
            Some(a) if a.is_online().await => {
                tracing::info!(
                    "[GB28181] Using device-associated media server: {}",
                    server_name.unwrap()
                );
                a
            }
            _ => {
                if server_name.is_some() {
                    tracing::warn!(
                        "[GB28181] Media server {} unavailable, falling back",
                        server_name.unwrap()
                    );
                }
                match self.deps.cluster.select_server().await {
                    Some(s) => s,
                    None => {
                        return Err(AppError::MediaServerError(
                            "No media server available".to_string(),
                        ))
                    }
                }
            }
        };

        let tag = adapter.tag().to_string();
        match adapter
            .open_rtp_server(stream_key, 0, RtpTransport::Udp)
            .await
        {
            Ok((port, ip)) => {
                tracing::info!(
                    "[GB28181] RTP allocated: stream={}, port={}, ip={}, server={}",
                    stream_key,
                    port,
                    ip,
                    tag
                );
                Ok((port, ip, tag))
            }
            Err(e) => {
                tracing::warn!("[GB28181] RTP failed on {}, trying fallback: {}", tag, e);
                if server_name.is_some() {
                    match self.deps.cluster.select_server().await {
                        Some(fallback) => {
                            let fb_tag = fallback.tag().to_string();
                            fallback
                                .open_rtp_server(stream_key, 0, RtpTransport::Udp)
                                .await
                                .map(|(port, ip)| {
                                    tracing::info!("[GB28181] RTP fallback: server={}", fb_tag);
                                    (port, ip, fb_tag)
                                })
                                .map_err(|e2| {
                                    AppError::MediaServerError(format!(
                                        "RTP fallback failed: {}",
                                        e2
                                    ))
                                })
                        }
                        None => Err(AppError::MediaServerError(format!(
                            "No media server available: {}",
                            e
                        ))),
                    }
                } else {
                    Err(AppError::MediaServerError(format!(
                        "No media server available: {}",
                        e
                    )))
                }
            }
        }
    }

    async fn handle_register(
        &mut self,
        headers: &[(String, String)],
        body: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        self.version = Self::detect_version(body);

        let call_id = Self::get_header(headers, "Call-ID")
            .unwrap_or_else(|| self.call_id.clone().unwrap_or_default());

        let device_id = Self::get_header(headers, "From")
            .or_else(|| Self::get_header(headers, "To"))
            .map(|v| Self::parse_sip_uri(&v))
            .unwrap_or_else(|| "unknown".to_string());

        tracing::info!("[GB28181] handle_register: device_id={}", device_id);
        self.device_tag = Some(device_id.clone());
        if let Some(from_hdr) = Self::get_header(headers, "From") {
            self.from_uri = Some(Self::parse_sip_uri_with_domain_only(&from_hdr));
        }
        if let Some(name) = Self::parse_xml_field(body, "DeviceName") {
            self.device_name = Some(name);
        }

        let expires = Self::get_header(headers, "Expires")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600);
        self.registration_expires = Some(expires.clamp(60, 86400));

        if self.call_id.is_none() {
            if let Some(cid) = Self::get_header(headers, "Call-ID") {
                self.call_id = Some(cid);
            }
        }
        if let Some(from_hdr) = Self::get_header(headers, "From") {
            if self.from_tag.is_none() {
                if let Some(tag) = Self::parse_tag_from_header(&from_hdr) {
                    self.from_tag = Some(tag);
                }
            }
        }
        if let Some(to_hdr) = Self::get_header(headers, "To") {
            if let Some(tag) = Self::parse_tag_from_header(&to_hdr) {
                if self.to_tag.is_none() {
                    self.to_tag = Some(tag);
                }
            } else if self.to_tag.is_none() {
                self.to_tag = Some(uuid::Uuid::new_v4().to_string().replace("-", ""));
            }
        }

        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);
        let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
        let to_hdr = Self::get_header(headers, "To").unwrap_or_default();
        let (request_from_uri, request_from_tag) = Self::parse_from_to_uri_and_tag(&from_hdr);
        let (request_to_uri, _) = Self::parse_from_to_uri_and_tag(&to_hdr);
        tracing::info!(
            "[GB28181] REGISTER: {} (expires={}s)",
            device_id,
            self.registration_expires()
        );

        let auth_header = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Authorization"))
            .map(|(_, v)| v.as_str());

        if auth_header.is_none() {
            let server_realm = super::get_gb28181_platform_config()
                .get()
                .map(|c| c.1.clone())
                .unwrap_or_else(|| device_id.clone());
            tracing::info!(
                "[GB28181] No Authorization header, sending 401 to {} with realm={}",
                device_id,
                server_realm
            );
            return (
                None,
                Some(self.build_401_response(&via_branch, &cseq, &server_realm)),
            );
        }

        let auth_params = match parse_sip_authorization(auth_header.unwrap()) {
            Some(p) => p,
            None => {
                tracing::warn!(
                    "[GB28181] Failed to parse Authorization header: {}",
                    auth_header.unwrap()
                );
                return (None, Some(self.build_403_response(&via_branch, &cseq)));
            }
        };
        tracing::info!(
            "[GB28181] Auth params: username={}, realm={}, nonce={}",
            auth_params.username,
            auth_params.realm,
            auth_params.nonce
        );
        if !verify_nonce(&auth_params.nonce) {
            let server_realm = super::get_gb28181_platform_config()
                .get()
                .map(|c| c.1.clone())
                .unwrap_or_else(|| device_id.clone());
            tracing::warn!(
                "[GB28181] Invalid nonce from {}, sending 401 with new nonce",
                device_id
            );
            return (
                None,
                Some(self.build_401_response(&via_branch, &cseq, &server_realm)),
            );
        }

        let device_opt = self.deps.device_lookup.find_by_tag(&device_id).await;
        let (username, password, internal_id, device_tag) = match device_opt {
            Some(d) => (
                d.device_tag.clone().unwrap_or_default(),
                d.device_password.clone().unwrap_or_default(),
                d.id,
                Some(d.device_tag.unwrap_or_default()),
            ),
            None => {
                tracing::warn!("[GB28181] Device not found: {}", device_id);
                return (None, Some(self.build_403_response(&via_branch, &cseq)));
            }
        };
        tracing::info!("[GB28181] Device found: username={}", username);
        if auth_params.username != username {
            tracing::warn!(
                "[GB28181] Username mismatch: auth={} vs db={}",
                auth_params.username,
                username
            );
            return (None, Some(self.build_403_response(&via_branch, &cseq)));
        }

        match verify_sip_digest(&auth_params, &password, "REGISTER") {
            VerifyResult::Valid => {
                tracing::info!("[GB28181] Device {} authenticated", device_id);
                consume_nonce(&auth_params.nonce);
                self.device_id = Some(internal_id as i64);

                let prev_state = self.registration_state;
                self.set_registration_state(RegistrationState::Registered);
                self.waiting_for_reregister = false;

                if let Some(ref entry) = self.adapter_entry {
                    (self.register_fn)(device_id.clone(), entry.clone());
                } else {
                    tracing::error!("[GB28181] adapter_entry not set for device {}", device_id);
                }
                let _ = self.deps.device_lookup.set_online(&device_id).await;

                if let Some((host, port)) = Self::parse_contact_host_port(headers) {
                    tracing::info!(
                        "[GB28181] Device {} Contact header parsed: host={}, port={}",
                        device_id,
                        host,
                        port
                    );
                    let _ = self
                        .deps
                        .device_lookup
                        .update_device_host_port(&device_id, &host, port)
                        .await;
                }

                let event = if prev_state == RegistrationState::Init {
                    tracing::info!(
                        "[GB28181] Device {} registered as new, sending catalog query",
                        device_id
                    );
                    self.set_registration_state(RegistrationState::CatalogSent);
                    let query = self.build_catalog_query();
                    let _ = self.send(&query).await;
                    self.start_wait_timer(30, 3);
                    SignalEvent::DeviceRegister {
                        device_id: internal_id,
                        device_tag,
                        name: self
                            .device_name
                            .clone()
                            .unwrap_or_else(|| "GB28181 Device".to_string()),
                        stream_key: None,
                        manufacturer: Self::parse_xml_field(body, "Manufacturer"),
                        model: Self::parse_xml_field(body, "Model"),
                        protocol: ProtocolType::Gb28181,
                    }
                } else {
                    tracing::debug!("[GB28181] Device {} re-registered (keepalive)", device_id);
                    SignalEvent::DeviceKeepalive {
                        device_id: internal_id,
                        device_tag,
                        timestamp: chrono::Utc::now(),
                    }
                };

                (
                    Some(event),
                    Some(self.build_sip_response_only_headers_with_to(
                        200, "OK", &via_branch, &cseq,
                        Some(&request_from_uri), request_from_tag.as_deref(),
                        Some(&request_to_uri), None,
                        None, Some(&call_id), None,
                    )),
                )
            }
            _ => (None, Some(self.build_403_response(&via_branch, &cseq))),
        }
    }

    async fn handle_message(
        &mut self,
        headers: &[(String, String)],
        body: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        let call_id = Self::get_header(headers, "Call-ID")
            .unwrap_or_else(|| self.call_id.clone().unwrap_or_default());
        if self.call_id.is_none() {
            self.call_id = Some(call_id.clone());
        }
        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);

        let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
        let to_hdr = Self::get_header(headers, "To").unwrap_or_default();
        let (request_from_uri, request_from_tag) = Self::parse_from_to_uri_and_tag(&from_hdr);
        let (request_to_uri, _) = Self::parse_from_to_uri_and_tag(&to_hdr);

        if self.registration_state == RegistrationState::Init {
            if !self.waiting_for_reregister {
                let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
                let device_id_from_header = Self::parse_sip_uri(&from_hdr);

                tracing::warn!("[GB28181] Device in Init state, rejecting MESSAGE (force re-registration)");
                let call_id_hdr = Self::get_header(headers, "Call-ID").unwrap_or_default();
                let resp = self.build_403_response_with_headers(
                    &via_branch,
                    &cseq,
                    &from_hdr,
                    &to_hdr,
                    &call_id_hdr,
                );

                if let Err(e) = self.send(resp.as_bytes()).await {
                    tracing::error!("[GB28181] Failed to send 403 response: {}", e);
                }

                if !device_id_from_header.is_empty() {
                    if let Err(e) = self.send_unregister(&device_id_from_header).await {
                        tracing::error!("[GB28181] Failed to send unregister: {}", e);
                    }
                }

                self.waiting_for_reregister = true;
            } else {
                tracing::debug!("[GB28181] Device in Init state, waiting for re-registration, ignoring MESSAGE");
            }
            return (None, None);
        }

        let device_tag = self.device_tag_str();

        if body.contains("<Response>") && body.contains("Broadcast") {
            let (accept, audio_port) = self.parse_broadcast_answer(body).unwrap_or((false, None));

            tracing::info!(
                "[GB28181] Broadcast answer received: accept={} audio_port={:?}",
                accept,
                audio_port
            );

            if accept {
                return (
                    Some(SignalEvent::AudioBroadcastAccept {
                        device_id: self.device_id_i64(),
                        device_tag: Some(device_tag.clone()),
                        channel_id: device_tag.clone(),
                        audio_port: audio_port.unwrap_or(0),
                    }),
                    Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
                );
            } else {
                let is_busy = body.to_lowercase().contains("voice talk is busy") || body.to_lowercase().contains("busy");
                if is_busy {
                    tracing::warn!("[GB28181] Device reported busy, attempting to clear session");
                    let sessions = self.sessions.read().await;
                    if let Some(session) = sessions.iter().find(|s| s.device_tag == device_tag && s.session_type == SessionType::Broadcast) {
                        tracing::info!("[GB28181] Found broadcast SubSession, sending BYE");
                        let session_clone = session.clone();
                        drop(sessions);
                        if let Err(e) = self.send_bye_to_session(&session_clone).await {
                            tracing::warn!("[GB28181] Failed to send BYE: {}", e);
                        }
                    } else {
                        tracing::warn!("[GB28181] No broadcast SubSession found");
                    }
                }
                return (
                    Some(SignalEvent::AudioBroadcastReject {
                        device_id: self.device_id_i64(),
                        device_tag: Some(device_tag.clone()),
                        channel_id: device_tag.clone(),
                        message: Some("Device rejected broadcast".to_string()),
                    }),
                    Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
                );
            }
        }

        if body.contains("<Response>") && body.contains("Catalog") {
            tracing::info!(
                "[GB28181] Raw Catalog Response from {}:\n{}",
                device_tag,
                body
            );
            let channels = self.parse_catalog_response(body);
            tracing::debug!(
                "[GB28181] parse_catalog_response returned {} channels",
                channels.len()
            );
            let channels = channels
                .into_iter()
                .map(|mut ch| {
                    if ch.parent_id.as_deref() != Some(&device_tag) {
                        tracing::debug!(
                            "[GB28181] Correcting ParentID for channel {}: {} -> {}",
                            ch.device_id,
                            ch.parent_id.as_deref().unwrap_or("<empty>"),
                            device_tag
                        );
                        ch.parent_id = Some(device_tag.clone());
                    }
                    ch
                })
                .collect::<Vec<_>>();
            for (i, ch) in channels.iter().enumerate() {
                tracing::debug!(
                    "[GB28181]   channel[{}]: id={}, name={}, parental={}",
                    i,
                    ch.device_id,
                    ch.name,
                    ch.parental
                );
            }
            if channels.is_empty() {
                tracing::info!("[GB28181] Catalog empty for device {} - IPC mode (device itself is the channel)", device_tag);
                self.set_registration_state(RegistrationState::Registered);
                let device_channel = CatalogChannel {
                    device_id: device_tag.clone(),
                    name: "Camera 01".to_string(),
                    manufacturer: None,
                    model: None,
                    status: "ON".to_string(),
                    parental: false,
                    parent_id: Some(device_tag.clone()),
                    civil_code: None,
                    address: None,
                    ip_address: None,
                    port: None,
                    owner: None,
                    secrecy: None,
                    device_type: None,
                    ptz_type: None,
                    info: None,
                };
                return (
                    Some(SignalEvent::CatalogResponse {
                        device_id: self.device_id_i64(),
                        device_tag: Some(device_tag.clone()),
                        channels: vec![device_channel],
                    }),
                    Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
                );
            }
            self.set_registration_state(RegistrationState::Registered);
            return (
                Some(SignalEvent::CatalogResponse {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                    channels,
                }),
                Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
            );
        }

        if body.contains("<CmdType>Alarm</CmdType>")
            || body.contains("<AlarmNotify>")
            || body.contains("<Alarm>")
        {
            let alarm_type =
                Self::parse_xml_field(body, "AlarmType").unwrap_or_else(|| "unknown".to_string());
            let alarm_msg = Self::parse_xml_field(body, "AlarmDescription").unwrap_or_default();
            let device_id_val = self.device_id_i64();
            return (
                Some(SignalEvent::Alarm {
                    device_id: device_id_val.to_string(),
                    alarm_type,
                    message: alarm_msg,
                    timestamp: chrono::Utc::now(),
                }),
                Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
            );
        }

        if body.contains("Catalog") || body.contains("Query") {
            let channel_id =
                Self::parse_channel_id(body).unwrap_or_else(|| "00000000000000000000".to_string());
            let catalog_xml = self.build_catalog_response(&channel_id);
            tracing::info!(
                "[GB28181] Catalog Response sent to {} (channel={}):\n{}",
                device_tag,
                channel_id,
                catalog_xml
            );
            let resp = format!(
                "{}\r\n{}",
                self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)
                    .trim_end_matches("\r\n\r\n"),
                catalog_xml
            );
            return (
                Some(SignalEvent::QueryDeviceInfo {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                }),
                Some(resp),
            );
        }
        if body.contains("DeviceInfo") {
            let resp = format!(
                "{}{}",
                self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None),
                self.build_device_info_response()
            );
            return (
                Some(SignalEvent::QueryDeviceInfo {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                }),
                Some(resp),
            );
        }
        if body.contains("DeviceStatus") {
            let resp = format!(
                "{}{}",
                self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None),
                self.build_device_status_response()
            );
            return (
                Some(SignalEvent::QueryDeviceStatus {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                }),
                Some(resp),
            );
        }
        if body.contains("ConfigDownload") || body.contains("DeviceConfig") {
            let config_type = Self::parse_xml_field(body, "ConfigType")
                .unwrap_or_else(|| "BasicParam".to_string());
            let resp = format!(
                "{}{}",
                self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None),
                self.build_device_config_response(&config_type)
            );
            tracing::info!(
                "[GB28181] DeviceConfig query: device={} type={}",
                device_tag,
                config_type
            );
            return (
                Some(SignalEvent::QueryDeviceConfig {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                    config_type,
                }),
                Some(resp),
            );
        }
        if body.contains("ListPreset") || body.contains("PresetList") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| device_tag.clone());
            let resp = format!(
                "{}{}",
                self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None),
                self.build_preset_list_response(&channel_id)
            );
            tracing::info!(
                "[GB28181] PresetList query: device={} channel={}",
                device_tag,
                channel_id
            );
            return (
                Some(SignalEvent::PresetQuery {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                    channel_id,
                }),
                Some(resp),
            );
        }
        if body.contains("SetPreset") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| device_tag.clone());
            let preset_id =
                Self::parse_xml_field(body, "PresetID").unwrap_or_else(|| "1".to_string());
            let preset_name =
                Self::parse_xml_field(body, "PresetName").unwrap_or_else(|| "Preset".to_string());
            let resp = format!(
                "{}{}",
                self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None),
                self.build_preset_response(&channel_id, &preset_id, &preset_name, "SetPreset")
            );
            tracing::info!(
                "[GB28181] SetPreset: device={} channel={} id={} name={}",
                device_tag,
                channel_id,
                preset_id,
                preset_name
            );
            return (
                Some(SignalEvent::PresetSet {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                    channel_id,
                    preset_name,
                }),
                Some(resp),
            );
        }
        if body.contains("GotoPreset") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| device_tag.clone());
            let preset_index = Self::parse_xml_field(body, "PresetIndex")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            let resp = format!(
                "{}{}",
                self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None),
                self.build_preset_response(
                    &channel_id,
                    &preset_index.to_string(),
                    "",
                    "GotoPreset"
                )
            );
            tracing::info!(
                "[GB28181] GotoPreset: device={} channel={} index={}",
                device_tag,
                channel_id,
                preset_index
            );
            return (
                Some(SignalEvent::PresetGoto {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                    channel_id,
                    preset_index,
                }),
                Some(resp),
            );
        }
        if body.contains("RemovePreset") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| device_tag.clone());
            let preset_index = Self::parse_xml_field(body, "PresetID")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            let resp = format!(
                "{}{}",
                self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None),
                self.build_preset_response(
                    &channel_id,
                    &preset_index.to_string(),
                    "",
                    "RemovePreset"
                )
            );
            tracing::info!(
                "[GB28181] RemovePreset: device={} channel={} index={}",
                device_tag,
                channel_id,
                preset_index
            );
            return (
                Some(SignalEvent::PresetRemove {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                    channel_id,
                    preset_index,
                }),
                Some(resp),
            );
        }

        match self.registration_state {
            RegistrationState::Init => {
                tracing::warn!("[GB28181] Keepalive in Init state, sending 401 to trigger re-registration");
                let server_realm = super::get_gb28181_platform_config()
                    .get()
                    .map(|c| c.1.clone())
                    .unwrap_or_else(|| self.device_tag_str());
                let resp = self.build_401_response(&via_branch, &cseq, &server_realm);
                return (None, Some(resp));
            }
            RegistrationState::Registered => {
                tracing::info!("[GB28181] Keepalive, Registered state");
            }
            RegistrationState::CatalogSent => {
                tracing::info!(
                    "[GB28181] Keepalive, CatalogSent state, waiting for catalog response"
                );
            }
            RegistrationState::Failed => {
                tracing::warn!("[GB28181] Keepalive in Failed state");
            }
        }
        (
            Some(SignalEvent::DeviceKeepalive {
                device_id: self.device_id_i64(),
                device_tag: Some(device_tag.clone()),
                timestamp: chrono::Utc::now(),
            }),
            Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
        )
    }

    async fn handle_bye(
        &mut self,
        headers: &[(String, String)],
    ) -> (Option<SignalEvent>, Option<String>) {
        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);
        let call_id = Self::get_header(headers, "Call-ID").unwrap_or_default();
        let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
        let to_hdr = Self::get_header(headers, "To").unwrap_or_default();
        let (request_from_uri, request_from_tag) = Self::parse_from_to_uri_and_tag(&from_hdr);
        let (request_to_uri, _) = Self::parse_from_to_uri_and_tag(&to_hdr);

        if self.registration_state == RegistrationState::Init {
            tracing::warn!("[GB28181] BYE: device not registered, rejecting");
            return (None, Some(self.build_403_response(&via_branch, &cseq)));
        }

        let sub_session = {
            let sessions = self.sessions.read().await;
            sessions.iter().find(|s| s.call_id == call_id).cloned()
        };

        if let Some(session) = sub_session {
            if session.device_tag == session.channel_id {
                tracing::info!(
                    "[GB28181] Single device BYE: channel={}, stream={}, session_type={}",
                    session.channel_id,
                    session.stream_key,
                    if session.session_type == SessionType::Broadcast { "Broadcast" } else { "Play/Playback" }
                );
                if session.session_type != SessionType::Broadcast {
                    if let Some(app) = self.get_device_app(&session.channel_id).await {
                        let _ = self
                            .deps
                            .stream_manager
                            .stop_stream(&app, &session.stream_key)
                            .await;
                    }
                } else {
                    tracing::info!("[GB28181] Broadcast BYE - not stopping media stream (internal to platform)");
                }

                self.sessions.write().await.retain(|s| s.call_id != call_id);
                return (
                    Some(SignalEvent::StopPlay {
                        device_id: self.device_id_i64(),
                        device_tag: Some(session.channel_id.clone()),
                        session_id: session.stream_key.clone(),
                        session_type: session.session_type.clone(),
                    }),
                    Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
                );
            }
            tracing::info!(
                "[GB28181] Sub-device BYE: channel={}, stream={}",
                session.channel_id,
                session.stream_key
            );
            if session.session_type != SessionType::Broadcast {
                if let Some(app) = self.get_device_app(&session.channel_id).await {
                    let _ = self
                        .deps
                        .stream_manager
                        .stop_stream(&app, &session.stream_key)
                        .await;
                }
            } else {
                tracing::info!("[GB28181] Sub-device Broadcast BYE - not stopping media stream (internal to platform)");
            }

            self.sessions.write().await.retain(|s| s.call_id != call_id);
            return (
                Some(SignalEvent::StopPlay {
                    device_id: self.device_id_i64(),
                    device_tag: Some(session.channel_id.clone()),
                    session_id: session.stream_key.clone(),
                    session_type: session.session_type.clone(),
                }),
                Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
            );
        }
        tracing::warn!(
            "[GB28181] BYE with unknown Call-ID: {}, responding OK without modifying sessions",
            call_id
        );

        (
            None,
            Some(self.build_sip_response_only_headers_with_to(200, "OK", &via_branch, &cseq, Some(&request_from_uri), request_from_tag.as_deref(), Some(&request_to_uri), None, None, Some(&call_id), None)),
        )
    }

    async fn handle_ack(
        &self,
        headers: &[(String, String)],
    ) -> (Option<SignalEvent>, Option<String>) {
        let call_id = Self::get_header(headers, "Call-ID").unwrap_or_default();
        
        if let Some((transport, node)) = self.pending_tcp_active.write().await.remove(&call_id) {
            tracing::info!("[GB28181] ACK received for TCP active, connecting and activating");
            match transport.connect().await {
                Ok(()) => {
                    transport.activate(node.clone());
                    tracing::info!("[GB28181] TCP active transport activated and pending_tcp_active cleaned");
                }
                Err(e) => {
                    tracing::error!("[GB28181] TCP active connect failed: {}", e);
                }
            }
        }
        
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.iter().find(|s| s.call_id == call_id) {
            tracing::info!(
                "[GB28181] ACK for session: call_id={} channel={} stream={}",
                call_id,
                session.channel_id,
                session.stream_key
            );

            if let Some(ref source_node_id) = session.node_id {
                if session.session_type == SessionType::Broadcast {
                    let event = crate::protocol::event::SignalEvent::AudioTalkReady {
                        node_id: source_node_id.clone(),
                    };
                    tracing::info!("[GB28181] Publishing AudioTalkReady for broadcast after ACK, source_node_id={}", source_node_id.id);
                    let _ = self.deps.event_publisher.publish(event).await;
                }
            }
        } else {
            tracing::warn!(
                "[GB28181] ACK for unknown session: {}, active_sessions={}",
                call_id,
                sessions.len()
            );
        }
        (None, None)
    }

    async fn handle_sip_response(
        &mut self,
        first_line: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        let call_id = Self::get_header(headers, "Call-ID").unwrap_or_default();
        let cseq_line = Self::get_header(headers, "CSeq").unwrap_or_default();
        let is_invite_response = cseq_line.to_lowercase().contains("invite");

        let sub_session = self
            .sessions
            .read()
            .await
            .iter()
            .find(|s| s.call_id == call_id)
            .cloned();

        let Some(session) = sub_session else {
            tracing::debug!(
                "[GB28181] Response for unknown session: {} status={}, cseq={}",
                call_id,
                first_line,
                cseq_line
            );
            return (None, None);
        };

        tracing::info!(
            "[GB28181] Response for sub-session: channel={} call_id={} status={}",
            session.channel_id,
            call_id,
            first_line
        );

        if !is_invite_response {
            return (None, None);
        }

        let is_200 = first_line.contains(" 200 ");

        let sdp_type = if session.session_type == SessionType::Playback {
            "Playback"
        } else {
            "Real-time"
        };

        let parse_error_code = |line: &str| -> Option<u16> {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                parts[1].parse().ok()
            } else {
                None
            }
        };

        let error_message = if first_line.contains(" 488 ") {
            Some("设备在请求的时间段内没有录像".to_string())
        } else if first_line.contains(" 404 ") {
            Some("设备不存在".to_string())
        } else if first_line.contains(" 401 ") || first_line.contains(" 403 ") {
            Some("认证失败".to_string())
        } else if first_line.contains(" 600 ") {
            Some("设备忙".to_string())
        } else if first_line.contains(" 500 ") || first_line.contains(" 503 ") {
            Some("服务器错误".to_string())
        } else {
            None
        };

        if let Some(msg) = error_message {
            let error_code = parse_error_code(first_line);
            tracing::warn!(
                "[GB28181] {} INVITE rejected: channel={}, call_id={}, error={}",
                sdp_type,
                session.channel_id,
                call_id,
                msg
            );

            if session.session_type == SessionType::Playback {
                let device_id = self.device_id.unwrap_or(0);
                let event = crate::protocol::event::SignalEvent::PlaybackResult {
                    device_id,
                    device_tag: self.device_tag.clone(),
                    session_id: call_id.clone(),
                    success: false,
                    error_code,
                    error_message: Some(msg),
                };
                if let Err(e) = self.deps.event_publisher.publish(event).await {
                    tracing::warn!("[GB28181] Failed to publish PlaybackResult event: {}", e);
                }
            }

            self.sessions.write().await.retain(|s| s.call_id != call_id);
            return (None, None);
        }

        if !is_200 {
            return (None, None);
        }

        let udp_peer_str = self
            .udp_peer
            .map(|a| a.to_string())
            .unwrap_or_else(|| "NONE".to_string());
        let via_header = Self::get_header(headers, "Via").unwrap_or_else(|| "MISSING".to_string());
        let to_header = Self::get_header(headers, "To").unwrap_or_else(|| "MISSING".to_string());
        let contact_header =
            Self::get_header(headers, "Contact").unwrap_or_else(|| "MISSING".to_string());
        tracing::info!("[GB28181] 200 OK received: call_id={}, udp_peer={}\n  Via: {}\n  To: {}\n  Contact: {}",
            call_id, udp_peer_str, via_header, to_header, contact_header);

        let via_received = Self::parse_via_received(headers);
        let via_rport = Self::parse_via_rport(headers);

        if let (Some(received_ip), Some(rport)) = (&via_received, via_rport) {
            tracing::debug!(
                "[GB28181] NAT detected: device at {}:{}",
                received_ip,
                rport
            );
            let sessions = &mut *self.sessions.write().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.call_id == call_id) {
                s.device_nated_ip = Some(received_ip.clone());
                s.device_nated_port = Some(rport);
            }
        }

        let to_tag = Self::get_header(headers, "To").and_then(|v| Self::parse_tag_from_header(&v));

        if let Some(expected_tag) = &session.to_tag {
            if let Some(received_tag) = &to_tag {
                if received_tag != expected_tag {
                    tracing::warn!(
                        "[GB28181] To-tag mismatch! expected={} received={}, Call-ID={}",
                        expected_tag,
                        received_tag,
                        session.call_id
                    );
                }
            }
        }

        let to_tag_to_use = to_tag.clone().unwrap_or_default();
        if session.to_tag.is_none() && to_tag.is_some() {
            let sessions = &mut *self.sessions.write().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.call_id == call_id) {
                s.to_tag = to_tag.clone();
            }
            tracing::debug!(
                "[GB28181] Stored device to_tag={} for channel={}",
                to_tag.as_deref().unwrap_or(""),
                session.channel_id
            );
        }

        // 解析200 OK的Contact头 - 用于ACK的Request-URI和To头
        // Contact头格式: <sip:device_id@device_ip:device_port> 或 sip:device_id@device_ip:device_port
        // 根据GB28181标准，ACK应该发送到Contact头指定的地址
        // 返回 (device_id, host, port) - device_id用于ACK的To头
        let use_contact = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Contact"))
            .and_then(|(k, v)| {
                let v = v.trim();
                tracing::debug!("[GB28181] Raw Contact header value: {:?}", v);
                let uri = v
                    .strip_prefix('<')
                    .and_then(|s| s.strip_suffix('>'))
                    .or_else(|| Some(v))
                    .map(|s| s.to_string())?;
                tracing::debug!("[GB28181] Contact URI after stripping <>: {:?}", uri);

                // 解析 sip:device_id@host:port 格式
                // 例如: sip:12010100001320000011@192.168.1.211:5060
                let without_sip = uri
                    .strip_prefix("sip:")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| uri.clone());
                tracing::debug!("[GB28181] Contact without_sip: {:?}", without_sip);
                // rsplitn(2, '@') 返回从右到左的元素
                // 对于 "device_id@host:port" -> parts[0]="host:port", parts[1]="device_id"
                let parts: Vec<&str> = without_sip.rsplitn(2, '@').collect();
                tracing::debug!("[GB28181] Contact parts after rsplitn(2, '@'): {:?}", parts);
                if parts.len() == 2 {
                    // parts[0] 是 host:port (右侧，rsplit从右开始)
                    let host_port = parts[0].to_string();
                    // parts[1] 是 device_id (左侧)
                    let device_id = parts[1].to_string();
                    tracing::debug!(
                        "[GB28181] Contact parsed: device_id={}, host_port={}",
                        device_id,
                        host_port
                    );
                    let hp_parts: Vec<&str> = host_port.rsplitn(2, ':').collect();
                    tracing::debug!(
                        "[GB28181] Contact host_port parts after rsplitn(2, ':'): {:?}",
                        hp_parts
                    );
                    if hp_parts.len() == 2 {
                        // hp_parts[0] = port, hp_parts[1] = host
                        let host = hp_parts[1];
                        let port: u16 = hp_parts[0].parse().ok()?;
                        tracing::debug!(
                            "[GB28181] Contact parsed final: device_id={}, host={}, port={}",
                            device_id,
                            host,
                            port
                        );
                        Some((device_id, host.to_string(), port))
                    } else {
                        // 没有端口的情况，使用默认5060
                        tracing::debug!(
                            "[GB28181] Contact parsed (no port): device_id={}, host={}",
                            device_id,
                            host_port
                        );
                        Some((device_id, host_port.to_string(), 5060))
                    }
                } else {
                    None
                }
            });

        // server_gb_domain - 平台域名
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;

        // ACK目标地址和To头:
        // 1. 优先使用Contact头中的地址 (GB28181标准)
        // 2. 如果Contact无效，用 device_id@server_gb_domain (无端口)
        // ack_target用于ACK的Request-URI，ack_to_uri用于ACK的To头(包含device_id)
        let (ack_target, ack_to_uri) = match use_contact {
            Some((device_id, host, port)) if port > 0 => {
                // Contact头有有效端口，使用它
                tracing::debug!(
                    "[GB28181] Using Contact for ACK: device_id={} {}:{}",
                    device_id,
                    host,
                    port
                );
                let target = format!("sip:{}@{}:{}", device_id, host, port);
                // ACK的To头需要包含完整的 device_id@host:port
                let to_uri = format!("{}@{}:{}", device_id, host, port);
                (target, to_uri)
            }
            _ => {
                // Contact无效，使用server_gb_domain (无端口)
                tracing::debug!(
                    "[GB28181] Using server_gb_domain for ACK: {}",
                    server_gb_domain
                );
                let target = format!("sip:{}@{}", session.channel_id, server_gb_domain);
                let to_uri = format!("{}@{}", session.channel_id, server_gb_domain);
                (target, to_uri)
            }
        };

        // Via头 - 格式: SIP/2.0/UDP IP:端口;rport;branch=xxx (rport在前)
        let via = if session.transport == TransportType::Tcp {
            format!(
                "SIP/2.0/TCP {}:{};rport;branch={}",
                session.server_ip, session.server_port, session.via_branch
            )
        } else {
            format!(
                "SIP/2.0/UDP {}:{};rport;branch={}",
                session.server_ip, session.server_port, session.via_branch
            )
        };

        // From URI - session.from_uri 格式: 平台ID@本地IP:端口 (无sip:前缀)
        // from_uri_full 格式: sip:31011500001000000001@192.168.1.7:5061
        let from_uri_full = if session.from_uri.starts_with("sip:") {
            session.from_uri.clone()
        } else {
            format!("sip:{}", session.from_uri)
        };

        // To头 - 格式: <sip:device_id@host:port>;tag=xxx
        // to_tag_to_use 来自200 OK的To头中的tag
        let to_tag_part = if to_tag_to_use.is_empty() {
            String::new()
        } else {
            format!(";tag={}", to_tag_to_use)
        };

        // ACK消息格式 (RFC 3261):
        // ACK sip:34020000001320000003@3402000000 SIP/2.0
        // Via: SIP/2.0/UDP 39.100.155.146:15063;rport;branch=z9hG4bKxxx
        // From: <sip:34020000002000000001@39.100.155.146:15063>;tag=xxx
        // To: <sip:34020000001320000003@3402000000>;tag=xxx
        // Call-ID: xxx
        // CSeq: xxx ACK
        // Content-Length: 0
        let ack = format!(
            "ACK {ack_target} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <{from_uri}>;tag={from_tag}\r\n\
             To: <sip:{to_uri}>{to_tag}\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {seq} ACK\r\n\
             Max-Forwards: 70\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\
             \r\n",
            ack_target = ack_target,
            via = via,
            from_uri = from_uri_full,
            from_tag = session.from_tag,
            to_uri = ack_to_uri,
            to_tag = to_tag_part,
            call_id = session.call_id,
            seq = session.seq
        );

        tracing::info!(
            "[GB28181] Sending ACK: {}\n--- ACK START ---\n{}\n--- ACK END ---",
            ack_target,
            ack
        );
        if let Err(e) = self.send(ack.as_bytes()).await {
            tracing::error!("[GB28181] ACK send failed: {}", e);
        } else {
            tracing::info!("[GB28181] ACK sent successfully");
        }

        let channel_tag = &session.channel_id;
        let device_tag = self.device_tag_str();
        tracing::debug!("[GB28181] 200 OK received, starting stream: device={} channel={} stream={} media_server={}", device_tag, channel_tag, session.stream_key, session.media_server_name);
        let start_result = self
            .deps
            .stream_manager
            .start_gb28181_stream(&device_tag, channel_tag, &session.stream_key)
            .await;

        if start_result.is_ok() {
            tracing::debug!(
                "[GB28181] Stream manager started successfully, updating session state"
            );
            let sessions = &mut *self.sessions.write().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.call_id == call_id) {
                s.state = SubSessionState::Ok;
            }

            if let Some(app) = self.get_device_app(&session.channel_id).await {
                if let Some(stream) = self
                    .deps
                    .stream_manager
                    .get_stream_by_stream_key(&app, &session.stream_key)
                    .await
                {
                    tracing::debug!("[GB28181] Stream found in manager, starting stream");
                    let mut s = stream.clone();
                    s.start();
                    let _ = self.deps.stream_manager.update_stream_state(&s).await;
                }
            }

            tracing::info!(
                "[GB28181] Stream started: channel={} stream={}",
                session.channel_id,
                session.stream_key
            );

            if session.session_type == SessionType::Playback {
                let event = crate::protocol::event::SignalEvent::PlaybackResult {
                    device_id: self.device_id.unwrap_or(0),
                    device_tag: self.device_tag.clone(),
                    session_id: call_id.clone(),
                    success: true,
                    error_code: None,
                    error_message: None,
                };
                if let Err(e) = self.deps.event_publisher.publish(event).await {
                    tracing::warn!("[GB28181] Failed to publish PlaybackResult event: {}", e);
                }
            }
        } else {
            tracing::error!(
                "[GB28181] Failed to start stream: channel={} err={:?}",
                session.channel_id,
                start_result
            );

            if session.session_type == SessionType::Playback {
                let event = crate::protocol::event::SignalEvent::PlaybackResult {
                    device_id: self.device_id.unwrap_or(0),
                    device_tag: self.device_tag.clone(),
                    session_id: call_id.clone(),
                    success: false,
                    error_code: None,
                    error_message: Some("流启动失败".to_string()),
                };
                if let Err(e) = self.deps.event_publisher.publish(event).await {
                    tracing::warn!("[GB28181] Failed to publish PlaybackResult event: {}", e);
                }
            }
        }

        (
            Some(SignalEvent::StartPlay {
                device_id: self.device_id_i64(),
                device_tag: Some(session.channel_id.clone()),
                session_id: session.stream_key.clone(),
                channel_id: Some(session.channel_id.clone()),
                transport: TransportType::Udp,
                media_server_name: Some(session.media_server_name.clone()),
                session_type: session.session_type.clone(),
            }),
            None,
        )
    }

    fn handle_options(&mut self, headers: &[(String, String)]) -> Option<String> {
        let call_id = Self::get_header(headers, "Call-ID")
            .unwrap_or_else(|| self.call_id_str());
        Some(self.build_sip_response_only_headers_with_to(
            200,
            "OK",
            &Self::parse_via_branch(headers),
            &Self::parse_cseq_line(headers),
            None,
            None,
            None,
            None,
            None,
            Some(&call_id),
            None,
        ))
    }

    async fn handle_subscribe(
        &mut self,
        headers: &[(String, String)],
        body: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);

        if self.registration_state == RegistrationState::Init {
            tracing::warn!("[GB28181] SUBSCRIBE: device not registered, rejecting");
            return (None, Some(self.build_403_response(&via_branch, &cseq)));
        }

        let event_type = if body.contains("Event") {
            Self::parse_xml_field(body, "Event").unwrap_or_else(|| "presence".to_string())
        } else {
            "presence".to_string()
        };

        let expires = Self::get_header(headers, "Expires")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(3600);

        let device_tag = self.device_tag_str();
        let device_id = self.device_id_i64();

        let sub_key = format!("{}:{}", device_id, event_type);
        let subscription = Subscription {
            event_type: event_type.clone(),
            expires,
            created_at: chrono::Utc::now(),
        };
        self.subscriptions
            .write()
            .await
            .insert(sub_key, subscription);

        tracing::info!(
            "[GB28181] Device {} subscribed to event={} expires={}s",
            device_tag,
            event_type,
            expires
        );

        let resp = format!(
            "SIP/2.0 200 OK\r\n\
             Via: {}\r\n\
             From: <sip:{}>\r\n\
             To: <sip:{}>;tag={}\r\n\
             Call-ID: {}\r\n\
             CSeq: {} SUBSCRIBE\r\n\
             Expires: {}\r\n\
             Contact: <sip:{}>\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            self.via_header(&via_branch),
            device_tag,
            self.deps.config.signaling_server.server_gb_id,
            self.to_tag.as_deref().unwrap_or(""),
            Self::get_header(headers, "Call-ID").unwrap_or_default(),
            cseq,
            expires,
            device_tag
        );

        (
            Some(SignalEvent::DeviceSubscribe {
                device_id,
                device_tag: Some(device_tag),
                event_types: vec![event_type],
                expires,
            }),
            Some(resp),
        )
    }

    pub async fn cleanup_expired_subscriptions(&self) -> usize {
        let mut count = 0;
        let now = chrono::Utc::now();
        let mut subs = self.subscriptions.write().await;
        let keys_to_remove: Vec<String> = subs
            .iter()
            .filter(|(_, sub)| {
                let age = now.signed_duration_since(sub.created_at);
                age.num_seconds() as u32 >= sub.expires
            })
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys_to_remove {
            subs.remove(&key);
            count += 1;
        }
        if count > 0 {
            tracing::info!("[GB28181] Cleaned up {} expired subscriptions", count);
        }
        count
    }

    async fn handle_audio_invite_from_device(
        &mut self,
        headers: &[(String, String)],
        body: &str,
        first_line: &str,
        via_branch: &str,
        cseq: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        let device_tag = self.device_tag_str();

        if self.to_tag.is_none() {
            self.to_tag = Some(uuid::Uuid::new_v4().to_string().replace("-", ""));
            tracing::debug!("[GB28181] Generated to_tag for audio INVITE: {}", self.to_tag.as_ref().unwrap());
        }

        let sdp_info = self.parse_audio_invite_sdp(body);

        if let Some((ssrc, device_ip, device_audio_port)) = sdp_info {
            tracing::info!(
                "[GB28181] Audio INVITE from device: {} port:{} ssrc={}",
                device_ip,
                device_audio_port,
                ssrc
            );

            let setup_mode = self.parse_setup_mode(body);
            tracing::info!("[GB28181] Media setup mode: {:?}", setup_mode);

            let direction = Self::parse_broadcast_direction(headers);
            tracing::info!("[GB28181] Broadcast direction: {:?}", direction);

            let app = match self.deps.device_lookup.find_by_tag(&device_tag).await {
                Some(d) => d.app.unwrap_or_else(|| "default".to_string()),
                None => "default".to_string(),
            };
            let broadcast_key = format!("{}_{}_broadcast", app, device_tag);
            let session_key_device = format!("{}_{}", app, device_tag);

            let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
            let (from_uri, _) = Self::parse_from_to_uri_and_tag(&from_hdr);
            let invite_device_id = Self::parse_device_id_from_from_uri(&from_uri);
            tracing::info!("[GB28181] INVITE device_id from From URI: {:?}", invite_device_id);

            let source_node_id = {
                let mut pending = self.pending_audio_talks.write().await;
                if let Some(pos) = pending.iter().position(|e| {
                    if let Some(ref invite_id) = invite_device_id {
                        e.node_id.device_tag == *invite_id
                    } else {
                        e.node_id.device_tag == device_tag
                    }
                }) {
                    tracing::info!("[GB28181] Matched pending entry by device_tag, removing from pending");
                    pending.remove(pos).node_id
                } else {
                    tracing::error!("[GB28181] No matching pending audio talk found for device {} (invite_device_id: {:?})", device_tag, invite_device_id);
                    let error_resp = self.build_sip_response_only_headers(
                        500, "Internal Server Error", via_branch, cseq, None
                    );
                    return (None, Some(error_resp));
                }
            };

            let channel_id = source_node_id.channel_tag.clone().unwrap_or_else(|| device_tag.clone());

            match setup_mode {
                SetupMode::Udp => {
                    if let Some(manager) = crate::media::get_rtp_server_manager() {
                        match manager
                            .allocate_udp(&device_tag, &device_tag, "broadcast_voice")
                            .await
                        {
                            Ok((port, allocated_ssrc, _)) => {
                                tracing::info!("[GB28181] UDP port allocated for broadcast: {} -> device {}:{}", port, device_ip, device_audio_port);

                                let peer_addr: SocketAddr =
                                    format!("{}:{}", device_ip, device_audio_port)
                                        .parse()
                                        .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

                                if let Some(udp_server) = manager.get_udp_server(port).await {
                                    let transport_ssrc = if ssrc != 0 { ssrc } else { allocated_ssrc };
                                    let transport =
                                        crate::media::GB28181BroadcastVoiceTransport::new_udp(
                                            udp_server,
                                            peer_addr,
                                            transport_ssrc,
                                            8,
                                        );

                                    let node_id = crate::media::MediaNodeId::new(
                                        crate::media::MediaNodeType::GB28181_BROADCAST_VOICE,
                                        &self.device_tag.clone().unwrap_or_default(),
                                        Some(channel_id.clone()),
                                    );
                                    let node: Arc<dyn crate::media::MediaNode> =
                                        Arc::new(crate::media::BasicMediaNode::new(
                                            node_id,
                                            transport.clone(),
                                        ));

                                    node.add_send_chain(
                                        crate::media::MediaNodeType::WebSocket,
                                        Box::new(SendToReceiversMiddleware::new(
                                            Arc::downgrade(&node),
                                            crate::media::MediaNodeType::WebSocket,
                                        )),
                                    );

                                    transport.activate(node.clone());

                                    if let Some(mgr) = crate::media::get_media_session_manager() {
                                        let session = mgr.get_or_create(&device_tag);
                                        session.start_broadcast(&channel_id.clone(), node.clone());
                                        if let Err(e) = session.add_node(node.clone()) {
                                            tracing::warn!("[GB28181] Failed to add broadcast node to session: {}", e);
                                        } else {
                                            tracing::info!(
                                                "[GB28181] Broadcast MediaNode added to session"
                                            );
                                        }
                                    }
                                }

                                let response_sdp =
                                    self.build_audio_invite_response_udp(port, if ssrc != 0 { ssrc } else { allocated_ssrc }, direction);

                                let to_hdr = Self::get_header(headers, "To").unwrap_or_default();
                                let (to_uri, _) = Self::parse_from_to_uri_and_tag(&to_hdr);
                                tracing::info!("[GB28181] Audio INVITE response To URI: {}", to_uri);

                                let contact_uri = format!(
                                    "sip:{}@{}:{}",
                                    self.deps.config.signaling_server.server_gb_id,
                                    self.deps.config.signaling_server.rtp_bind_addr,
                                    self.deps.config.signaling_server.udp_signaling_port
                                );
                                tracing::debug!("[GB28181] Audio INVITE Contact URI: {}", contact_uri);

                                let call_id = Self::get_header(headers, "Call-ID")
                                    .unwrap_or_else(|| self.call_id_str());
                                tracing::debug!("[GB28181] Audio INVITE Call-ID: {}", call_id);

                                let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
                                let (from_uri, from_tag) = Self::parse_from_to_uri_and_tag(&from_hdr);
                                tracing::debug!("[GB28181] Audio INVITE From URI: {}, tag: {:?}", from_uri, from_tag);

                                let cseq_num: u32 = cseq.split_whitespace().next()
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(1);

                                let session = SubSession {
                                    call_id: call_id.clone(),
                                    session_type: SessionType::Broadcast,
                                    device_tag: device_tag.clone(),
                                    seq: cseq_num,
                                    channel_id: channel_id.clone(),
                                    stream_key: format!("{}_{}_broadcast", app, device_tag),
                                    media_server_name: "default".to_string(),
                                    from_uri: format!("sip:{}", from_uri),
                                    to_uri: to_uri.clone(),
                                    from_tag: from_tag.clone().unwrap_or_default(),
                                    to_tag: self.to_tag.clone(),
                                    via_branch: via_branch.to_string(),
                                    server_ip: self.deps.config.signaling_server.rtp_bind_addr.clone(),
                                    server_port: port,
                                    transport: TransportType::Tcp,
                                    device_host: device_ip.clone(),
                                    device_port: device_audio_port,
                                    rtp_port: port,
                                    state: SubSessionState::Ok,
                                    retry_count: 0,
                                    max_retries: 3,
                                    device_nated_ip: None,
                                    device_nated_port: None,
                                    node_id: Some(source_node_id.clone()),
                                };

                                {
                                    let mut sessions = self.sessions.write().await;
                                    sessions.push(session);
                                    tracing::info!("[GB28181] Added broadcast SubSession for call_id={} with source_node_id={}", call_id, source_node_id.id);
                                }

                                let response = format!(
                                    "{}\r\n{}",
                                    self.build_sip_response_only_headers_with_to(
                                        200, "OK", via_branch, cseq, 
                                        Some(&from_uri), from_tag.as_deref(),
                                        Some(&to_uri), None,
                                        Some(&contact_uri), Some(&call_id),
                                        Some(response_sdp.len())
                                    ),
                                    response_sdp
                                );

                                let event = crate::protocol::event::SignalEvent::AudioTalkReady {
                                    node_id: source_node_id.clone(),
                                };
                                tracing::info!("[GB28181] Publishing AudioTalkReady for source_node_id={}", source_node_id.id);
                                let _ = self.deps.event_publisher.publish(event).await;

                                return (
                                    Some(SignalEvent::AudioBroadcastInvite {
                                        device_id: self.device_id_i64(),
                                        device_tag: Some(device_tag.clone()),
                                        ssrc,
                                        sdp: body.to_string(),
                                    }),
                                    Some(response),
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "[GB28181] Failed to allocate UDP port for broadcast: {}",
                                    e
                                );
                            }
                        }
                    }
                }
                SetupMode::TcpPassive => {
                    if let Some(manager) = crate::media::get_rtp_server_manager() {
                        match manager
                            .allocate_tcp(&device_tag, &device_tag, "broadcast_voice")
                            .await
                        {
                            Ok((port, allocated_ssrc, tcp_server)) => {
                                let device_tag_clone = device_tag.clone();
                                let channel_id_clone = channel_id.clone();
                                let transport_ssrc = if ssrc != 0 { ssrc } else { allocated_ssrc };

                                let transport = crate::media::GB28181BroadcastVoiceTransport::new_tcp_passive(
                                    tcp_server.clone(),
                                    transport_ssrc,
                                    8,
                                );
                                
                                tokio::spawn(async move {
                                    tracing::info!("[GB28181] TCP passive: waiting for device connection on port {}", port);
                                    match transport.accept().await {
                                        Ok(addr) => {
                                            tracing::info!("[GB28181] Device {} connected from {} for broadcast (TCP passive)", device_tag_clone, addr);
                                            
                                            let node_id = MediaNodeId::new(
                                                MediaNodeType::GB28181_BROADCAST_VOICE,
                                                &device_tag_clone,
                                                Some(channel_id_clone.clone()),
                                            );
                                            let node: Arc<dyn MediaNode> = Arc::new(
                                                BasicMediaNode::new(node_id, transport.clone())
                                            );
                                            
                                            node.add_send_chain(
                                                MediaNodeType::WebSocket,
                                                Box::new(SendToReceiversMiddleware::new(
                                                    Arc::downgrade(&node),
                                                    MediaNodeType::WebSocket,
                                                )),
                                            );
                                            
                                            transport.activate(node.clone());
                                            
                                            if let Some(mgr) = crate::media::get_media_session_manager() {
                                                let session = mgr.get_or_create(&device_tag_clone);
                                                session.start_broadcast(&channel_id_clone, node.clone());
                                                if let Err(e) = session.add_node(node.clone()) {
                                                    tracing::warn!("[GB28181] Failed to add TCP passive node to session: {}", e);
                                                } else {
                                                    tracing::info!("[GB28181] TCP passive MediaNode added to session");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("[GB28181] TCP passive accept error: {}", e);
                                        }
                                    }
                                });

                                let response_sdp =
                                    self.build_audio_invite_response_tcp(port, if ssrc != 0 { ssrc } else { allocated_ssrc });

                                let to_hdr = Self::get_header(headers, "To").unwrap_or_default();
                                let (to_uri, _) = Self::parse_from_to_uri_and_tag(&to_hdr);
                                let contact_uri = format!(
                                    "sip:{}@{}:{}",
                                    self.deps.config.signaling_server.server_gb_id,
                                    self.deps.config.signaling_server.rtp_bind_addr,
                                    self.deps.config.signaling_server.tcp_signaling_port
                                );
                                let call_id = Self::get_header(headers, "Call-ID")
                                    .unwrap_or_else(|| self.call_id_str());
                                let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
                                let (from_uri, from_tag) = Self::parse_from_to_uri_and_tag(&from_hdr);

                                let cseq_num: u32 = cseq.split_whitespace().next()
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(1);

                                let session = SubSession {
                                    call_id: call_id.clone(),
                                    session_type: SessionType::Broadcast,
                                    device_tag: device_tag.clone(),
                                    seq: cseq_num,
                                    channel_id: channel_id.clone(),
                                    stream_key: format!("{}_{}_broadcast", app, device_tag),
                                    media_server_name: "default".to_string(),
                                    from_uri: format!("sip:{}", from_uri),
                                    to_uri: to_uri.clone(),
                                    from_tag: from_tag.clone().unwrap_or_default(),
                                    to_tag: self.to_tag.clone(),
                                    via_branch: via_branch.to_string(),
                                    server_ip: self.deps.config.signaling_server.rtp_bind_addr.clone(),
                                    server_port: port,
                                    transport: TransportType::Tcp,
                                    device_host: device_ip.clone(),
                                    device_port: device_audio_port,
                                    rtp_port: port,
                                    state: SubSessionState::Ok,
                                    retry_count: 0,
                                    max_retries: 3,
                                    device_nated_ip: None,
                                    device_nated_port: None,
                                    node_id: Some(source_node_id.clone()),
                                };

                                {
                                    let mut sessions = self.sessions.write().await;
                                    sessions.push(session);
                                    tracing::info!("[GB28181] Added TCP passive broadcast SubSession for call_id={} with source_node_id={}", call_id, source_node_id.id);
                                }

                                let event = crate::protocol::event::SignalEvent::AudioTalkReady {
                                    node_id: source_node_id.clone(),
                                };
                                tracing::info!("[GB28181] Publishing AudioTalkReady for TCP passive, source_node_id={}", source_node_id.id);
                                let _ = self.deps.event_publisher.publish(event).await;

                                let response = format!(
                                    "{}\r\n{}",
                                    self.build_sip_response_only_headers_with_to(
                                        200, "OK", via_branch, cseq, 
                                        Some(&from_uri), from_tag.as_deref(),
                                        Some(&to_uri), None,
                                        Some(&contact_uri), Some(&call_id),
                                        Some(response_sdp.len())
                                    ),
                                    response_sdp
                                );

                                return (
                                    Some(SignalEvent::AudioBroadcastInvite {
                                        device_id: self.device_id_i64(),
                                        device_tag: Some(device_tag.clone()),
                                        ssrc,
                                        sdp: body.to_string(),
                                    }),
                                    Some(response),
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "[GB28181] Failed to allocate TCP port for broadcast: {}",
                                    e
                                );
                            }
                        }
                    }
                }
                SetupMode::TcpActive => {
                    tracing::info!(
                        "[GB28181] TCP active mode: platform connects to device {}:{}",
                        device_ip,
                        device_audio_port
                    );

                    if let Some(manager) = crate::media::get_rtp_server_manager() {
                        match manager
                            .allocate_tcp_active()
                            .await
                        {
                            Ok((port, allocated_ssrc, server)) => {
                                let peer_addr: SocketAddr = format!("{}:{}", device_ip, device_audio_port)
                                    .parse()
                                    .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
                                
                                let call_id = Self::get_header(headers, "Call-ID")
                                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                                let transport_ssrc = if ssrc != 0 { ssrc } else { allocated_ssrc };
                                let transport =
                                    crate::media::GB28181BroadcastVoiceTransport::new_tcp_active(
                                        server,
                                        peer_addr,
                                        transport_ssrc,
                                        8,
                                    );
                                
                                let node_id = MediaNodeId::new(
                                    MediaNodeType::GB28181_BROADCAST_VOICE,
                                    &self.device_tag.clone().unwrap_or_default(),
                                    Some(channel_id.clone()),
                                );
                                let node: Arc<dyn MediaNode> = Arc::new(
                                    BasicMediaNode::new(node_id, transport.clone())
                                );
                                
                                node.add_send_chain(
                                    MediaNodeType::WebSocket,
                                    Box::new(SendToReceiversMiddleware::new(
                                        Arc::downgrade(&node),
                                        MediaNodeType::WebSocket,
                                    )),
                                );
                                
                                self.pending_tcp_active.write().await.insert(
                                    call_id.clone(),
                                    (transport.clone(), node.clone())
                                );
                                
                                if let Some(mgr) = crate::media::get_media_session_manager() {
                                    let session = mgr.get_or_create(&device_tag);
                                    session.start_broadcast(&channel_id, node.clone());
                                    if let Err(e) = session.add_node(node.clone()) {
                                        tracing::warn!("[GB28181] Failed to add TCP active node to session: {}", e);
                                    } else {
                                        tracing::info!("[GB28181] TCP active MediaNode added to session");
                                    }
                                }

                                let response_sdp = self.build_audio_invite_response_tcp_active(
                                    port,
                                    if ssrc != 0 { ssrc } else { allocated_ssrc },
                                );

                                let to_hdr = Self::get_header(headers, "To").unwrap_or_default();
                                let (to_uri, _) = Self::parse_from_to_uri_and_tag(&to_hdr);
                                let contact_uri = format!(
                                    "sip:{}@{}:{}",
                                    self.deps.config.signaling_server.server_gb_id,
                                    self.deps.config.signaling_server.rtp_bind_addr,
                                    self.deps.config.signaling_server.tcp_signaling_port
                                );
                                let call_id = Self::get_header(headers, "Call-ID")
                                    .unwrap_or_else(|| self.call_id_str());
                                let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
                                let (from_uri, from_tag) = Self::parse_from_to_uri_and_tag(&from_hdr);

                                let cseq_num: u32 = cseq.split_whitespace().next()
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(1);

                                let session = SubSession {
                                    call_id: call_id.clone(),
                                    session_type: SessionType::Broadcast,
                                    device_tag: device_tag.clone(),
                                    seq: cseq_num,
                                    channel_id: channel_id.clone(),
                                    stream_key: format!("{}_{}_broadcast", app, device_tag),
                                    media_server_name: "default".to_string(),
                                    from_uri: format!("sip:{}", from_uri),
                                    to_uri: to_uri.clone(),
                                    from_tag: from_tag.clone().unwrap_or_default(),
                                    to_tag: self.to_tag.clone(),
                                    via_branch: via_branch.to_string(),
                                    server_ip: self.deps.config.signaling_server.rtp_bind_addr.clone(),
                                    server_port: port,
                                    transport: TransportType::Tcp,
                                    device_host: device_ip.clone(),
                                    device_port: device_audio_port,
                                    rtp_port: port,
                                    state: SubSessionState::Ok,
                                    retry_count: 0,
                                    max_retries: 3,
                                    device_nated_ip: None,
                                    device_nated_port: None,
                                    node_id: Some(source_node_id.clone()),
                                };

                                {
                                    let mut sessions = self.sessions.write().await;
                                    sessions.push(session);
                                    tracing::info!("[GB28181] Added TCP active broadcast SubSession for call_id={} with source_node_id={}", call_id, source_node_id.id);
                                }

                                let response = format!(
                                    "{}\r\n{}",
                                    self.build_sip_response_only_headers_with_to(
                                        200, "OK", via_branch, cseq, 
                                        Some(&from_uri), from_tag.as_deref(),
                                        Some(&to_uri), None,
                                        Some(&contact_uri), Some(&call_id),
                                        Some(response_sdp.len())
                                    ),
                                    response_sdp
                                );

                                return (
                                    Some(SignalEvent::AudioBroadcastInvite {
                                        device_id: self.device_id_i64(),
                                        device_tag: Some(device_tag.clone()),
                                        ssrc,
                                        sdp: body.to_string(),
                                    }),
                                    Some(response),
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "[GB28181] Failed to allocate TCP active transport: {}",
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }


        (None, None)
    }

    fn device_tag_str(&self) -> String {
        self.device_tag.clone().unwrap_or_default()
    }

    fn device_id_i64(&self) -> i64 {
        self.device_id.unwrap_or(0)
    }

    fn device_id_str(&self) -> Option<String> {
        self.device_id.map(|i| i.to_string())
    }

    fn has_timed_out(&self) -> bool {
        if let Some(timeout_secs) = self.idle_timeout() {
            if let Some(last_msg) = self.last_message_at {
                let elapsed = chrono::Utc::now().signed_duration_since(last_msg);
                return elapsed.num_seconds() as u64 > timeout_secs;
            }
        }
        false
    }
}

// #[async_trait]
// impl EventHandler for Gb28181Adapter {
//     async fn handle(&mut self, event: SignalEvent) {
//         let SignalEvent::PtzControl { device_id: did, command, speed } = event else { return; };
//
//         let dev_tag = self.device_tag_str();
//         let dev_id_str = self.device_id_str();
//         if Some(did.clone()) != dev_id_str && did != dev_tag {
//             return;
//         }
//
//         let spd = speed.unwrap_or(50);
//         let xml = match command {
//             PtzCommand::Up => Self::build_ptz_xml(&dev_tag, "up", spd),
//             PtzCommand::Down => Self::build_ptz_xml(&dev_tag, "down", spd),
//             PtzCommand::Left => Self::build_ptz_xml(&dev_tag, "left", spd),
//             PtzCommand::Right => Self::build_ptz_xml(&dev_tag, "right", spd),
//             PtzCommand::ZoomIn => Self::build_ptz_xml(&dev_tag, "zoom_in", spd),
//             PtzCommand::ZoomOut => Self::build_ptz_xml(&dev_tag, "zoom_out", spd),
//             PtzCommand::Stop => Self::build_ptz_xml(&dev_tag, "stop", 0),
//             PtzCommand::ContinuousMove { pan, tilt, zoom } => {
//                 if pan.abs() < 0.01 && tilt.abs() < 0.01 && zoom.abs() < 0.01 {
//                     Self::build_ptz_xml(&dev_tag, "stop", 0)
//                 } else {
//                     let dir = Self::best_direction(pan, tilt, zoom);
//                     let spd = ((pan.abs().max(tilt.abs()).max(zoom.abs()) * 100.0) as u8).clamp(1, 100);
//                     Self::build_ptz_xml(&dev_tag, dir, spd)
//                 }
//             }
//             _ => return,
//         };
//
//         if let Some(ref write_arc) = self.write {
//             let mut write = write_arc.write().await;
//             let _ = write.write_all(xml.as_bytes()).await;
//             let _ = write.flush().await;
//         } else if let Some(ref addr) = self.udp_peer {
//             if let Some(sender) = super::get_udp_sender() {
//                 let _ = sender.send_to(xml.as_bytes(), *addr).await;
//             }
//         }
//     }
// }

#[async_trait]
impl SignalAdapter for Gb28181Adapter {
    async fn parse(&mut self, data: &[u8]) -> Result<Vec<SignalEvent>> {
        self.recv_buffer.append(data);

        const MAX_BUFFER_SIZE: usize = 1024 * 1024;
        if self.recv_buffer.len() > MAX_BUFFER_SIZE {
            tracing::error!(
                "[GB28181] Buffer overflow, clearing. Device: {:?}",
                self.device_tag
            );
            self.recv_buffer.clear();
            return Ok(Vec::new());
        }

        let mut events = Vec::new();

        loop {
            let (sip_msg, _consumed) = match self.recv_buffer.next_message() {
                Some((msg, len)) => (msg, len),
                None => break,
            };

            self.last_message_at = Some(chrono::Utc::now());

            if let Some(ref via) = sip_msg.headers.via {
                let via_transport = via.transport.to_uppercase();
                if via_transport == "TCP" && self.transport == TransportType::Udp {
                    tracing::info!("[GB28181] Detected TCP transport from Via header, switching from UDP to TCP");
                    self.transport = TransportType::Tcp;
                }
            }

            let (first_line, headers, body) = sip_msg.to_old_format();

            let (event, response) = if first_line.starts_with("SIP/2.0") {
                self.handle_sip_response(&first_line, &headers, &body).await
            } else if first_line.starts_with("REGISTER") {
                self.handle_register(&headers, &body).await
            } else if first_line.starts_with("MESSAGE") {
                self.handle_message(&headers, &body).await
            } else if first_line.starts_with("BYE") {
                self.handle_bye(&headers).await
            } else if first_line.starts_with("ACK") {
                self.handle_ack(&headers).await
            } else if first_line.starts_with("OPTIONS") {
                let resp = self.handle_options(&headers);
                (None, resp)
            } else if first_line.starts_with("SUBSCRIBE") {
                self.handle_subscribe(&headers, &body).await
            } else if first_line.starts_with("INVITE") {
                if body.contains("m=audio") && !body.contains("m=video") {
                    let via_branch = Self::parse_via_branch(&headers);
                    let cseq = Self::parse_cseq_line(&headers);
                    self.handle_audio_invite_from_device(
                        &headers,
                        &body,
                        &first_line,
                        &via_branch,
                        &cseq,
                    )
                    .await
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            if let Some(resp) = &response {
                let peer_info = if self.write.is_some() {
                    format!("TCP (transport={:?})", self.transport)
                } else {
                    format!(
                        "UDP ({})",
                        self.udp_peer
                            .map(|a| a.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    )
                };
                tracing::debug!(
                    "[GB28181] Sending response via {}: {}",
                    peer_info,
                    &resp[..resp.len().min(200)]
                );
                if let Err(e) = self.send(resp.as_bytes()).await {
                    tracing::error!("[GB28181] Failed to send response: {}", e);
                }
            }

            if let Some(evt) = event {
                events.push(evt);
            }
        }

        Ok(events)
    }

    async fn on_connected(&mut self, addr: SocketAddr) -> Result<()> {
        self.remote_addr = Some(addr);
        Ok(())
    }

    async fn on_disconnected(&mut self) -> Result<()> {
        let device_tag = self.device_tag_str();
        if !device_tag.is_empty() {
            tracing::info!(
                "[GB28181] Device {} disconnected, cleaning {} sessions",
                device_tag,
                self.sessions.read().await.len()
            );

            let sessions: Vec<SubSession> = self.sessions.read().await.iter().cloned().collect();
            for session in &sessions {
                if let Err(e) = self.send_bye_to_session(session).await {
                    tracing::warn!(
                        "[GB28181] Failed to send BYE to {}:{}",
                        session.device_host,
                        e
                    );
                }
            }

            if let Some(ref name) = self.event_handler_name {
                registry().infra.event_bus.off(name);
            }
            let _ = self
                .deps
                .device_lookup
                .set_offline(&device_tag, Some("Connection closed"))
                .await;

            if let Err(e) = self
                .deps
                .stream_manager
                .stop_streams_by_device(&device_tag)
                .await
            {
                tracing::warn!(
                    "[GB28181] on_disconnected: failed to stop streams for device {}: {}",
                    device_tag,
                    e
                );
            }

            self.sessions.write().await.clear();
            self.pending_tcp_active.write().await.clear();
        }
        if let Some(ref dt) = self.device_tag {
            (self.unregister_fn)(dt.clone());
        }
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        let data_str = String::from_utf8_lossy(data);

        if let Some(ref write_arc) = self.write {
            tracing::info!("[GB28181] SEND TCP:\n{}", data_str);
            let mut write = write_arc.write().await;
            write
                .write_all(data)
                .await
                .map_err(|e| AppError::Internal(format!("TCP write: {}", e)))?;
            let _ = write.flush().await;
            return Ok(());
        }
        if let Some(ref addr) = self.udp_peer {
            if let Some(sender) = super::get_udp_sender() {
                tracing::info!("[GB28181] SEND UDP to {}:\n{}", addr, data_str);
                if let Err(e) = sender.send_to(data, *addr).await {
                    tracing::error!("[GB28181] UDP send failed to {}: {}", addr, e);
                    return Err(AppError::Internal(format!("UDP send: {}", e)));
                }
            } else {
                tracing::warn!("[GB28181] UDP sender not initialized");
                return Err(AppError::Internal("UDP sender not initialized".to_string()));
            }
        } else {
            tracing::warn!("[GB28181] UDP peer not set, cannot send response");
            return Err(AppError::Internal("UDP peer not set".to_string()));
        }
        Ok(())
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::Gb28181
    }
    fn name(&self) -> &'static str {
        "GB28181"
    }
    fn keepalive(&self) -> bool {
        true
    }
    fn idle_timeout(&self) -> Option<u64> {
        Some(300)
    }

    fn set_tcp_write(&mut self, write: OwnedWriteHalf) {
        self.write = Some(Arc::new(tokio::sync::RwLock::new(write)));
        if self.transport == TransportType::Udp {
            tracing::info!("[GB28181] TCP connection established, setting transport to TCP");
            self.transport = TransportType::Tcp;
        }
    }

    async fn set_udp_peer(&mut self, addr: SocketAddr) -> Result<()> {
        self.udp_peer = Some(addr);
        self.transport = TransportType::Udp;
        Ok(())
    }

    fn set_adapter_entry(&mut self, entry: crate::protocol::adapter_manager::AdapterEntry) {
        self.adapter_entry = Some(entry);
    }

    async fn start(&mut self, device_tag: &str, channel_tag: &str) -> Result<()> {
        let device = self
            .deps
            .device_lookup
            .find_by_tag(device_tag)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", device_tag)))?;

        if let Some(ext) = &device.extended {
            if let Some(transport) = ext.get("gb_transport").and_then(|v| v.as_str()) {
                let transport_str = match transport.to_uppercase().as_str() {
                    "TCP" => {
                        self.transport = TransportType::Tcp;
                        "TCP"
                    }
                    _ => {
                        self.transport = TransportType::Udp;
                        "UDP"
                    }
                };
                tracing::info!(
                    "[GB28181] Using transport {} for device {}",
                    transport_str,
                    device_tag
                );
            }
        }

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let ssrc = format!("{:08x}", rand::random::<u32>());
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        let server_port = match self.transport {
            TransportType::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportType::Udp => self.deps.config.signaling_server.udp_signaling_port,
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        };
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;

        let channel_id = if channel_tag.is_empty() {
            device_tag.to_string()
        } else {
            channel_tag.to_string()
        };
        let stream_key = format!("{}_{}", device_tag, channel_id);
        let (rtp_port, server_ip, media_server_name) = self
            .alloc_rtp_port(&stream_key, device.media_server_tag.as_deref())
            .await?;
        let device_host = device.host.clone();
        let device_port = device.port;

        // local_ip: 平台本地IP地址（设备能访问到的地址）
        // 优先级：local_addr(从socket获取) > external_ip配置 > detect_local_ip(设备IP) > remote_addr IP
        let local_ip = if let Some(ref local) = self.local_addr {
            local.ip().to_string()
        } else if let Some(ref ext_ip) = self.deps.config.signaling_server.external_ip {
            ext_ip.clone()
        } else if let Some(ref detected) = crate::protocol::gb28181::detect_local_ip(&device_host) {
            detected.clone()
        } else {
            self.remote_addr
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|| "127.0.0.1".to_string())
        };

        // Via: SIP/2.0/UDP 192.168.1.211:5060;branch=z9hG4bKa396e08;rport
        let via = if self.transport == TransportType::Tcp {
            format!("SIP/2.0/TCP {local_ip}:{server_port};branch={branch};rport")
        } else {
            format!("SIP/2.0/UDP {local_ip}:{server_port};branch={branch};rport")
        };

        // SDP c=: 媒体服务器地址 (从 ZLMediaKit URL 解析)
        // 设备发送 RTP 到这个地址
        // Example: c=IN IP4 192.168.1.100
        let sdp = self.build_sdp(
            &server_gb_id,
            &server_ip,
            rtp_port,
            &ssrc,
            device.stream_config.as_ref(),
            None,
            None,
        );
        let sdp_len = sdp.len();

        // to_uri: 格式 "设备ID@设备IP:设备端口" (设备自己的地址)
        // Example: "12010100001320000011@192.168.1.211:5060"
        let to_uri = format!("{}@{}:{}", channel_id, device_host, device_port);

        // from_uri: 格式 "平台ID@平台IP:信令端口" (使用local_ip，设备回复到这里)
        // Example: "31011500001000000001@192.168.1.211:5060"
        let from_uri = format!("{}@{}:{}", server_gb_id, local_ip, server_port);
        // Subject格式: 发送者ID:发送方序列号,接收者ID:接收方序列号
        // 实时播放时序列号用0
        let subject = format!(
            "{}:0,{}:0",
            Self::truncate_20(&channel_id),
            Self::truncate_20(server_gb_id)
        );

        let session = SubSession {
            call_id: call_id.clone(),
            session_type: SessionType::Play,
            device_tag: device_tag.to_string(),
            seq: sn,
            channel_id: channel_id.clone(),
            stream_key: stream_key.clone(),
            media_server_name: media_server_name.clone(),
            from_uri: from_uri.clone(),
            to_uri: to_uri.clone(),
            from_tag: from_tag.clone(),
            to_tag: None,
            via_branch: branch.clone(),
            server_ip: server_ip.clone(),
            server_port,
            transport: self.transport,
            device_host: device_host.clone(),
            device_port,
            rtp_port,
            state: SubSessionState::Pending,
            retry_count: 0,
            max_retries: 3,
            device_nated_ip: None,
            device_nated_port: None,
            node_id: None,
        };

        let old_session = {
            let sessions = &mut *self.sessions.write().await;
            let idx = sessions.iter().position(|s| {
                s.channel_id == session.channel_id && s.device_tag == session.device_tag
            });
            if let Some(idx) = idx {
                let old = sessions.remove(idx);
                if old.state == SubSessionState::Ok {
                    if let Some(app) = self.get_device_app(&old.channel_id).await {
                        let _ = self
                            .deps
                            .stream_manager
                            .stop_stream(&app, &old.stream_key)
                            .await;
                    }
                }
                Some(old)
            } else {
                None
            }
        };
        if old_session.is_some() {
            tracing::info!(
                "[GB28181] start: cleaning up old session for channel={}",
                old_session.as_ref().unwrap().channel_id
            );
        }
        self.sessions.write().await.push(session);

        let invite = format!(
            "INVITE sip:{to_uri} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <sip:{from_uri}>;tag={from_tag}\r\n\
             To: <sip:{to_uri}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {sn} INVITE\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Subject: {subject}\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {sdp_len}\r\n\
             \r\n\
             {sdp}",
            to_uri = to_uri,
            via = via,
            from_uri = from_uri,
            from_tag = from_tag,
            call_id = call_id,
            sn = sn,
            subject = subject,
            sdp_len = sdp_len,
            sdp = sdp
        );

        tracing::info!("[GB28181] Sending INVITE to device={}, stream={}, rtp={}, local_ip={}\n--- INVITE START ---\n{}\n--- INVITE END ---",
            device_tag, stream_key, rtp_port, local_ip, invite);
        self.send(invite.as_bytes()).await?;

        Ok(())
    }

    async fn start_playback(
        &mut self,
        device_tag: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<()> {
        let device = self
            .deps
            .device_lookup
            .find_by_tag(device_tag)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", device_tag)))?;

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let ssrc = format!("{:08x}", rand::random::<u32>());
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        let server_port = match self.transport {
            TransportType::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportType::Udp => self.deps.config.signaling_server.udp_signaling_port,
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        };
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;

        let channel_id = device_tag.to_string();
        let stream_key = format!("{}_{}", device_tag, channel_id);
        let (rtp_port, server_ip, media_server_name) = self
            .alloc_rtp_port(&stream_key, device.media_server_tag.as_deref())
            .await?;
        let device_host = device.host.clone();
        let device_port = device.port;

        // local_ip: 平台本地IP地址（设备能访问到的地址）
        let local_ip = if let Some(ref local) = self.local_addr {
            local.ip().to_string()
        } else if let Some(ref ext_ip) = self.deps.config.signaling_server.external_ip {
            ext_ip.clone()
        } else if let Some(ref detected) = crate::protocol::gb28181::detect_local_ip(&device_host) {
            detected.clone()
        } else {
            self.remote_addr
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|| "127.0.0.1".to_string())
        };

        let via = if self.transport == TransportType::Tcp {
            format!("SIP/2.0/TCP {local_ip}:{server_port};branch={branch};rport")
        } else {
            format!("SIP/2.0/UDP {local_ip}:{server_port};branch={branch};rport")
        };

        // SDP c=: 媒体服务器地址 (从 ZLMediaKit URL 解析)
        let sdp = self.build_sdp(
            &server_gb_id,
            &server_ip,
            rtp_port,
            &ssrc,
            device.stream_config.as_ref(),
            Some(start_time),
            Some(end_time),
        );
        let sdp_len = sdp.len();
        let to_uri = format!("{}@{}:{}", channel_id, device_host, device_port);
        let from_uri = format!("{}@{}:{}", server_gb_id, local_ip, server_port);
        let start_ntp = Self::datetime_to_ntp_string(start_time);
        let end_ntp = Self::datetime_to_ntp_string(end_time);
        let subject = format!(
            "{}:{}:{},{}",
            Self::truncate_20(&channel_id),
            Self::truncate_20(server_gb_id),
            start_ntp,
            end_ntp
        );

        let session = SubSession {
            call_id: call_id.clone(),
            session_type: SessionType::Playback,
            device_tag: device_tag.to_string(),
            seq: sn,
            channel_id: channel_id.clone(),
            stream_key: stream_key.clone(),
            media_server_name: media_server_name.clone(),
            from_uri: from_uri.clone(),
            to_uri: to_uri.clone(),
            from_tag: from_tag.clone(),
            to_tag: None,
            via_branch: branch.clone(),
            server_ip: server_ip.clone(),
            server_port,
            transport: self.transport,
            device_host: device_host.clone(),
            device_port,
            rtp_port,
            state: SubSessionState::Pending,
            retry_count: 0,
            max_retries: 3,
            device_nated_ip: None,
            device_nated_port: None,
            node_id: None,
        };

        let old_session = {
            let sessions = &mut *self.sessions.write().await;
            let idx = sessions.iter().position(|s| {
                s.channel_id == session.channel_id && s.device_tag == session.device_tag
            });
            if let Some(idx) = idx {
                let old = sessions.remove(idx);
                if old.state == SubSessionState::Ok {
                    if let Some(app) = self.get_device_app(&old.channel_id).await {
                        let _ = self
                            .deps
                            .stream_manager
                            .stop_stream(&app, &old.stream_key)
                            .await;
                    }
                }
                Some(old)
            } else {
                None
            }
        };
        if old_session.is_some() {
            tracing::info!(
                "[GB28181] start_playback: cleaning up old session for channel={}",
                old_session.as_ref().unwrap().channel_id
            );
        }
        self.sessions.write().await.push(session);

        let invite = format!(
            "INVITE sip:{to_uri} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <sip:{from_uri}>;tag={from_tag}\r\n\
             To: <sip:{to_uri}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {sn} INVITE\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Subject: {subject}\r\n\
             Content-Type: application/sdp\r\n\
             Content-Length: {sdp_len}\r\n\
             \r\n\
             {sdp}",
            to_uri = to_uri,
            via = via,
            from_uri = from_uri,
            from_tag = from_tag,
            call_id = call_id,
            sn = sn,
            subject = subject,
            sdp_len = sdp_len,
            sdp = sdp
        );

        tracing::info!(
            "[GB28181] Sending Playback INVITE to device={}, stream={}, rtp={}, start={}, end={}",
            device_tag,
            stream_key,
            rtp_port,
            start_ntp,
            end_ntp
        );
        self.send(invite.as_bytes()).await?;

        tracing::info!(
            "[GB28181] Playback INVITE sent, waiting for device response asynchronously"
        );
        Ok(())
    }

    async fn ptz_control(
        &mut self,
        channel_id: &str,
        command: &crate::protocol::event::PtzCommand,
        speed: Option<u8>,
    ) -> Result<()> {
        let device = self
            .deps
            .device_lookup
            .find_by_tag(channel_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", channel_id)))?;

        let spd = speed.unwrap_or(50);

        match command {
            PtzCommand::ContinuousMove { pan, tilt, zoom } => {
                if pan.abs() < 0.01 && tilt.abs() < 0.01 && zoom.abs() < 0.01 {
                    let ptz_body = Self::build_ptz_xml(channel_id, "stop", 0);
                    let msg = self.build_ptz_message(channel_id, &ptz_body);
                    tracing::debug!("[GB28181] PTZ: channel={} compound=stop", channel_id);
                    self.send(msg.as_bytes()).await?;
                } else {
                    let ptz_body =
                        Self::build_ptz_xml_compound(channel_id, *pan, *tilt, *zoom);
                    let msg = self.build_ptz_message(channel_id, &ptz_body);
                    tracing::debug!(
                        "[GB28181] PTZ: channel={} compound=pan:{:.2} tilt:{:.2} zoom:{:.2}",
                        channel_id,
                        pan,
                        tilt,
                        zoom
                    );
                    self.send(msg.as_bytes()).await?;
                }
            }
            PtzCommand::Stop => {
                let ptz_body = Self::build_ptz_xml(channel_id, "stop", 0);
                let msg = self.build_ptz_message(channel_id, &ptz_body);
                tracing::debug!("[GB28181] PTZ: channel={} dir=stop", channel_id);
                self.send(msg.as_bytes()).await?;
            }
            _ => {
                let (direction, spd_val) = match command {
                    PtzCommand::Up => ("up", spd),
                    PtzCommand::Down => ("down", spd),
                    PtzCommand::Left => ("left", spd),
                    PtzCommand::Right => ("right", spd),
                    PtzCommand::ZoomIn => ("zoom_in", spd),
                    PtzCommand::ZoomOut => ("zoom_out", spd),
                    PtzCommand::FocusIn => ("focus_in", spd),
                    PtzCommand::FocusOut => ("focus_out", spd),
                    PtzCommand::IrisIn => ("iris_in", spd),
                    PtzCommand::IrisOut => ("iris_out", spd),
                    _ => return Ok(()),
                };

                let ptz_body = Self::build_ptz_xml(channel_id, direction, spd_val);
                let msg = self.build_ptz_message(channel_id, &ptz_body);
                tracing::debug!(
                    "[GB28181] PTZ: channel={} dir={} spd={}",
                    channel_id,
                    direction,
                    spd_val
                );
                self.send(msg.as_bytes()).await?;
            }
        }
        Ok(())
    }

    async fn send_notify(
        &mut self,
        device_tag: &str,
        event_type: &str,
        content: &str,
    ) -> Result<()> {
        let device = self
            .deps
            .device_lookup
            .find_by_tag(device_tag)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", device_tag)))?;

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let to_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        let server_port = match self.transport {
            TransportType::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportType::Udp => self.deps.config.signaling_server.udp_signaling_port,
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        };
        let server_ip = &self.deps.config.signaling_server.bind_ip;
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;

        let device_host = device.host.clone();
        let device_port = device.port;

        let via = if self.transport == TransportType::Tcp {
            format!("SIP/2.0/TCP {server_ip}:{server_port};branch={branch};rport")
        } else {
            format!("SIP/2.0/UDP {server_ip}:{server_port};branch={branch};rport")
        };

        let from_uri = format!("{}@{}:{}", server_gb_id, server_ip, server_port);
        let to_uri = format!("{}@{}:{}", device_tag, device_host, device_port);

        let notify = format!(
            "NOTIFY sip:{to_uri} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <sip:{from_uri}>;domain={server_gb_domain};tag={from_tag}\r\n\
             To: <sip:{to_uri}>;tag={to_tag}\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {sn} NOTIFY\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Event: {event_type}\r\n\
             Content-Type: Application/MANSCDP+xml\r\n\
             Content-Length: {content_len}\r\n\
             \r\n\
             {content}",
            to_uri = to_uri,
            via = via,
            from_uri = from_uri,
            server_gb_domain = server_gb_domain,
            from_tag = from_tag,
            to_tag = to_tag,
            call_id = call_id,
            sn = sn,
            event_type = event_type,
            content_len = content.len(),
            content = content
        );

        tracing::info!(
            "[GB28181] Sending NOTIFY to device={} event={}",
            device_tag,
            event_type
        );
        self.send(notify.as_bytes()).await?;
        Ok(())
    }

    async fn start_audio_talk_internal(&mut self, device_tag: &str, node_id: MediaNodeId) -> Result<()> {
        tracing::info!(
            "[GB28181] start_audio_talk_internal: device={} node_id={}",
            device_tag,
            node_id.id
        );

        let device = self
            .deps
            .device_lookup
            .find_by_tag(device_tag)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", device_tag)))?;

        let device_ip = device.host.clone();
        let device_audio_port = device.port;
        let channel_tag = device_tag.to_string();

        let transport = if let Some(ext) = &device.extended {
            if let Some(t) = ext.get("gb_transport").and_then(|v| v.as_str()) {
                match t.to_uppercase().as_str() {
                    "TCP" => TransportType::Tcp,
                    _ => TransportType::Udp,
                }
            } else {
                TransportType::Udp
            }
        } else {
            TransportType::Udp
        };

        let stream_key = format!("{}_{}_audio", device_tag, channel_tag);
        let (platform_audio_port, _, _) = self
            .alloc_rtp_port(&stream_key, device.media_server_tag.as_deref())
            .await?;

        self.start_audio_talk(
            device_tag,
            &channel_tag,
            &device_ip,
            device_audio_port,
            platform_audio_port,
            transport,
            node_id,
        )
        .await
    }

    async fn stop_audio_talk_internal(&mut self, node_id: MediaNodeId) -> Result<()> {
        tracing::info!("[GB28181] stop_audio_talk_internal: node_id={}", node_id.id);

        let device_tag = self.device_tag_str();
        if device_tag.is_empty() {
            return Ok(());
        }

        let manager = match crate::media::get_media_session_manager() {
            Some(m) => m,
            None => return Ok(()),
        };

        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.iter().find(|s| s.device_tag == device_tag && s.session_type == SessionType::Broadcast) {
                tracing::info!("[GB28181] Sending BYE to device {} for audio talk stop", device_tag);
                if let Err(e) = self.send_bye_to_session(session).await {
                    tracing::warn!("[GB28181] Failed to send BYE: {}", e);
                }
            }
        }

        if let Some(session) = manager.get(&device_tag) {
            let removed = session.remove_and_stop_node(&node_id).await;
            tracing::info!(
                "[GB28181] Broadcast node removed for device {}: {}",
                device_tag,
                removed
            );
        }

        Ok(())
    }
}

impl Gb28181Adapter {
    async fn send_unregister(&mut self, device_id: &str) -> Result<()> {
        let device = self
            .deps
            .device_lookup
            .find_by_tag(device_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", device_id)))?;

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        let server_port = match self.transport {
            TransportType::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportType::Udp => self.deps.config.signaling_server.udp_signaling_port,
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        };
        let server_ip = &self.deps.config.signaling_server.bind_ip;
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;

        let device_host = device.host.clone();
        let device_port = device.port;

        let via = if self.transport == TransportType::Tcp {
            format!("SIP/2.0/TCP {server_ip}:{server_port};branch={branch};rport")
        } else {
            format!("SIP/2.0/UDP {server_ip}:{server_port};branch={branch};rport")
        };

        let from_uri = format!("{}@{}:{}", server_gb_id, server_ip, server_port);
        let to_uri = format!("{}@{}:{}", device_id, device_host, device_port);

        let register = format!(
            "REGISTER sip:{to_uri} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <sip:{from_uri}>;domain={server_gb_domain};tag={from_tag}\r\n\
             To: <sip:{to_uri}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {sn} REGISTER\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Expires: 0\r\n\
             Content-Length: 0\r\n\r\n",
            to_uri = to_uri,
            via = via,
            from_uri = from_uri,
            server_gb_domain = server_gb_domain,
            from_tag = from_tag,
            call_id = call_id,
            sn = sn
        );

        tracing::info!(
            "[GB28181] Sending REGISTER Expires:0 to device={} to force re-registration",
            device_id
        );
        self.send(register.as_bytes()).await?;
        Ok(())
    }
}

impl Gb28181Adapter {
    async fn send_broadcast_message(&mut self, target_id: &str, content: &str) -> Result<()> {
        tracing::info!("[GB28181] send_broadcast_message: BEGIN");
        tracing::info!("[GB28181]   target_id: {}", target_id);
        tracing::info!("[GB28181]   content length: {} bytes", content.len());

        let device = self
            .deps
            .device_lookup
            .find_by_tag(target_id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", target_id)))?;
        tracing::info!(
            "[GB28181]   device found: host={} port={}",
            device.host,
            device.port
        );

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        tracing::info!("[GB28181]   sequence number: {}", sn);

        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        tracing::info!("[GB28181]   call_id: {}", call_id);
        tracing::info!("[GB28181]   from_tag: {}", from_tag);
        tracing::info!("[GB28181]   branch: {}", branch);

        let server_port = match self.transport {
            TransportType::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportType::Udp => self.deps.config.signaling_server.udp_signaling_port,
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        };
        tracing::info!(
            "[GB28181]   server_port: {} (transport={:?})",
            server_port,
            self.transport
        );

        let server_ip = &self.deps.config.signaling_server.bind_ip;
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;
        tracing::info!("[GB28181]   server_ip (from config): {}", server_ip);
        tracing::info!("[GB28181]   server_gb_id: {}", server_gb_id);

        let device_host = device.host.clone();
        let device_port = device.port;

        let to_uri = format!("sip:{}@{}:{}", target_id, device_host, device_port);
        tracing::info!("[GB28181]   to_uri: {}", to_uri);

        let via = if self.transport == TransportType::Tcp {
            format!(
                "SIP/2.0/TCP {}:{};branch={};rport",
                server_ip, server_port, branch
            )
        } else {
            format!(
                "SIP/2.0/UDP {}:{};branch={};rport",
                server_ip, server_port, branch
            )
        };

        let from_uri = format!("{}@{}:{}", server_gb_id, server_ip, server_port);

        let msg = format!(
            "MESSAGE {to_uri} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <sip:{from_uri}>;domain={server_gb_domain};tag={from_tag}\r\n\
             To: <{to_uri}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {sn} MESSAGE\r\n\
             Content-Type: Application/MANSCDP+xml\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: {content_len}\r\n\
             \r\n\
             {content}",
            to_uri = to_uri,
            via = via,
            from_uri = from_uri,
            server_gb_domain = server_gb_domain,
            from_tag = from_tag,
            call_id = call_id,
            sn = sn,
            content_len = content.len(),
            content = content
        );

        tracing::info!("[GB28181]   ============================================");
        tracing::info!("[GB28181]   SIP MESSAGE to be sent ({} bytes):", msg.len());
        tracing::info!(
            "[GB28181]   {}",
            msg.lines()
                .map(|l| format!("   {}", l))
                .collect::<Vec<_>>()
                .join("\n")
        );
        tracing::info!("[GB28181]   ============================================");

        tracing::info!("[GB28181] send_broadcast_message: calling self.send()...");
        self.send(msg.as_bytes()).await?;
        tracing::info!("[GB28181] send_broadcast_message: send() completed successfully");
        tracing::info!("[GB28181] send_broadcast_message: END");
        Ok(())
    }

    pub async fn send_broadcast_notification(&mut self, target_id: &str) -> Result<()> {
        tracing::info!("[GB28181] send_broadcast_notification: BEGIN");
        tracing::info!("[GB28181]   target_id: {}", target_id);

        let xml = self.build_broadcast_notification(target_id);
        tracing::info!(
            "[GB28181]   built Broadcast Notify XML ({} bytes):",
            xml.len()
        );
        tracing::info!("[GB28181]   XML content:\n{}", xml);

        tracing::info!("[GB28181] send_broadcast_notification: calling send_broadcast_message...");
        self.send_broadcast_message(target_id, &xml).await
    }

    pub async fn notify_alarm(
        &mut self,
        device_tag: &str,
        alarm_type: &str,
        description: &str,
    ) -> Result<()> {
        let sn = chrono::Utc::now().timestamp_millis() % 1000000;
        let content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>Alarm</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<AlarmType>{}</AlarmType><AlarmPriority>0</AlarmPriority><AlarmMethod>1</AlarmMethod>
<AlarmTime>{}</AlarmTime><Description>{}</Description></Notify>"#,
            sn,
            device_tag,
            alarm_type,
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S"),
            description
        );
        self.send_notify(device_tag, "Alarm", &content).await
    }

    pub async fn notify_device_status(&mut self, device_tag: &str, online: bool) -> Result<()> {
        let sn = chrono::Utc::now().timestamp_millis() % 1000000;
        let status = if online { "ON" } else { "OFF" };
        let content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>DeviceStatus</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<Online>{}</Online><Status>OK</Status></Notify>"#,
            sn, device_tag, status
        );
        self.send_notify(device_tag, "presence", &content).await
    }

    pub fn build_audio_invite_sdp(
        &self,
        device_ip: &str,
        audio_port: u16,
        ssrc: u32,
        codecs: &[&str],
    ) -> String {
        let (platform_id, platform_ip, platform_port) =
            crate::protocol::gb28181::get_gb28181_platform_config()
                .get()
                .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
                .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));

        let codec_info: Vec<(&str, u8, &str)> = codecs
            .iter()
            .map(|c| {
                let pt = Self::codec_to_audio_pt(c);
                let (encoding, clock) = Self::codec_to_audio_rtp_map(c);
                (*c, pt, encoding)
            })
            .collect();

        let pt_list: Vec<String> = codec_info.iter().map(|(_, pt, _)| pt.to_string()).collect();
        let m_line = format!("m=audio {} RTP/AVP {}", audio_port, pt_list.join(" "));

        let mut rtpmap_lines = String::new();
        for (_, pt, encoding) in &codec_info {
            rtpmap_lines.push_str(&format!("a=rtpmap:{} {}/8000\r\n", pt, encoding));
        }

        format!(
            r#"v=0
o={} 0 0 IN IP4 {}
s=Talk
c=IN IP4 {}
t=0 0
{}
a=sendrecv
y={:010}
a=ssrc:{}
{}
"#,
            platform_id, platform_ip, platform_ip, m_line, ssrc, ssrc, rtpmap_lines
        )
    }

    pub fn build_audio_invite_sdp_tcp(
        &self,
        device_ip: &str,
        tcp_port: u16,
        ssrc: u32,
        codecs: &[&str],
    ) -> String {
        let (platform_id, platform_ip, platform_port) =
            crate::protocol::gb28181::get_gb28181_platform_config()
                .get()
                .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
                .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));

        let codec_info: Vec<(&str, u8, &str)> = codecs
            .iter()
            .map(|c| {
                let pt = Self::codec_to_audio_pt(c);
                let (encoding, clock) = Self::codec_to_audio_rtp_map(c);
                (*c, pt, encoding)
            })
            .collect();

        let pt_list: Vec<String> = codec_info.iter().map(|(_, pt, _)| pt.to_string()).collect();
        let m_line = format!("m=audio {} TCP/RTP/AVP {}", tcp_port, pt_list.join(" "));

        let mut rtpmap_lines = String::new();
        for (_, pt, encoding) in &codec_info {
            rtpmap_lines.push_str(&format!("a=rtpmap:{} {}/8000\r\n", pt, encoding));
        }

        format!(
            r#"v=0
o={} 0 0 IN IP4 {}
s=Talk
c=IN IP4 {}
t=0 0
{}
a=setup:passive
a=connection:new
a=sendrecv
y={:010}
a=ssrc:{}
{}
"#,
            platform_id, platform_ip, platform_ip, m_line, ssrc, ssrc, rtpmap_lines
        )
    }

    fn build_audio_invite_response_tcp(&self, port: u16, ssrc: u32) -> String {
        let (platform_id, platform_ip, _) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
            .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));

        format!(
            "v=0\r\n\
             o={} 0 0 IN IP4 {}\r\n\
             s=Play\r\n\
             c=IN IP4 {}\r\n\
             t=0 0\r\n\
             m=audio {} TCP/RTP/AVP 8\r\n\
             a=rtpmap:8 PCMA/8000\r\n\
             a=recvonly\r\n\
             y={:010}\r\n\
             a=ssrc:{}\r\n",
            platform_id, platform_ip, platform_ip, port, ssrc, ssrc
        )
    }

    fn build_audio_invite_response_tcp_active(&self, platform_port: u16, ssrc: u32) -> String {
        let (platform_id, platform_ip, _) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
            .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));

        format!(
            "v=0\r\n\
             o={} 0 0 IN IP4 {}\r\n\
             s=Play\r\n\
             c=IN IP4 {}\r\n\
             t=0 0\r\n\
             m=audio {} TCP/RTP/AVP 8\r\n\
             a=rtpmap:8 PCMA/8000\r\n\
             a=sendonly\r\n\
             a=setup:active\r\n\
             y={:010}\r\n\
             a=ssrc:{}\r\n",
            platform_id, platform_ip, platform_ip, platform_port, ssrc, ssrc
        )
    }

    fn parse_setup_mode(&self, sdp: &str) -> SetupMode {
        let has_tcp = sdp.contains("TCP/RTP/AVP") || sdp.contains("TCP");
        if !has_tcp {
            return SetupMode::Udp;
        }

        let setup_line = sdp.lines().find(|l| l.starts_with("a=setup:"));

        match setup_line {
            Some(line) => {
                if line.contains("active") {
                    SetupMode::TcpActive
                } else {
                    SetupMode::TcpPassive
                }
            }
            None => SetupMode::TcpPassive,
        }
    }

    fn parse_broadcast_direction(headers: &[(String, String)]) -> BroadcastDirection {
        let subject = Self::get_header(headers, "Subject")
            .unwrap_or_default();
        
        tracing::info!("[GB28181] Subject header: {}", subject);
        
        let (platform_id, _, _, _) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, domain, ip, port)| (id.clone(), domain.clone(), ip.clone(), *port))
            .unwrap_or_else(|| (String::new(), String::new(), String::new(), 0));
        
        let parts: Vec<&str> = subject.split(',').collect();
        if parts.len() != 2 {
            return BroadcastDirection::PlatformToDevice;
        }
        
        let sender_part = parts[0].trim();
        let receiver_part = parts[1].trim();
        
        let sender_id = sender_part.split(':').next().unwrap_or("").trim();
        let receiver_id = receiver_part.split(':').next().unwrap_or("").trim();
        
        tracing::info!("[GB28181] Direction check: sender={}, receiver={}, platform_id={}", 
            sender_id, receiver_id, platform_id);
        
        if sender_id.is_empty() || sender_id == platform_id {
            BroadcastDirection::PlatformToDevice
        } else if sender_id == receiver_id {
            BroadcastDirection::PlatformToDevice
        } else {
            BroadcastDirection::DeviceToPlatform
        }
    }

    fn parse_audio_invite_sdp(&self, sdp: &str) -> Option<(u32, String, u16)> {
        let ssrc = sdp
            .lines()
            .find(|l| l.starts_with("y="))
            .and_then(|l| l[2..].parse().ok())
            .unwrap_or(0);

        let device_ip = sdp
            .lines()
            .find(|l| l.starts_with("c=IN IP4 "))
            .and_then(|l| l.strip_prefix("c=IN IP4 "))
            .unwrap_or("0.0.0.0")
            .to_string();

        let audio_port = sdp
            .lines()
            .find(|l| l.starts_with("m=audio "))
            .and_then(|l| {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 2 {
                    parts[1].parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        Some((ssrc, device_ip, audio_port))
    }

    fn build_audio_invite_response_udp(&self, port: u16, ssrc: u32, direction: BroadcastDirection) -> String {
        let (platform_id, platform_ip, _) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
            .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));

        let dir_attr = match direction {
            BroadcastDirection::PlatformToDevice => "a=sendonly",
            BroadcastDirection::DeviceToPlatform => "a=recvonly",
        };

        format!(
            "v=0\r\n\
             o={} 0 0 IN IP4 {}\r\n\
             s=Play\r\n\
             c=IN IP4 {}\r\n\
             t=0 0\r\n\
             m=audio {} RTP/AVP 8\r\n\
             a=rtpmap:8 PCMA/8000\r\n\
             {}\r\n\
             y={:010}\r\n\
             a=ssrc:{}\r\n",
            platform_id, platform_ip, platform_ip, port, dir_attr, ssrc, ssrc
        )
    }

    pub async fn start_audio_talk(
        &mut self,
        device_tag: &str,
        channel_tag: &str,
        device_ip: &str,
        device_audio_port: u16,
        platform_audio_port: u16,
        transport: TransportType,
        node_id: MediaNodeId,
    ) -> Result<()> {
        tracing::info!(
            "[GB28181] start_audio_talk (standard broadcast flow): device={} channel={} ip={} port={} transport={:?} node_id={}",
            device_tag, channel_tag, device_ip, device_audio_port, transport, node_id.id
        );

        {
            let mut pending = self.pending_audio_talks.write().await;
            let entry = PendingAudioTalkEntry { node_id };
            pending.push(entry);
            tracing::info!("[GB28181] Saved node_id to pending_audio_talks for device={}, channel={}", device_tag, channel_tag);
        }

        self.send_broadcast_notification(device_tag).await?;

        tracing::info!(
            "[GB28181] Broadcast notification sent to device {}, waiting for INVITE",
            device_tag
        );

        Ok(())
    }

    pub async fn finish_audio_talk_setup(
        &mut self,
        device_tag: &str,
        codec: crate::media::AudioCodec,
        peer_addr: SocketAddr,
        transport: TransportType,
        platform_audio_port: u16,
        tcp_server_port: Option<u16>,
    ) -> Result<()> {
        tracing::info!(
            "[GB28181] finish_audio_talk_setup: device={} codec={:?} transport={:?}",
            device_tag,
            codec,
            transport
        );

        let channel_id = {
            let sessions = self.sessions.read().await;
            sessions.iter()
                .find(|s| s.device_tag == device_tag && s.session_type == SessionType::Broadcast)
                .map(|s| s.channel_id.clone())
                .unwrap_or_else(|| {
                    tracing::warn!("[GB28181] finish_audio_talk_setup: No broadcast session found for device {}", device_tag);
                    String::new()
                })
        };

        if transport == TransportType::Tcp {
            if let Some(port) = tcp_server_port {
                if let Some(manager) = crate::media::get_rtp_server_manager() {
                    if let Some(tcp_server) = manager.get_tcp_server(port).await {
                        let transport_arc = crate::media::GB28181BroadcastVoiceTransport::new_tcp_passive(
                            tcp_server.clone(),
                            rand::random::<u32>(),
                            codec.payload_type(),
                        );
                        let transport_accept = transport_arc.clone();
                        let device_tag_clone = device_tag.to_string();
                        let channel_id_clone = channel_id.clone();

                        tokio::spawn(async move {
                            match transport_accept.accept().await {
                                Ok(addr) => {
                                    tracing::info!(
                                        "[GB28181] TCP accept succeeded for device {} from {}",
                                        device_tag_clone,
                                        addr
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "[GB28181] TCP accept failed for device {}: {}",
                                        device_tag_clone,
                                        e
                                    );
                                }
                            }
                        });

                        let node_id = MediaNodeId::new(
                            MediaNodeType::GB28181_BROADCAST_VOICE,
                            &self.device_tag.clone().unwrap_or("".to_string()),
                            Some(channel_id.clone()),
                        );
                        let node: Arc<dyn MediaNode> =
                            Arc::new(BasicMediaNode::new(node_id, transport_arc.clone()));

                        if let Some(mgr) = crate::media::get_media_session_manager() {
                            let session = mgr.get_or_create(device_tag);
                            session.add_node(node.clone())?;
                            tracing::info!(
                                "[GB28181] TCP MediaNode added to session: {}_{}",
                                device_tag,
                                channel_id_clone
                            );
                        }
                    }
                }
            }
            tracing::info!(
                "[GB28181] TCP Session confirmed for device {} with codec {:?}",
                device_tag,
                codec
            );
        } else {
            tracing::info!(
                "[GB28181] Setting up UDP media for device {} peer={} port={}",
                device_tag,
                peer_addr,
                platform_audio_port
            );

            if let Some(manager) = crate::media::get_rtp_server_manager() {
                if let Some(udp_server) = manager.get_udp_server(platform_audio_port).await {
                    let voice_transport = crate::media::GB28181BroadcastVoiceTransport::new_udp(
                        udp_server,
                        peer_addr,
                        rand::random::<u32>(),
                        codec.payload_type(),
                    );

                    let node_id = MediaNodeId::new(
                        MediaNodeType::GB28181_BROADCAST_VOICE,
                        &self.device_tag.clone().unwrap_or("".to_string()),
                        Some(channel_id.clone()),
                    );
                    let node: Arc<dyn MediaNode> =
                        Arc::new(BasicMediaNode::new(node_id, voice_transport));

                    if let Some(mgr) = crate::media::get_media_session_manager() {
                        let session = mgr.get_or_create(device_tag);
                        session.add_node(node.clone())?;
                        tracing::info!(
                            "[GB28181] UDP MediaNode added to session: {}_{}",
                            device_tag,
                            channel_id.clone()
                        );
                    }
                } else {
                    tracing::warn!(
                        "[GB28181] UDP server not found for port {}",
                        platform_audio_port
                    );
                }
            }
        }

        tracing::info!("[GB28181] Media sessions created for device {}", device_tag);

        let node_id = {
            let sessions = self.sessions.read().await;
            sessions.iter()
                .find(|s| s.device_tag == device_tag && s.session_type == SessionType::Broadcast)
                .and_then(|s| s.node_id.clone())
                .unwrap_or_else(|| {
                    tracing::warn!("[GB28181] finish_audio_talk_setup: No node_id found in SubSession for device {}", device_tag);
                    MediaNodeId::new(MediaNodeType::WebSocket, device_tag, None)
                })
        };

        let event = crate::protocol::event::SignalEvent::AudioTalkReady {
            node_id,
        };

        if let Err(e) = self.deps.event_publisher.publish(event).await {
            tracing::warn!("[GB28181] Failed to publish AudioTalkReady event: {}", e);
        } else {
            tracing::info!(
                "[GB28181] Published AudioTalkReady event for device {}",
                device_tag
            );
        }

        Ok(())
    }

    pub async fn stop_audio_talk_v2(&self, device_tag: &str) -> Result<()> {
        tracing::info!("[GB28181] stop_audio_talk_v2 called for device_tag={}", device_tag);
        let manager = match crate::media::get_media_session_manager() {
            Some(m) => m,
            None => {
                tracing::warn!("[GB28181] stop_audio_talk_v2: no manager");
                return Ok(());
            }
        };

        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.iter().find(|s| s.device_tag == device_tag && s.session_type == SessionType::Broadcast) {
                tracing::info!("[GB28181] Sending BYE to device {} for audio talk stop", device_tag);
                if let Err(e) = self.send_bye_to_session(session).await {
                    tracing::warn!("[GB28181] Failed to send BYE: {}", e);
                }
            } else {
                tracing::warn!("[GB28181] No broadcast session found in self.sessions for {}", device_tag);
            }
        }

        match manager.get(device_tag) {
            Some(session) => {
                tracing::info!("[GB28181] Found session in manager, calling end_all_broadcasts");
                let removed = session.end_all_broadcasts().await;
                tracing::info!(
                    "[GB28181] Broadcast sessions stopped: {} for device {}",
                    removed,
                    device_tag
                );
            }
            None => {
                tracing::warn!("[GB28181] No session found in manager for device_tag={}", device_tag);
            }
        }

        Ok(())
    }

    pub async fn send_audio_to_device(&self, device_tag: &str, pcm_data: &[i16]) -> Result<()> {
        let manager = match crate::media::get_media_session_manager() {
            Some(m) => m,
            None => return Err(AppError::Internal("No MediaSessionManager".into())),
        };

        let session = match manager.get(device_tag) {
            Some(s) => s,
            None => {
                return Err(AppError::Internal(format!(
                    "No session for device: {}",
                    device_tag
                )))
            }
        };

        let nodes = session.get_broadcast_nodes();
        if nodes.is_empty() {
            return Err(AppError::Internal(format!(
                "No broadcast session for device: {}",
                device_tag
            )));
        }

        let mut pcm_bytes = Vec::with_capacity(pcm_data.len() * 2);
        for &sample in pcm_data {
            pcm_bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let mut data_map = HashMap::new();
        data_map.insert(DataType::OrgData, BytesMut::from(&pcm_bytes[..]));
        for node in nodes {
            node.sendto_receivers(data_map.clone()).await?;
        }
        Ok(())
    }

    pub async fn is_audio_talk_active(&self, device_tag: &str) -> bool {
        if let Some(manager) = crate::media::get_media_session_manager() {
            if let Some(session) = manager.get(device_tag) {
                return session.has_broadcast_session();
            }
        }
        false
    }

    pub async fn recv_audio_from_device_v2(&self, device_tag: &str) -> Result<Option<Vec<i16>>> {
        let channel_id = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .find(|s| {
                    s.device_tag == device_tag
                        && s.session_type == SessionType::Broadcast
                        && s.state == SubSessionState::Ok
                })
                .map(|s| s.channel_id.clone())
                .unwrap_or_default()
        };

        if channel_id.is_empty() {
            return Ok(None);
        }

        let manager = match crate::media::get_media_session_manager() {
            Some(m) => m,
            None => return Ok(None),
        };

        let session = match manager.get(device_tag) {
            Some(s) => s,
            None => return Ok(None),
        };

        let node = match session.get_broadcast_node(&channel_id) {
            Some(n) => n,
            None => return Ok(None),
        };

        match node.recv().await {
            Ok(()) => Ok(Some(Vec::new())),
            Err(e) => {
                tracing::warn!("[GB28181] Failed to recv from node: {}", e);
                Err(AppError::Internal(e.to_string()))
            }
        }
    }

    async fn send_bye_to_session(&self, session: &SubSession) -> Result<()> {
        let (platform_id, platform_ip, platform_port) =
            crate::protocol::gb28181::get_gb28181_platform_config()
                .get()
                .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
                .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));

        let device_tag = self.device_tag_str();
        let via_transport = match session.transport {
            TransportType::Tcp => "TCP",
            TransportType::Udp => "UDP",
            _ => unreachable!("GB28181 only supports TCP and UDP transport"),
        };
        let sip_msg = format!(
            "BYE sip:{}@{}:{} SIP/2.0\r\n\
             Via: SIP/2.0/{} {}:{};branch=z9hG4bK{};rport\r\n\
             From: <sip:{}@{}:{}>;tag={}\r\n\
             To: <sip:{}@{}:{}>;tag={}\r\n\
             Call-ID: {}\r\n\
             CSeq: 2 BYE\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            device_tag,
            session.device_host,
            session.device_port,
            via_transport,
            platform_ip,
            platform_port,
            chrono::Utc::now().timestamp_millis(),
            platform_id,
            platform_ip,
            platform_port,
            &session.from_tag,
            device_tag,
            session.device_host,
            session.device_port,
            session.to_tag.as_deref().unwrap_or(""),
            &session.call_id
        );

        let send_once = || async {
            match session.transport {
                TransportType::Tcp => {
                    if let Some(ref write_arc) = self.write {
                        let mut write = write_arc.write().await;
                        write
                            .write_all(sip_msg.as_bytes())
                            .await
                            .map_err(|e| AppError::Internal(e.to_string()))?;
                        let _ = write.flush().await;
                    }
                }
                TransportType::Udp => {
                    if let Some(sender) = crate::protocol::gb28181::get_udp_sender() {
                        let addr = format!("{}:{}", session.device_host, session.device_port);
                        sender
                            .send_to(sip_msg.as_bytes(), &addr)
                            .await
                            .map_err(|e| AppError::Internal(e.to_string()))?;
                    }
                }
                _ => unreachable!("GB28181 only supports TCP and UDP transport"),
            }
        };

        for attempt in 1..=3 {
            tracing::info!("[GB28181] BYE attempt {} to {}:{} via {:?}, call_id={}", attempt, session.device_host, session.device_port, session.transport, session.call_id);
            if let Err(e) = send_once().await {
                tracing::warn!("[GB28181] BYE attempt {} failed: {}", attempt, e);
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                continue;
            }
            tracing::info!("[GB28181] BYE sent successfully on attempt {}", attempt);
            return Ok(());
        }

        tracing::warn!("[GB28181] All 3 BYE attempts failed, giving up");
        Ok(())
    }

    async fn send_bye_to_clear_session(&self, device_tag: &str) -> Result<()> {
        let (platform_id, platform_ip, platform_port) =
            crate::protocol::gb28181::get_gb28181_platform_config()
                .get()
                .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
                .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));

        let device = match self.deps.device_lookup.find_by_tag(device_tag).await {
            Some(d) => d,
            None => {
                tracing::warn!("[GB28181] send_bye_to_clear_session: device not found {}", device_tag);
                return Ok(());
            }
        };

        let device_host = device.host.clone();
        let device_port = device.port;
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");

        let sip_msg = format!(
            "BYE sip:{}@{}:{} SIP/2.0\r\n\
             Via: SIP/2.0/UDP {}:{};branch=z9hG4bK{};rport\r\n\
             From: <sip:{}@{}:{}>;tag={}\r\n\
             To: <sip:{}@{}:{}>\r\n\
             Call-ID: {}\r\n\
             CSeq: 2 BYE\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            device_tag,
            device_host,
            device_port,
            platform_ip,
            platform_port,
            &call_id[..7],
            platform_id,
            platform_ip,
            platform_port,
            from_tag,
            device_tag,
            device_host,
            device_port,
            call_id
        );

        if let Some(sender) = crate::protocol::gb28181::get_udp_sender() {
            let addr = format!("{}:{}", device_host, device_port);
            tracing::info!("[GB28181] Sending BYE to clear session to {}:{}", device_host, device_port);
            for attempt in 1..=3 {
                if let Err(e) = sender.send_to(sip_msg.as_bytes(), &addr).await {
                    tracing::warn!("[GB28181] BYE clear attempt {} failed: {}", attempt, e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    continue;
                }
                tracing::info!("[GB28181] BYE clear sent successfully on attempt {}", attempt);
                return Ok(());
            }
            tracing::warn!("[GB28181] All BYE clear attempts failed");
        }
        Ok(())
    }
}

fn rand_ssrc() -> u32 {
    rand::random()
}

fn rand_u16() -> u16 {
    rand::random()
}

fn rand_u32() -> u32 {
    rand::random()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_ptz_cmd_compound_up_right() {
        let result = Gb28181Adapter::encode_ptz_cmd_compound(0.5, 0.5, 0.0);
        assert!(
            result.contains("28"),
            "Up(0x08)+Right(0x20)=0x28, got: {}",
            result
        );
        assert!(result.contains("00 7F"), "No zoom (0), got: {}", result);
    }

    #[test]
    fn test_encode_ptz_cmd_compound_down_left() {
        let result = Gb28181Adapter::encode_ptz_cmd_compound(-0.5, -0.5, 0.0);
        assert!(
            result.contains("12"),
            "Down(0x10)+Left(0x02)=0x12, got: {}",
            result
        );
    }

    #[test]
    fn test_encode_ptz_cmd_compound_with_zoom() {
        let result = Gb28181Adapter::encode_ptz_cmd_compound(0.5, 0.0, 0.3);
        assert!(result.contains("20"), "Right(0x20), got: {}", result);
        assert!(
            result.contains("80"),
            "ZoomIn(0x80) in cmd2, got: {}",
            result
        );
    }

    #[test]
    fn test_encode_ptz_cmd_compound_all_stop() {
        let result = Gb28181Adapter::encode_ptz_cmd_compound(0.0, 0.0, 0.0);
        assert!(
            result.contains("00 00 00 00 00"),
            "All stop should be 00s, got: {}",
            result
        );
    }

    #[test]
    fn test_encode_ptz_cmd_single_up() {
        let result = Gb28181Adapter::encode_ptz_cmd("up", 50);
        assert!(
            result.contains("08"),
            "Up should set cmd1=0x08, got: {}",
            result
        );
    }

    #[test]
    fn test_encode_ptz_cmd_single_right() {
        let result = Gb28181Adapter::encode_ptz_cmd("right", 50);
        assert!(
            result.contains("40"),
            "Right should set cmd1=0x40, got: {}",
            result
        );
    }

    #[test]
    fn test_encode_ptz_cmd_zoom_in() {
        let result = Gb28181Adapter::encode_ptz_cmd("zoom_in", 50);
        assert!(
            result.contains("80"),
            "ZoomIn should set cmd2=0x80, got: {}",
            result
        );
    }

    #[test]
    fn test_datetime_to_ntp_string() {
        use chrono::{TimeZone, Utc};
        let dt = Utc.with_ymd_and_hms(2024, 8, 7, 14, 30, 0).unwrap();
        let result = Gb28181Adapter::datetime_to_ntp_string(dt);
        assert_eq!(result, "20240807143000");
    }

    #[test]
    fn test_build_ptz_xml_compound() {
        let result =
            Gb28181Adapter::build_ptz_xml_compound("34020000001110000001", 0.5, 0.5, 0.0, 50);
        assert!(result.contains("34020000001110000001"));
        assert!(result.contains("PTZCmd"));
    }

    #[tokio::test]
    async fn test_build_catalog_query() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();

        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());
        adapter.udp_peer = Some("192.168.1.211:5060".parse().unwrap());

        let query = adapter.build_catalog_query();
        let query_str = String::from_utf8_lossy(&query);

        assert!(
            query_str.contains("MESSAGE sip:"),
            "Should contain MESSAGE sip:"
        );
        assert!(
            query_str.contains("12010100001320000011"),
            "Should contain device ID"
        );
        assert!(
            query_str.contains("CmdType>Catalog"),
            "Should contain Catalog command"
        );
        assert!(query_str.contains("Via:"), "Should contain Via header");
        assert!(query_str.contains("CSeq:"), "Should contain CSeq header");
        assert!(query_str.contains("From:"), "Should contain From header");
        assert!(query_str.contains("To:"), "Should contain To header");
        assert!(
            query_str.contains("Call-ID:"),
            "Should contain Call-ID header"
        );
    }

    #[test]
    fn test_parse_contact_header_with_port() {
        let headers = vec![(
            "Contact".to_string(),
            "<sip:12010100001320000011@192.168.1.211:5060>".to_string(),
        )];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_without_port() {
        let headers = vec![(
            "Contact".to_string(),
            "<sip:12010100001320000011@192.168.1.211>".to_string(),
        )];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_no_brackets() {
        let headers = vec![(
            "Contact".to_string(),
            "sip:12010100001320000011@192.168.1.211:5060".to_string(),
        )];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_case_insensitive() {
        let headers = vec![(
            "contact".to_string(),
            "<sip:12010100001320000011@192.168.1.211:5060>".to_string(),
        )];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_no_contact() {
        let headers = vec![(
            "From".to_string(),
            "<sip:12010100001320000011@192.168.1.211:5060>".to_string(),
        )];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_contact_header_ipv6() {
        let headers = vec![(
            "Contact".to_string(),
            "<sip:12010100001320000011@[::1]:5060>".to_string(),
        )];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "[::1]");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_with_params() {
        let headers = vec![(
            "Contact".to_string(),
            "<sip:12010100001320000011@192.168.1.211:5060>;expires=3600".to_string(),
        )];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_tag_from_header() {
        let header = "From: <sip:12010100001320000011@192.168.1.211:5060>;tag=abc123";
        let result = Gb28181Adapter::parse_tag_from_header(header);
        assert_eq!(result, Some("abc123".to_string()));
    }

    #[test]
    fn test_parse_tag_from_header_multiple_tags() {
        let header = "From: <sip:12010100001320000011@192.168.1.211:5060>;tag=first, tag=second";
        let result = Gb28181Adapter::parse_tag_from_header(header);
        assert_eq!(result, Some("first".to_string()));
    }

    #[test]
    fn test_parse_tag_from_header_no_tag() {
        let header = "From: <sip:12010100001320000011@192.168.1.211:5060>";
        let result = Gb28181Adapter::parse_tag_from_header(header);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sip_uri() {
        let uri = "sip:12010100001320000011@192.168.1.211:5060";
        let result = Gb28181Adapter::parse_sip_uri(uri);
        assert_eq!(result, "12010100001320000011".to_string());
    }

    #[test]
    fn test_parse_sip_uri_with_angle_brackets() {
        let uri = "<sip:12010100001320000011@192.168.1.211:5060>";
        let result = Gb28181Adapter::parse_sip_uri(uri);
        assert_eq!(result, "12010100001320000011".to_string());
    }

    #[test]
    fn test_parse_sip_uri_no_user() {
        let uri = "sip:192.168.1.211:5060";
        let result = Gb28181Adapter::parse_sip_uri(uri);
        assert_eq!(result, "192.168.1.211:5060".to_string());
    }

    #[test]
    fn test_parse_sip_uri_with_domain() {
        let uri = "sip:12010100001320000011@192.168.1.211:5060";
        let result = Gb28181Adapter::parse_sip_uri_with_domain(uri);
        assert_eq!(
            result,
            "sip:12010100001320000011@192.168.1.211:5060".to_string()
        );
    }

    #[test]
    fn test_parse_sip_uri_with_domain_no_sip_prefix() {
        let uri = "12010100001320000011@192.168.1.211:5060";
        let result = Gb28181Adapter::parse_sip_uri_with_domain(uri);
        assert_eq!(
            result,
            "sip:12010100001320000011@192.168.1.211:5060".to_string()
        );
    }

    #[test]
    fn test_parse_via_branch() {
        let headers = vec![(
            "Via".to_string(),
            "SIP/2.0/UDP 192.168.1.100:5060;branch=z9hG4bKabc123;rport".to_string(),
        )];
        let result = Gb28181Adapter::parse_via_branch(&headers);
        assert_eq!(result, "z9hG4bKabc123".to_string());
    }

    #[test]
    fn test_parse_via_branch_no_branch() {
        let headers = vec![(
            "Via".to_string(),
            "SIP/2.0/UDP 192.168.1.100:5060;rport".to_string(),
        )];
        let result = Gb28181Adapter::parse_via_branch(&headers);
        assert_eq!(result, "z9hG4bKdefault".to_string());
    }

    #[test]
    fn test_parse_via_received() {
        let headers = vec![(
            "Via".to_string(),
            "SIP/2.0/UDP 192.168.1.100:5060;received=10.0.0.1;branch=z9hG4bKabc123".to_string(),
        )];
        let result = Gb28181Adapter::parse_via_received(&headers);
        assert_eq!(result, Some("10.0.0.1".to_string()));
    }

    #[test]
    fn test_parse_via_rport() {
        let headers = vec![(
            "Via".to_string(),
            "SIP/2.0/UDP 192.168.1.100:5060;rport=5070;branch=z9hG4bKabc123".to_string(),
        )];
        let result = Gb28181Adapter::parse_via_rport(&headers);
        assert_eq!(result, Some(5070));
    }

    #[test]
    fn test_parse_via_rport_only() {
        let headers = vec![(
            "Via".to_string(),
            "SIP/2.0/UDP 192.168.1.100:5060;rport;branch=z9hG4bKabc123".to_string(),
        )];
        let result = Gb28181Adapter::parse_via_rport(&headers);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_parse_cseq_line() {
        let headers = vec![("CSeq".to_string(), "1 REGISTER".to_string())];
        let result = Gb28181Adapter::parse_cseq_line(&headers);
        assert_eq!(result, "1 REGISTER".to_string());
    }

    #[test]
    fn test_parse_cseq_line_default() {
        let headers = vec![];
        let result = Gb28181Adapter::parse_cseq_line(&headers);
        assert_eq!(result, "1 REGISTER".to_string());
    }

    #[test]
    fn test_build_sdp_video_only() {
        use chrono::{TimeZone, Utc};
        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());
        adapter.from_tag = Some("from_tag_123".to_string());
        adapter.to_tag = Some("to_tag_456".to_string());
        adapter.call_id = Some("call_id_789".to_string());
        adapter.transport = TransportType::Udp;

        let response = adapter.build_401_response("z9hG4bKbranch", "1 REGISTER", "realm123");

        assert!(response.contains("SIP/2.0 401 Unauthorized"));
        assert!(response.contains("Via:"));
        assert!(response.contains("From:"));
        assert!(response.contains("To:"));
        assert!(response.contains("CSeq: 1 REGISTER"));
        assert!(response.contains("Call-ID: call_id_789"));
        assert!(response.contains("WWW-Authenticate: Digest realm=\"realm123\""));
    }

    #[test]
    fn test_build_403_response() {
        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());
        adapter.from_tag = Some("from_tag_123".to_string());
        adapter.to_tag = Some("to_tag_456".to_string());
        adapter.call_id = Some("call_id_789".to_string());
        adapter.transport = TransportType::Udp;

        let response = adapter.build_403_response("z9hG4bKbranch", "1 REGISTER");

        assert!(response.contains("SIP/2.0 403 Forbidden"));
        assert!(response.contains("Via:"));
        assert!(response.contains("From:"));
        assert!(response.contains("To:"));
        assert!(response.contains("CSeq: 1 REGISTER"));
        assert!(response.contains("Call-ID: call_id_789"));
    }

    #[test]
    fn test_codec_to_video_pt() {
        assert_eq!(Gb28181Adapter::codec_to_video_pt("PS"), 96);
        assert_eq!(Gb28181Adapter::codec_to_video_pt("H264"), 98);
        assert_eq!(Gb28181Adapter::codec_to_video_pt("H265"), 99);
        assert_eq!(Gb28181Adapter::codec_to_video_pt("h264"), 98);
        assert_eq!(Gb28181Adapter::codec_to_video_pt("MPEG4"), 97);
    }

    #[test]
    fn test_codec_to_audio_pt() {
        assert_eq!(Gb28181Adapter::codec_to_audio_pt("PCMA"), 8);
        assert_eq!(Gb28181Adapter::codec_to_audio_pt("G711A"), 8);
        assert_eq!(Gb28181Adapter::codec_to_audio_pt("PCMU"), 0);
        assert_eq!(Gb28181Adapter::codec_to_audio_pt("G711U"), 0);
        assert_eq!(Gb28181Adapter::codec_to_audio_pt("AAC"), 97);
    }

    #[test]
    fn test_codec_to_rtp_map() {
        let (encoding, clock) = Gb28181Adapter::codec_to_rtp_map("PS");
        assert_eq!(encoding, "PS");
        assert_eq!(clock, 90000);

        let (encoding, clock) = Gb28181Adapter::codec_to_rtp_map("H264");
        assert_eq!(encoding, "H264");
        assert_eq!(clock, 90000);
    }

    #[test]
    fn test_codec_to_audio_rtp_map() {
        let (encoding, clock) = Gb28181Adapter::codec_to_audio_rtp_map("PCMA");
        assert_eq!(encoding, "PCMA");
        assert_eq!(clock, 8000);

        let (encoding, clock) = Gb28181Adapter::codec_to_audio_rtp_map("G722");
        assert_eq!(encoding, "G722");
        assert_eq!(clock, 8000);
    }

    #[test]
    fn test_is_valid_gb28181_device_id() {
        assert!(Gb28181Adapter::is_valid_gb28181_device_id(
            "12010100001320000011"
        ));
        assert!(Gb28181Adapter::is_valid_gb28181_device_id("1234567890"));
        assert!(!Gb28181Adapter::is_valid_gb28181_device_id("123456"));
        assert!(!Gb28181Adapter::is_valid_gb28181_device_id(""));
        assert!(!Gb28181Adapter::is_valid_gb28181_device_id(
            "123456789012345678901"
        ));
        assert!(!Gb28181Adapter::is_valid_gb28181_device_id(
            "12010100001320000abc"
        ));
    }

    #[test]
    fn test_parse_subject_device_id() {
        assert_eq!(
            Gb28181Adapter::parse_subject_device_id("12010100001320000011:31011500001000000001:0"),
            Some("12010100001320000011".to_string())
        );
        assert_eq!(Gb28181Adapter::parse_subject_device_id("invalid"), None);
    }

    #[test]
    fn test_parse_ssrc_from_sdp() {
        let sdp = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=Play\r\ny=00001234\r\n";
        let result = Gb28181Adapter::parse_ssrc_from_sdp(sdp);
        assert_eq!(result, Some(1234));
    }

    #[test]
    fn test_parse_ssrc_from_sdp_not_found() {
        let sdp = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=Play\r\n";
        let result = Gb28181Adapter::parse_ssrc_from_sdp(sdp);
        assert_eq!(result, None);
    }

    #[test]
    fn test_truncate_20() {
        assert_eq!(
            Gb28181Adapter::truncate_20("12345678901234567890"),
            "12345678901234567890"
        );
        assert_eq!(
            Gb28181Adapter::truncate_20("123456789012345678901"),
            "12345678901234567890"
        );
        assert_eq!(Gb28181Adapter::truncate_20("short"), "short");
    }

    #[test]
    fn test_truncate_32() {
        assert_eq!(
            Gb28181Adapter::truncate_32("12345678901234567890123456789012"),
            "12345678901234567890123456789012"
        );
        assert_eq!(
            Gb28181Adapter::truncate_32("123456789012345678901234567890123"),
            "12345678901234567890123456789012"
        );
        assert_eq!(Gb28181Adapter::truncate_32("short"), "short");
    }

    #[test]
    fn test_detect_version() {
        assert_eq!(
            Gb28181Adapter::detect_version("GB28181"),
            Gb28181Version::Gb2016
        );
        assert_eq!(
            Gb28181Adapter::detect_version("something GB28181 something"),
            Gb28181Version::Gb2016
        );
        assert_eq!(
            Gb28181Adapter::detect_version("something 28181 something"),
            Gb28181Version::Gb2016
        );
        assert_eq!(
            Gb28181Adapter::detect_version("other"),
            Gb28181Version::Gb2016
        );
    }

    #[test]
    fn test_best_direction() {
        assert_eq!(Gb28181Adapter::best_direction(0.5, 0.0, 0.0), "right");
        assert_eq!(Gb28181Adapter::best_direction(-0.5, 0.0, 0.0), "left");
        assert_eq!(Gb28181Adapter::best_direction(0.0, 0.5, 0.0), "up");
        assert_eq!(Gb28181Adapter::best_direction(0.0, -0.5, 0.0), "down");
        assert_eq!(Gb28181Adapter::best_direction(0.0, 0.0, 0.5), "zoom_in");
        assert_eq!(Gb28181Adapter::best_direction(0.0, 0.0, -0.5), "zoom_out");
        assert_eq!(Gb28181Adapter::best_direction(0.3, 0.3, 0.0), "right");
    }

    #[test]
    fn test_build_device_info_response() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());
        adapter.seq = Arc::new(AtomicU32::new(100));

        let response = adapter.build_device_info_response();

        assert!(response.contains("<CmdType>DeviceInfo</CmdType>"));
        assert!(response.contains("<DeviceID>12010100001320000011</DeviceID>"));
        assert!(response.contains("<DeviceName>IPC</DeviceName>"));
        assert!(response.contains("<Manufacturer>RustCam</Manufacturer>"));
        assert!(response.contains("<Model>RustCam-GB28181</Model>"));
    }

    #[test]
    fn test_build_device_status_response() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());
        adapter.seq = Arc::new(AtomicU32::new(100));

        let response = adapter.build_device_status_response();

        assert!(response.contains("<CmdType>DeviceStatus</CmdType>"));
        assert!(response.contains("<DeviceID>12010100001320000011</DeviceID>"));
        assert!(response.contains("<Online>ON</Online>"));
        assert!(response.contains("<Status>OK</Status>"));
    }

    #[test]
    fn test_build_catalog_response() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());
        adapter.device_name = Some("Test Camera".to_string());
        adapter.seq = Arc::new(AtomicU32::new(100));

        let response = adapter.build_catalog_response("34020000001110000001");

        assert!(response.contains("<CmdType>Catalog</CmdType>"));
        assert!(response.contains("<DeviceID>12010100001320000011</DeviceID>"));
        assert!(response.contains("<SN>100</SN>"));
        assert!(response.contains("<DeviceID>34020000001110000001</DeviceID>"));
        assert!(response.contains("<SumNum>1</SumNum>"));
    }

    #[test]
    fn test_parse_channel_id_from_device_id() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>1</SN><DeviceID>12010100001320000011</DeviceID>
<ChannelID>34020000001110000001</ChannelID></Response>"#;

        let result = Gb28181Adapter::parse_channel_id(body);
        assert_eq!(result, Some("34020000001110000001".to_string()));
    }

    #[test]
    fn test_parse_channel_id_from_channel_id() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>1</SN><DeviceID>12010100001320000011</DeviceID>
<ChannelID>34020000001110000001</ChannelID></Response>"#;

        let result = Gb28181Adapter::parse_channel_id(body);
        assert_eq!(result, Some("34020000001110000001".to_string()));
    }

    #[test]
    fn test_parse_xml_field() {
        let xml = r#"<Item><DeviceID>34020000001110000001</DeviceID><Name>Camera1</Name><Status>ON</Status></Item>"#;

        assert_eq!(
            Gb28181Adapter::parse_xml_field(xml, "DeviceID"),
            Some("34020000001110000001".to_string())
        );
        assert_eq!(
            Gb28181Adapter::parse_xml_field(xml, "Name"),
            Some("Camera1".to_string())
        );
        assert_eq!(
            Gb28181Adapter::parse_xml_field(xml, "Status"),
            Some("ON".to_string())
        );
        assert_eq!(Gb28181Adapter::parse_xml_field(xml, "NonExistent"), None);
    }

    #[test]
    fn test_parse_catalog_response_single_channel() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());

        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>1</SN><DeviceID>12010100001320000011</DeviceID><SumNum>1</SumNum>
<DeviceList><Item><DeviceID>34020000001110000001</DeviceID><Name>Camera1</Name><Manufacturer>Hikvision</Manufacturer>
<Model>DS-2CD3T46FWD-I</Model><Status>ON</Status><PTZType>3</PTZType>
<Parental>0</Parental><ParentID>12010100001320000011</ParentID><Address>192.168.1.100</Address><IPAddress>192.168.1.100</IPAddress><Port>8000</Port>
<Info><DeviceType>IPC</DeviceType><Protocol>GB28181</Protocol></Info></Item></DeviceList></Response>"#;

        let channels = adapter.parse_catalog_response(body);

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].device_id, "34020000001110000001");
        assert_eq!(channels[0].name, "Camera1");
        assert_eq!(channels[0].manufacturer, Some("Hikvision".to_string()));
        assert_eq!(channels[0].model, Some("DS-2CD3T46FWD-I".to_string()));
        assert_eq!(channels[0].status, "ON");
        assert!(!channels[0].parental);
        assert_eq!(
            channels[0].parent_id,
            Some("12010100001320000011".to_string())
        );
        assert_eq!(channels[0].ip_address, Some("192.168.1.100".to_string()));
        assert_eq!(channels[0].port, Some(8000));
    }

    #[test]
    fn test_parse_catalog_response_multiple_channels() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());

        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>1</SN><DeviceID>12010100001320000011</DeviceID><SumNum>2</SumNum>
<DeviceList>
<Item><DeviceID>34020000001110000001</DeviceID><Name>Camera1</Name><Status>ON</Status><Parental>0</Parental></Item>
<Item><DeviceID>34020000001110000002</DeviceID><Name>Camera2</Name><Status>OFF</Status><Parental>0</Parental></Item>
</DeviceList></Response>"#;

        let channels = adapter.parse_catalog_response(body);

        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].device_id, "34020000001110000001");
        assert_eq!(channels[0].name, "Camera1");
        assert_eq!(channels[1].device_id, "34020000001110000002");
        assert_eq!(channels[1].name, "Camera2");
    }

    #[test]
    fn test_parse_catalog_response_with_parent() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());

        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>1</SN><DeviceID>12010100001320000011</DeviceID><SumNum>1</SumNum>
<DeviceList><Item><DeviceID>34020000001110000001</DeviceID><Name>Camera1</Name><Status>ON</Status><Parental>1</Parental></Item></DeviceList></Response>"#;

        let channels = adapter.parse_catalog_response(body);

        assert_eq!(channels.len(), 1);
        assert!(channels[0].parental);
    }

    #[test]
    fn test_build_ptz_xml_up() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "up", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("<DeviceID>34020000001110000001</DeviceID>"));
        assert!(result.contains("PTZCmd>80 01 04 08"));
    }

    #[test]
    fn test_build_ptz_xml_down() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "down", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 04 10"));
    }

    #[test]
    fn test_build_ptz_xml_left() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "left", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 04 20"));
    }

    #[test]
    fn test_build_ptz_xml_right() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "right", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 04 40"));
    }

    #[test]
    fn test_build_ptz_xml_zoom_in() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "zoom_in", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 04 80"));
    }

    #[test]
    fn test_build_ptz_xml_zoom_out() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "zoom_out", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 04 00 01"));
    }

    #[test]
    fn test_build_ptz_xml_iris_in() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "iris_in", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 49"));
    }

    #[test]
    fn test_build_ptz_xml_iris_out() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "iris_out", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 48"));
    }

    #[test]
    fn test_build_ptz_xml_stop() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "stop", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 04 00 00"));
    }

    #[test]
    fn test_parse_contact_with_device_id() {
        let contact = "<sip:12010100001320000011@192.168.1.211:5060>";

        let uri = contact
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .map(|s| s.to_string())
            .unwrap_or_else(|| contact.to_string());
        assert_eq!(uri, "sip:12010100001320000011@192.168.1.211:5060");

        let without_sip = uri
            .strip_prefix("sip:")
            .map(|s| s.to_string())
            .unwrap_or_else(|| uri.clone());
        assert_eq!(without_sip, "12010100001320000011@192.168.1.211:5060");

        // rsplitn returns elements from RIGHT to LEFT
        // "device_id@host:port" -> parts[0]="host:port", parts[1]="device_id"
        let parts: Vec<&str> = without_sip.rsplitn(2, '@').collect();
        assert_eq!(
            parts.len(),
            2,
            "Should have 2 parts after @ split, got {:?}",
            parts
        );
        assert_eq!(
            parts[0], "192.168.1.211:5060",
            "parts[0] should be host:port"
        );
        assert_eq!(
            parts[1], "12010100001320000011",
            "parts[1] should be device_id"
        );

        let host_port = parts[0];
        let device_id = parts[1];

        let hp_parts: Vec<&str> = host_port.rsplitn(2, ':').collect();
        assert_eq!(
            hp_parts.len(),
            2,
            "Should have 2 parts after : split, got {:?}",
            hp_parts
        );
        assert_eq!(hp_parts[0], "5060", "hp_parts[0] should be port");
        assert_eq!(hp_parts[1], "192.168.1.211", "hp_parts[1] should be host");

        let host = hp_parts[1].to_string();
        let port: u16 = hp_parts[0].parse().unwrap();

        assert_eq!(device_id, "12010100001320000011");
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);

        let ack_target = format!("sip:{}@{}:{}", device_id, host, port);
        assert_eq!(ack_target, "sip:12010100001320000011@192.168.1.211:5060");
    }

    #[tokio::test]
    async fn test_build_broadcast_notification() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();
        let mut adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));
        adapter.device_tag = Some("12010100001320000011".to_string());
        adapter.udp_peer = Some("192.168.1.211:5060".parse().unwrap());

        let xml = adapter.build_broadcast_notification("12010100001320000012");

        assert!(
            xml.contains("<CmdType>Broadcast</CmdType>"),
            "Should contain Broadcast command type"
        );
        assert!(
            xml.contains("<SourceID>31011500001000000001</SourceID>"),
            "Should contain source platform ID"
        );
        assert!(
            xml.contains("<TargetID>12010100001320000012</TargetID>"),
            "Should contain target ID"
        );
    }

    #[tokio::test]
    async fn test_parse_broadcast_answer_accept() {
        let deps = ProtocolDeps::for_test();
        let adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));

        let body = r#"<?xml version="1.0"?>
<Response>
<CmdType>Broadcast</CmdType>
<SN>1</SN>
<DeviceID>12010100001320000011</DeviceID>
<Result>Accept</Result>
<AudioPort>50000</AudioPort>
</Response>"#;

        let result = adapter.parse_broadcast_answer(body);
        assert!(result.is_some());
        let (accept, port) = result.unwrap();
        assert!(accept, "Should be accepted");
        assert_eq!(port, Some(50000), "Audio port should be 50000");
    }

    #[tokio::test]
    async fn test_parse_broadcast_answer_reject() {
        let deps = ProtocolDeps::for_test();
        let adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));

        let body = r#"<?xml version="1.0"?>
<Response>
<CmdType>Broadcast</CmdType>
<SN>1</SN>
<DeviceID>12010100001320000011</DeviceID>
<Result>Reject</Result>
</Response>"#;

        let result = adapter.parse_broadcast_answer(body);
        assert!(result.is_some());
        let (accept, port) = result.unwrap();
        assert!(!accept, "Should be rejected");
        assert_eq!(port, None, "Audio port should be None");
    }

    #[tokio::test]
    async fn test_build_audio_invite_response_tcp() {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            crate::protocol::gb28181::set_gb28181_platform_config(
                "31011500001000000001".to_string(),
                "3101150000".to_string(),
                "192.168.1.100".to_string(),
                5060,
            );
        });

        let deps = ProtocolDeps::for_test();
        let adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));

        let sdp = adapter.build_audio_invite_response_tcp(50000, 12345678);

        assert!(
            sdp.contains("m=audio 50000 TCP/RTP/AVP 8"),
            "Should contain audio m-line with TCP/RTP/AVP"
        );
        assert!(
            sdp.contains("a=rtpmap:8 PCMA/8000"),
            "Should contain PCMA/8000 rtpmap"
        );
        assert!(
            sdp.contains("a=recvonly"),
            "Should contain recvonly direction"
        );
        assert!(
            sdp.contains("y=0012345678"),
            "Should contain SSRC in y-line"
        );
    }

    #[test]
    fn test_parse_xml_field_with_attributes() {
        let xml = r#"<Response><DeviceID attr=">">34020000001110000001</DeviceID></Response>"#;
        let result = Gb28181Adapter::parse_xml_field(xml, "DeviceID");
        assert_eq!(result, Some("34020000001110000001".to_string()));
    }

    #[test]
    fn test_parse_xml_field_with_special_chars_in_content() {
        let xml = r#"<Name>Camera <Front> 1</Name>"#;
        let result = Gb28181Adapter::parse_xml_field(xml, "Name");
        assert_eq!(result, Some("Camera <Front> 1".to_string()));
    }

    #[test]
    fn test_parse_catalog_response_with_info_block() {
        let deps = ProtocolDeps::for_test();
        let adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));

        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>1</SN><DeviceID>12010100001320000011</DeviceID><SumNum>1</SumNum>
<DeviceList><Item><DeviceID>34020000001110000001</DeviceID><Name>Camera1</Name><Status>ON</Status><Parental>0</Parental>
<Info><DeviceType>IPC</DeviceType><Protocol>GB28181</Protocol><PTZType>3</PTZType><VideoInputNumber>1</VideoInputNumber><AudioInputNumber>1</AudioInputNumber><AlarmOutputNumber>0</AlarmOutputNumber></Info>
</Item></DeviceList></Response>"#;

        let channels = adapter.parse_catalog_response(body);

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].device_id, "34020000001110000001");
        assert_eq!(channels[0].name, "Camera1");
        assert!(channels[0].info.is_some());
        let info = channels[0].info.as_ref().unwrap();
        assert_eq!(info.device_type, Some("IPC".to_string()));
        assert_eq!(info.protocol, Some("GB28181".to_string()));
        assert_eq!(info.ptz_type, Some(3));
        assert_eq!(info.video_input_number, Some(1));
        assert_eq!(info.audio_input_number, Some(1));
        assert_eq!(info.alarm_output_number, Some(0));
    }

    #[test]
    fn test_parse_catalog_response_empty_returns_empty_vec() {
        let deps = ProtocolDeps::for_test();
        let adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));

        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>1</SN><DeviceID>12010100001320000011</DeviceID><SumNum>0</SumNum>
<DeviceList></DeviceList></Response>"#;

        let channels = adapter.parse_catalog_response(body);
        assert_eq!(channels.len(), 0);
    }

    #[test]
    fn test_parse_catalog_response_real_data() {
        let deps = ProtocolDeps::for_test();
        let adapter = Gb28181Adapter::new(deps, Arc::new(|_, _| {}), Arc::new(|_| {}));

        let body = r#"<?xml version="1.0" encoding="GB2312"?>
<Response>
<CmdType>Catalog</CmdType>
<SN>1</SN>
<DeviceID>12010100001320000011</DeviceID>
<SumNum>2</SumNum>
<DeviceList Num="2">
<Item>
<DeviceID>12010100001310000011</DeviceID>
<Name>Camera 01</Name>
<Manufacturer>Hikvision</Manufacturer>
<Model>IP Camera</Model>
<Owner>Owner</Owner>
<CivilCode>3101150000</CivilCode>
<Address>Address</Address>
<Parental>0</Parental>
<ParentID>31011500001000000001</ParentID>
<SafetyWay>0</SafetyWay>
<RegisterWay>1</RegisterWay>
<Secrecy>0</Secrecy>
<Status>ON</Status>
</Item>
<Item>
<DeviceID>12010100001370000011</DeviceID>
<Name></Name>
<Manufacturer>Hikvision</Manufacturer>
<Model>AudioOut</Model>
<Owner>Owner</Owner>
<CivilCode>3101150000</CivilCode>
<Address>Address</Address>
<Parental>0</Parental>
<ParentID>12010100001320000011</ParentID>
<SafetyWay>0</SafetyWay>
<RegisterWay>1</RegisterWay>
<Secrecy>0</Secrecy>
<Status>ON</Status>
</Item>
</DeviceList>
</Response>"#;

        let channels = adapter.parse_catalog_response(body);

        assert_eq!(channels.len(), 2);

        assert_eq!(channels[0].device_id, "12010100001310000011");
        assert_eq!(channels[0].name, "Camera 01");
        assert_eq!(channels[0].manufacturer, Some("Hikvision".to_string()));
        assert_eq!(channels[0].model, Some("IP Camera".to_string()));
        assert_eq!(channels[0].status, "ON");
        assert!(!channels[0].parental);
        assert_eq!(
            channels[0].parent_id,
            Some("31011500001000000001".to_string())
        );
        assert_eq!(channels[0].civil_code, Some("3101150000".to_string()));

        assert_eq!(channels[1].device_id, "12010100001370000011");
        assert_eq!(channels[1].name, "");
        assert_eq!(
            channels[1].parent_id,
            Some("12010100001320000011".to_string())
        );
    }
}
