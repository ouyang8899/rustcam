use std::collections::HashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use async_trait::async_trait;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::io::{AsyncWriteExt, AsyncWrite};
use chrono::{DateTime, Utc};

use crate::context::registry;
use crate::protocol::event::{EventHandler, SignalEvent, ProtocolType, TransportType, CatalogChannel, PtzCommand};

use crate::adapter::media_server::RtpTransport;
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::traits::ProtocolDeps;
use crate::protocol::gb28181::auth::{
    parse_sip_authorization, verify_sip_digest,
    generate_nonce, verify_nonce, consume_nonce, VerifyResult,
};
use crate::protocol::gb28181::sip::{SipMessage, SipUri, SipMethod, SipNameAddr, SipMessageBuffer};
use crate::error::{Result, AppError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gb28181Version { Gb2016, Gb2011 }

impl Default for Gb28181Version {
    fn default() -> Self { Gb28181Version::Gb2016 }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegistrationState {
    Init,        // 初始化
    Registered,  // 注册完成，等待发送 catalog
    CatalogSent, // 已发送 catalog，等待响应
    InviteSent,  // 已发送 INVITE，等待 ACK
    Streaming,   // 收到 ACK，媒体流进行中
    Failed,      // 失败状态，需要重置
}

impl Default for RegistrationState {
    fn default() -> Self { RegistrationState::Init }
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportProtocol { Tcp, Udp }

type GbWriteHalf = Arc<tokio::sync::RwLock<OwnedWriteHalf>>;

#[derive(Clone)]
pub struct SubSession {
    pub call_id: String,
    pub seq: u32,
    pub channel_id: String,
    pub parent_device_id: String,
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
    pub transport: TransportProtocol,
    pub device_host: String,
    pub device_port: u16,
    pub rtp_port: u16,
    pub media_started: bool,
    pub device_nated_ip: Option<String>,
    pub device_nated_port: Option<u16>,
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
    pub transport: TransportProtocol,
    pub call_id: Option<String>,
    pub from_tag: Option<String>,
    pub to_tag: Option<String>,
    pub udp_peer: Option<SocketAddr>,
    pub registration_expires: Option<u64>,
    pub deps: Arc<ProtocolDeps>,
    pub sessions: Arc<tokio::sync::RwLock<HashMap<String, SubSession>>>,
    pub subscriptions: Arc<tokio::sync::RwLock<HashMap<String, Subscription>>>,
    pub audio_talk_sessions: Arc<tokio::sync::RwLock<HashMap<String, AudioTalkSession>>>,
    pub registration_state: RegistrationState,
    pub wait_timer: Option<WaitTimer>,
    pub event_handler_name: Option<String>,
    pub register_fn: Arc<dyn Fn(String, crate::protocol::adapter_manager::AdapterEntry) + Send + Sync>,
    pub unregister_fn: Arc<dyn Fn(String) + Send + Sync>,
    pub last_message_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub event_type: String,
    pub expires: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct AudioTalkSession {
    pub device_tag: String,
    pub device_ip: String,
    pub device_audio_port: u16,
    pub platform_audio_port: u16,
    pub call_id: String,
    pub from_tag: String,
    pub to_tag: Option<String>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub audio_socket: Option<Arc<tokio::net::UdpSocket>>,
    pub tcp_stream: Option<Arc<tokio::sync::RwLock<tokio::net::TcpStream>>>,
    pub rtp_ssrc: u32,
    pub rtp_sequence: u16,
    pub rtp_timestamp: u32,
}

impl Gb28181Adapter {
    async fn get_device_app(&self, channel_id: &str) -> Option<String> {
        self.deps.device_lookup.find_by_tag(channel_id).await.and_then(|d| d.app)
    }

    pub fn set_local_addr(&mut self, addr: SocketAddr) {
        self.local_addr = Some(addr);
    }

    pub fn new(
        deps: ProtocolDeps,
        register_fn: Arc<dyn Fn(String, crate::protocol::adapter_manager::AdapterEntry) + Send + Sync>,
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
            transport: TransportProtocol::Tcp,
            call_id: None,
            from_tag: None,
            to_tag: None,
            udp_peer: None,
            registration_expires: None,
            deps: Arc::new(deps),
            sessions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            subscriptions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            audio_talk_sessions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            registration_state: RegistrationState::Init,
            wait_timer: None,
            event_handler_name: None,
            register_fn,
            unregister_fn,
            last_message_at: None,
        }
    }

    pub fn registration_expires(&self) -> u64 {
        self.registration_expires.unwrap_or(3600)
    }

    pub fn set_registration_state(&mut self, state: RegistrationState) {
        if self.registration_state != state {
            tracing::info!("[GB28181] Registration state transition: {:?} -> {:?}", self.registration_state, state);
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
            RegistrationState::Init | RegistrationState::Streaming | RegistrationState::Failed => {
                return;
            }
            RegistrationState::Registered | RegistrationState::CatalogSent => {
                let timer = match &mut self.wait_timer {
                    Some(t) => t,
                    None => return,
                };
                if timer.should_retry() {
                    tracing::info!("[GB28181] Catalog timeout, retry {}/{}", timer.retry_count + 1, timer.max_retries);
                    timer.increment_retry();
                    timer.reset_with_time();
                    let query = self.build_catalog_query();
                    let _ = self.send(&query).await;
                } else {
                    tracing::warn!("[GB28181] Catalog timeout, max retries reached");
                    self.set_registration_state(RegistrationState::Failed);
                }
            }
            RegistrationState::InviteSent => {
                let timer = match &mut self.wait_timer {
                    Some(t) => t,
                    None => return,
                };
                if timer.should_retry() {
                    tracing::info!("[GB28181] INVITE timeout, retry {}/{}", timer.retry_count + 1, timer.max_retries);
                    timer.increment_retry();
                    timer.reset_with_time();
                    if let Err(e) = self.start(&device_tag).await {
                        tracing::error!("[GB28181] Retry INVITE failed: {}", e);
                    }
                } else {
                    tracing::warn!("[GB28181] INVITE timeout, max retries reached");
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
        if s.len() <= 20 { s.to_string() } else { s[..20].to_string() }
    }

    fn truncate_32(s: &str) -> String {
        if s.len() <= 32 { s.to_string() } else { s[..32].to_string() }
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
        if buffer.len() < total_len { return None; }

        let first_line = header_section.lines().next()?;
        let body = if content_length > 0 { &msg_str[body_start..total_len] } else { "" };

        Some((first_line.to_string(), headers, body.to_string()))
    }

    fn get_header(headers: &[(String, String)], name: &str) -> Option<String> {
        headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }

    fn parse_sip_uri(uri: &str) -> String {
        let uri = uri.trim();
        let uri = uri.strip_prefix('<').unwrap_or(uri).trim_end_matches('>').trim();
        let uri = uri.strip_prefix("sip:").or_else(|| uri.strip_prefix("SIP:")).unwrap_or(uri);
        uri.split('@').next().unwrap_or(uri).trim().to_string()
    }

    fn parse_sip_uri_with_domain(uri: &str) -> String {
        let uri = uri.trim();
        let uri = uri.strip_prefix('<').unwrap_or(uri).trim_end_matches('>').trim();
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

    fn parse_tag_from_header(header_value: &str) -> Option<String> {
        if let Some(tag_start) = header_value.find("tag=") {
            let after_tag = &header_value[tag_start + 4..];
            let end = after_tag.find(&[',', ';', '\r', '\n'][..]).unwrap_or(after_tag.len());
            if end > 0 { return Some(after_tag[..end].to_string()); }
        }
        None
    }

    fn parse_contact_host_port(headers: &[(String, String)]) -> Option<(String, u16)> {
        let contact_value = headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Contact"))
            .map(|(_, v)| v.trim())?;

        let uri = if contact_value.starts_with('<') && contact_value.ends_with('>') {
            &contact_value[1..contact_value.len()-1]
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
            
            let host = hp_clean.trim_end_matches(|c| c == '>' || c == ';' || c == '\r' || c == '\n');
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
        let start = format!("<{}>", tag);
        let end = format!("</{}>", tag);
        if let Some(s) = xml.find(&start) {
            let data_start = s + start.len();
            if let Some(e) = xml[data_start..].find(&end) {
                return Some(xml[data_start..data_start + e].trim().to_string());
            }
        }
        None
    }

    fn parse_channel_id(body: &str) -> Option<String> {
        Self::parse_xml_field(body, "ChannelID")
            .or_else(|| Self::parse_xml_field(body, "DeviceID"))
    }

    fn best_direction(pan: f64, tilt: f64, zoom: f64) -> &'static str {
        let p = pan.abs();
        let t = tilt.abs();
        let z = zoom.abs();
        if p >= t && p >= z {
            if pan > 0.0 { "right" } else { "left" }
        } else if t >= p && t >= z {
            if tilt > 0.0 { "up" } else { "down" }
        } else {
            if zoom > 0.0 { "zoom_in" } else { "zoom_out" }
        }
    }

    fn encode_ptz_cmd_compound(pan: f64, tilt: f64, zoom: f64, base_speed: u8) -> String {
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

        format!("80 01 {:02X} {:02X} {:02X} {:02X} {:02X} 00", cmd1, cmd2, p_byte, t_byte, z_byte)
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

    fn build_ptz_xml_compound(channel_id: &str, pan: f64, tilt: f64, zoom: f64, base_speed: u8) -> String {
        let ptz_cmd = Self::encode_ptz_cmd_compound(pan, tilt, zoom, base_speed);
        let sn = chrono::Utc::now().timestamp_millis() % 1000000;
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>PTZCmd</CmdType><SN>{}</SN><DeviceID>{}</DeviceID><PTZCmd>{}</PTZCmd></Notify>"#,
            sn, channel_id, ptz_cmd
        )
    }

    fn build_ptz_message(
        &self, channel_id: &str, parent_device_id: &str, body: &str,
    ) -> String {
        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let server_port = self.deps.config.server.port;
        let server_ip = self.remote_addr.map(|a| a.ip().to_string()).unwrap_or_else(|| "0.0.0.0".to_string());
        let branch = format!("z9hG4bK{}", &call_id[..7]);

        let via = if self.transport == TransportProtocol::Tcp {
            format!("SIP/2.0/TCP {}:{};branch={};rport", server_ip, server_port, branch)
        } else {
            format!("SIP/2.0/UDP {}:{};branch={};rport", server_ip, server_port, branch)
        };

        let body_len = body.len();
        let to_uri = format!("sip:{}@{}", parent_device_id,
            self.remote_addr.map(|a| a.to_string()).unwrap_or_else(|| "0.0.0.0:5060".to_string()));
        let from_uri = format!("sip:rustcam@{}:{}", server_ip, server_port);
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");

        format!(
            "MESSAGE {to_uri} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <{from_uri}>;tag={from_tag}\r\n\
             To: <{to_uri}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {sn} MESSAGE\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Type: Application/MANSCDP+xml\r\n\
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
        let sn = self.seq.load(Ordering::SeqCst);
        let device_tag = self.device_tag_str();
        let (platform_id, platform_domain, platform_ip, platform_port) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, domain, ip, port)| (id.as_str(), domain.as_str(), ip.as_str(), *port))
            .unwrap_or(("00000000000000000000", "0000000000", "0.0.0.0", 5060));

        let local_ip = if platform_ip == "0.0.0.0" {
            if let Some(peer) = self.udp_peer {
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
            host: format!("{}:{}", local_ip, platform_port),
            port: None,
            params: HashMap::new(),
        };
        let device_ip = self.udp_peer
            .map(|p| p.ip().to_string())
            .unwrap_or_else(|| local_ip.clone());
        let to_uri = SipUri {
            user: Some(device_tag.to_string()),
            host: format!("{}:{}", device_ip, platform_port),
            port: None,
            params: HashMap::new(),
        };

        let from_name_addr = SipNameAddr::new(from_uri)
            .with_param("domain", platform_domain)
            .with_param("tag", &format!("cat{}", ts));
        let to_name_addr = SipNameAddr::new(to_uri.clone());

        let via_str = format!("SIP/2.0/UDP {}:{};branch=z9hG4bKcat{}", local_ip, platform_port, ts);
        
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
        let mut channels = Vec::new();
        let mut start = 0;
        while let Some(item_start) = body[start..].find("<Item>") {
            let abs_start = start + item_start;
            if let Some(item_end) = body[abs_start..].find("</Item>") {
                let item_str = &body[abs_start..abs_start + item_end + 7];

                let device_id = Self::parse_xml_field(item_str, "DeviceID").unwrap_or_default();
                if !device_id.is_empty() {
                    let parental = Self::parse_xml_field(item_str, "Parental")
                        .and_then(|v| v.parse::<u32>().ok())
                        .map(|v| v > 0)
                        .unwrap_or(false);

                    let port = Self::parse_xml_field(item_str, "Port")
                        .and_then(|v| v.parse::<u16>().ok());

                    let secrecy = Self::parse_xml_field(item_str, "Secrecy")
                        .and_then(|v| v.parse::<u8>().ok());

                    let ptz_type = Self::parse_xml_field(item_str, "PTZType")
                        .and_then(|v| v.parse::<u8>().ok());

                    let info = self.parse_device_info_block(item_str);

                    channels.push(CatalogChannel {
                        device_id,
                        name: Self::parse_xml_field(item_str, "Name").unwrap_or_default(),
                        manufacturer: Self::parse_xml_field(item_str, "Manufacturer"),
                        model: Self::parse_xml_field(item_str, "Model"),
                        status: Self::parse_xml_field(item_str, "Status").unwrap_or_else(|| "OFF".to_string()),
                        parental,
                        parent_id: Some(self.device_tag_str()),
                        civil_code: Self::parse_xml_field(item_str, "CivilCode"),
                        address: Self::parse_xml_field(item_str, "Address"),
                        ip_address: Self::parse_xml_field(item_str, "IPAddress"),
                        port,
                        owner: Self::parse_xml_field(item_str, "Owner"),
                        secrecy,
                        device_type: Self::parse_xml_field(item_str, "DeviceType"),
                        ptz_type,
                        info,
                    });
                }
                start = abs_start + item_end + 7;
            } else {
                break;
            }
        }

        channels
    }

    fn parse_device_info_block(&self, xml: &str) -> Option<crate::protocol::event::DeviceInfoBlock> {
        let info_start = xml.find("<Info>")?;
        let info_end = xml.find("</Info>")?;
        let info_str = &xml[info_start + 6..info_end];

        Some(crate::protocol::event::DeviceInfoBlock {
            device_type: Self::parse_xml_field(info_str, "DeviceType"),
            protocol: Self::parse_xml_field(info_str, "Protocol"),
            ptz_type: Self::parse_xml_field(info_str, "PTZType").and_then(|v| v.parse::<u8>().ok()),
            video_input_number: Self::parse_xml_field(info_str, "VideoInputNumber").and_then(|v| v.parse::<u8>().ok()),
            audio_input_number: Self::parse_xml_field(info_str, "AudioInputNumber").and_then(|v| v.parse::<u8>().ok()),
            alarm_output_number: Self::parse_xml_field(info_str, "AlarmOutputNumber").and_then(|v| v.parse::<u8>().ok()),
        })
    }

    fn detect_version(body: &str) -> Gb28181Version {
        if body.contains("GB28181") || body.contains("28181") { Gb28181Version::Gb2016 }
        else { Gb28181Version::Gb2016 }
    }

    fn via_header(&self, branch: &str) -> String {
        let addr = self.remote_addr.map(|a| a.to_string()).unwrap_or_else(|| "0.0.0.0".to_string());
        match self.transport {
            TransportProtocol::Tcp => format!("SIP/2.0/TCP {};rport;branch={}", addr, branch),
            TransportProtocol::Udp => format!("SIP/2.0/UDP {};rport;branch={}", addr, branch),
        }
    }

    fn call_id_str(&self) -> String {
        self.call_id.clone().unwrap_or_else(|| "rustcam-call-id".to_string())
    }

    fn from_tag_param(&self) -> String {
        self.from_tag.as_ref().map(|t| format!(";tag={}", t)).unwrap_or_default()
    }

    fn to_tag_param(&self) -> String {
        self.to_tag.as_ref().map(|t| format!(";tag={}", t)).unwrap_or_default()
    }

    fn build_sip_response_only_headers(
        &self, status_code: u16, status_text: &str,
        via_branch: &str, cseq_line: &str,
    ) -> String {
        let device = self.device_tag_str();
        let expires_line = if let Some(exp) = self.registration_expires {
            format!("Expires: {}\r\n", exp)
        } else {
            String::new()
        };
        let from_uri = self.from_uri.clone().unwrap_or_else(|| format!("sip:{}", self.device_tag_str()));
        format!(
            "SIP/2.0 {} {}\r\n\
             Via: {}\r\n\
             From: <{}>{}\r\n\
             To: <{}>{}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             {}User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            status_code, status_text,
            self.via_header(via_branch),
            from_uri, self.from_tag_param(),
            from_uri, self.to_tag_param(),
            cseq_line, self.call_id_str(),
            expires_line,
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
        tracing::info!("[GB28181] build_401_response: from_uri='{}', from_tag='{}', to_tag='{}'", 
            from_uri, self.from_tag.as_deref().unwrap_or(""), self.to_tag.as_deref().unwrap_or(""));
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
            from_uri, self.from_tag_param(),
            from_uri, self.to_tag_param(),
            cseq_line, self.call_id_str(), www_auth
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
            from_uri, self.from_tag_param(),
            from_uri, self.to_tag_param(),
            cseq_line, self.call_id_str()
        )
    }
    
    fn build_403_response_with_headers(&self, via_branch: &str, cseq_line: &str, from_hdr: &str, to_hdr: &str, call_id_hdr: &str) -> String {
        let from_tag = Self::parse_tag_from_header(from_hdr).map(|t| format!(";tag={}", t)).unwrap_or_default();
        let to_tag = Self::parse_tag_from_header(to_hdr).map(|t| format!(";tag={}", t)).unwrap_or_default();
        format!(
            "SIP/2.0 403 Forbidden\r\n\
             Via: {}\r\n\
             From: {}{}\r\n\
             To: {}{}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            self.via_header(via_branch),
            from_hdr, from_tag,
            to_hdr, to_tag,
            cseq_line, call_id_hdr
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
            "BasicParam" => r#"<BasicParam><VideoEnable>true</VideoEnable><AudioEnable>true</AudioEnable></BasicParam>"#,
            "NetworkParam" => r#"<NetworkParam><IPAddress>0.0.0.0</IPAddress><Port>0</Port></NetworkParam>"#,
            "VideoParam" => r#"<VideoParam><Resolution>1920x1080</Resolution><BitRate>2048</BitRate><FrameRate>25</FrameRate></VideoParam>"#,
            "VideoSrcParam" => r#"<VideoSrcParam><Resolution>1920x1080</Resolution><Quality>85</Quality></VideoSrcParam>"#,
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
        let presets = vec![
            ("1", "Preset 1"),
            ("2", "Preset 2"),
            ("3", "Preset 3"),
        ];
        let mut preset_items = String::new();
        for (idx, name) in presets {
            preset_items.push_str(&format!(r#"<PresetItem><id>{}</id><presetName>{}</presetName></PresetItem>"#, idx, name));
        }
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Notify><CmdType>PresetList</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<PresetList>{}</PresetList></Notify>"#,
            sn, channel_id, preset_items
        )
    }

    fn build_preset_response(&self, channel_id: &str, preset_id: &str, preset_name: &str, cmd_type: &str) -> String {
        let sn = self.seq.load(Ordering::SeqCst);
        let result = if cmd_type == "SetPreset" {
            format!(r#"<PresetID>{}</PresetID><PresetName>{}</PresetName>"#, preset_id, preset_name)
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
        let video_codec = config.and_then(|c| c.video_codec.clone()).unwrap_or_else(|| "PS".to_string());
        let audio_codec = config.and_then(|c| c.audio_codec.clone()).unwrap_or_else(|| "PCMA".to_string());
        let stream_mode = config.and_then(|c| c.stream_mode.clone()).unwrap_or_else(|| "recvonly".to_string());
        let profile_level_id = config.and_then(|c| c.profile_level_id.clone());
        let packaging_mode = config.and_then(|c| c.packaging_mode.clone());
        let sprop_parameter_sets = config.and_then(|c| c.sprop_parameter_sets.clone());

        let video_pt = config.and_then(|c| c.video_payload_type)
            .unwrap_or_else(|| Self::codec_to_video_pt(&video_codec));
        let audio_pt = config.and_then(|c| c.audio_payload_type)
            .unwrap_or_else(|| Self::codec_to_audio_pt(&audio_codec));

        let (video_encoding, video_clock) = Self::codec_to_rtp_map(&video_codec);
        let (audio_encoding, audio_clock) = Self::codec_to_audio_rtp_map(&audio_codec);

        let mut video_line = format!("m=video {} RTP/AVP {}\r\n", rtp_port, video_pt);
        video_line.push_str(&format!("a={}\r\n", stream_mode));
        video_line.push_str(&format!("a=rtpmap:{} {}/{}\r\n", video_pt, video_encoding, video_clock));

        if let Some(ref pli) = profile_level_id {
            if video_codec == "PS" {
                if let Some(ref pm) = packaging_mode {
                    video_line.push_str(&format!("a=fmtp:{} profile-level-id={};packaging_mode={}\r\n", video_pt, pli, pm));
                }
            } else if video_codec == "H264" || video_codec == "H265" {
                if let Some(ref sps) = sprop_parameter_sets {
                    video_line.push_str(&format!("a=fmtp:{} profile-level-id={};sprop-parameter-sets={}\r\n", video_pt, pli, sps));
                } else {
                    video_line.push_str(&format!("a=fmtp:{} profile-level-id={}\r\n", video_pt, pli));
                }
            } else {
                video_line.push_str(&format!("a=fmtp:{} profile-level-id={}\r\n", video_pt, pli));
            }
        }

        let audio_enabled = !audio_codec.is_empty() && audio_codec.to_uppercase() != "NONE" && audio_codec.to_uppercase() != "OFF";
        let audio_line = if audio_enabled {
            format!(
                "m=audio {} RTP/AVP {}\r\n\
                 a={}\r\n\
                 a=rtpmap:{} {}/{}\r\n",
                rtp_port + 2,
                audio_pt,
                stream_mode,
                audio_pt,
                audio_encoding,
                audio_clock
            )
        } else {
            String::new()
        };

        let (sdp_type, time_range) = if let (Some(start), Some(end)) = (start_time, end_time) {
            ("Playback", format!("{} {}", Self::datetime_to_ntp_string(start), Self::datetime_to_ntp_string(end)))
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
        sdp.push_str(&format!("y={}\r\n", ssrc));

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

    async fn alloc_rtp_port(&self, stream_key: &str, server_name: Option<&str>) -> Result<(u16, String, String)> {
        let adapter = if let Some(name) = server_name {
            self.deps.cluster.get_server(name)
        } else {
            None
        };

        let adapter = match adapter {
            Some(a) if a.is_online().await => {
                tracing::info!("[GB28181] Using device-associated media server: {}", server_name.unwrap());
                a
            }
            _ => {
                if server_name.is_some() {
                    tracing::warn!("[GB28181] Media server {} unavailable, falling back", server_name.unwrap());
                }
                match self.deps.cluster.select_server().await {
                    Some(s) => s,
                    None => return Err(AppError::MediaServerError("No media server available".to_string())),
                }
            }
        };

        let tag = adapter.tag().to_string();
        match adapter.open_rtp_server(stream_key, 0, RtpTransport::Udp).await {
            Ok((port, ip)) => {
                tracing::info!("[GB28181] RTP allocated: stream={}, port={}, ip={}, server={}", stream_key, port, ip, tag);
                Ok((port, ip, tag))
            }
            Err(e) => {
                tracing::warn!("[GB28181] RTP failed on {}, trying fallback: {}", tag, e);
                if server_name.is_some() {
                    match self.deps.cluster.select_server().await {
                        Some(fallback) => {
                            let fb_tag = fallback.tag().to_string();
                            fallback.open_rtp_server(stream_key, 0, RtpTransport::Udp).await
                                .map(|(port, ip)| {
                                    tracing::info!("[GB28181] RTP fallback: server={}", fb_tag);
                                    (port, ip, fb_tag)
                                })
                                .map_err(|e2| AppError::MediaServerError(format!("RTP fallback failed: {}", e2)))
                        }
                        None => Err(AppError::MediaServerError(format!("No media server available: {}", e))),
                    }
                } else {
                    Err(AppError::MediaServerError(format!("No media server available: {}", e)))
                }
            }
        }
    }

    async fn handle_register(
        &mut self, headers: &[(String, String)], body: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        self.version = Self::detect_version(body);

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
            if let Some(cid) = Self::get_header(headers, "Call-ID") { self.call_id = Some(cid); }
        }
        if let Some(from_hdr) = Self::get_header(headers, "From") {
            if self.from_tag.is_none() {
                if let Some(tag) = Self::parse_tag_from_header(&from_hdr) { self.from_tag = Some(tag); }
            }
        }
        if let Some(to_hdr) = Self::get_header(headers, "To") {
            if let Some(tag) = Self::parse_tag_from_header(&to_hdr) {
                if self.to_tag.is_none() { self.to_tag = Some(tag); }
            } else if self.to_tag.is_none() {
                self.to_tag = Some(uuid::Uuid::new_v4().to_string().replace("-", ""));
            }
        }

        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);
        tracing::info!("[GB28181] REGISTER: {} (expires={}s)", device_id, self.registration_expires());

        let auth_header = headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Authorization"))
            .map(|(_, v)| v.as_str());

        if auth_header.is_none() {
            let server_realm = super::get_gb28181_platform_config()
                .get().map(|c| c.1.clone()).unwrap_or_else(|| device_id.clone());
            tracing::info!("[GB28181] No Authorization header, sending 401 to {} with realm={}", device_id, server_realm);
            return (None, Some(self.build_401_response(&via_branch, &cseq, &server_realm)));
        }

        let auth_params = match parse_sip_authorization(auth_header.unwrap()) {
            Some(p) => p,
            None => {
                tracing::warn!("[GB28181] Failed to parse Authorization header: {}", auth_header.unwrap());
                return (None, Some(self.build_403_response(&via_branch, &cseq)));
            }
        };
        tracing::info!("[GB28181] Auth params: username={}, realm={}, nonce={}", auth_params.username, auth_params.realm, auth_params.nonce);
        if !verify_nonce(&auth_params.nonce) {
            let server_realm = super::get_gb28181_platform_config()
                .get().map(|c| c.1.clone()).unwrap_or_else(|| device_id.clone());
            tracing::warn!("[GB28181] Invalid nonce from {}, sending 401 with new nonce", device_id);
            return (None, Some(self.build_401_response(&via_branch, &cseq, &server_realm)));
        }

        let device_opt = self.deps.device_lookup.find_by_tag(&device_id).await;
        let (username, password, internal_id, device_tag) = match device_opt {
            Some(d) => (d.device_tag.clone().unwrap_or_default(), d.device_password.clone().unwrap_or_default(), d.id, Some(d.device_tag.unwrap_or_default())),
            None => {
                tracing::warn!("[GB28181] Device not found: {}", device_id);
                return (None, Some(self.build_403_response(&via_branch, &cseq)));
            }
        };
        tracing::info!("[GB28181] Device found: username={}, password={}", username, password);
        if auth_params.username != username {
            tracing::warn!("[GB28181] Username mismatch: auth={} vs db={}", auth_params.username, username);
            return (None, Some(self.build_403_response(&via_branch, &cseq)));
        }

        match verify_sip_digest(&auth_params, &password, "REGISTER") {
            VerifyResult::Valid => {
                tracing::info!("[GB28181] Device {} authenticated", device_id);
                consume_nonce(&auth_params.nonce);
                self.device_id = Some(internal_id as i64);

                let prev_state = self.registration_state;
                self.set_registration_state(RegistrationState::Registered);

                let entry: crate::protocol::adapter_manager::AdapterEntry = Arc::new(tokio::sync::Mutex::new(Box::new(self.clone())));
                (self.register_fn)(device_id.clone(), entry);
                let _ = self.deps.device_lookup.set_online(&device_id).await;

                if let Some((host, port)) = Self::parse_contact_host_port(headers) {
                    tracing::info!("[GB28181] Device {} Contact header parsed: host={}, port={}", device_id, host, port);
                    let _ = self.deps.device_lookup.update_device_host_port(&device_id, &host, port).await;
                }

                let event = if prev_state == RegistrationState::Init {
                    tracing::info!("[GB28181] Device {} registered as new, sending catalog query", device_id);
                    self.set_registration_state(RegistrationState::CatalogSent);
                    let query = self.build_catalog_query();
                    let _ = self.send(&query).await;
                    self.start_wait_timer(30, 3);
                    SignalEvent::DeviceRegister {
                        device_id: internal_id,
                        device_tag,
                        name: self.device_name.clone().unwrap_or_else(|| "GB28181 Device".to_string()),
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

                (Some(event), Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)))
            }
            _ => (None, Some(self.build_403_response(&via_branch, &cseq))),
        }
    }
    

    async fn handle_message(
        &mut self, headers: &[(String, String)], body: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        if self.call_id.is_none() {
            if let Some(cid) = Self::get_header(headers, "Call-ID") { self.call_id = Some(cid); }
        }
        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);

        if self.registration_state == RegistrationState::Init {
            let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
            let device_id_from_header = Self::parse_sip_uri(&from_hdr);
            if !device_id_from_header.is_empty() {
                if let Some(device) = self.deps.device_lookup.find_by_tag(&device_id_from_header).await {
                    tracing::info!("[GB28181] Restoring registration for device {} in new adapter", device_id_from_header);
                    self.device_tag = Some(device_id_from_header.clone());
                    self.device_id = Some(device.id);
                    self.set_registration_state(RegistrationState::InviteSent);
                    self.start_wait_timer(30, 3);
                    self.from_uri = Some(Self::parse_sip_uri_with_domain_only(&from_hdr));
                    tracing::info!("[GB28181] Init state + non-REGISTER, directly sending INVITE for device {}", device_id_from_header);
                    match self.start(&device_id_from_header).await {
                        Ok(_) => tracing::info!("[GB28181] INVITE sent for device {}", device_id_from_header),
                        Err(e) => tracing::error!("[GB28181] INVITE failed for device {}: {}", device_id_from_header, e),
                    }
                    return (Some(SignalEvent::DeviceKeepalive { device_id: self.device_id_i64(), device_tag: Some(device_id_from_header.clone()), timestamp: chrono::Utc::now() }),
                            Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)));
                }
            }
            tracing::warn!("[GB28181] Device not found, rejecting MESSAGE");
            let from_hdr = Self::get_header(headers, "From").unwrap_or_default();
            let to_hdr = Self::get_header(headers, "To").unwrap_or_default();
            let call_id_hdr = Self::get_header(headers, "Call-ID").unwrap_or_default();
            let resp = self.build_403_response_with_headers(&via_branch, &cseq, &from_hdr, &to_hdr, &call_id_hdr);
            return (None, Some(resp));
        }

        let device_tag = self.device_tag_str();

        if body.contains("<Response>") && body.contains("Catalog") {
            let channels = self.parse_catalog_response(body);
            tracing::debug!("[GB28181] parse_catalog_response returned {} channels", channels.len());
            for (i, ch) in channels.iter().enumerate() {
                tracing::debug!("[GB28181]   channel[{}]: id={}, name={}, parental={}", i, ch.device_id, ch.name, ch.parental);
            }
            if channels.is_empty() {
                tracing::info!("[GB28181] Catalog empty for device {} - IPC mode (device itself is the channel)", device_tag);
                self.set_registration_state(RegistrationState::InviteSent);
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
                    Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)),
                );
            }
            self.set_registration_state(RegistrationState::InviteSent);
            return (
                Some(SignalEvent::CatalogResponse {
                    device_id: self.device_id_i64(),
                    device_tag: Some(device_tag.clone()),
                    channels,
                }),
                Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)),
            );
        }

        if body.contains("<CmdType>Alarm</CmdType>") || body.contains("<AlarmNotify>") || body.contains("<Alarm>") {
            let alarm_type = Self::parse_xml_field(body, "AlarmType").unwrap_or_else(|| "unknown".to_string());
            let alarm_msg = Self::parse_xml_field(body, "AlarmDescription").unwrap_or_default();
            let device_id_val = self.device_id_i64();
            return (Some(SignalEvent::Alarm {
                device_id: device_id_val.to_string(),
                alarm_type,
                message: alarm_msg,
                timestamp: chrono::Utc::now(),
            }), Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)));
        }

        if body.contains("Catalog") || body.contains("Query") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| "00000000000000000000".to_string());
            let resp = format!("{}\r\n{}",
                               self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq).trim_end_matches("\r\n\r\n"),
                               self.build_catalog_response(&channel_id));
            return (Some(SignalEvent::QueryDeviceInfo { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()) }), Some(resp));
        }
        if body.contains("DeviceInfo") {
            let resp = format!("{}{}",
                               self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq),
                               self.build_device_info_response());
            return (Some(SignalEvent::QueryDeviceInfo { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()) }), Some(resp));
        }
        if body.contains("DeviceStatus") {
            let resp = format!("{}{}",
                               self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq),
                               self.build_device_status_response());
            return (Some(SignalEvent::QueryDeviceStatus { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()) }), Some(resp));
        }
        if body.contains("ConfigDownload") || body.contains("DeviceConfig") {
            let config_type = Self::parse_xml_field(body, "ConfigType").unwrap_or_else(|| "BasicParam".to_string());
            let resp = format!("{}{}",
                               self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq),
                               self.build_device_config_response(&config_type));
            tracing::info!("[GB28181] DeviceConfig query: device={} type={}", device_tag, config_type);
            return (Some(SignalEvent::QueryDeviceConfig { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()), config_type }), Some(resp));
        }
        if body.contains("ListPreset") || body.contains("PresetList") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| device_tag.clone());
            let resp = format!("{}{}",
                               self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq),
                               self.build_preset_list_response(&channel_id));
            tracing::info!("[GB28181] PresetList query: device={} channel={}", device_tag, channel_id);
            return (Some(SignalEvent::PresetQuery { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()), channel_id }), Some(resp));
        }
        if body.contains("SetPreset") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| device_tag.clone());
            let preset_id = Self::parse_xml_field(body, "PresetID").unwrap_or_else(|| "1".to_string());
            let preset_name = Self::parse_xml_field(body, "PresetName").unwrap_or_else(|| "Preset".to_string());
            let resp = format!("{}{}",
                               self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq),
                               self.build_preset_response(&channel_id, &preset_id, &preset_name, "SetPreset"));
            tracing::info!("[GB28181] SetPreset: device={} channel={} id={} name={}", device_tag, channel_id, preset_id, preset_name);
            return (Some(SignalEvent::PresetSet { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()), channel_id, preset_name }), Some(resp));
        }
        if body.contains("GotoPreset") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| device_tag.clone());
            let preset_index = Self::parse_xml_field(body, "PresetIndex").and_then(|v| v.parse().ok()).unwrap_or(1);
            let resp = format!("{}{}",
                               self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq),
                               self.build_preset_response(&channel_id, &preset_index.to_string(), "", "GotoPreset"));
            tracing::info!("[GB28181] GotoPreset: device={} channel={} index={}", device_tag, channel_id, preset_index);
            return (Some(SignalEvent::PresetGoto { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()), channel_id, preset_index }), Some(resp));
        }
        if body.contains("RemovePreset") {
            let channel_id = Self::parse_channel_id(body).unwrap_or_else(|| device_tag.clone());
            let preset_index = Self::parse_xml_field(body, "PresetID").and_then(|v| v.parse().ok()).unwrap_or(1);
            let resp = format!("{}{}",
                               self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq),
                               self.build_preset_response(&channel_id, &preset_index.to_string(), "", "RemovePreset"));
            tracing::info!("[GB28181] RemovePreset: device={} channel={} index={}", device_tag, channel_id, preset_index);
            return (Some(SignalEvent::PresetRemove { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()), channel_id, preset_index }), Some(resp));
        }

        match self.registration_state {
            RegistrationState::Init => {
                tracing::warn!("[GB28181] Keepalive in Init state, rejecting");
                let resp = self.build_403_response(&via_branch, &cseq);
                return (None, Some(resp));
            }
            RegistrationState::Registered => {
                tracing::info!("[GB28181] Keepalive, Registered state, waiting for catalog");
            }
            RegistrationState::CatalogSent => {
                tracing::info!("[GB28181] Keepalive, CatalogSent state, waiting for catalog response");
            }
            RegistrationState::InviteSent => {
                tracing::info!("[GB28181] Keepalive, InviteSent state, waiting for ACK");
            }
            RegistrationState::Streaming => {
                tracing::debug!("[GB28181] Keepalive, Streaming state, stream active");
            }
            RegistrationState::Failed => {
                tracing::warn!("[GB28181] Keepalive in Failed state, resetting to InviteSent");
                self.set_registration_state(RegistrationState::InviteSent);
                self.start_wait_timer(30, 3);
                if let Err(e) = self.start(&device_tag).await {
                    tracing::error!("[GB28181] INVITE failed: {}", e);
                }
            }
        }
        (Some(SignalEvent::DeviceKeepalive { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()), timestamp: chrono::Utc::now() }),
         Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)))
    }

    async fn handle_bye(
        &mut self, headers: &[(String, String)],
    ) -> (Option<SignalEvent>, Option<String>) {
        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);
        let call_id = Self::get_header(headers, "Call-ID").unwrap_or_default();

        if self.registration_state == RegistrationState::Init {
            tracing::warn!("[GB28181] BYE: device not registered, rejecting");
            return (None, Some(self.build_403_response(&via_branch, &cseq)));
        }

        let sub_session = {
            let sessions = self.sessions.read().await;
            sessions.values().find(|s| s.call_id == call_id).cloned()
        };

        if let Some(session) = sub_session {
            if session.parent_device_id == session.channel_id {
                tracing::info!("[GB28181] Single device BYE: channel={}, stream={}", session.channel_id, session.stream_key);
                if let Some(app) = self.get_device_app(&session.channel_id).await {
                    let _ = self.deps.stream_manager.stop_stream(&app, &session.stream_key).await;
                }
                self.deps.rtp_tunnel.unregister(&session.stream_key);
                self.sessions.write().await.clear();
                return (
                    Some(SignalEvent::StopPlay {
                        device_id: self.device_id_i64(),
                        device_tag: Some(session.channel_id),
                        session_id: session.stream_key,
                    }),
                    Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)),
                );
            }
            tracing::info!("[GB28181] Sub-device BYE: channel={}, stream={}", session.channel_id, session.stream_key);
            if let Some(app) = self.get_device_app(&session.channel_id).await {
                let _ = self.deps.stream_manager.stop_stream(&app, &session.stream_key).await;
            }
            self.deps.rtp_tunnel.unregister(&session.stream_key);
            self.sessions.write().await.retain(|_, s| s.call_id != call_id);
            return (
                Some(SignalEvent::StopPlay {
                    device_id: self.device_id_i64(),
                    device_tag: Some(session.channel_id),
                    session_id: session.stream_key,
                }),
                Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)),
            );
        }

        tracing::warn!("[GB28181] BYE with unknown Call-ID: {}, iterating all sessions", call_id);
        for session in self.sessions.read().await.values() {
            if let Some(app) = self.get_device_app(&session.channel_id).await {
                let _ = self.deps.stream_manager.stop_stream(&app, &session.stream_key).await;
            }
            self.deps.rtp_tunnel.unregister(&session.stream_key);
        }
        self.sessions.write().await.clear();

        let device_tag = self.device_tag_str();
        let _ = self.deps.device_lookup.set_offline(&device_tag, Some("BYE received")).await;

        (Some(SignalEvent::StopPlay {
            device_id: self.device_id_i64(),
            device_tag: Some(device_tag),
            session_id: call_id,
        }), Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)))
    }

    async fn handle_ack(&self, headers: &[(String, String)]) -> (Option<SignalEvent>, Option<String>) {
        let call_id = Self::get_header(headers, "Call-ID").unwrap_or_default();
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.values().find(|s| s.call_id == call_id) {
            tracing::info!("[GB28181] ACK for session: call_id={} channel={} stream={}", 
                call_id, session.channel_id, session.stream_key);
        } else {
            tracing::warn!("[GB28181] ACK for unknown session: {}, active_sessions={}", 
                call_id, sessions.len());
        }
        (None, None)
    }

    async fn handle_sip_response(
        &mut self, first_line: &str, headers: &[(String, String)], _body: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        let call_id = Self::get_header(headers, "Call-ID").unwrap_or_default();
        let cseq_line = Self::get_header(headers, "CSeq").unwrap_or_default();
        let is_invite_response = cseq_line.to_lowercase().contains("invite");

        let sub_session = {
            let sessions = self.sessions.read().await;
            sessions.values().find(|s| s.call_id == call_id).cloned()
        };

        let Some(session) = sub_session else {
            // No session found - might be a response to MESSAGE (e.g., catalog query)
            tracing::debug!("[GB28181] Response for unknown session: {} status={}, cseq={}", call_id, first_line, cseq_line);
            
            // Check if this is a 200 OK response to a MESSAGE (catalog query)
            if !is_invite_response && first_line.contains(" 200 ") && self.registration_state == RegistrationState::CatalogSent {
                tracing::info!("[GB28181] Catalog query response received, canceling timer and starting INVITE");
                self.wait_timer = None;  // Cancel the timer
                self.set_registration_state(RegistrationState::InviteSent);
                let device_tag = self.device_tag_str();
                self.start_wait_timer(30, 3);
                if let Err(e) = self.start(&device_tag).await {
                    tracing::error!("[GB28181] Failed to start INVITE after catalog: {}", e);
                }
            }
            return (None, None);
        };

        tracing::info!("[GB28181] Response for sub-session: channel={} call_id={} status={}", session.channel_id, call_id, first_line);

        if !is_invite_response {
            return (None, None);
        }

        let is_200 = first_line.contains(" 200 ");
        let is_error = first_line.starts_with("SIP/2.0 ") && (
            first_line.contains(" 401 ") || first_line.contains(" 403 ") ||
            first_line.contains(" 404 ") || first_line.contains(" 488 ") ||
            first_line.contains(" 500 ") || first_line.contains(" 503 ") ||
            first_line.contains(" 600 "));

        if is_error {
            tracing::warn!("[GB28181] INVITE failed: channel={} status={}", session.channel_id, first_line);
            self.sessions.write().await.retain(|_, s| s.call_id != call_id);
            self.deps.rtp_tunnel.unregister(&session.stream_key);
            return (None, None);
        }

        if !is_200 {
            return (None, None);
        }

        let udp_peer_str = self.udp_peer.map(|a| a.to_string()).unwrap_or_else(|| "NONE".to_string());
        let via_header = Self::get_header(headers, "Via").unwrap_or_else(|| "MISSING".to_string());
        let to_header = Self::get_header(headers, "To").unwrap_or_else(|| "MISSING".to_string());
        let contact_header = Self::get_header(headers, "Contact").unwrap_or_else(|| "MISSING".to_string());
        tracing::info!("[GB28181] 200 OK received: call_id={}, udp_peer={}\n  Via: {}\n  To: {}\n  Contact: {}", 
            call_id, udp_peer_str, via_header, to_header, contact_header);
        
        let via_received = Self::parse_via_received(headers);
        let via_rport = Self::parse_via_rport(headers);

        if let (Some(received_ip), Some(rport)) = (&via_received, via_rport) {
            tracing::debug!("[GB28181] NAT detected: device at {}:{}", received_ip, rport);
            self.sessions.write().await
                .entry(session.channel_id.clone())
                .and_modify(|s| {
                    s.device_nated_ip = Some(received_ip.clone());
                    s.device_nated_port = Some(rport);
                });
        }

        let to_tag = Self::get_header(headers, "To")
            .and_then(|v| Self::parse_tag_from_header(&v));

        if let Some(expected_tag) = &session.to_tag {
            if let Some(received_tag) = &to_tag {
                if received_tag != expected_tag {
                    tracing::warn!("[GB28181] To-tag mismatch! expected={} received={}, Call-ID={}", expected_tag, received_tag, session.call_id);
                }
            }
        }

        let to_tag_to_use = to_tag.clone().unwrap_or_default();
        if session.to_tag.is_none() && to_tag.is_some() {
            self.sessions.write().await
                .entry(session.channel_id.clone())
                .and_modify(|s| s.to_tag = to_tag.clone());
            tracing::debug!("[GB28181] Stored device to_tag={} for channel={}", to_tag.as_deref().unwrap_or(""), session.channel_id);
        }

        // 解析200 OK的Contact头 - 用于ACK的Request-URI和To头
        // Contact头格式: <sip:device_id@device_ip:device_port> 或 sip:device_id@device_ip:device_port
        // 根据GB28181标准，ACK应该发送到Contact头指定的地址
        // 返回 (device_id, host, port) - device_id用于ACK的To头
        let use_contact = headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Contact"))
            .and_then(|(k, v)| {
                let v = v.trim();
                tracing::debug!("[GB28181] Raw Contact header value: {:?}", v);
                let uri = v.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
                    .or_else(|| Some(v))
                    .map(|s| s.to_string())?;
                tracing::debug!("[GB28181] Contact URI after stripping <>: {:?}", uri);
                
                // 解析 sip:device_id@host:port 格式
                // 例如: sip:12010100001320000011@192.168.1.211:5060
                let without_sip = uri.strip_prefix("sip:").map(|s| s.to_string()).unwrap_or_else(|| uri.clone());
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
                    tracing::debug!("[GB28181] Contact parsed: device_id={}, host_port={}", device_id, host_port);
                    let hp_parts: Vec<&str> = host_port.rsplitn(2, ':').collect();
                    tracing::debug!("[GB28181] Contact host_port parts after rsplitn(2, ':'): {:?}", hp_parts);
                    if hp_parts.len() == 2 {
                        // hp_parts[0] = port, hp_parts[1] = host
                        let host = hp_parts[1];
                        let port: u16 = hp_parts[0].parse().ok()?;
                        tracing::debug!("[GB28181] Contact parsed final: device_id={}, host={}, port={}", device_id, host, port);
                        Some((device_id, host.to_string(), port))
                    } else {
                        // 没有端口的情况，使用默认5060
                        tracing::debug!("[GB28181] Contact parsed (no port): device_id={}, host={}", device_id, host_port);
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
                tracing::debug!("[GB28181] Using Contact for ACK: device_id={} {}:{}", device_id, host, port);
                let target = format!("sip:{}@{}:{}", device_id, host, port);
                // ACK的To头需要包含完整的 device_id@host:port
                let to_uri = format!("{}@{}:{}", device_id, host, port);
                (target, to_uri)
            }
            _ => {
                // Contact无效，使用server_gb_domain (无端口)
                tracing::debug!("[GB28181] Using server_gb_domain for ACK: {}", server_gb_domain);
                let target = format!("sip:{}@{}", session.parent_device_id, server_gb_domain);
                let to_uri = format!("{}@{}", session.parent_device_id, server_gb_domain);
                (target, to_uri)
            }
        };

        // Via头 - 格式: SIP/2.0/UDP IP:端口;rport;branch=xxx (rport在前)
        let via = if session.transport == TransportProtocol::Tcp {
            format!("SIP/2.0/TCP {}:{};rport;branch={}", session.server_ip, session.server_port, session.via_branch)
        } else {
            format!("SIP/2.0/UDP {}:{};rport;branch={}", session.server_ip, session.server_port, session.via_branch)
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
        
        tracing::info!("[GB28181] Sending ACK: {}\n--- ACK START ---\n{}\n--- ACK END ---", 
            ack_target, ack);
        if let Err(e) = self.send(ack.as_bytes()).await {
            tracing::error!("[GB28181] ACK send failed: {}", e);
        } else {
            tracing::info!("[GB28181] ACK sent successfully");
        }

        let channel_tag = &session.channel_id;
        let device_tag = self.device_tag_str();
        tracing::debug!("[GB28181] 200 OK received, starting stream: device={} channel={} stream={} media_server={}", device_tag, channel_tag, session.stream_key, session.media_server_name);
        let start_result = self.deps.stream_manager
            .start_gb28181_stream(&device_tag, channel_tag, &session.stream_key)
            .await;

        if start_result.is_ok() {
            tracing::debug!("[GB28181] Stream manager started successfully, updating session state");
            self.sessions.write().await
                .entry(session.channel_id.clone())
                .and_modify(|s| s.media_started = true);

            if let Some(app) = self.get_device_app(&session.channel_id).await {
                if let Some(stream) = self.deps.stream_manager.get_stream_by_stream_key(&app, &session.stream_key).await {
                    tracing::debug!("[GB28181] Stream found in manager, starting stream");
                    let mut s = stream.clone();
                    s.start();
                    let _ = self.deps.stream_manager.update_stream_state(&s).await;
                }
            }

            tracing::info!("[GB28181] Stream started: channel={} stream={}", session.channel_id, session.stream_key);
            self.set_registration_state(RegistrationState::Streaming);
        } else {
            tracing::error!("[GB28181] Failed to start stream: channel={} err={:?}", session.channel_id, start_result);
        }

        (Some(SignalEvent::StartPlay {
            device_id: self.device_id_i64(),
            device_tag: Some(session.channel_id.clone()),
            session_id: session.stream_key.clone(),
            channel_id: Some(session.channel_id.clone()),
            transport: TransportType::UDP,
            media_server_name: Some(session.media_server_name.clone()),
        }), None)
    }

    fn handle_options(&mut self, headers: &[(String, String)]) -> Option<String> {
        Some(self.build_sip_response_only_headers(200, "OK", &Self::parse_via_branch(headers), &Self::parse_cseq_line(headers)))
    }

    async fn handle_subscribe(&mut self, headers: &[(String, String)], body: &str) -> (Option<SignalEvent>, Option<String>) {
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
        self.subscriptions.write().await.insert(sub_key, subscription);

        tracing::info!("[GB28181] Device {} subscribed to event={} expires={}s", device_tag, event_type, expires);

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

        (Some(SignalEvent::DeviceSubscribe {
            device_id,
            device_tag: Some(device_tag),
            event_types: vec![event_type],
            expires,
        }), Some(resp))
    }

    pub async fn cleanup_expired_subscriptions(&self) -> usize {
        let mut count = 0;
        let now = chrono::Utc::now();
        let mut subs = self.subscriptions.write().await;
        let keys_to_remove: Vec<String> = subs.iter()
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
            tracing::error!("[GB28181] Buffer overflow, clearing. Device: {:?}", self.device_tag);
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
            } else {
                (None, None)
            };

            if let Some(resp) = &response {
                tracing::debug!("[GB28181] Sending response to {}: {}", self.udp_peer.map(|a| a.to_string()).unwrap_or_else(|| "unknown".to_string()), &resp[..resp.len().min(200)]);
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
            tracing::info!("[GB28181] Device {} disconnected, cleaning {} sessions", device_tag, self.sessions.read().await.len());
            
            let sessions: Vec<SubSession> = self.sessions.read().await.values().cloned().collect();
            for session in &sessions {
                if let Err(e) = self.send_bye_to_session(session).await {
                    tracing::warn!("[GB28181] Failed to send BYE to {}:{}", session.device_host, e);
                }
            }

            if let Some(ref name) = self.event_handler_name {
                registry().infra.event_bus.off(name);
            }
            let _ = self.deps.device_lookup.set_offline(&device_tag, Some("Connection closed")).await;

            if let Err(e) = self.deps.stream_manager.stop_streams_by_device(&device_tag).await {
                tracing::warn!("[GB28181] on_disconnected: failed to stop streams for device {}: {}", device_tag, e);
            }

            for session in sessions {
                self.deps.rtp_tunnel.unregister(&session.stream_key);
            }
            self.sessions.write().await.clear();
        }
        if let Some(ref dt) = self.device_tag {
            (self.unregister_fn)(dt.clone());
        }
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref write_arc) = self.write {
            let mut write = write_arc.write().await;
            write.write_all(data).await
                .map_err(|e| AppError::Internal(format!("TCP write: {}", e)))?;
            let _ = write.flush().await;
            return Ok(());
        }
        if let Some(ref addr) = self.udp_peer {
            if let Some(sender) = super::get_udp_sender() {
                let data_str = String::from_utf8_lossy(data);
                tracing::debug!("[GB28181] UDP send to {}: {} bytes\n{}", addr, data.len(), data_str);
                if let Err(e) = sender.send_to(data, *addr).await {
                    tracing::error!("[GB28181] UDP send failed to {}: {}", addr, e);
                    return Err(AppError::Internal(format!("UDP send: {}", e)));
                } else {
                    tracing::info!("[GB28181] UDP send SUCCESS to {} ({} bytes)", addr, data.len());
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

    fn protocol_type(&self) -> ProtocolType { ProtocolType::Gb28181 }
    fn name(&self) -> &'static str { "GB28181" }
    fn keepalive(&self) -> bool { true }
    fn idle_timeout(&self) -> Option<u64> { Some(300) }

    fn set_tcp_write(&mut self, write: OwnedWriteHalf) {
        self.write = Some(Arc::new(tokio::sync::RwLock::new(write)));
    }

    async fn set_udp_peer(&mut self, addr: SocketAddr) -> Result<()> {
        self.udp_peer = Some(addr);
        self.transport = TransportProtocol::Udp;
        Ok(())
    }

    async fn start(&mut self, device_tag: &str) -> Result<()> {
        let device = self.deps.device_lookup.find_by_tag(device_tag).await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", device_tag)))?;

        if let Some(ext) = &device.extended {
            if let Some(transport) = ext.get("gb_transport").and_then(|v| v.as_str()) {
                let transport_str = match transport.to_uppercase().as_str() {
                    "TCP" => {
                        self.transport = TransportProtocol::Tcp;
                        "TCP"
                    },
                    _ => {
                        self.transport = TransportProtocol::Udp;
                        "UDP"
                    },
                };
                tracing::info!("[GB28181] Using transport {} for device {}", transport_str, device_tag);
            }
        }

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let ssrc = format!("{:08x}", rand::random::<u32>());
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        let server_port = match self.transport {
            TransportProtocol::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportProtocol::Udp => self.deps.config.signaling_server.udp_signaling_port,
        };
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;

        let parent_device_id = device.parent_device_tag.clone()
            .unwrap_or_else(|| device_tag.to_string());
        let channel_id = device_tag.to_string();
        let stream_key = format!("{}_{}", parent_device_id, channel_id);
        let (rtp_port, server_ip, media_server_name) = self.alloc_rtp_port(&stream_key, device.media_server_tag.as_deref()).await?;
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
            self.remote_addr.map(|a| a.ip().to_string()).unwrap_or_else(|| "127.0.0.1".to_string())
        };
        
        // Via: SIP/2.0/UDP 192.168.1.211:5060;branch=z9hG4bKa396e08;rport
        let via = if self.transport == TransportProtocol::Tcp {
            format!("SIP/2.0/TCP {local_ip}:{server_port};branch={branch};rport")
        } else {
            format!("SIP/2.0/UDP {local_ip}:{server_port};branch={branch};rport")
        };

        // SDP c=: 媒体服务器地址 (从 ZLMediaKit URL 解析)
        // 设备发送 RTP 到这个地址
        // Example: c=IN IP4 192.168.1.100
        let sdp = self.build_sdp(&server_gb_id, &server_ip, rtp_port, &ssrc, device.stream_config.as_ref(), None, None);
        let sdp_len = sdp.len();
        
        // to_uri: 格式 "设备ID@设备IP:设备端口" (设备自己的地址)
        // Example: "12010100001320000011@192.168.1.211:5060"
        let to_uri = format!("{}@{}:{}", parent_device_id, device_host, device_port);
        
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
            seq: sn,
            channel_id: channel_id.clone(),
            parent_device_id: parent_device_id.clone(),
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
            media_started: false,
            device_nated_ip: None,
            device_nated_port: None,
        };

        let old_session = self.sessions.write().await.insert(device_tag.to_string(), session.clone());
        if let Some(old) = old_session {
            tracing::info!("[GB28181] start: cleaning up old session for channel={}", device_tag);
            if old.media_started {
                if let Some(app) = self.get_device_app(&old.channel_id).await {
                    let _ = self.deps.stream_manager.stop_stream(&app, &old.stream_key).await;
                }
                self.deps.rtp_tunnel.unregister(&old.stream_key);
            }
        }

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

    async fn start_playback(&mut self, device_tag: &str, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> Result<()> {
        let device = self.deps.device_lookup.find_by_tag(device_tag).await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", device_tag)))?;

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let ssrc = format!("{:08x}", rand::random::<u32>());
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        let server_port = match self.transport {
            TransportProtocol::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportProtocol::Udp => self.deps.config.signaling_server.udp_signaling_port,
        };
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;

        let parent_device_id = device.parent_device_tag.clone()
            .unwrap_or_else(|| device_tag.to_string());
        let channel_id = device_tag.to_string();
        let stream_key = format!("{}_{}", parent_device_id, channel_id);
        let (rtp_port, server_ip, media_server_name) = self.alloc_rtp_port(&stream_key, device.media_server_tag.as_deref()).await?;
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
            self.remote_addr.map(|a| a.ip().to_string()).unwrap_or_else(|| "127.0.0.1".to_string())
        };
        
        let via = if self.transport == TransportProtocol::Tcp {
            format!("SIP/2.0/TCP {local_ip}:{server_port};branch={branch};rport")
        } else {
            format!("SIP/2.0/UDP {local_ip}:{server_port};branch={branch};rport")
        };

        // SDP c=: 媒体服务器地址 (从 ZLMediaKit URL 解析)
        let sdp = self.build_sdp(&server_gb_id, &server_ip, rtp_port, &ssrc, device.stream_config.as_ref(), Some(start_time), Some(end_time));
        let sdp_len = sdp.len();
        let to_uri = format!("{}@{}:{}", parent_device_id, device_host, device_port);
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
            seq: sn,
            channel_id: channel_id.clone(),
            parent_device_id: parent_device_id.clone(),
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
            media_started: false,
            device_nated_ip: None,
            device_nated_port: None,
        };

        let old_session = self.sessions.write().await.insert(device_tag.to_string(), session.clone());
        if let Some(old) = old_session {
            tracing::info!("[GB28181] start_playback: cleaning up old session for channel={}", device_tag);
            if old.media_started {
                if let Some(app) = self.get_device_app(&old.channel_id).await {
                    let _ = self.deps.stream_manager.stop_stream(&app, &old.stream_key).await;
                }
                self.deps.rtp_tunnel.unregister(&old.stream_key);
            }
        }

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

        tracing::info!("[GB28181] Sending Playback INVITE to device={}, stream={}, rtp={}, start={}, end={}", device_tag, stream_key, rtp_port, start_ntp, end_ntp);
        self.send(invite.as_bytes()).await?;

        Ok(())
    }

    async fn ptz_control(&mut self, channel_id: &str, command: &crate::protocol::event::PtzCommand, speed: Option<u8>) -> Result<()> {
        let device = self.deps.device_lookup.find_by_tag(channel_id).await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", channel_id)))?;

        let parent_device_id = device.parent_device_tag.clone().unwrap_or_else(|| channel_id.to_string());
        let spd = speed.unwrap_or(50);

        match command {
            PtzCommand::ContinuousMove { pan, tilt, zoom } => {
                if pan.abs() < 0.01 && tilt.abs() < 0.01 && zoom.abs() < 0.01 {
                    let ptz_body = Self::build_ptz_xml(channel_id, "stop", 0);
                    let msg = self.build_ptz_message(channel_id, &parent_device_id, &ptz_body);
                    tracing::debug!("[GB28181] PTZ: channel={} compound=stop", channel_id);
                    self.send(msg.as_bytes()).await?;
                } else {
                    let ptz_body = Self::build_ptz_xml_compound(channel_id, *pan, *tilt, *zoom, spd);
                    let msg = self.build_ptz_message(channel_id, &parent_device_id, &ptz_body);
                    tracing::debug!("[GB28181] PTZ: channel={} compound=pan:{:.2} tilt:{:.2} zoom:{:.2}", channel_id, pan, tilt, zoom);
                    self.send(msg.as_bytes()).await?;
                }
            }
            PtzCommand::Stop => {
                let ptz_body = Self::build_ptz_xml(channel_id, "stop", 0);
                let msg = self.build_ptz_message(channel_id, &parent_device_id, &ptz_body);
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
                    _ => return Ok(()),
                };

                let ptz_body = Self::build_ptz_xml(channel_id, direction, spd_val);
                let msg = self.build_ptz_message(channel_id, &parent_device_id, &ptz_body);
                tracing::debug!("[GB28181] PTZ: channel={} dir={} spd={}", channel_id, direction, spd_val);
                self.send(msg.as_bytes()).await?;
            }
        }
        Ok(())
    }

    async fn send_notify(&mut self, device_tag: &str, event_type: &str, content: &str) -> Result<()> {
        let device = self.deps.device_lookup.find_by_tag(device_tag).await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", device_tag)))?;

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let to_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        let server_port = match self.transport {
            TransportProtocol::Tcp => self.deps.config.signaling_server.tcp_signaling_port,
            TransportProtocol::Udp => self.deps.config.signaling_server.udp_signaling_port,
        };
        let server_ip = self.remote_addr.map(|a| a.ip().to_string()).unwrap_or_else(|| "0.0.0.0".to_string());
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let server_gb_domain = &self.deps.config.signaling_server.server_gb_domain;

        let parent_device_id = device.parent_device_tag.clone().unwrap_or_else(|| device_tag.to_string());
        let device_host = device.host.clone();
        let device_port = device.port;

        let via = if self.transport == TransportProtocol::Tcp {
            format!("SIP/2.0/TCP {server_ip}:{server_port};branch={branch};rport")
        } else {
            format!("SIP/2.0/UDP {server_ip}:{server_port};branch={branch};rport")
        };

        let from_uri = format!("{}@{}:{}", server_gb_id, server_ip, server_port);
        let to_uri = format!("{}@{}:{}", parent_device_id, device_host, device_port);

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

        tracing::info!("[GB28181] Sending NOTIFY to device={} event={}", device_tag, event_type);
        self.send(notify.as_bytes()).await?;
        Ok(())
    }
}

