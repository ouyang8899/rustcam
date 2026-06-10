use std::collections::HashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use async_trait::async_trait;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::io::AsyncWriteExt;

use crate::context::registry;
use crate::protocol::event::{EventHandler, SignalEvent, ProtocolType, TransportType, CatalogChannel, PtzCommand};

use crate::adapter::media_server::RtpTransport;
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::traits::ProtocolDeps;
use crate::protocol::gb28181::auth::{
    parse_sip_authorization, verify_sip_digest,
    generate_nonce, verify_nonce, consume_nonce, VerifyResult,
};
use crate::error::{Result, AppError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gb28181Version { Gb2016, Gb2011 }

impl Default for Gb28181Version {
    fn default() -> Self { Gb28181Version::Gb2016 }
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
    pub from_uri: String,
    pub to_uri: String,
    pub from_tag: String,
    pub via_branch: String,
    pub server_ip: String,
    pub server_port: u16,
    pub transport: TransportProtocol,
    pub device_host: String,
    pub device_port: u16,
    pub rtp_port: u16,
    pub media_started: bool,
}

#[derive(Clone)]
pub struct Gb28181Adapter {
    pub version: Gb28181Version,
    pub device_tag: Option<String>,
    pub device_id: Option<i64>,
    pub device_name: Option<String>,
    pub seq: Arc<AtomicU32>,
    pub remote_addr: Option<SocketAddr>,
    pub recv_buffer: Arc<Vec<u8>>,
    pub write: Option<GbWriteHalf>,
    pub catalog_retry_count: u32,
    pub max_buffer_size: usize,
    pub transport: TransportProtocol,
    pub call_id: Option<String>,
    pub from_tag: Option<String>,
    pub to_tag: Option<String>,
    pub udp_peer: Option<SocketAddr>,
    pub registration_expires: Option<u64>,
    pub deps: Arc<ProtocolDeps>,
    pub sessions: Arc<tokio::sync::RwLock<HashMap<String, SubSession>>>,
    pub pending_catalog_query: bool,
    pub event_handler_name: Option<String>,
    pub register_fn: Arc<dyn Fn(String, crate::protocol::adapter_manager::AdapterEntry) + Send + Sync>,
    pub unregister_fn: Arc<dyn Fn(String) + Send + Sync>,
}

impl Gb28181Adapter {
    pub fn new(
        deps: ProtocolDeps,
        register_fn: Arc<dyn Fn(String, crate::protocol::adapter_manager::AdapterEntry) + Send + Sync>,
        unregister_fn: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        Self {
            version: Gb28181Version::default(),
            device_tag: None,
            device_id: None,
            device_name: None,
            seq: Arc::new(AtomicU32::new(0)),
            remote_addr: None,
            recv_buffer: Arc::new(Vec::new()),
            write: None,
            catalog_retry_count: 0,
            max_buffer_size: 1024 * 1024,
            transport: TransportProtocol::Tcp,
            call_id: None,
            from_tag: None,
            to_tag: None,
            udp_peer: None,
            registration_expires: None,
            deps: Arc::new(deps),
            sessions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            pending_catalog_query: false,
            event_handler_name: None,
            register_fn,
            unregister_fn,
        }
    }

    pub fn registration_expires(&self) -> u64 {
        self.registration_expires.unwrap_or(3600)
    }

    fn truncate_20(s: &str) -> String {
        if s.len() <= 20 { s.to_string() } else { s[..20].to_string() }
    }

    fn truncate_32(s: &str) -> String {
        if s.len() <= 32 { s.to_string() } else { s[..32].to_string() }
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
        uri.strip_prefix("sip:").unwrap_or(uri).split('@').next().unwrap_or(uri).to_string()
    }

    fn parse_tag_from_header(header_value: &str) -> Option<String> {
        if let Some(tag_start) = header_value.find("tag=") {
            let after_tag = &header_value[tag_start + 4..];
            let end = after_tag.find(&[',', ';', '\r', '\n'][..]).unwrap_or(after_tag.len());
            if end > 0 { return Some(after_tag[..end].to_string()); }
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
        let (platform_id, platform_ip, platform_port) = crate::protocol::gb28181::get_gb28181_platform_config()
            .get()
            .map(|(id, ip, port)| (id.as_str(), ip.as_str(), *port))
            .unwrap_or(("00000000000000000000", "0.0.0.0", 5060));

        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Query><CmdType>Catalog</CmdType><SN>{}</SN><DeviceID>{}</DeviceID>
<Info><DeviceID>{}</DeviceID></Info></Query>"#,
            sn, platform_id, device_tag
        );

        let content_len = body.len();
        let ts = chrono::Utc::now().timestamp_millis() % 1000000;
        let call_id = format!("cat-{}", ts);

        let via = format!("SIP/2.0/UDP {}:{};branch=z9hG4bKcat{}", platform_ip, platform_port, ts);
        let from = format!("<sip:{}@{}:{}>;tag=cat{}", platform_id, platform_ip, platform_port, ts);
        let to = format!("<sip:{}>", device_tag);

        format!(
            "MESSAGE {} SIP/2.0\r\n\
             Via: {}\r\n\
             From: {}\r\n\
             To: {}\r\n\
             Call-ID: {}\r\n\
             CSeq: {} MESSAGE\r\n\
             Content-Type: Application/MANSCDP+xml\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: {}\r\n\r\n{}",
            to, via, from, to, call_id, sn, content_len, body
        ).into_bytes()
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
                    channels.push(CatalogChannel {
                        device_id,
                        name: Self::parse_xml_field(item_str, "Name").unwrap_or_default(),
                        manufacturer: Self::parse_xml_field(item_str, "Manufacturer"),
                        model: Self::parse_xml_field(item_str, "Model"),
                        status: Self::parse_xml_field(item_str, "Status").unwrap_or_else(|| "OFF".to_string()),
                        parental: Self::parse_xml_field(item_str, "Parental")
                            .and_then(|v| v.parse::<u32>().ok())
                            .map(|v| v > 0)
                            .unwrap_or(false),
                        parent_id: Some(self.device_tag_str()),
                    });
                }
                start = abs_start + item_end + 7;
            } else {
                break;
            }
        }

        channels
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
        format!(
            "SIP/2.0 {} {}\r\n\
             Via: {}\r\n\
             From: <sip:{}>{}\r\n\
             To: <sip:{}>{}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             {}User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            status_code, status_text,
            self.via_header(via_branch),
            device, self.from_tag_param(),
            device, self.to_tag_param(),
            cseq_line, self.call_id_str(),
            expires_line,
        )
    }

    fn build_401_response(&self, via_branch: &str, cseq_line: &str, realm: &str) -> String {
        let nonce = generate_nonce();
        let device = self.device_tag_str();
        let www_auth = format!(r#"Digest realm="{}", nonce="{}""#, realm, nonce);
        format!(
            "SIP/2.0 401 Unauthorized\r\n\
             Via: {}\r\n\
             From: <sip:{}>{}\r\n\
             To: <sip:{}>{}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             WWW-Authenticate: {}\r\n\
             Content-Length: 0\r\n\r\n",
            self.via_header(via_branch),
            device, self.from_tag_param(),
            device, self.to_tag_param(),
            cseq_line, self.call_id_str(), www_auth
        )
    }

    fn build_403_response(&self, via_branch: &str, cseq_line: &str) -> String {
        let device = self.device_tag_str();
        format!(
            "SIP/2.0 403 Forbidden\r\n\
             Via: {}\r\n\
             From: <sip:{}>{}\r\n\
             To: <sip:{}>{}\r\n\
             CSeq: {}\r\n\
             Call-ID: {}\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             Content-Length: 0\r\n\r\n",
            self.via_header(via_branch),
            device, self.from_tag_param(),
            device, self.to_tag_param(),
            cseq_line, self.call_id_str()
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

    fn build_catalog_response(&self, channel_id: &str) -> String {
        let device_tag = self.device_tag_str();
        let device_name = self.device_name.as_deref().unwrap_or("IPC");
        let sn = self.seq.load(Ordering::SeqCst);
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><CmdType>Catalog</CmdType><SN>{}</SN><DeviceID>{}</DeviceID><SumNum>1</SumNum>
<DeviceList><Item><DeviceID>{}</DeviceID><Name>{}</Name><Manufacturer>RustCam</Manufacturer>
<Model>IPC</Model><Owner>Owner</Owner><CivilCode>CivilCode</CivilCode><Address>Address</Address>
<Parental>0</Parental><SafetyWay>0</SafetyWay><RegisterWay>1</RegisterWay><CertNum>CertNum</CertNum>
<ErrCode>0</ErrCode><Secrecy>0</Secrecy><IPAddress>0.0.0.0</IPAddress><Port>0</Port>
<Password></Password><Status>ON</Status></Item></DeviceList></Response>"#,
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
                tracing::info!("[GB28181] RTP: stream={}, port={}, ip={}, server={}", stream_key, port, ip, tag);
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

        self.device_tag = Some(device_id.clone());
        if let Some(name) = Self::parse_xml_field(body, "DeviceName") {
            self.device_name = Some(name);
        }

        let expires = Self::get_header(headers, "Expires")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600);
        self.registration_expires = Some(expires.clamp(60, 3600));

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
            return (None, Some(self.build_401_response(&via_branch, &cseq, &device_id)));
        }

        let auth_params = match parse_sip_authorization(auth_header.unwrap()) {
            Some(p) => p,
            None => return (None, Some(self.build_403_response(&via_branch, &cseq))),
        };
        if !verify_nonce(&auth_params.nonce) {
            return (None, Some(self.build_401_response(&via_branch, &cseq, &device_id)));
        }

        let device_opt = self.deps.device_lookup.find_by_tag(&device_id).await;
        let (username, password, internal_id, device_tag) = match device_opt {
            Some(d) => (d.device_tag.clone().unwrap_or_default(), d.device_password.clone().unwrap_or_default(), d.id, Some(d.device_tag.unwrap_or_default())),
            None => return (None, Some(self.build_403_response(&via_branch, &cseq))),
        };
        if auth_params.username != username {
            return (None, Some(self.build_403_response(&via_branch, &cseq)));
        }

        match verify_sip_digest(&auth_params, &password, "REGISTER") {
            VerifyResult::Valid => {
                tracing::info!("[GB28181] Device {} authenticated", device_id);
                consume_nonce(&auth_params.nonce);
                self.device_id = Some(internal_id as i64);
                let entry: crate::protocol::adapter_manager::AdapterEntry = Arc::new(tokio::sync::Mutex::new(Box::new(self.clone())));
                (self.register_fn)(device_id.clone(), entry);
                let _ = self.deps.device_lookup.set_online(&device_id).await;

                let already_exists = self.deps.device_lookup.find_by_tag(&device_id).await.is_some();

                if !already_exists {
                    self.pending_catalog_query = true;
                }

               // let bus = registry().infra.event_bus.clone();
               // let handler_name = format!("gb28181:{}", device_id);
             //   self.event_handler_name = Some(handler_name.clone());
              //  bus.on(&handler_name, Box::new(self.clone()));
             //   tracing::info!("[GB28181] EventHandler registered as {} for {}", handler_name, device_id);

                let event = if already_exists {
                    SignalEvent::DeviceKeepalive {
                        device_id: internal_id,
                        device_tag,
                        timestamp: chrono::Utc::now(),
                    }
                } else {
                    SignalEvent::DeviceRegister {
                        device_id: internal_id,
                        device_tag,
                        name: self.device_name.clone().unwrap_or_else(|| "GB28181 Device".to_string()),
                        stream_key: None,
                        manufacturer: Self::parse_xml_field(body, "Manufacturer"),
                        model: Self::parse_xml_field(body, "Model"),
                        protocol: ProtocolType::Gb28181,
                    }
                };

                (Some(event), Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)))
            }
            _ => (None, Some(self.build_403_response(&via_branch, &cseq))),
        }
    }
    

    fn handle_message(
        &mut self, headers: &[(String, String)], body: &str,
    ) -> (Option<SignalEvent>, Option<String>) {
        if self.call_id.is_none() {
            if let Some(cid) = Self::get_header(headers, "Call-ID") { self.call_id = Some(cid); }
        }
        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);

        if body.contains("<CmdType>Alarm</CmdType>") || body.contains("<AlarmNotify>") || body.contains("<Alarm>") {
            let alarm_type = Self::parse_xml_field(body, "AlarmType").unwrap_or_else(|| "unknown".to_string());
            let alarm_msg = Self::parse_xml_field(body, "AlarmDescription").unwrap_or_default();
            let device_tag = self.device_tag_str();
            let device_id_val = self.device_id_i64();
            return (Some(SignalEvent::Alarm {
                device_id: device_id_val.to_string(),
                alarm_type,
                message: alarm_msg,
                timestamp: chrono::Utc::now(),
            }), Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)));
        }

        if self.pending_catalog_query && !body.contains("Catalog") {
            self.catalog_retry_count += 1;
            if self.catalog_retry_count >= 3 {
                self.pending_catalog_query = false;
                self.catalog_retry_count = 0;
                tracing::warn!("[GB28181] Catalog query retries exhausted, clearing pending flag");
            }
        }

        if body.is_empty() || body.contains("Keepalive") || body.contains("heartbeat") {
            let device_tag = self.device_tag_str();
            return (Some(SignalEvent::DeviceKeepalive { device_id: self.device_id_i64(), device_tag: Some(device_tag), timestamp: chrono::Utc::now() }),
                    Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)));
        }

        let device_tag = self.device_tag_str();

        if body.contains("<Response>") && body.contains("Catalog") {
            let channels = self.parse_catalog_response(body);
            if !channels.is_empty() {
                self.pending_catalog_query = false;
                self.catalog_retry_count = 0;
                return (
                    Some(SignalEvent::CatalogResponse {
                        device_id: self.device_id_i64(),
                        device_tag: Some(device_tag.clone()),
                        channels,
                    }),
                    Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)),
                );
            }
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

        (Some(SignalEvent::DeviceKeepalive { device_id: self.device_id_i64(), device_tag: Some(device_tag.clone()), timestamp: chrono::Utc::now() }),
         Some(self.build_sip_response_only_headers(200, "OK", &via_branch, &cseq)))
    }

    async fn handle_bye(
        &mut self, headers: &[(String, String)],
    ) -> (Option<SignalEvent>, Option<String>) {
        let via_branch = Self::parse_via_branch(headers);
        let cseq = Self::parse_cseq_line(headers);
        let call_id = Self::get_header(headers, "Call-ID").unwrap_or_default();

        let sub_session = {
            let sessions = self.sessions.read().await;
            sessions.values().find(|s| s.call_id == call_id).cloned()
        };

        if let Some(session) = sub_session {
            if session.parent_device_id == session.channel_id {
                tracing::info!("[GB28181] Single device BYE: channel={}, stream={}", session.channel_id, session.stream_key);
                let _ = self.deps.stream_manager.stop_stream(&session.stream_key).await;
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
            let _ = self.deps.stream_manager.stop_stream(&session.stream_key).await;
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
            let _ = self.deps.stream_manager.stop_stream(&session.stream_key).await;
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
            tracing::info!("[GB28181] ACK for session: channel={}", session.channel_id);
        } else {
            tracing::info!("[GB28181] ACK for unknown session: {}", call_id);
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
            tracing::debug!("[GB28181] Response for unknown session: {} status={}", call_id, first_line);
            return (None, None);
        };

        tracing::info!("[GB28181] Response for sub-session: channel={} status={}", session.channel_id, first_line);

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

        let contact = headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Contact"))
            .and_then(|(_, v)| {
                let v = v.trim();
                v.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
                    .or_else(|| v.strip_prefix("sip:").filter(|s| !s.starts_with('<')))
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("sip:{}@{}:{}", session.parent_device_id, session.device_host, session.device_port));

        let via = if session.transport == TransportProtocol::Tcp {
            format!("SIP/2.0/TCP {}:{};branch={};rport", session.server_ip, session.server_port, session.via_branch)
        } else {
            format!("SIP/2.0/UDP {}:{};branch={};rport", session.server_ip, session.server_port, session.via_branch)
        };

        let ack = format!(
            "ACK {contact} SIP/2.0\r\n\
             Via: {via}\r\n\
             From: <{from_uri}>;tag={from_tag}\r\n\
             To: <{to_uri}>\r\n\
             Call-ID: {call_id}\r\n\
             CSeq: {seq} ACK\r\n\
             User-Agent: RustCam-Media/2.0\r\n\
             \r\n",
            contact = contact,
            via = via,
            from_uri = session.from_uri,
            from_tag = session.from_tag,
            to_uri = session.to_uri,
            call_id = session.call_id,
            seq = session.seq
        );
        self.send(ack.as_bytes()).await.ok();

        let device_id = self.deps.device_lookup.find_by_tag(&session.channel_id).await
            .map(|d| d.id)
            .unwrap_or(0);
        let start_result = self.deps.stream_manager
            .start_gb28181_stream(device_id, &session.stream_key, 0, Some(&session.media_server_name))
            .await;

        if start_result.is_ok() {
            self.sessions.write().await
                .entry(session.channel_id.clone())
                .and_modify(|s| s.media_started = true);

            if let Some(stream) = self.deps.stream_manager.get_stream_by_stream_key(&session.stream_key).await {
                let mut s = stream.clone();
                s.start();
                let _ = self.deps.stream_manager.update_stream_state(&s).await;
            }

            tracing::info!("[GB28181] Stream started: channel={} stream={}", session.channel_id, session.stream_key);
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

    fn device_tag_str(&self) -> String {
        self.device_tag.clone().unwrap_or_default()
    }

    fn device_id_i64(&self) -> i64 {
        self.device_id.unwrap_or(0)
    }

    fn device_id_str(&self) -> Option<String> {
        self.device_id.map(|i| i.to_string())
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
        let mut buf = (*self.recv_buffer).clone();
        buf.extend_from_slice(data);
        self.recv_buffer = Arc::new(buf);

        const MAX_BUFFER_SIZE: usize = 1024 * 1024;
        if self.recv_buffer.len() > MAX_BUFFER_SIZE {
            tracing::error!("[GB28181] Buffer overflow, clearing. Device: {:?}", self.device_tag);
            self.recv_buffer = Arc::new(Vec::new());
            return Ok(Vec::new());
        }

        let mut events = Vec::new();

        loop {
            let buffer = (*self.recv_buffer).clone();
            let (first_line, headers, body) = match Self::parse_sip_message(&buffer) {
                Some(msg) => msg,
                None => break,
            };

            let consumed = {
                let header_end = String::from_utf8_lossy(&buffer).find("\r\n\r\n").unwrap_or(0) + 4;
                let cl = Self::get_header(&headers, "Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
                header_end + cl
            };
            self.recv_buffer = Arc::new(buffer[consumed..].to_vec());

            let (event, response) = if first_line.starts_with("SIP/2.0") {
                self.handle_sip_response(&first_line, &headers, &body).await
            } else if first_line.starts_with("REGISTER") {
                self.handle_register(&headers, &body).await
            } else if first_line.starts_with("MESSAGE") {
                self.handle_message(&headers, &body)
            } else if first_line.starts_with("BYE") {
                self.handle_bye(&headers).await
            } else if first_line.starts_with("ACK") {
                self.handle_ack(&headers).await
            } else if first_line.starts_with("OPTIONS") {
                let resp = self.handle_options(&headers);
                (None, resp)
            } else {
                (None, None)
            };

            if let Some(resp) = &response {
                let _ = self.send(resp.as_bytes()).await;
            }

            if self.pending_catalog_query {
                self.pending_catalog_query = false;
                let query = self.build_catalog_query();
                tracing::info!("[GB28181] Sending catalog query to {}", self.device_tag_str());
                let _ = self.send(&query).await;
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
            if let Some(ref name) = self.event_handler_name {
                registry().infra.event_bus.off(name);
            }
            let _ = self.deps.device_lookup.set_offline(&device_tag, Some("Connection closed")).await;

            if let Some(device) = self.deps.device_lookup.find_by_tag(&device_tag).await {
                if let Err(e) = self.deps.stream_manager.stop_streams_by_device(device.id).await {
                    tracing::warn!("[GB28181] on_disconnected: failed to stop streams for device {}: {}", device.id, e);
                }
            }

            for session in self.sessions.read().await.values() {
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
                sender.send_to(data, *addr).await
                    .map_err(|e| AppError::Internal(format!("UDP send: {}", e)))?;
            }
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

        let stream_key = device.stream_key.clone()
            .filter(|sk| !sk.is_empty())
            .unwrap_or_else(|| format!("gb_{}", device_tag));
        let (rtp_port, server_ip, media_server_name) = self.alloc_rtp_port(&stream_key, device.media_server_name.as_deref()).await?;

        let sn = self.seq.fetch_add(1, Ordering::SeqCst);
        let ssrc = format!("{:08x}", rand::random::<u32>());
        let call_id = uuid::Uuid::new_v4().to_string().replace("-", "");
        let from_tag = uuid::Uuid::new_v4().to_string().replace("-", "");
        let branch = format!("z9hG4bK{}", &call_id[..7]);
        let server_port = self.deps.config.server.port;
        let server_gb_id = &self.deps.config.signaling_server.server_gb_id;
        let uuid_part = uuid::Uuid::new_v4().to_string().replace("-", "");
        let media_serial = format!("0_{}", &uuid_part[..18]);

        let parent_device_id = device.parent_device_tag.clone()
            .unwrap_or_else(|| device_tag.to_string());
        let channel_id = device_tag.to_string();
        let device_host = device.host.clone();
        let device_port = device.port;

        let via = if self.transport == TransportProtocol::Tcp {
            format!("SIP/2.0/TCP {server_ip}:{server_port};branch={branch};rport")
        } else {
            format!("SIP/2.0/UDP {server_ip}:{server_port};branch={branch};rport")
        };

        let sdp = format!(
            "v=0\r\n\
             o={} 0 0 IN IP4 {}\r\n\
             s=Play\r\n\
             c=IN IP4 {}\r\n\
             t=0 0\r\n\
             m=video {} RTP/AVP 96 98\r\n\
             a=rtpmap:96 PS/90000\r\n\
             a=fmtp:96 profile-level-id=4D001F;packaging_mode=HIS\r\n\
             a=rtpmap:98 H264/90000\r\n\
             a=fmtp:98 profile-level-id=4D001F;packetization-mode=1;sprop-parameter-sets=Z00AH9oPBqWA8aA==,aM44gA==\r\n             m=audio {} RTP/AVP 8\r\n\
             a=rtpmap:8 PCMA/8000\r\n\
             y={}\r\n",
            server_gb_id, server_ip, server_ip, rtp_port, rtp_port + 2, ssrc
        );
        let sdp_len = sdp.len();
        let to_uri = format!("sip:{}@{}:{}", parent_device_id, device_host, device_port);
        let from_uri = format!("sip:{}@{}:{}", server_gb_id, server_ip, server_port);
        let subject = format!(
            "{}:{},{}:{}",
            Self::truncate_20(&channel_id),
            Self::truncate_32(&media_serial),
            Self::truncate_20(server_gb_id),
            Self::truncate_32(&media_serial)
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
            via_branch: branch.clone(),
            server_ip: server_ip.clone(),
            server_port,
            transport: self.transport,
            device_host: device_host.clone(),
            device_port,
            rtp_port,
            media_started: false,
        };

        let old_session = self.sessions.write().await.insert(device_tag.to_string(), session.clone());
        if let Some(old) = old_session {
            tracing::info!("[GB28181] start: cleaning up old session for channel={}", device_tag);
            if old.media_started {
                let _ = self.deps.stream_manager.stop_stream(&old.stream_key).await;
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

        tracing::info!("[GB28181] Sending INVITE to device={}, stream={}, rtp={}", device_tag, stream_key, rtp_port);
        self.send(invite.as_bytes()).await?;

        Ok(())
    }

    async fn ptz_control(&mut self, channel_id: &str, command: &crate::protocol::event::PtzCommand, speed: Option<u8>) -> Result<()> {
        let device = self.deps.device_lookup.find_by_tag(channel_id).await
            .ok_or_else(|| AppError::NotFound(format!("Device not found: {}", channel_id)))?;

        let parent_device_id = device.parent_device_tag.clone().unwrap_or_else(|| channel_id.to_string());
        let spd = speed.unwrap_or(50);

        let (direction, spd_val) = match command {
            PtzCommand::Up => ("up", spd),
            PtzCommand::Down => ("down", spd),
            PtzCommand::Left => ("left", spd),
            PtzCommand::Right => ("right", spd),
            PtzCommand::ZoomIn => ("zoom_in", spd),
            PtzCommand::ZoomOut => ("zoom_out", spd),
            PtzCommand::Stop => ("stop", 0),
            PtzCommand::FocusIn => ("focus_in", spd),
            PtzCommand::FocusOut => ("focus_out", spd),
            PtzCommand::ContinuousMove { pan, tilt, zoom } => {
                if pan.abs() < 0.01 && tilt.abs() < 0.01 && zoom.abs() < 0.01 {
                    ("stop", 0)
                } else {
                    let dir = Self::best_direction(*pan, *tilt, *zoom);
                    let spd_val = ((pan.abs().max(tilt.abs()).max(zoom.abs()) * 100.0) as u8).clamp(1, 100);
                    (dir, spd_val)
                }
            }
            _ => return Ok(()),
        };

        let ptz_body = Self::build_ptz_xml(channel_id, direction, spd_val);
        let msg = self.build_ptz_message(channel_id, &parent_device_id, &ptz_body);

        tracing::debug!("[GB28181] PTZ: channel={} dir={} spd={}", channel_id, direction, spd_val);
        self.send(msg.as_bytes()).await?;
        Ok(())
    }

}
