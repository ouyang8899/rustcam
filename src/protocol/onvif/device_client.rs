use crate::protocol::onvif::auth::UsernameToken;
use crate::protocol::onvif::soap::extract_element_text;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum AuthMethod {
    Anonymous,
    HttpDigest,
    WsSecurityDigest,
    WsSecurityText,
}

#[derive(Debug, Clone, Default)]
pub struct OnvifDeviceInfo {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub firmware_version: Option<String>,
    pub serial_number: Option<String>,
    pub hardware_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OnvifProfile {
    pub token: String,
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub encoding: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OnvifStreamUri {
    pub uri: String,
    pub invalid_after_connect: bool,
    pub invalid_after_reboot: bool,
    pub timeout: String,
}

#[derive(Debug, Clone, Default)]
pub struct OnvifCapabilities {
    pub media: Option<String>,
    pub ptz: Option<String>,
    pub events: Option<String>,
    pub imaging: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OnvifServiceEndpoint {
    pub namespace: String,
    pub xaddr: String,
}

pub struct OnvifDeviceClient {
    x_addr: String,
    username: Option<String>,
    password: Option<String>,
    auth_method: Option<AuthMethod>,
}

impl OnvifDeviceClient {
    pub fn new(x_addr: &str) -> Self {
        Self {
            x_addr: x_addr.to_string(),
            username: None,
            password: None,
            auth_method: None,
        }
    }

    pub fn with_credentials(mut self, username: &str, password: &str) -> Self {
        self.username = Some(username.to_string());
        self.password = Some(password.to_string());
        self
    }

    fn build_soap_envelope_no_auth(&self, body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
<s:Body>{}</s:Body>
</s:Envelope>"#,
            body
        )
    }

    fn build_soap_envelope(&self, body: &str) -> String {
        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            let (nonce, created, digest) = UsernameToken::build_digest(u, p);
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">
<s:Header>
  <wsse:Security s:mustUnderstand="true">
    <wsse:UsernameToken>
      <wsse:Username>{}</wsse:Username>
      <wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{}</wsse:Password>
      <wsse:Nonce EncodingType="http://docs.oasis-open.org/wss/200401-wss-soap-message-security-1.0#Base64Binary">{}</wsse:Nonce>
      <wsu:Created>{}</wsu:Created>
    </wsse:UsernameToken>
  </wsse:Security>
</s:Header>
<s:Body>{}</s:Body>
</s:Envelope>"#,
                u, digest, nonce, created, body
            )
        } else {
            self.build_soap_envelope_no_auth(body)
        }
    }