impl Gb28181Adapter {
    pub async fn notify_alarm(&mut self, device_tag: &str, alarm_type: &str, description: &str) -> Result<()> {
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
            sn,
            device_tag,
            status
        );
        self.send_notify(device_tag, "presence", &content).await
    }

    pub fn build_audio_invite_sdp(&self, device_ip: &str, audio_port: u16) -> String {
        let (platform_id, platform_ip, platform_port) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
            .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));
        
        format!(
            r#"v=0
o={} 0 0 IN IP4 {}
s=Talk
c=IN IP4 {}
t=0 0
m=audio {} RTP/AVP 8
a=sendonly
a=rtpmap:8 PCMA/8000
"#,
            platform_id, platform_ip, platform_ip, audio_port
        )
    }

    pub fn build_audio_invite_sdp_tcp(&self, device_ip: &str, tcp_port: u16) -> String {
        let (platform_id, platform_ip, platform_port) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
            .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));
        
        format!(
            r#"v=0
o={} 0 0 IN IP4 {}
s=Talk
c=IN IP4 {}
t=0 0
m=audio {} TCP/RTP/AVP 8
a=setup:passive
a=connection:new
a=rtpmap:8 PCMA/8000
"#,
            platform_id, platform_ip, platform_ip, tcp_port
        )
    }

    pub async fn start_audio_talk(
        &self,
        device_tag: &str,
        device_ip: &str,
        device_audio_port: u16,
        platform_audio_port: u16,
        transport: TransportType,
    ) -> Result<()> {
        let call_id = format!("audio-{}", chrono::Utc::now().timestamp_millis());
        let from_tag = format!("ft-{}", chrono::Utc::now().timestamp_millis() % 1000000);
        
        let sdp = match transport {
            TransportType::TCP => self.build_audio_invite_sdp_tcp(device_ip, platform_audio_port),
            _ => self.build_audio_invite_sdp(device_ip, device_audio_port),
        };
        
        let tcp_stream = if transport == TransportType::TCP {
            let addr = format!("{}:{}", device_ip, device_audio_port);
            match tokio::net::TcpStream::connect(&addr).await {
                Ok(stream) => {
                    tracing::info!("[GB28181] TCP audio connection established to {}", addr);
                    Some(Arc::new(tokio::sync::RwLock::new(stream)))
                }
                Err(e) => {
                    tracing::warn!("[GB28181] Failed to establish TCP audio connection: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        let session = AudioTalkSession {
            device_tag: device_tag.to_string(),
            device_ip: device_ip.to_string(),
            device_audio_port,
            platform_audio_port,
            call_id: call_id.clone(),
            from_tag: from_tag.clone(),
            to_tag: None,
            start_time: chrono::Utc::now(),
            audio_socket: None,
            tcp_stream,
            rtp_ssrc: rand_ssrc(),
            rtp_sequence: rand_u16(),
            rtp_timestamp: rand_u32(),
        };
        
        let mut sessions = self.audio_talk_sessions.write().await;
        sessions.insert(device_tag.to_string(), session);
        
        tracing::info!("[GB28181] Starting audio talk: device={} ip={} port={} transport={:?}", device_tag, device_ip, device_audio_port, transport);
        
        let (platform_id, platform_ip, platform_port) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
            .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));
        
        let via_transport = match transport {
            TransportType::TCP => "TCP",
            _ => "UDP",
        };
        
        let sip_msg = format!(
            "INVITE sip:{}@{}:{} SIP/2.0\r\n\
             Via: SIP/2.0/{} {}:{};branch=z9hG4bK{};rport\r\n\
             From: <sip:{}@{}:{}>;tag={}\r\n\
             To: <sip:{}@{}:{}>\r\n\
             Call-ID: {}\r\n\
             CSeq: 1 INVITE\r\n\
             Contact: <sip:{}@{}:{}>\r\n\
             Content-Type: application/sdp\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: {}\r\n\r\n\
             {}",
            device_tag, device_ip, device_audio_port,
            via_transport, platform_ip, platform_port,
            chrono::Utc::now().timestamp_millis(),
            platform_id, platform_ip, platform_port, &from_tag,
            device_tag, device_ip, device_audio_port,
            &call_id,
            platform_id, platform_ip, platform_port,
            sdp.len(),
            sdp
        );
        
        if let Some(sender) = crate::protocol::gb28181::get_udp_sender() {
            sender.send(sip_msg.as_bytes()).await.map_err(|e| anyhow::anyhow!("{}", e))?;
        }
        
        Ok(())
    }

    pub async fn stop_audio_talk(&self, device_tag: &str) -> Result<()> {
        let mut sessions = self.audio_talk_sessions.write().await;
        if let Some(session) = sessions.remove(device_tag) {
            tracing::info!("[GB28181] Stopping audio talk: device={}", device_tag);
            
            let (platform_id, platform_ip, platform_port) = crate::protocol::gb28181::get_gb28181_platform_config()
                .get()
                .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
                .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));
            
            let sip_msg = format!(
                "BYE sip:{}@{}:{} SIP/2.0\r\n\
                 Via: SIP/2.0/UDP {}:{};branch=z9hG4bK{};rport\r\n\
                 From: <sip:{}@{}:{}>;tag={}\r\n\
                 To: <sip:{}@{}:{}>;tag={}\r\n\
                 Call-ID: {}\r\n\
                 CSeq: 2 BYE\r\n\
                 User-Agent: RustCam-Media/2.0\r\n\
                 Content-Length: 0\r\n\r\n",
                device_tag, session.device_ip, session.device_audio_port,
                platform_ip, platform_port,
                chrono::Utc::now().timestamp_millis(),
                platform_id, platform_ip, platform_port, &session.from_tag,
                device_tag, session.device_ip, session.device_audio_port,
                session.to_tag.as_deref().unwrap_or(""),
                &session.call_id
            );
            
            if let Some(sender) = crate::protocol::gb28181::get_udp_sender() {
                let addr = format!("{}:{}", session.device_ip, session.device_audio_port);
                let _ = sender.send_to(sip_msg.as_bytes(), &addr).await;
            }
        }
        Ok(())
    }

    pub async fn send_audio_to_device(&self, device_tag: &str, pcm_data: &[i16]) -> Result<()> {
        let session_opt = {
            let mut sessions = self.audio_talk_sessions.write().await;
            if let Some(session) = sessions.get_mut(device_tag) {
                let encoded = crate::protocol::gb28181::audio::linear_to_alaw(pcm_data);
                let timestamp = session.rtp_timestamp;
                let ssrc = session.rtp_ssrc;
                let sequence = session.rtp_sequence;
                
                session.rtp_timestamp = session.rtp_timestamp.wrapping_add(160);
                session.rtp_sequence = session.rtp_sequence.wrapping_add(1);
                
                Some((encoded, timestamp, ssrc, sequence, session.device_ip.clone(), session.device_audio_port))
            } else {
                None
            }
        };
        
        if let Some((encoded, timestamp, ssrc, sequence, device_ip, device_port)) = session_opt {
            let rtp_packet = crate::protocol::gb28181::rtp::build_audio_rtp_packet_with_seq(
                &encoded,
                8,
                timestamp,
                ssrc,
                sequence,
            );
            
            let addr = format!("{}:{}", device_ip, device_port);
            if let Some(sender) = crate::protocol::gb28181::get_udp_sender() {
                sender.send_to(&rtp_packet, &addr).await.map_err(|e| anyhow::anyhow!("{}", e))?;
            }
        }
        
        Ok(())
    }
    
    pub async fn send_audio_to_device_tcp(&self, device_tag: &str, pcm_data: &[i16]) -> Result<()> {
        let rtp_packet_with_len = {
            let mut sessions = self.audio_talk_sessions.write().await;
            if let Some(session) = sessions.get_mut(device_tag) {
                if session.tcp_stream.is_none() {
                    tracing::warn!("[GB28181] No TCP stream for device {}, attempting reconnect", device_tag);
                    let addr = format!("{}:{}", session.device_ip, session.device_audio_port);
                    if let Ok(stream) = tokio::net::TcpStream::connect(&addr).await {
                        session.tcp_stream = Some(Arc::new(tokio::sync::RwLock::new(stream)));
                        tracing::info!("[GB28181] TCP audio reconnected to {}", addr);
                    } else {
                        return Ok(());
                    }
                }
                
                let encoded = crate::protocol::gb28181::audio::linear_to_alaw(pcm_data);
                let timestamp = session.rtp_timestamp;
                let ssrc = session.rtp_ssrc;
                let sequence = session.rtp_sequence;
                
                session.rtp_timestamp = session.rtp_timestamp.wrapping_add(160);
                session.rtp_sequence = session.rtp_sequence.wrapping_add(1);
                
                let rtp_packet = crate::protocol::gb28181::rtp::build_audio_rtp_packet_with_seq(
                    &encoded, 8, timestamp, ssrc, sequence
                );
                
                let len = rtp_packet.len() as u16;
                let mut frame = Vec::with_capacity(2 + rtp_packet.len());
                frame.extend_from_slice(&len.to_be_bytes());
                frame.extend_from_slice(&rtp_packet);
                frame
            } else {
                return Ok(());
            }
        };
        
        if let Some(ref stream_arc) = self.audio_talk_sessions.read().await.get(device_tag).and_then(|s| s.tcp_stream.clone()) {
            let mut stream = stream_arc.write().await;
            if let Err(e) = stream.write_all(&rtp_packet_with_len).await {
                tracing::error!("[GB28181] TCP write failed: {}", e);
                if let Some(session) = self.audio_talk_sessions.write().await.get_mut(device_tag) {
                    session.tcp_stream = None;
                }
                return Err(anyhow::anyhow!("TCP write failed: {}", e).into());
            }
        }
        
        Ok(())
    }
    
    async fn send_bye_to_session(&self, session: &SubSession) -> Result<()> {
        let (platform_id, platform_ip, platform_port) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, _, ip, port)| (id.as_str(), ip.as_str(), *port))
            .unwrap_or(("34020000000000000000", "0.0.0.0", 5060));

        let device_tag = self.device_tag_str();
        let via_transport = match session.transport {
            TransportProtocol::Tcp => "TCP",
            TransportProtocol::Udp => "UDP",
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
            device_tag, session.device_host, session.device_port,
            via_transport, platform_ip, platform_port,
            chrono::Utc::now().timestamp_millis(),
            platform_id, platform_ip, platform_port, &session.from_tag,
            device_tag, session.device_host, session.device_port,
            session.to_tag.as_deref().unwrap_or(""),
            &session.call_id
        );

        match session.transport {
            TransportProtocol::Tcp => {
                if let Some(ref write_arc) = self.write {
                    let mut write = write_arc.write().await;
                    write.write_all(sip_msg.as_bytes()).await.map_err(|e| AppError::Internal(e.to_string()))?;
                    let _ = write.flush().await;
                }
            }
            TransportProtocol::Udp => {
                if let Some(sender) = crate::protocol::gb28181::get_udp_sender() {
                    let addr = format!("{}:{}", session.device_host, session.device_port);
                    sender.send_to(sip_msg.as_bytes(), &addr).await.map_err(|e| AppError::Internal(e.to_string()))?;
                }
            }
        }
        Ok(())
    }
}

