use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::Reader;

use std::net::SocketAddr;
use std::sync::{ Arc };
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;

use crate::domain::SessionType;
use crate::error::{AppError, Result};
use crate::media::transport::OnvifBackchannelTransport;
use crate::media::{
    get_media_session_manager, BasicMediaNode, MediaNode, MediaNodeId, MediaNodeType,
};
use crate::protocol::adapter::SignalAdapter;
use crate::protocol::event::{
    ProtocolType, PtzCommand, SignalEvent, TalkResponseStatus, TransportType,
};
use crate::protocol::onvif::auth::UsernameToken;
use crate::protocol::onvif::events::PullPointServer;
use crate::protocol::onvif::ptz_client::OnvifPtzClient;
use crate::protocol::traits::ProtocolDeps;

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
const MAX_BUFFER_BYTES: usize = MAX_HEADER_BYTES + MAX_BODY_BYTES;

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn local_name(name_bytes: &[u8]) -> &str {
    let name_str = std::str::from_utf8(name_bytes).unwrap_or("");
    if let Some(idx) = name_str.find(':') {
        &name_str[idx + 1..]
    } else {
        name_str
    }
}

#[derive(Debug, Clone, Default)]
pub struct OnvifDeviceInfo {
    pub manufacturer: String,
    pub model: String,
    pub firmware_version: String,
    pub serial_number: String,
    pub hardware_id: String,
    pub password: Option<String>,
}

pub struct OnvifAdapter {
    device_id: Option<String>,
    remote_addr: Option<SocketAddr>,
    recv_buffer: Vec<u8>,
    device_info: OnvifDeviceInfo,
    pull_point_server: PullPointServer,
    rtsp_host: Option<String>,
    rtsp_port: Option<u16>,
    write: Option<std::sync::Arc<tokio::sync::RwLock<OwnedWriteHalf>>>,
    deps: ProtocolDeps,
}

impl OnvifAdapter {
    pub fn new(deps: ProtocolDeps) -> Self {

        let rtsp_port = deps
            .config
            .media_servers
            .servers
            .first()
            .and_then(|s| s.protocol_ports.rtsp)
            .unwrap_or(8554);

        Self {
            device_id: None,
            remote_addr: None,
            recv_buffer: Vec::new(),
            device_info: OnvifDeviceInfo {
                manufacturer: "RustCam".to_string(),
                model: "ONVIF-NVT".to_string(),
                firmware_version: "2.0.0".to_string(),
                serial_number: "00000001".to_string(),
                hardware_id: "rustcam-onvif".to_string(),
                password: Some("admin".to_string()),
            },
            pull_point_server: PullPointServer::new(),
            rtsp_host: Some(deps.config.server.host.clone()),
            rtsp_port: Some(rtsp_port),
            write: None,
            deps,
        }
    }

    pub fn with_rtsp_server(mut self, host: String, port: u16) -> Self {
        self.rtsp_host = Some(host);
        self.rtsp_port = Some(port);
        self
    }

    pub fn with_device_info(mut self, info: OnvifDeviceInfo) -> Self {
        self.device_id = Some(info.serial_number.clone());
        self.device_info = info;
        self
    }

    fn extract_message(buffer: &[u8]) -> std::result::Result<Option<(usize, usize)>, &'static str> {
        let header_end = match memchr::memmem::find(buffer, b"\r\n\r\n") {
            Some(p) => p,
            None => {
                if buffer.len() > MAX_HEADER_BYTES {
                    return Err("header too large");
                }
                return Ok(None);
            }
        };

        if header_end > MAX_HEADER_BYTES {
            return Err("header too large");
        }

