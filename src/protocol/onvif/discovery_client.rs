use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};

const MULTICAST_ADDR: &str = "239.255.255.250";
const DISCOVERY_PORT: u16 = 3702;
const DISCOVER_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, serde::Serialize)]
pub struct OnvifDiscoveredDevice {
    pub x_addr: String,
    pub types: Vec<String>,
    pub scopes: Vec<String>,
    pub address: String,
    pub metadata_version: u32,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub firmware_version: Option<String>,
    pub serial_number: Option<String>,
    pub name: Option<String>,
}

pub struct OnvifDiscoveryClient;

impl OnvifDiscoveryClient {
    pub async fn discover() -> anyhow::Result<Vec<OnvifDiscoveredDevice>> {
        let bind_addr = SocketAddr::from((Ipv4Addr::new(0, 0, 0, 0), 0));
        let sock = UdpSocket::bind(bind_addr).await?;
        sock.join_multicast_v4(Ipv4Addr::from_str(MULTICAST_ADDR)?, Ipv4Addr::UNSPECIFIED)?;

        let probe = Self::build_probe();
        let multicast = format!("{}:{}", MULTICAST_ADDR, DISCOVERY_PORT);
        tracing::info!("[ONVIF Discovery] Sending multicast probe to {}:{}", MULTICAST_ADDR, DISCOVERY_PORT);
        sock.send_to(probe.as_bytes(), &multicast).await?;

        let mut buf = vec![0u8; 65536];
        let mut devices = Vec::new();
        let deadline = Duration::from_secs(DISCOVER_TIMEOUT_SECS);

        loop {
            match timeout(deadline, sock.recv_from(&mut buf)).await {
                Ok(Ok((len, addr))) => {
                    let xml = String::from_utf8_lossy(&buf[..len]);
                    tracing::info!("[ONVIF Discovery] RECEIVED from {}:\n{}", addr, xml);
                    if let Some(device) = Self::parse_probe_match(&xml) {
                        tracing::info!("[ONVIF Discovery] Found device: x_addr={}, address={}, types={:?}", 
                            device.x_addr, device.address, device.types);
                        if !devices.iter().any(|d: &OnvifDiscoveredDevice| d.x_addr == device.x_addr) {
                            devices.push(device);
                        }
                    }
                }
                Ok(Err(e)) => tracing::warn!("[ONVIF Discovery] recv error: {}", e),
                Err(_) => break,
            }
        }

        tracing::info!("[ONVIF Discovery] Multicast discovery finished, found {} devices", devices.len());

        // 获取设备详细信息
        for device in &mut devices {
            tracing::debug!("[ONVIF Discovery] Getting device info from: {}", device.x_addr);
            if let Ok(info) = crate::protocol::onvif::OnvifDeviceClient::new(&device.x_addr)
                .get_device_info().await
            {
                tracing::info!("[ONVIF Discovery] Device info: manufacturer={:?}, model={:?}, sn={:?}", 
                    info.manufacturer, info.model, info.serial_number);
                device.manufacturer = info.manufacturer.clone();
                device.model = info.model.clone();
                device.firmware_version = info.firmware_version.clone();
                device.serial_number = info.serial_number.clone();
                if let (Some(mfr), Some(mdl)) = (&info.manufacturer, &info.model) {
                    device.name = Some(format!("{} {}", mfr, mdl));
                }
            } else {
                tracing::warn!("[ONVIF Discovery] Failed to get device info from: {}", device.x_addr);
            }
        }

        Ok(devices)
    }

    fn build_probe() -> String {
        let msg_id = uuid::Uuid::new_v4().to_string();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:a="http://www.w3.org/2005/08/addressing" xmlns:wsd="http://schemas.xmlsoap.org/ws/2005/04/discovery">
<s:Header>
  <a:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</a:Action>
  <a:MessageID>urn:uuid:{}</a:MessageID>
  <a:To>urn:schemas-xmlsoap-org:ws:2005:04:discovery</a:To>
</s:Header>
<s:Body>
  <wsd:Probe><wsd:Types>dn:NetworkVideoTransmitter</wsd:Types></wsd:Probe>
</s:Body>
</s:Envelope>"#,
            msg_id
        )
    }

    fn parse_probe_match(xml: &str) -> Option<OnvifDiscoveredDevice> {
        use crate::protocol::onvif::soap::extract_element_text;
        let x_addrs_full = extract_element_text(xml.as_bytes(), "XAddrs")?;
        let x_addr = x_addrs_full.split_whitespace().next().unwrap_or(&x_addrs_full).to_string();
        let address = extract_element_text(xml.as_bytes(), "Address").unwrap_or_default();
        let types = extract_element_text(xml.as_bytes(), "Types")
            .map(|t| t.split_whitespace().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let scopes = extract_element_text(xml.as_bytes(), "Scopes")
            .map(|s| s.split_whitespace().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let metadata_version = extract_element_text(xml.as_bytes(), "MetadataVersion")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        Some(OnvifDiscoveredDevice {
            x_addr, types, scopes,
            address, metadata_version,
            manufacturer: None, model: None, firmware_version: None,
            serial_number: None, name: None,
        })
    }

    pub async fn probe_unicast(ip: &str, port: u16) -> anyhow::Result<Option<OnvifDiscoveredDevice>> {
        let bind_addr = SocketAddr::from((Ipv4Addr::new(0, 0, 0, 0), 0));
        let sock = UdpSocket::bind(bind_addr).await?;

        let probe = Self::build_probe();
        let target = format!("{}:{}", ip, port);
        tracing::info!("[ONVIF Discovery] Sending unicast probe to {}", target);
        sock.send_to(probe.as_bytes(), &target).await?;

        let mut buf = vec![0u8; 65536];
        let deadline = Duration::from_secs(5);
        match tokio::time::timeout(deadline, sock.recv_from(&mut buf)).await {
                Ok(Ok((len, addr))) => {
                    let xml = String::from_utf8_lossy(&buf[..len]);
                    tracing::info!("[ONVIF Discovery] RECEIVED from {}:\n{}", addr, xml);
                    let mut device = Self::parse_probe_match(&xml)
                    .ok_or_else(|| anyhow::anyhow!("Invalid ProbeMatch response"))?;
                tracing::info!("[ONVIF Discovery] Unicast found device: x_addr={}", device.x_addr);
                if let Ok(info) = crate::protocol::onvif::OnvifDeviceClient::new(&device.x_addr)
                    .get_device_info().await
                {
                    tracing::info!("[ONVIF Discovery] Device info: manufacturer={:?}, model={:?}", 
                        info.manufacturer, info.model);
                    device.manufacturer = info.manufacturer.clone();
                    device.model = info.model.clone();
                    device.firmware_version = info.firmware_version.clone();
                    device.serial_number = info.serial_number.clone();
                    if let (Some(mfr), Some(mdl)) = (&info.manufacturer, &info.model) {
                        device.name = Some(format!("{} {}", mfr, mdl));
                    }
                }
                Ok(Some(device))
            }
            Ok(Err(e)) => {
                tracing::error!("[ONVIF Discovery] UDP recv error: {}", e);
                Err(anyhow::anyhow!("UDP recv error: {}", e))
            }
            Err(_) => {
                tracing::info!("[ONVIF Discovery] Unicast probe timed out for {}:{}", ip, port);
                Ok(None)
            }
        }
    }
}