fn rand_ssrc() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    (now.as_nanos() as u32).wrapping_add(12345)
}

fn rand_u16() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    (now.as_nanos() as u16).wrapping_add(1)
}

fn rand_u32() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    (now.as_nanos() as u32).wrapping_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_ptz_cmd_compound_up_right() {
        let result = Gb28181Adapter::encode_ptz_cmd_compound(0.5, 0.5, 0.0, 50);
        assert!(result.contains("28"), "Up(0x08)+Right(0x20)=0x28, got: {}", result);
        assert!(result.contains("00 7F"), "No zoom (0), got: {}", result);
    }

    #[test]
    fn test_encode_ptz_cmd_compound_down_left() {
        let result = Gb28181Adapter::encode_ptz_cmd_compound(-0.5, -0.5, 0.0, 50);
        assert!(result.contains("12"), "Down(0x10)+Left(0x02)=0x12, got: {}", result);
    }

    #[test]
    fn test_encode_ptz_cmd_compound_with_zoom() {
        let result = Gb28181Adapter::encode_ptz_cmd_compound(0.5, 0.0, 0.3, 50);
        assert!(result.contains("20"), "Right(0x20), got: {}", result);
        assert!(result.contains("80"), "ZoomIn(0x80) in cmd2, got: {}", result);
    }

    #[test]
    fn test_encode_ptz_cmd_compound_all_stop() {
        let result = Gb28181Adapter::encode_ptz_cmd_compound(0.0, 0.0, 0.0, 50);
        assert!(result.contains("00 00 00 00 00"), "All stop should be 00s, got: {}", result);
    }

    #[test]
    fn test_encode_ptz_cmd_single_up() {
        let result = Gb28181Adapter::encode_ptz_cmd("up", 50);
        assert!(result.contains("08"), "Up should set cmd1=0x08, got: {}", result);
    }

    #[test]
    fn test_encode_ptz_cmd_single_right() {
        let result = Gb28181Adapter::encode_ptz_cmd("right", 50);
        assert!(result.contains("40"), "Right should set cmd1=0x40, got: {}", result);
    }

    #[test]
    fn test_encode_ptz_cmd_zoom_in() {
        let result = Gb28181Adapter::encode_ptz_cmd("zoom_in", 50);
        assert!(result.contains("80"), "ZoomIn should set cmd2=0x80, got: {}", result);
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
        let result = Gb28181Adapter::build_ptz_xml_compound("34020000001110000001", 0.5, 0.5, 0.0, 50);
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
        
        assert!(query_str.contains("MESSAGE sip:"), "Should contain MESSAGE sip:");
        assert!(query_str.contains("12010100001320000011"), "Should contain device ID");
        assert!(query_str.contains("CmdType>Catalog"), "Should contain Catalog command");
        assert!(query_str.contains("Via:"), "Should contain Via header");
        assert!(query_str.contains("CSeq:"), "Should contain CSeq header");
        assert!(query_str.contains("From:"), "Should contain From header");
        assert!(query_str.contains("To:"), "Should contain To header");
        assert!(query_str.contains("Call-ID:"), "Should contain Call-ID header");
    }

    #[test]
    fn test_parse_contact_header_with_port() {
        let headers = vec![
            ("Contact".to_string(), "<sip:12010100001320000011@192.168.1.211:5060>".to_string()),
        ];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_without_port() {
        let headers = vec![
            ("Contact".to_string(), "<sip:12010100001320000011@192.168.1.211>".to_string()),
        ];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_no_brackets() {
        let headers = vec![
            ("Contact".to_string(), "sip:12010100001320000011@192.168.1.211:5060".to_string()),
        ];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_case_insensitive() {
        let headers = vec![
            ("contact".to_string(), "<sip:12010100001320000011@192.168.1.211:5060>".to_string()),
        ];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_no_contact() {
        let headers = vec![
            ("From".to_string(), "<sip:12010100001320000011@192.168.1.211:5060>".to_string()),
        ];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_contact_header_ipv6() {
        let headers = vec![
            ("Contact".to_string(), "<sip:12010100001320000011@[::1]:5060>".to_string()),
        ];
        let result = Gb28181Adapter::parse_contact_host_port(&headers);
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "[::1]");
        assert_eq!(port, 5060);
    }

    #[test]
    fn test_parse_contact_header_with_params() {
        let headers = vec![
            ("Contact".to_string(), "<sip:12010100001320000011@192.168.1.211:5060>;expires=3600".to_string()),
        ];
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
        assert_eq!(result, "sip:12010100001320000011@192.168.1.211:5060".to_string());
    }

    #[test]
    fn test_parse_sip_uri_with_domain_no_sip_prefix() {
        let uri = "12010100001320000011@192.168.1.211:5060";
        let result = Gb28181Adapter::parse_sip_uri_with_domain(uri);
        assert_eq!(result, "sip:12010100001320000011@192.168.1.211:5060".to_string());
    }

    #[test]
    fn test_parse_via_branch() {
        let headers = vec![
            ("Via".to_string(), "SIP/2.0/UDP 192.168.1.100:5060;branch=z9hG4bKabc123;rport".to_string()),
        ];
        let result = Gb28181Adapter::parse_via_branch(&headers);
        assert_eq!(result, "z9hG4bKabc123".to_string());
    }

    #[test]
    fn test_parse_via_branch_no_branch() {
        let headers = vec![
            ("Via".to_string(), "SIP/2.0/UDP 192.168.1.100:5060;rport".to_string()),
        ];
        let result = Gb28181Adapter::parse_via_branch(&headers);
        assert_eq!(result, "z9hG4bKdefault".to_string());
    }

    #[test]
    fn test_parse_via_received() {
        let headers = vec![
            ("Via".to_string(), "SIP/2.0/UDP 192.168.1.100:5060;received=10.0.0.1;branch=z9hG4bKabc123".to_string()),
        ];
        let result = Gb28181Adapter::parse_via_received(&headers);
        assert_eq!(result, Some("10.0.0.1".to_string()));
    }

    #[test]
    fn test_parse_via_rport() {
        let headers = vec![
            ("Via".to_string(), "SIP/2.0/UDP 192.168.1.100:5060;rport=5070;branch=z9hG4bKabc123".to_string()),
        ];
        let result = Gb28181Adapter::parse_via_rport(&headers);
        assert_eq!(result, Some(5070));
    }

    #[test]
    fn test_parse_via_rport_only() {
        let headers = vec![
            ("Via".to_string(), "SIP/2.0/UDP 192.168.1.100:5060;rport;branch=z9hG4bKabc123".to_string()),
        ];
        let result = Gb28181Adapter::parse_via_rport(&headers);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_parse_cseq_line() {
        let headers = vec![
            ("CSeq".to_string(), "1 REGISTER".to_string()),
        ];
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
        adapter.transport = TransportProtocol::Udp;
        
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
        adapter.transport = TransportProtocol::Udp;
        
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
        assert!(Gb28181Adapter::is_valid_gb28181_device_id("12010100001320000011"));
        assert!(Gb28181Adapter::is_valid_gb28181_device_id("1234567890"));
        assert!(!Gb28181Adapter::is_valid_gb28181_device_id("123456"));
        assert!(!Gb28181Adapter::is_valid_gb28181_device_id(""));
        assert!(!Gb28181Adapter::is_valid_gb28181_device_id("123456789012345678901"));
        assert!(!Gb28181Adapter::is_valid_gb28181_device_id("12010100001320000abc"));
    }

    #[test]
    fn test_parse_subject_device_id() {
        assert_eq!(
            Gb28181Adapter::parse_subject_device_id("12010100001320000011:31011500001000000001:0"),
            Some("12010100001320000011".to_string())
        );
        assert_eq!(
            Gb28181Adapter::parse_subject_device_id("invalid"),
            None
        );
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
        assert_eq!(Gb28181Adapter::truncate_20("12345678901234567890"), "12345678901234567890");
        assert_eq!(Gb28181Adapter::truncate_20("123456789012345678901"), "12345678901234567890");
        assert_eq!(Gb28181Adapter::truncate_20("short"), "short");
    }

    #[test]
    fn test_truncate_32() {
        assert_eq!(Gb28181Adapter::truncate_32("12345678901234567890123456789012"), "12345678901234567890123456789012");
        assert_eq!(Gb28181Adapter::truncate_32("123456789012345678901234567890123"), "12345678901234567890123456789012");
        assert_eq!(Gb28181Adapter::truncate_32("short"), "short");
    }

    #[test]
    fn test_detect_version() {
        assert_eq!(Gb28181Adapter::detect_version("GB28181"), Gb28181Version::Gb2016);
        assert_eq!(Gb28181Adapter::detect_version("something GB28181 something"), Gb28181Version::Gb2016);
        assert_eq!(Gb28181Adapter::detect_version("something 28181 something"), Gb28181Version::Gb2016);
        assert_eq!(Gb28181Adapter::detect_version("other"), Gb28181Version::Gb2016);
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
        
        assert_eq!(Gb28181Adapter::parse_xml_field(xml, "DeviceID"), Some("34020000001110000001".to_string()));
        assert_eq!(Gb28181Adapter::parse_xml_field(xml, "Name"), Some("Camera1".to_string()));
        assert_eq!(Gb28181Adapter::parse_xml_field(xml, "Status"), Some("ON".to_string()));
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
<Parental>0</Parental><Address>192.168.1.100</Address><IPAddress>192.168.1.100</IPAddress><Port>8000</Port>
<Info><DeviceType>IPC</DeviceType><Protocol>GB28181</Protocol></Info></Item></DeviceList></Response>"#;
        
        let channels = adapter.parse_catalog_response(body);
        
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].device_id, "34020000001110000001");
        assert_eq!(channels[0].name, "Camera1");
        assert_eq!(channels[0].manufacturer, Some("Hikvision".to_string()));
        assert_eq!(channels[0].model, Some("DS-2CD3T46FWD-I".to_string()));
        assert_eq!(channels[0].status, "ON");
        assert!(!channels[0].parental);
        assert_eq!(channels[0].parent_id, Some("12010100001320000011".to_string()));
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
    fn test_build_ptz_xml_stop() {
        let result = Gb28181Adapter::build_ptz_xml("34020000001110000001", "stop", 50);
        assert!(result.contains("<CmdType>PTZCmd</CmdType>"));
        assert!(result.contains("PTZCmd>80 01 04 00 00"));
    }

    #[test]
    fn test_parse_contact_with_device_id() {
        let contact = "<sip:12010100001320000011@192.168.1.211:5060>";
        
        let uri = contact.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
            .map(|s| s.to_string()).unwrap_or_else(|| contact.to_string());
        assert_eq!(uri, "sip:12010100001320000011@192.168.1.211:5060");
        
        let without_sip = uri.strip_prefix("sip:").map(|s| s.to_string()).unwrap_or_else(|| uri.clone());
        assert_eq!(without_sip, "12010100001320000011@192.168.1.211:5060");
        
        // rsplitn returns elements from RIGHT to LEFT
        // "device_id@host:port" -> parts[0]="host:port", parts[1]="device_id"
        let parts: Vec<&str> = without_sip.rsplitn(2, '@').collect();
        assert_eq!(parts.len(), 2, "Should have 2 parts after @ split, got {:?}", parts);
        assert_eq!(parts[0], "192.168.1.211:5060", "parts[0] should be host:port");
        assert_eq!(parts[1], "12010100001320000011", "parts[1] should be device_id");
        
        let host_port = parts[0];
        let device_id = parts[1];
        
        let hp_parts: Vec<&str> = host_port.rsplitn(2, ':').collect();
        assert_eq!(hp_parts.len(), 2, "Should have 2 parts after : split, got {:?}", hp_parts);
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
}
