use crate::protocol::onvif::soap::extract_element_text;
use crate::protocol::onvif::soap_client::OnvifSoapClient;

const DEVICE_WSDL: &str = "http://www.onvif.org/ver10/device/wsdl";
const MEDIA_WSDL: &str = "http://www.onvif.org/ver10/media/wsdl";

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
    soap: OnvifSoapClient,
}

impl OnvifDeviceClient {
    pub fn new(x_addr: &str) -> Self {
        Self {
            soap: OnvifSoapClient::new(x_addr, None, None),
        }
    }

    pub fn with_credentials(mut self, username: &str, password: &str) -> Self {
        self.soap = self.soap.with_credentials(username, password);
        self
    }

    pub fn with_soap_client(soap: OnvifSoapClient) -> Self {
        Self { soap }
    }

    async fn send_soap_internal(
        &mut self,
        action: &str,
        body: &str,
    ) -> anyhow::Result<String> {
        self.soap.post(Some(action), body).await
    }

    pub async fn get_device_info(&mut self) -> anyhow::Result<OnvifDeviceInfo> {
        let body = r#"<GetDeviceInformation xmlns="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let xml = self
            .send_soap_internal(&format!("{}/GetDeviceInformation", DEVICE_WSDL), body)
            .await?;

        let result = OnvifDeviceInfo {
            manufacturer: extract_element_text(xml.as_bytes(), "Manufacturer"),
            model: extract_element_text(xml.as_bytes(), "Model"),
            firmware_version: extract_element_text(xml.as_bytes(), "FirmwareVersion"),
            serial_number: extract_element_text(xml.as_bytes(), "SerialNumber"),
            hardware_id: extract_element_text(xml.as_bytes(), "HardwareId"),
        };
        Ok(result)
    }

    pub async fn get_audio_outputs(
        &mut self,
    ) -> anyhow::Result<Vec<crate::protocol::onvif::types::AudioOutput>> {
        let body = r#"<GetAudioOutputs xmlns="http://www.onvif.org/ver10/media/wsdl"/>"#;
        let xml = self
            .send_soap_internal(&format!("{}/GetAudioOutputs", MEDIA_WSDL), body)
            .await?;

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
                    tracing::warn!(
                        "[ONVIF] XML parsing error at position {}: {:?}",
                        reader.error_position(),
                        e
                    );
                    break;
                }
                _ => {}
            }
        }

        Ok(outputs)
    }

    pub async fn get_profiles(&mut self) -> anyhow::Result<Vec<OnvifProfile>> {
        let body = r#"<GetProfiles xmlns="http://www.onvif.org/ver10/media/wsdl"/>"#;
        let xml = self
            .send_soap_internal(&format!("{}/GetProfiles", MEDIA_WSDL), body)
            .await?;

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
                            if p.name.is_empty() && !in_video_encoder && !in_audio_encoder {}
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let Some(ref mut p) = current_profile {
                        let text = (&*e).to_string();
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            match current_element.as_str() {
                                "Name"
                                    if p.name.is_empty()
                                        && !in_video_encoder
                                        && !in_audio_encoder =>
                                {
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
        let xml = self
            .send_soap_internal(&format!("{}/GetStreamUri", MEDIA_WSDL), &body)
            .await?;

        let uri = extract_element_text(xml.as_bytes(), "Uri").unwrap_or_default();

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
        let xml = self
            .send_soap_internal(&format!("{}/GetCapabilities", DEVICE_WSDL), body)
            .await?;

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
        let xml = self
            .send_soap_internal(&format!("{}/GetServices", DEVICE_WSDL), body)
            .await?;
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
                if caps.media.is_some()
                    || caps.ptz.is_some()
                    || caps.events.is_some()
                    || caps.imaging.is_some()
                {
                    return Ok(caps);
                }
            }
        }
        self.get_capabilities().await
    }

    pub async fn get_all_stream_uris(
        &mut self,
    ) -> anyhow::Result<Vec<(OnvifProfile, OnvifStreamUri)>> {
        let profiles = self.get_profiles().await?;
        let mut results = Vec::new();
        for profile in profiles {
            match self.get_stream_uri(&profile.token).await {
                Ok(uri) => results.push((profile, uri)),
                Err(e) => tracing::warn!(
                    "[ONVIF] Failed to get stream URI for profile {}: {}",
                    profile.token,
                    e
                ),
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
                                        if en == "XAddr" {
                                            break;
                                        }
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
                        if depth == 0 {
                            in_category = false;
                        }
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
                                    if e.name().local_name().into_inner() == "XAddr" {
                                        break;
                                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::onvif::soap_client::AuthMethod;

    #[test]
    fn device_client_keeps_endpoint_and_credentials() {
        let client = OnvifDeviceClient::new("http://cam.local/onvif/device_service")
            .with_credentials("admin", "secret");
        assert_eq!(client.soap.endpoint(), "http://cam.local/onvif/device_service");
        assert_eq!(client.soap.auth_method(), AuthMethod::WsSecurityDigest);
    }

    #[test]
    fn anonymous_client_starts_without_credentials() {
        let client = OnvifDeviceClient::new("http://cam.local/onvif/device_service");
        assert_eq!(client.soap.auth_method(), AuthMethod::Anonymous);
    }
}
