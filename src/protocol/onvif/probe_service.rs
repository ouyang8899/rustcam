use crate::protocol::onvif::{
    OnvifDiscoveryClient, OnvifDeviceClient, OnvifDiscoveredDevice,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProbeResult {
    pub host: String,
    pub port: u16,
    pub urn: Option<String>,
    pub x_addr: String,
    pub name: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub types: Vec<String>,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub has_ptz: bool,
    #[serde(default)]
    pub has_streaming: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CapabilitiesResult {
    pub device_info: DeviceInfo,
    pub capabilities: CapabilityUrls,
    pub profiles: Vec<ProfileInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceInfo {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub firmware_version: Option<String>,
    pub serial_number: Option<String>,
    pub hardware_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CapabilityUrls {
    pub media: Option<String>,
    pub ptz: Option<String>,
    pub events: Option<String>,
    pub imaging: Option<String>,
    #[serde(default)]
    pub has_audio: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileInfo {
    pub token: String,
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub encoding: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamResult {
    pub streams: Vec<StreamInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamInfo {
    pub token: String,
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub encoding: Option<String>,
    pub rtsp_url: String,
}

pub struct OnvifProbeService;

impl OnvifProbeService {
    pub async fn discover_multicast() -> anyhow::Result<Vec<ProbeResult>> {
        let devices = OnvifDiscoveryClient::discover().await?;
        Ok(devices.into_iter().map(|d| Self::to_probe_result(d)).collect())
    }

    pub async fn probe_unicast(ip: &str, port: u16) -> anyhow::Result<Option<ProbeResult>> {
        let device = OnvifDiscoveryClient::probe_unicast(ip, port).await?;
        Ok(device.map(|d| Self::to_probe_result(d)))
    }

    pub async fn get_capabilities(x_addr: &str, username: &str, password: &str) -> anyhow::Result<CapabilitiesResult> {
        tracing::debug!("[ONVIF Probe] get_capabilities: x_addr={}, username={}", x_addr, username);
        let mut client = OnvifDeviceClient::new(x_addr)
            .with_credentials(username, password);

        let device_info = client.get_device_info().await
            .map_err(|e| { tracing::error!("[ONVIF] get_device_info failed: x_addr={}, error={}", x_addr, e); e })?;
        tracing::debug!("[ONVIF Probe] get_device_info success: manufacturer={:?}, model={:?}", 
            device_info.manufacturer, device_info.model);
        
        let caps = client.get_capabilities_with_fallback().await
            .map_err(|e| { tracing::error!("[ONVIF] get_capabilities_with_fallback failed: x_addr={}, error={}", x_addr, e); e })?;
        tracing::debug!("[ONVIF Probe] get_capabilities_with_fallback success: media={:?}, ptz={:?}", 
            caps.media.is_some(), caps.ptz.is_some());
        
        let profiles = client.get_profiles().await
            .map_err(|e| { tracing::error!("[ONVIF] get_profiles failed: x_addr={}, error={}", x_addr, e); e })?;
        tracing::debug!("[ONVIF Probe] get_profiles success: count={}", profiles.len());

        let audio_outputs = match client.get_audio_outputs().await {
            Ok(outputs) => outputs,
            Err(e) => {
                tracing::warn!("[ONVIF] get_audio_outputs failed: x_addr={}, error={}", x_addr, e);
                Vec::new()
            }
        };
        let has_audio = !audio_outputs.is_empty();
        tracing::debug!("[ONVIF Probe] get_audio_outputs success: has_audio={}, count={}", has_audio, audio_outputs.len());

        Ok(CapabilitiesResult {
            device_info: DeviceInfo {
                manufacturer: device_info.manufacturer,
                model: device_info.model,
                firmware_version: device_info.firmware_version,
                serial_number: device_info.serial_number,
                hardware_id: device_info.hardware_id,
            },
            capabilities: CapabilityUrls {
                media: caps.media,
                ptz: caps.ptz,
                events: caps.events,
                imaging: caps.imaging,
                has_audio,
            },
            profiles: profiles.into_iter().map(|p| ProfileInfo {
                token: p.token,
                name: p.name,
                width: p.width,
                height: p.height,
                encoding: p.encoding,
            }).collect(),
        })
    }

    pub async fn check_online(x_addr: &str, username: &str, password: &str) -> bool {
        let mut client = OnvifDeviceClient::new(x_addr)
            .with_credentials(username, password);
        client.is_online().await
    }

    pub async fn get_stream_uris(
        media_x_addr: Option<&str>,
        x_addr: &str,
        username: &str,
        password: &str,
        profile_tokens: &[String],
    ) -> anyhow::Result<StreamResult> {
        let mut client = OnvifDeviceClient::new(x_addr)
            .with_credentials(username, password);

        let mut media_url: String;
        match client.get_services().await {
            Ok(services) => {
                media_url = services.iter()
                    .find(|s| s.namespace.to_lowercase().contains("media"))
                    .map(|s| s.xaddr.clone())
                    .unwrap_or_else(|| media_x_addr.map(String::from).unwrap_or_else(|| x_addr.to_string()));
                tracing::info!("[ONVIF Probe] get_stream_uris: using GetServices media_url={}", media_url);
            }
            Err(e) => {
                tracing::warn!("[ONVIF] get_services failed, using provided media_x_addr: {}", e);
                media_url = media_x_addr.map(String::from).unwrap_or_else(|| x_addr.to_string());
            }
        }

        tracing::debug!("[ONVIF Probe] get_stream_uris: url={}, username={}, tokens={:?}", media_url, username, profile_tokens);
        let mut media_client = OnvifDeviceClient::new(&media_url)
            .with_credentials(username, password);

        let profiles = media_client.get_profiles().await
            .map_err(|e| { tracing::error!("[ONVIF] get_profiles failed: url={}, error={}", media_url, e); e })?;
        tracing::debug!("[ONVIF Probe] get_profiles success: count={}", profiles.len());
        
        let mut streams = Vec::new();
        for token in profile_tokens {
            match media_client.get_stream_uri(token).await {
                Ok(uri) => {
                    let profile_info = profiles
                        .iter()
                        .find(|p| p.token == *token);

                    let profile_name = profile_info
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| token.clone());

                    tracing::info!("[ONVIF Probe] got stream URI: token={}, name={}, rtsp_url={}", token, profile_name, uri.uri);
                    streams.push(StreamInfo {
                        token: token.clone(),
                        name: profile_name,
                        width: profile_info.and_then(|p| p.width),
                        height: profile_info.and_then(|p| p.height),
                        encoding: profile_info.and_then(|p| p.encoding.clone()),
                        rtsp_url: uri.uri,
                    });
                }
                Err(e) => {
                    tracing::warn!("[ONVIF] Failed to get stream URI for {}: {}", token, e);
                }
            }
        }

        Ok(StreamResult { streams })
    }

    fn to_probe_result(d: OnvifDiscoveredDevice) -> ProbeResult {
        let has_ptz = d.types.iter().any(|t| t.contains("PTZ")) || d.scopes.iter().any(|s| s.contains("/PTZ"));
        let has_streaming = d.types.iter().any(|t| t.contains("Streaming")) || d.scopes.iter().any(|s| s.contains("/Streaming"));
        let (host, port) = Self::parse_x_addr(&d.x_addr);
        ProbeResult {
            host,
            port,
            urn: Some(d.address).filter(|s| !s.is_empty()),
            x_addr: d.x_addr,
            name: d.name.or_else(|| {
                d.manufacturer.as_ref().zip(d.model.as_ref())
                    .map(|(mfr, mdl)| format!("{} {}", mfr, mdl))
            }),
            manufacturer: d.manufacturer,
            model: d.model,
            types: d.types,
            scopes: d.scopes,
            has_ptz,
            has_streaming,
        }
    }

    fn parse_x_addr(x_addr: &str) -> (String, u16) {
        let is_https = x_addr.starts_with("https://");
        let url = x_addr.trim_start_matches("http://").trim_start_matches("https://");
        let parts: Vec<&str> = url.splitn(2, '/').collect();
        let host_part = parts.first().unwrap_or(&url);
        let mut segments = host_part.split(':');
        let host = segments.next().unwrap_or("").to_string();
        let default_port = if is_https { 443 } else { 80 };
        let port: u16 = segments.next().and_then(|p| p.parse().ok()).unwrap_or(default_port);
        (host, port)
    }
}