        let header_str = String::from_utf8_lossy(&buffer[..header_end]);
        let mut content_length: usize = 0;
        for line in header_str.lines() {
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    let parsed = v
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| "bad content-length")?;
                    if parsed > MAX_BODY_BYTES {
                        return Err("body too large");
                    }
                    content_length = parsed;
                    break;
                }
            }
        }

        let body_start = header_end + 4;
        let total_len = body_start + content_length;

        if buffer.len() < total_len {
            if total_len > MAX_BUFFER_BYTES {
                return Err("total message too large");
            }
            return Ok(None);
        }

        Ok(Some((body_start, total_len)))
    }

    fn get_soap_action(headers: &str) -> Option<String> {
        for line in headers.lines() {
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case("soapaction") {
                    return Some(v.trim().trim_matches('"').to_string());
                }
            }
        }
        None
    }

    fn action_name(soap_action: &str) -> &str {
        soap_action.rsplit('/').next().unwrap_or(soap_action).trim()
    }

    fn build_soap_response(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
<s:Body>{}</s:Body>
</s:Envelope>"#,
            body
        )
    }

    fn build_soap_fault(fault_code: &str, fault_subcode: &str, fault_string: &str) -> String {
        Self::build_soap_response(&format!(
            r#"<s:Fault>
  <s:Code><s:Value>{}</s:Value><s:Subcode><s:Value>{}</s:Value></s:Subcode></s:Code>
  <s:Reason><s:Text>{}</s:Text></s:Reason>
</s:Fault>"#,
            fault_code, fault_subcode, fault_string
        ))
    }

    fn build_get_device_info_response(&self) -> String {
        let body = format!(
            r#"<GetDeviceInformationResponse xmlns="http://www.onvif.org/ver10/device/wsdl">
<Manufacturer>{}</Manufacturer>
<Model>{}</Model>
<FirmwareVersion>{}</FirmwareVersion>
<SerialNumber>{}</SerialNumber>
<HardwareId>{}</HardwareId>
</GetDeviceInformationResponse>"#,
            self.device_info.manufacturer,
            self.device_info.model,
            self.device_info.firmware_version,
            self.device_info.serial_number,
            self.device_info.hardware_id,
        );
        Self::build_soap_response(&body)
    }

    fn build_get_capabilities_response(&self) -> String {
        let host = self.rtsp_host.as_deref().unwrap_or("127.0.0.1");
        let port = self.rtsp_port.unwrap_or(8554);
        let body = format!(
            r#"<GetCapabilitiesResponse xmlns="http://www.onvif.org/ver10/device/wsdl">
<Capabilities>
  <Device><XAddr>http://{}:{}/onvif/device_service</XAddr></Device>
  <Media><XAddr>http://{}:{}/onvif/media_service</XAddr>
    <StreamingCapabilities>
      <RTPMulticast>true</RTPMulticast>
      <RTP_TCP>true</RTP_TCP>
      <RTP_RTSP_TCP>true</RTP_RTSP_TCP>
    </StreamingCapabilities>
  </Media>
  <PTZ><XAddr>http://{}:{}/onvif/ptz_service</XAddr></PTZ>
  <Events><XAddr>http://{}:{}/onvif/event_service</XAddr>
    <WSSubscriptionPolicySupport>true</WSSubscriptionPolicySupport>
    <WSPullPointSupport>true</WSPullPointSupport>
    <WSPausableSubscriptionManagerInterfaceSupport>true</WSPausableSubscriptionManagerInterfaceSupport>
  </Events>
</Capabilities>
</GetCapabilitiesResponse>"#,
            host, port, host, port, host, port, host, port,
        );
        Self::build_soap_response(&body)
    }

    fn build_get_profiles_response(&self) -> String {
        let body = r#"<GetProfilesResponse xmlns="http://www.onvif.org/ver10/media/wsdl">
<Profiles token="Profile_1" fixed="true">
  <Name>MainStream</Name>
  <VideoSourceConfiguration token="VideoSource_1">
    <Name>VideoSource</Name><UseCount>1</UseCount>
    <SourceToken>VideoSource_1</SourceToken>
    <Bounds x="0" y="0" width="1920" height="1080"/>
  </VideoSourceConfiguration>
  <VideoEncoderConfiguration token="VideoEncoder_1">
    <Name>H264_1080P</Name><UseCount>1</UseCount>
    <Encoding>H264</Encoding>
    <Resolution><Width>1920</Width><Height>1080</Height></Resolution>
    <RateControl><FrameRateLimit>30</FrameRateLimit><BitrateLimit>4096</BitrateLimit></RateControl>
  </VideoEncoderConfiguration>
  <PTZConfiguration token="PTZ_1">
    <Name>PTZ</Name><UseCount>1</UseCount><NodeToken>PTZNode_1</NodeToken>
  </PTZConfiguration>
</Profiles>
<Profiles token="Profile_2" fixed="false">
  <Name>SubStream</Name>
  <VideoSourceConfiguration token="VideoSource_2">
    <Name>VideoSource</Name><UseCount>1</UseCount>
    <SourceToken>VideoSource_1</SourceToken>
    <Bounds x="0" y="0" width="1280" height="720"/>
  </VideoSourceConfiguration>
  <VideoEncoderConfiguration token="VideoEncoder_2">
    <Name>H264_720P</Name><UseCount>1</UseCount>
    <Encoding>H264</Encoding>
    <Resolution><Width>1280</Width><Height>720</Height></Resolution>
    <RateControl><FrameRateLimit>25</FrameRateLimit><BitrateLimit>2048</BitrateLimit></RateControl>
  </VideoEncoderConfiguration>
</Profiles>
</GetProfilesResponse>"#;
        Self::build_soap_response(body)
    }

    fn build_get_stream_uri_response(&self, profile_token: &str) -> String {
        let host = self.rtsp_host.as_deref().unwrap_or("127.0.0.1");
        let port = self.rtsp_port.unwrap_or(8554);
        let stream_key = format!("live/{}", profile_token);
        let rtsp_uri = format!("rtsp://{}:{}/{}", host, port, stream_key);
        let body = format!(
            r#"<GetStreamUriResponse xmlns="http://www.onvif.org/ver10/media/wsdl">
<MediaUri>
  <Uri>{}</Uri>
  <InvalidAfterConnect>false</InvalidAfterConnect>
  <InvalidAfterReboot>false</InvalidAfterReboot>
  <Timeout>PT0S</Timeout>
</MediaUri>
</GetStreamUriResponse>"#,
            rtsp_uri
        );
        Self::build_soap_response(&body)
    }

    fn build_get_snapshot_uri_response(&self) -> String {
        let device_id = self.device_id.as_deref().unwrap_or("onvif");
        let host = self.rtsp_host.as_deref().unwrap_or("127.0.0.1");
        let port = self.rtsp_port.unwrap_or(8080);
        let body = format!(
            r#"<GetSnapshotUriResponse xmlns="http://www.onvif.org/ver10/media/wsdl">
<MediaUri><Uri>http://{}:{}/snapshot/{}.jpg</Uri></MediaUri>
</GetSnapshotUriResponse>"#,
            host, port, device_id,
        );
        Self::build_soap_response(&body)
    }

    fn build_get_video_sources_response(&self) -> String {
        let body = r#"<GetVideoSourcesResponse xmlns="http://www.onvif.org/ver10/media/wsdl">
<VideoSources token="VideoSource_1">
  <Framerate>30</Framerate>
  <Resolution><Width>1920</Width><Height>1080</Height></Resolution>
</VideoSources>
</GetVideoSourcesResponse>"#;
        Self::build_soap_response(body)
    }

    fn build_ptz_response() -> String {
        Self::build_soap_response(
            r#"<ContinuousMoveResponse xmlns="http://www.onvif.org/ver20/ptz/wsdl"/>"#,
        )
    }

    fn build_get_system_date_time_response(&self) -> String {
        use chrono::Local;
        let now = Local::now();
        let body = format!(
            r#"<GetSystemDateAndTimeResponse xmlns="http://www.onvif.org/ver10/device/wsdl">
<SystemDateAndTime>
  <DateTimeType>Manual</DateTimeType>
  <DaylightSavings>false</DaylightSavings>
  <TimeZone><TZ>UTC+8</TZ></TimeZone>
  <UTCDateTime>
    <Time><Hour>{}</Hour><Minute>{}</Minute><Second>{}</Second></Time>
    <Date><Year>{}</Year><Month>{}</Month><Day>{}</Day></Date>
  </UTCDateTime>
</SystemDateAndTime>
</GetSystemDateAndTimeResponse>"#,
            now.format("%H"),
            now.format("%M"),
            now.format("%S"),
            now.format("%Y"),
            now.format("%m"),
            now.format("%d"),
        );
        Self::build_soap_response(&body)
    }

    fn build_get_network_interfaces_response(&self) -> String {
        let host = self.rtsp_host.as_deref().unwrap_or("192.168.1.100");
        let body = format!(
            r#"<GetNetworkInterfacesResponse xmlns="http://www.onvif.org/ver10/device/wsdl">
<NetworkInterfaces token="eth0">
  <Enabled>true</Enabled>
  <Info><Name>eth0</Name><HwAddress>00:00:00:00:00:00</HwAddress><MTU>1500</MTU></Info>
  <IPv4><Enabled>true</Enabled>
    <Config><Manual><Address>{}</Address><PrefixLength>24</PrefixLength></Manual><DHCP>false</DHCP></Config>
  </IPv4>
</NetworkInterfaces>
</GetNetworkInterfacesResponse>"#,
            host
        );
        Self::build_soap_response(&body)
    }

    fn build_get_users_response(&self) -> String {
        let body = r#"<GetUsersResponse xmlns="http://www.onvif.org/ver10/device/wsdl">
<User><Username>admin</Username><UserLevel>Administrator</UserLevel></User>
<User><Username>operator</Username><UserLevel>Operator</UserLevel></User>
</GetUsersResponse>"#;
        Self::build_soap_response(body)
    }

    fn extract_element_text(data: &[u8], element: &str) -> Option<String> {
        let mut reader = Reader::from_reader(data);
        reader.config_mut().trim_text(true);
        let mut capture = false;
        let mut result = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.name().local_name();
                    if name.as_ref() == element {
                        capture = true;
                    }
                }
                Ok(Event::Text(ref e)) if capture => {
                    result.push_str(&*e);
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name().local_name();
                    if name.as_ref() == element {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result.trim().to_string())
        }
    }

    fn extract_profile_token(data: &[u8]) -> String {
        let mut reader = Reader::from_reader(data);
        reader.config_mut().trim_text(true);
        let mut text_content = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.name().local_name();
                    if name.as_ref() == "ProfileToken" || name.as_ref() == "Profile" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == "token" {
                                return attr.value.to_string();
                            }
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    text_content = (&*e).to_string();
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name().local_name();
                    if name.as_ref() == "ProfileToken" && !text_content.is_empty() {
                        return text_content.clone();
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }

        "Profile_1".to_string()
    }

    fn parse_ptz_velocity(data: &[u8]) -> (f64, f64, f64) {
        let mut reader = Reader::from_reader(data);
        reader.config_mut().trim_text(true);
        let mut pan: f64 = 0.0;
        let mut tilt: f64 = 0.0;
        let mut zoom: f64 = 0.0;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.name().local_name();
                    if name.as_ref() == "PanTilt" || name.as_ref() == "Zoom" {
                        for attr in e.attributes().flatten() {
                            let key = attr.key.as_ref();
                            if let Ok(val) = attr.value.parse::<f64>() {
                                match key {
                                    "x" => {
                                        if name.as_ref() == "Zoom" {
                                            zoom = val;
                                        } else {
                                            pan = val;
                                        }
                                    }
                                    "y" => tilt = val,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }

        (pan, tilt, zoom)
    }

    fn parse_duration_hours(termination: &str) -> i64 {
        if termination.starts_with("PT") {
            let rest = &termination[2..];
            if let Some(h) = rest.strip_suffix('H') {
                return h.parse().unwrap_or(24);
            }
            if let Some(m) = rest.strip_suffix('M') {
                let mins: i64 = m.parse().unwrap_or(1440);
                return mins / 60 + 1;
            }
            if let Some(s) = rest.strip_suffix('S') {
                let secs: i64 = s.parse().unwrap_or(86400);
                return secs / 3600 + 1;
            }
        }
        24
    }

    async fn send_raw(&self, data: &[u8]) -> Result<()> {
        if let Some(ref write_arc) = self.write {
            let mut write = write_arc.write().await;
            write
                .write_all(data)
                .await
                .map_err(|e| AppError::Internal(format!("TCP write error: {}", e)))?;
            let _ = write.flush().await;
        }
        Ok(())
    }

    fn handle_soap_action(&self, action: &str, data: &[u8]) -> Option<String> {
        if Self::action_name(action).contains("GetDeviceInformation") {
            Some(self.build_get_device_info_response())
        } else if Self::action_name(action).contains("GetCapabilities") {
            Some(self.build_get_capabilities_response())
        } else if Self::action_name(action).contains("GetProfiles") {
            Some(self.build_get_profiles_response())
        } else if Self::action_name(action).contains("GetStreamUri") {
            let profile = Self::extract_profile_token(data);
            Some(self.build_get_stream_uri_response(&profile))
        } else if Self::action_name(action).contains("GetSnapshotUri") {
            Some(self.build_get_snapshot_uri_response())
        } else if Self::action_name(action).contains("GetVideoSources") {
            Some(self.build_get_video_sources_response())
        } else if Self::action_name(action).contains("ContinuousMove")
            || Self::action_name(action).contains("AbsoluteMove")
            || Self::action_name(action).contains("RelativeMove")
            || Self::action_name(action).contains("Stop")
        {
            Some(Self::build_ptz_response())
        } else if Self::action_name(action).contains("GetPresets") {
            let presets = self.pull_point_server.get_presets();
            Some(PullPointServer::build_get_presets_response(&presets))
        } else if Self::action_name(action).contains("SetPreset") {
            let name = Self::extract_element_text(data, "PresetName")
                .unwrap_or_else(|| "Preset".to_string());
            let token = self.pull_point_server.save_preset(name);
            let body = format!(
                r#"<SetPresetResponse xmlns="http://www.onvif.org/ver20/ptz/wsdl"><PresetToken>{}</PresetToken></SetPresetResponse>"#,
                token
            );
            Some(Self::build_soap_response(&body))
        } else if Self::action_name(action).contains("RemovePreset") {
            let token = Self::extract_element_text(data, "PresetToken").unwrap_or_default();
            self.pull_point_server.remove_preset(&token);
            Some(Self::build_soap_response(
                r#"<RemovePresetResponse xmlns="http://www.onvif.org/ver20/ptz/wsdl"/>"#,
            ))
        } else if Self::action_name(action).contains("GotoPreset") {
            Some(Self::build_soap_response(
                r#"<GotoPresetResponse xmlns="http://www.onvif.org/ver20/ptz/wsdl"/>"#,
            ))
        } else if Self::action_name(action).contains("GetStatus") {
            let status = r#"<GetStatusResponse xmlns="http://www.onvif.org/ver20/ptz/wsdl">
  <PTZStatus>
    <Position><PanTilt x="0.0" y="0.0"/><Zoom x="1.0"/></Position>
    <MoveStatus><PanTilt>IDLE</PanTilt><Zoom>IDLE</Zoom></MoveStatus>
  </PTZStatus>
</GetStatusResponse>"#;
            Some(Self::build_soap_response(status))
        } else if Self::action_name(action).contains("CreatePullPointSubscription") {
            let timeout = Self::extract_element_text(data, "TerminationTime")
                .as_deref()
                .map(Self::parse_duration_hours)
                .unwrap_or(24);
            let sub_ref = self
                .pull_point_server
                .create_subscription_with_timeout(timeout);
            Some(PullPointServer::build_create_subscription_response(
                &sub_ref,
            ))
        } else if Self::action_name(action).contains("PullMessages") {
            let sub_ref = Self::extract_element_text(data, "SubscriptionReference")
                .unwrap_or_else(|| "urn:uuid:unknown".to_string());
            let messages = self.pull_point_server.pull_messages(&sub_ref, 100, 5);
            Some(PullPointServer::build_pull_messages_response(
                &messages, &sub_ref,
            ))
        } else if Self::action_name(action).contains("GetEventProperties") {
            Some(PullPointServer::build_get_event_properties_response())
        } else if Self::action_name(action).contains("Renew") {
            let sub_ref =
                Self::extract_element_text(data, "SubscriptionReference").unwrap_or_default();
            if let Some(termination) = self.pull_point_server.renew_subscription(&sub_ref, 24) {
                Some(PullPointServer::build_renew_response(&termination))
            } else {
                Some(Self::build_soap_fault(
                    "s:Receiver",
                    "wsa:InvalidMessageReference",
                    "Subscription not found",
                ))
            }
        } else if Self::action_name(action).contains("Unsubscribe") {
            let sub_ref =
                Self::extract_element_text(data, "SubscriptionReference").unwrap_or_default();
            self.pull_point_server.unsubscribe(&sub_ref);
            Some(PullPointServer::build_unsubscribe_response())
        } else if Self::action_name(action).contains("GetSystemDateAndTime") {
            Some(self.build_get_system_date_time_response())
        } else if Self::action_name(action).contains("GetNetworkInterfaces") {
            Some(self.build_get_network_interfaces_response())
        } else if Self::action_name(action).contains("GetUsers") {
            Some(self.build_get_users_response())
        } else if Self::action_name(action).contains("CreateUsers") {
            Some(Self::build_soap_response(
                r#"<CreateUsersResponse xmlns="http://www.onvif.org/ver10/device/wsdl"/>"#,
            ))
        } else if Self::action_name(action).contains("DeleteUsers") {
            Some(Self::build_soap_response(
                r#"<DeleteUsersResponse xmlns="http://www.onvif.org/ver10/device/wsdl"/>"#,
            ))
        } else if Self::action_name(action).contains("GetNetworkDefaultSettings") {
            Some(Self::build_soap_response(
                r#"<GetNetworkDefaultSettingsResponse xmlns="http://www.onvif.org/ver10/device/wsdl">
  <IPv4><Enabled>true</Enabled><Config><DHCP>true</DHCP></Config></IPv4>
</GetNetworkDefaultSettingsResponse>"#,
            ))
        } else {
            Some(Self::build_soap_fault(
                "s:Client",
                "ter:ActionNotSupported",
                &format!("Unsupported action: {}", action),
            ))
        }
    }

    fn parse_message(&mut self, data: &[u8]) -> Result<(Vec<SignalEvent>, Option<Vec<u8>>)> {
        let header_end = memchr::memmem::find(data, b"\r\n\r\n")
            .ok_or_else(|| AppError::Internal("ONVIF: no header end".to_string()))?;
        let header_str = String::from_utf8_lossy(&data[..header_end]);

        let soap_action = Self::get_soap_action(&header_str);
        let device_id = Self::extract_element_text(data, "SerialNumber")
            .or_else(|| Self::extract_element_text(data, "Address"))
            .or_else(|| self.device_id.clone())
            .unwrap_or_else(|| "onvif_device".to_string());

        if self.device_id.is_none() {
            self.device_id = Some(device_id.clone());
        }

        let mut events = Vec::new();

        if let Some(ref action) = soap_action {
            if let Some(token) = UsernameToken::from_xml(data) {
                let password = self.device_info.password.as_deref().unwrap_or("admin");
                if !token.verify(password) {
                    tracing::warn!("[ONVIF] Auth failed for user: {}", token.username);
                    let fault = Self::build_soap_fault(
                        "env:Sender",
                        "ter:NotAuthenticated",
                        "Authentication failed",
                    );
                    return Ok((events, Some(fault.into_bytes())));
                }
            }

            if let Some(response) = self.handle_soap_action(action, data) {
                return Ok((events, Some(response.into_bytes())));
            }
        }

        if let Some(action) = soap_action.as_deref() {
            if Self::action_name(action).contains("GetDeviceInformation")
                || Self::action_name(action).contains("GetCapabilities")
            {
                events.push(SignalEvent::QueryDeviceInfo {
                    device_tag: self.device_id.clone(),
                });
            }
            if Self::action_name(action).contains("GetStreamUri")
                || Self::action_name(action).contains("GetSnapshotUri")
            {
                events.push(SignalEvent::StartPlay {
                    device_tag: self.device_id.clone(),
                    session_id: format!("onvif_{}", chrono::Utc::now().timestamp_millis()),
                    channel_id: Some(Self::extract_profile_token(data)),
                    transport: TransportType::Tcp,
                    media_server_name: None,
                    session_type: SessionType::Play,
                });
            }
            if Self::action_name(action).contains("ContinuousMove") {
                let (pan, tilt, zoom) = Self::parse_ptz_velocity(data);
                let speed =
                    ((pan.abs().max(tilt.abs()).max(zoom.abs()) * 100.0) as u8).clamp(1, 100);
                events.push(SignalEvent::PtzControl {
                    device_id: device_id.clone(),
                    command: PtzCommand::ContinuousMove { pan, tilt, zoom },
                    speed: Some(speed),
                });
            }
            if Self::action_name(action).contains("AbsoluteMove") {
                let (pan, tilt, zoom) = Self::parse_ptz_velocity(data);
                events.push(SignalEvent::PtzControl {
                    device_id: device_id.clone(),
                    command: PtzCommand::AbsoluteMove { pan, tilt, zoom },
                    speed: Some(50),
                });
            }
            if Self::action_name(action).contains("RelativeMove") {
                let (pan, tilt, zoom) = Self::parse_ptz_velocity(data);
                events.push(SignalEvent::PtzControl {
                    device_id: device_id.clone(),
                    command: PtzCommand::RelativeMove { pan, tilt, zoom },
                    speed: Some(50),
                });
            }
            if Self::action_name(action).contains("Stop") {
                events.push(SignalEvent::PtzControl {
                    device_id: device_id.clone(),
                    command: PtzCommand::Stop,
                    speed: Some(0),
                });
            }
        }

        Ok((events, None))
    }
}

#[async_trait]
impl SignalAdapter for OnvifAdapter {
    async fn parse(&mut self, data: &[u8]) -> Result<Vec<SignalEvent>> {
        self.recv_buffer.extend_from_slice(data);

        if self.recv_buffer.len() > MAX_BUFFER_BYTES {
            tracing::error!(
                "[ONVIF] recv_buffer exceeded limit ({} bytes)",
                self.recv_buffer.len()
            );
            self.recv_buffer.clear();
            return Err(AppError::Internal("ONVIF: buffer overflow".into()));
        }

        let mut events = Vec::new();

        loop {
            let (body_start, total_len) = match Self::extract_message(&self.recv_buffer) {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(e) => {
                    tracing::error!("[ONVIF] extract_message error: {}", e);
                    self.recv_buffer.clear();
                    return Err(AppError::Internal(format!("ONVIF: {}", e)));
                }
            };

            let msg = self.recv_buffer[..total_len].to_vec();
            self.recv_buffer.drain(..total_len);
            let _ = body_start;

            let (msg_events, response) = self.parse_message(&msg)?;
            events.extend(msg_events);

            if let Some(resp) = response {
                self.send_raw(&resp).await?;
            }
        }

        Ok(events)
    }

    async fn on_connected(&mut self, addr: SocketAddr) -> Result<()> {
        self.remote_addr = Some(addr);
        Ok(())
    }

    async fn on_disconnected(&mut self) -> Result<()> {
        let device_id = self.device_id.as_deref().unwrap_or("unknown");
        self.recv_buffer.clear();
        if self.device_id.is_some() {
            let _ = self
                .deps
                .device_lookup
                .set_offline(device_id, Some("Connection closed"))
                .await;
        }
        Ok(())
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.send_raw(data).await
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::Onvif
    }

    fn name(&self) -> &'static str {
        "ONVIF"
    }

    fn keepalive(&self) -> bool {
        true
    }

    fn idle_timeout(&self) -> Option<u64> {
        Some(120)
    }

    fn set_tcp_write(&mut self, write: OwnedWriteHalf) {
        self.write = Some(std::sync::Arc::new(tokio::sync::RwLock::new(write)));
    }

    async fn start(&mut self, _device_tag: &str, _channel_tag: &str) -> Result<()> {
        Ok(())
    }

    async fn ptz_control(
        &mut self,
        channel_id: &str,
        command: &PtzCommand,
        speed: Option<u8>,
    ) -> Result<()> {
        let device = match self.deps.device_lookup.find_by_tag(channel_id).await {
            Some(d) => d,
            None => {
                tracing::warn!("[ONVIF] PTZ device not found: {}", channel_id);
                return Ok(());
            }
        };

        let parent_tag = match &device.parent_device_tag {
            Some(tag) => tag.clone(),
            None => {
                channel_id.to_string()
            }
        };

        let parent = match self.deps.device_lookup.find_by_tag(&parent_tag).await {
            Some(p) => p,
            None => {
                tracing::warn!("[ONVIF] PTZ parent device not found: {}", parent_tag);
                return Ok(());
            }
        };

        let ptz_url = match &parent.extended {
            Some(ext) => ext
                .get("capabilities")
                .and_then(|c| c.get("ptz"))
                .and_then(|u| u.as_str())
                .and_then(|s| {
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                }),
            None => None,
        };

        let ptz_url = match ptz_url {
            Some(url) => url,
            None => {
                tracing::warn!("[ONVIF] No PTZ URL found for device {}", parent_tag);
                return Ok(());
            }
        };

        let profile_token = match &parent.extended {
            Some(ext) => ext
                .get("onvif_token")
                .and_then(|v| v.as_str())
                .unwrap_or("Profile_1")
                .to_string(),
            None => "Profile_1".to_string(),
        };

        let client = OnvifPtzClient::new(
            ptz_url,
            parent.device_username.clone(),
            parent.device_password.clone(),
        )
        .with_profile(profile_token);

        match command {
            PtzCommand::ContinuousMove { pan, tilt, zoom } => {
                if pan.abs() < 0.01 && tilt.abs() < 0.01 && zoom.abs() < 0.01 {
                    let _ = client.stop().await;
                } else {
                    let _ = client.continuous_move(*pan, *tilt, *zoom).await;
                }
            }
            PtzCommand::Up => {
                let s = speed.map(|v| v as f64 / 100.0).unwrap_or(0.5);
                let _ = client.continuous_move(0.0, s, 0.0).await;
            }
            PtzCommand::Down => {
                let s = speed.map(|v| v as f64 / 100.0).unwrap_or(0.5);
                let _ = client.continuous_move(0.0, -s, 0.0).await;
            }
            PtzCommand::Left => {
                let s = speed.map(|v| v as f64 / 100.0).unwrap_or(0.5);
                let _ = client.continuous_move(-s, 0.0, 0.0).await;
            }
            PtzCommand::Right => {
                let s = speed.map(|v| v as f64 / 100.0).unwrap_or(0.5);
                let _ = client.continuous_move(s, 0.0, 0.0).await;
            }
            PtzCommand::ZoomIn => {
                let s = speed.map(|v| v as f64 / 100.0).unwrap_or(0.5);
                let _ = client.continuous_move(0.0, 0.0, s).await;
            }
            PtzCommand::ZoomOut => {
                let s = speed.map(|v| v as f64 / 100.0).unwrap_or(0.5);
                let _ = client.continuous_move(0.0, 0.0, -s).await;
            }
            PtzCommand::Stop => {
                let _ = client.stop().await;
            }
            PtzCommand::GotoPreset { preset_token } => {
                let _ = client.goto_preset(preset_token).await;
            }
            PtzCommand::SetPreset { preset_name } => {
                let _ = client
                    .set_preset("", preset_name.as_deref())
                    .await
                    .map_err(|e| tracing::warn!("[ONVIF] SetPreset failed: {}", e));
            }
            PtzCommand::RemovePreset { preset_token } => {
                let _ = client
                    .remove_preset(preset_token)
                    .await
                    .map_err(|e| tracing::warn!("[ONVIF] RemovePreset failed: {}", e));
            }
            _ => {
            }
        }

        Ok(())
    }

    async fn start_audio_talk_internal(
        &mut self,
        device_tag: &str,
        requester_node_id: MediaNodeId,
    ) -> Result<()> {

        let try_send_error = |msg: &str| {
            let resp = SignalEvent::AudioTalkResponse {
                node_id: requester_node_id.clone(),
                status: TalkResponseStatus::Error,
                message: Some(msg.to_string()),
            };
            let _ = self.deps.event_publisher.clone().publish(resp);
        };

        if let Some(mgr) = get_media_session_manager() {
            let session = mgr.get_or_create(device_tag);
            if session.has_node_of_type(MediaNodeType::ONVIF_BROADCAST_VOICE) {
                let event = SignalEvent::AudioTalkReady {
                    node_id: requester_node_id.clone(),
                };
                let _ = self.deps.event_publisher.clone().publish(event);
                return Ok(());
            }
        }

        let device = match self.deps.device_lookup.find_by_tag(device_tag).await {
            Some(d) => d,
            None => {
                tracing::error!("[ONVIF] Device not found: {}", device_tag);
                try_send_error(&format!("Device not found: {}", device_tag));
                return Ok(());
            }
        };

        let rtsp_url = match device
            .pull_urls
            .iter()
            .find(|p| p.protocol == crate::domain::StreamProtocol::Rtsp)
            .map(|p| p.url.clone())
        {
            Some(url) => url,
            None => {
                tracing::error!("[ONVIF] No RTSP pull_url for device {}", device_tag);
                try_send_error(&format!("No RTSP pull_url for device {}", device_tag));
                return Ok(());
            }
        };

        let username = device.device_username.clone().unwrap_or_default();
        let password = device.device_password.clone().unwrap_or_default();

        let node_id = MediaNodeId::new(MediaNodeType::ONVIF_BROADCAST_VOICE, device_tag, None);

        let transport = OnvifBackchannelTransport::new(rtsp_url, username, password);

        let node: Arc<dyn MediaNode> =
            Arc::new(BasicMediaNode::new(node_id.clone(), transport.clone()));

        node.add_send_chain(
            MediaNodeType::WebSocket,
            Box::new(crate::media::node::SendToReceiversMiddleware::new(
                Arc::downgrade(&node),
                MediaNodeType::WebSocket,
            )),
        );

        transport.set_node(node.clone());

        if let Err(e) = transport.connect().await {
            tracing::error!("[ONVIF] Backchannel transport connect failed: {}", e);
            try_send_error(&format!("Backchannel connect failed: {}", e));
            return Ok(());
        }

        if let Some(mgr) = get_media_session_manager() {
            let session = mgr.get_or_create(device_tag);
            if session.has_node_of_type(MediaNodeType::ONVIF_BROADCAST_VOICE) {
                return Ok(());
            }
            if let Err(e) = session.add_node(node.clone()) {
                tracing::error!("[ONVIF] Failed to add node to session: {}", e);
                try_send_error(&format!("Failed to add node: {}", e));
                return Ok(());
            }
        }

        let event = SignalEvent::AudioTalkReady {
            node_id: requester_node_id.clone(),
        };
        if let Err(e) = self.deps.event_publisher.publish(event).await {
            tracing::error!("[ONVIF] Failed to publish AudioTalkReady: {}", e);
        }

        Ok(())
    }

    async fn stop_audio_talk_internal(&mut self, node_id: MediaNodeId) -> Result<()> {
        let device_tag = &node_id.device_tag;

        let manager = match get_media_session_manager() {
            Some(m) => m,
            None => {
                tracing::warn!("[ONVIF] Media session manager not available");
                return Ok(());
            }
        };

        if let Some(session) = manager.get(device_tag) {
            let bc_node_id = session
                .nodes_iter()
                .find(|n| n.node_type() == MediaNodeType::ONVIF_BROADCAST_VOICE)
                .map(|n| n.id().clone());

            if let Some(bc_node_id) = bc_node_id {
                let _removed = session.remove_and_stop_node(&bc_node_id).await;
            } else {
                tracing::warn!("[ONVIF] No ONVIF_BROADCAST_VOICE node found in session for device {}", device_tag);
            }
        } else {
            tracing::warn!("[ONVIF] No media session found for device {}", device_tag);
        }

        Ok(())
    }
}