    fn build_soap_envelope_text(&self, body: &str) -> String {
        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" xmlns:wsu="http://docs.oasis-open.org/wss/200401-wss-wssecurity-utility-1.0.xsd">
<s:Header>
  <wsse:Security s:mustUnderstand="true">
    <wsse:UsernameToken>
      <wsse:Username>{}</wsse:Username>
      <wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText">{}</wsse:Password>
    </wsse:UsernameToken>
  </wsse:Security>
</s:Header>
<s:Body>{}</s:Body>
</s:Envelope>"#,
                u, p, body
            )
        } else {
            self.build_soap_envelope_no_auth(body)
        }
    }

    fn extract_digest_param(header: &str, param: &str) -> Option<String> {
        let search = format!("{}=\"", param);
        let start = header.find(&search)? + search.len();
        let rest = &header[start..];
        let end = rest.find('"').unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }

    fn build_http_digest_authorization(&self, method: &str, uri: &str, realm: &str, nonce: &str) -> String {
        use base64::{engine::general_purpose::STANDARD, Engine};

        let username = self.username.as_deref().unwrap_or("");
        let password = self.password.as_deref().unwrap_or("");

        let cnonce_bytes: [u8; 16] = rand::random();
        let cnonce = STANDARD.encode(cnonce_bytes);

        let ha1 = format!("{}:{}:{}", username, realm, password);
        let ha2 = format!("{}:{}", method, uri);

        let ha1_md5 = format!("{:x}", md5::compute(ha1.as_bytes()));
        let ha2_md5 = format!("{:x}", md5::compute(ha2.as_bytes()));

        let response = format!("{}:{}:00000001:{}:auth:{}", ha1_md5, nonce, cnonce, ha2_md5);
        let response_md5 = format!("{:x}", md5::compute(response.as_bytes()));

        let auth_header = format!(
            r#"Digest username="{}", realm="{}", nonce="{}", uri="{}", qop=auth, nc=00000001, cnonce="{}", response="{}""#,
            username, realm, nonce, uri, cnonce, response_md5
        );

        auth_header
    }

    async fn discover_auth(&mut self) -> anyhow::Result<AuthMethod> {
        let body = r#"<GetDeviceInformation xmlns="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let envelope = self.build_soap_envelope_no_auth(body);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let resp = client
            .post(&self.x_addr)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await?;

        let status = resp.status();

        if status == reqwest::StatusCode::OK {
            return Ok(AuthMethod::Anonymous);
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(www_auth) = resp.headers().get("www-authenticate") {
                if let Ok(www_auth_str) = www_auth.to_str() {
                    if www_auth_str.contains("Digest") {
                        return Ok(AuthMethod::HttpDigest);
                    }
                }
            }
        }

        if status == reqwest::StatusCode::BAD_REQUEST {
            let body = resp.text().await?;
            if body.contains("NotAuthorized") || body.contains("ter:NotAuthorized") {
                return Ok(AuthMethod::WsSecurityDigest);
            }
        }

        tracing::warn!("[ONVIF Auth] Unknown auth method, status={}, trying WS-Security", status);
        Ok(AuthMethod::WsSecurityDigest)
    }

    async fn send_soap_internal(&mut self, body: &str) -> anyhow::Result<String> {
        if self.auth_method.is_none() {
            self.auth_method = Some(self.discover_auth().await?);
        }

        let mut current_auth_method = self.auth_method.clone().unwrap_or(AuthMethod::WsSecurityDigest);
        let _retry_with_text = false;

        loop {
            match current_auth_method {
                AuthMethod::Anonymous => {
                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(10))
                        .build()?;
                    let resp = client
                        .post(&self.x_addr)
                        .header("Content-Type", "application/soap+xml; charset=utf-8")
                        .body(body.to_string())
                        .send()
                        .await?;
                    return self.handle_response(resp).await;
                }

                AuthMethod::HttpDigest => {
                    let envelope = self.build_soap_envelope_no_auth(body);

                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(10))
                        .build()?;
                    let resp = client
                        .post(&self.x_addr)
                        .header("Content-Type", "application/soap+xml; charset=utf-8")
                        .body(envelope.clone())
                        .send()
                        .await?;
                    let status = resp.status();

                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        let headers = resp.headers().clone();
                        let resp_text = resp.text().await?;
                        if let Some(www_auth) = headers.get("www-authenticate") {
                            if let Ok(www_auth_str) = www_auth.to_str() {
                                let realm = Self::extract_digest_param(www_auth_str, "realm")
                                    .unwrap_or_else(|| "IP Camera".to_string());
                                let nonce = Self::extract_digest_param(www_auth_str, "nonce")
                                    .unwrap_or_default();

                                let uri = if let Some(path) = self.x_addr.split("://").nth(1) {
                                    format!("/{}", path.split('/').skip(1).collect::<Vec<_>>().join("/"))
                                } else {
                                    "/onvif/device_service".to_string()
                                };
                                let auth_header = self.build_http_digest_authorization("POST", &uri, &realm, &nonce);

                                let client2 = reqwest::Client::builder()
                                    .timeout(Duration::from_secs(10))
                                    .build()?;
                                let resp = client2
                                    .post(&self.x_addr)
                                    .header("Content-Type", "application/soap+xml; charset=utf-8")
                                    .header("Authorization", &auth_header)
                                    .body(envelope)
                                    .send()
                                    .await?;

                                return self.handle_response(resp).await;
                            }
                        }
                        return Err(anyhow::anyhow!("HTTP Digest auth failed: {}", resp_text));
                    }

                    return self.handle_response(resp).await;
                }

                AuthMethod::WsSecurityDigest => {
                    let envelope = self.build_soap_envelope(body);

                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(10))
                        .build()?;
                    let resp = client
                        .post(&self.x_addr)
                        .header("Content-Type", "application/soap+xml; charset=utf-8")
                        .body(envelope)
                        .send()
                        .await?;

                    let status = resp.status();
                    if status == reqwest::StatusCode::BAD_REQUEST {
                        let _error_body = resp.text().await?;
                        tracing::warn!("[ONVIF Auth] WS-Security Digest failed (400), trying PasswordText");
                        current_auth_method = AuthMethod::WsSecurityText;
                        continue;
                    }
                    return self.handle_response(resp).await;
                }

                AuthMethod::WsSecurityText => {
                    let envelope = self.build_soap_envelope_text(body);

                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(10))
                        .build()?;
                    let resp = client
                        .post(&self.x_addr)
                        .header("Content-Type", "application/soap+xml; charset=utf-8")
                        .body(envelope)
                        .send()
                        .await?;

                    return self.handle_response(resp).await;
                }
            }
        }
    }

    async fn handle_response(&self, resp: reqwest::Response) -> anyhow::Result<String> {
        let status = resp.status();

        if !status.is_success() {
            let err_body = resp.text().await?;
            let err = format!("HTTP {} from {}:\n{}", status, self.x_addr, err_body);
            tracing::error!("[ONVIF SOAP] <<< ERROR: {}", err);
            return Err(anyhow::anyhow!(err));
        }

        let response_text = resp.text().await?;
        Ok(response_text)
    }

    pub async fn get_device_info(&mut self) -> anyhow::Result<OnvifDeviceInfo> {
        let body = r#"<GetDeviceInformation xmlns="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let xml = self.send_soap_internal(body).await?;

        let result = OnvifDeviceInfo {
            manufacturer: extract_element_text(xml.as_bytes(), "Manufacturer"),
            model: extract_element_text(xml.as_bytes(), "Model"),
            firmware_version: extract_element_text(xml.as_bytes(), "FirmwareVersion"),
            serial_number: extract_element_text(xml.as_bytes(), "SerialNumber"),
            hardware_id: extract_element_text(xml.as_bytes(), "HardwareId"),
        };
        Ok(result)
    }

    pub async fn get_audio_outputs(&mut self) -> anyhow::Result<Vec<crate::protocol::onvif::types::AudioOutput>> {
        let body = r#"<GetAudioOutputs xmlns="http://www.onvif.org/ver10/media/wsdl"/>"#;
        let xml = self.send_soap_internal(body).await?;

        let bytes = xml.as_bytes();
        let mut reader = quick_xml::Reader::from_reader(bytes);
        reader.config_mut().trim_text(true);

        let mut outputs = Vec::new();
        let mut current_output: Option<crate::protocol::onvif::types::AudioOutput> = None;
        let mut current_element = String::new();

        loop {
            use quick_xml::events::Event;
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    current_element = e.name().local_name().into_inner().to_string();
                    let name = &current_element;

                    if name == "AudioOutput" {
                        let mut token = None;
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner();
                            if key == "token" {
                                token = Some(attr.value.to_string());
                                break;
                            }
                        }
                        if let Some(t) = token {
                            current_output = Some(crate::protocol::onvif::types::AudioOutput {
                                token: t,
                                name: None,
                            });
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let Some(ref mut output) = current_output {
                        let text = (&*e).to_string();
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            match current_element.as_str() {
                                "Name" => {
                                    output.name = Some(trimmed.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name().local_name().into_inner().to_string();
                    if name == "AudioOutput" {
                        if let Some(output) = current_output.take() {
                            outputs.push(output);
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    tracing::warn!("[ONVIF] XML parsing error at position {}: {:?}", reader.error_position(), e);
                    break;
                }
                _ => {}
            }
        }

        Ok(outputs)
    }

    pub async fn get_profiles(&mut self) -> anyhow::Result<Vec<OnvifProfile>> {
        let body = r#"<GetProfiles xmlns="http://www.onvif.org/ver10/media/wsdl"/>"#;
        let xml = self.send_soap_internal(body).await?;

        let bytes = xml.as_bytes();
        let mut reader = quick_xml::Reader::from_reader(bytes);
        reader.config_mut().trim_text(true);

        let mut profiles = Vec::new();
        let mut current_profile: Option<OnvifProfile> = None;
        let mut in_video_encoder = false;
        let mut in_audio_encoder = false;
        let mut in_resolution = false;
        let mut current_element = String::new();

        loop {
            use quick_xml::events::Event;
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    current_element = e.name().local_name().into_inner().to_string();
                    let name = &current_element;

                    if name == "Profiles" {
                        let mut token = None;
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner();
                            if key == "token" {
                                token = Some(attr.value.to_string());
                                break;
                            }
                        }
                        if let Some(t) = token {
                            current_profile = Some(OnvifProfile {
                                token: t,
                                name: String::new(),
                                width: None,
                                height: None,
                                encoding: None,
                            });
                        }
                    } else if name == "VideoEncoderConfiguration" {
                        in_video_encoder = true;
                    } else if name == "AudioEncoderConfiguration" {
                        in_audio_encoder = true;
                    } else if name == "Resolution" {
                        in_resolution = true;
                    } else if name == "Name" {
                        if let Some(ref mut p) = current_profile {
                            if p.name.is_empty() && !in_video_encoder && !in_audio_encoder {

                            }
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let Some(ref mut p) = current_profile {
                        let text = (&*e).to_string();
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            match current_element.as_str() {
                                "Name" if p.name.is_empty() && !in_video_encoder && !in_audio_encoder => {
                                    p.name = trimmed.to_string();
                                }
                                "Encoding" if in_video_encoder && !in_resolution => {
                                    p.encoding = Some(trimmed.to_string());
                                }
                                "Width" if in_resolution => {
                                    if let Ok(w) = trimmed.parse() {
                                        p.width = Some(w);
                                    }
                                }
                                "Height" if in_resolution => {
                                    if let Ok(h) = trimmed.parse() {
                                        p.height = Some(h);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name().local_name().into_inner();

                    if name == "Profiles" {
                        if let Some(p) = current_profile.take() {
                            profiles.push(p);
                        }
                    } else if name == "VideoEncoderConfiguration" {
                        in_video_encoder = false;
                    } else if name == "AudioEncoderConfiguration" {
                        in_audio_encoder = false;
                    } else if name == "Resolution" {
                        in_resolution = false;
                    }
                    current_element.clear();
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    tracing::warn!("[ONVIF] XML parsing error: {:?}", e);
                    break;
                }
                _ => {}
            }
        }

        for _p in &profiles {
        }
        Ok(profiles)
    }

    pub async fn get_stream_uri(&mut self, profile_token: &str) -> anyhow::Result<OnvifStreamUri> {
        let body = format!(
            r#"<GetStreamUri xmlns="http://www.onvif.org/ver10/media/wsdl">
  <StreamSetup>
    <Stream xmlns="http://www.onvif.org/ver10/schema">RTP-Unicast</Stream>
    <Transport xmlns="http://www.onvif.org/ver10/schema">
      <Protocol>RTSP</Protocol>
    </Transport>
  </StreamSetup>
  <ProfileToken>{}</ProfileToken>
</GetStreamUri>"#,
            profile_token
        );
        let xml = self.send_soap_internal(&body).await?;

        let uri = extract_element_text(xml.as_bytes(), "Uri")
            .unwrap_or_default();

        Ok(OnvifStreamUri {
            uri,
            invalid_after_connect: false,
            invalid_after_reboot: false,
            timeout: String::new(),
        })
    }

    pub async fn get_first_rtsp_url(&mut self) -> anyhow::Result<String> {
        let profiles = self.get_profiles().await?;
        if profiles.is_empty() {
            return Err(anyhow::anyhow!("No profiles found"));
        }
        let stream_uri = self.get_stream_uri(&profiles[0].token).await?;
        Ok(stream_uri.uri)
    }

    pub async fn get_capabilities(&mut self) -> anyhow::Result<OnvifCapabilities> {
        let body = r#"<GetCapabilities xmlns="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let xml = self.send_soap_internal(body).await?;

        let caps = OnvifCapabilities {
            media: Self::extract_capability_url(&xml, "Media"),
            ptz: Self::extract_capability_url(&xml, "PTZ"),
            events: Self::extract_capability_url(&xml, "Events"),
            imaging: Self::extract_capability_url(&xml, "Imaging"),
        };

        Ok(caps)
    }

    pub async fn get_services(&mut self) -> anyhow::Result<Vec<OnvifServiceEndpoint>> {
        let body = r#"<GetServices xmlns="http://www.onvif.org/ver10/device/wsdl"><IncludeCapability>false</IncludeCapability></GetServices>"#;
        let xml = self.send_soap_internal(body).await?;
        let result = Self::parse_service_endpoints(&xml);
        Ok(result)
    }

    pub async fn get_capabilities_with_fallback(&mut self) -> anyhow::Result<OnvifCapabilities> {
        if let Ok(services) = self.get_services().await {
            if !services.is_empty() {
                let mut caps = OnvifCapabilities::default();
                for svc in &services {
                    let ns = svc.namespace.to_lowercase();
                    let url = &svc.xaddr;
                    if ns.contains("media") && caps.media.is_none() {
                        caps.media = Some(url.clone());
                    } else if ns.contains("ptz") && caps.ptz.is_none() {
                        caps.ptz = Some(url.clone());
                    } else if ns.contains("events") && caps.events.is_none() {
                        caps.events = Some(url.clone());
                    } else if ns.contains("imaging") && caps.imaging.is_none() {
                        caps.imaging = Some(url.clone());
                    }
                }
                if caps.media.is_some() || caps.ptz.is_some() || caps.events.is_some() || caps.imaging.is_some() {
                    return Ok(caps);
                }
            }
        }
        self.get_capabilities().await
    }

    pub async fn get_all_stream_uris(&mut self) -> anyhow::Result<Vec<(OnvifProfile, OnvifStreamUri)>> {
        let profiles = self.get_profiles().await?;
        let mut results = Vec::new();
        for profile in profiles {
            match self.get_stream_uri(&profile.token).await {
                Ok(uri) => results.push((profile, uri)),
                Err(e) => tracing::warn!("[ONVIF] Failed to get stream URI for profile {}: {}", profile.token, e),
            }
        }
        Ok(results)
    }

    pub async fn is_online(&mut self) -> bool {
        self.get_device_info().await.is_ok()
    }

    fn extract_capability_url(xml: &str, category: &str) -> Option<String> {
        let bytes = xml.as_bytes();
        let mut reader = quick_xml::Reader::from_reader(bytes);
        reader.config_mut().trim_text(true);
        let mut in_category = false;
        let mut depth = 0;

        loop {
            use quick_xml::events::Event;
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let name = e.name().local_name().into_inner();
                    if name == category {
                        in_category = true;
                        depth = 1;
                        continue;
                    }
                    if in_category {
                        depth += 1;
                        if name == "XAddr" {
                            let mut text = String::new();
                            loop {
                                match reader.read_event() {
                                    Ok(Event::Text(ref e)) => text.push_str(&*e),
                                    Ok(Event::End(ref e)) => {
                                        let en = e.name().local_name().into_inner();
                                        if en == "XAddr" { break; }
                                    }
                                    Ok(Event::Eof) | Err(_) => break,
                                    _ => {}
                                }
                            }
                            if !text.trim().is_empty() {
                                return Some(text.trim().to_string());
                            }
                        }
                    }
                }
                Ok(Event::End(_)) => {
                    if in_category {
                        depth -= 1;
                        if depth == 0 { in_category = false; }
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        None
    }

    fn parse_service_endpoints(xml: &str) -> Vec<OnvifServiceEndpoint> {
        let mut endpoints = Vec::new();
        let bytes = xml.as_bytes();
        let mut reader = quick_xml::Reader::from_reader(bytes);
        reader.config_mut().trim_text(true);
        let mut current_ns = String::new();
        let mut current_xaddr = String::new();
        let mut in_service = false;

        loop {
            use quick_xml::events::Event;
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let name = e.name().local_name().into_inner();
                    if name == "Service" {
                        in_service = true;
                        current_ns.clear();
                        current_xaddr.clear();
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner();
                            if key == "Namespace" {
                                current_ns = attr.value.to_string();
                            }
                        }
                    }
                    if in_service && name == "XAddr" {
                        let mut text = String::new();
                        loop {
                            match reader.read_event() {
                                Ok(Event::Text(ref e)) => text.push_str(&*e),
                                Ok(Event::End(ref e)) => {
                                    if e.name().local_name().into_inner() == "XAddr" { break; }
                                }
                                Ok(Event::Eof) | Err(_) => break,
                                _ => {}
                            }
                        }
                        current_xaddr = text.trim().to_string();
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name().local_name().into_inner();
                    if name == "Service" && in_service {
                        if !current_xaddr.is_empty() {
                            endpoints.push(OnvifServiceEndpoint {
                                namespace: current_ns.clone(),
                                xaddr: current_xaddr.clone(),
                            });
                        }
                        in_service = false;
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        endpoints
    }
}
