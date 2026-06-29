use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;
use chrono::Utc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use dashmap::DashMap;
use crate::context::{InfraContext, MediaContext};
use crate::domain::{Device, DeviceStatus, Protocol, CreateDeviceRequest, UpdateDeviceRequest, StreamState};
use crate::domain::traits::{CacheStoreExt, DeviceLookup};
use crate::domain::region::{Region, RegionNode};
use crate::domain::device_group::{DeviceGroup, DeviceGroupNode, CreateGroupRequest, UpdateGroupRequest};
use crate::domain::ptz::{PtzPreset, PtzControlResult, PtzStatus};
use crate::error::{AppError, Result};
use crate::protocol::event::{SignalEvent, ProtocolType, PtzCommand};
use crate::protocol::onvif::OnvifPtzService;
use crate::protocol::adapter_manager;
use crate::domain::{PullUrl, StreamProtocol};
use crate::application::StreamService;

pub struct DeviceService {
    infra: InfraContext,
    media: MediaContext,
    stream_service: Arc<StreamService>,
    shutdown_tx: Arc<RwLock<Option<tokio::sync::watch::Sender<()>>>>,
    ptz_last_cmd: Arc<DashMap<i64, Instant>>,
}

impl DeviceService {
    pub fn new(infra: InfraContext, media: MediaContext, stream_service: Arc<StreamService>) -> Self {
        Self {
            infra,
            media,
            stream_service,
            shutdown_tx: Arc::new(RwLock::new(None)),
            ptz_last_cmd: Arc::new(DashMap::new()),
        }
    }

    fn embed_rtsp_auth(rtsp_url: &str, username: &str, password: &str) -> String {
        if let Some(stripped) = rtsp_url.strip_prefix("rtsp://") {
            format!("rtsp://{}:{}@{}", username, password, stripped)
        } else {
            rtsp_url.to_string()
        }
    }

    pub async fn start(&self) {
        let (tx, mut rx) = tokio::sync::watch::channel(());
        *self.shutdown_tx.write().await = Some(tx);

        let infra = self.infra.clone();
        let media = self.media.clone();
        let ptz_last_cmd = self.ptz_last_cmd.clone();
        let mut event_rx = self.infra.subscribe_events();

        tokio::spawn(async move {
            tracing::info!("[DeviceService] Started event listener");
            loop {
                tokio::select! {
                    _ = rx.changed() => {
                        tracing::info!("[DeviceService] Shutting down");
                        break;
                    }
                    result = event_rx.recv() => {
                        match result {
                            Ok(event) => {
                                if let Err(e) = Self::handle_event(&infra, &media, &ptz_last_cmd, &event).await {
                                    tracing::error!("[DeviceService] Event handler error: {}", e);
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!("[DeviceService] Lagged {} events", n);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::info!("[DeviceService] Event channel closed");
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn stop(&self) {
        if let Some(tx) = self.shutdown_tx.read().await.as_ref() {
            let _ = tx.send(());
        }
    }

    async fn handle_event(
        infra: &InfraContext,
        media: &MediaContext,
        ptz_last_cmd: &Arc<DashMap<i64, Instant>>,
        event: &SignalEvent,
    ) -> anyhow::Result<()> {
        match event {
            SignalEvent::DeviceRegister { device_id, device_tag, name, manufacturer, model, protocol, stream_key } => {
                tracing::info!("[DeviceService] Device registered: {} via {:?}", name, protocol);

                let proto = match protocol {
                    ProtocolType::Gb28181 => Protocol::Gb28181,
                    ProtocolType::Onvif => Protocol::Onvif,
                    ProtocolType::Rtsp => Protocol::Rtsp,
                    ProtocolType::WebRtc => Protocol::Rtmp,
                    ProtocolType::Custom(_) => Protocol::Rtsp,
                };

                let existing = if let Some(ref tag) = device_tag {
                    infra.db.get_device_by_device_tag(tag).await
                } else if let Some(ref sk) = stream_key {
                    infra.db.get_device_by_stream_key(sk).await
                } else {
                    None
                };

                if let Some(mut device) = existing {
                    device.name = name.clone();
                    device.set_online();
                    if let Some(ref sk) = stream_key {
                        device.stream_key = Some(sk.clone());
                    }
                    infra.db.update_device(&device).await?;
                    infra.ws_broadcaster.device_online(device.id);
                } else {
                    let mut device = Device::new(name.clone(), proto);
                    device.device_tag = device_tag.clone();
                    device.stream_key = stream_key.clone().or_else(|| Some(Uuid::new_v4().to_string()));
                    device.set_online();
                    infra.db.create_device(&device).await?;
                    infra.ws_broadcaster.device_online(device.id);
                }

                let _ = infra.redis.publish_event("device_registered", &serde_json::json!({
                    "device_tag": device_tag, "name": name, "stream_key": stream_key,
                    "manufacturer": manufacturer, "model": model,
                    "protocol": protocol.to_string(),
                })).await;
            }

            SignalEvent::DeviceOnline { device_id, device_tag } => {
                let tag = device_tag.as_deref().unwrap_or("");
                if let Some(mut device) = infra.db.get_device_by_device_tag(tag).await {
                    device.set_online();
                    infra.db.update_device(&device).await?;
                    infra.ws_broadcaster.device_online(device.id);
                }
            }

            SignalEvent::DeviceOffline { device_id, device_tag, reason } => {
                let tag = device_tag.as_deref().unwrap_or("");
                if let Some(mut device) = infra.db.get_device_by_device_tag(tag).await {
                    device.set_offline();
                    infra.db.update_device(&device).await?;
                    infra.ws_broadcaster.device_offline(device.id, reason.as_deref());
                }
            }

            SignalEvent::DeviceKeepalive { device_id: _, device_tag, timestamp: _ } => {
                let tag = device_tag.as_deref().unwrap_or("");
                if let Some(mut device) = infra.db.get_device_by_device_tag(tag).await {
                    let was_offline = device.status == crate::domain::DeviceStatus::Offline;
                    device.set_online();
                    infra.db.update_device(&device).await?;
                    infra.ws_broadcaster.device_online(device.id);
                    if was_offline {
                        let _ = infra.publish_event(SignalEvent::DeviceOnline {
                            device_id: device.id,
                            device_tag: Some(tag.to_string()),
                        }).await;
                    }
                }
            }

            SignalEvent::CatalogResponse { device_id: _, device_tag, channels } => {
                let parent_tag = device_tag.as_deref().unwrap_or("");
                let device_count = channels.iter().filter(|ch| !ch.is_directory()).count();
                tracing::info!("[DeviceService] Catalog from {}: {} total, {} devices (excluding dirs)", parent_tag, channels.len(), device_count);

                for ch in channels {
                    if ch.is_directory() {
                        tracing::debug!("[DeviceService] Skipping directory: {}", ch.device_id);
                        continue;
                    }

                    let extended = serde_json::json!({
                        "manufacturer": ch.manufacturer,
                        "model": ch.model,
                    });

                    if let Some(mut device) = infra.db.get_device_by_device_tag(&ch.device_id).await {
                        device.name = ch.name.clone();
                        device.status = if ch.is_online() { DeviceStatus::Online } else { DeviceStatus::Offline };
                        device.extended = Some(extended);
                        if let Err(e) = infra.db.update_device(&device).await {
                            tracing::error!("[DeviceService] Update channel {} failed: {}", ch.device_id, e);
                        }
                    } else {
                        let mut device = Device::new(ch.name.clone(), Protocol::Gb28181);
                        device.device_tag = Some(ch.device_id.clone());
                        device.parent_device_tag = Some(parent_tag.to_string());
                        device.status = if ch.is_online() { DeviceStatus::Online } else { DeviceStatus::Offline };
                        device.stream_key = Some(Uuid::new_v4().to_string());
                        device.extended = Some(extended);
                        if let Err(e) = infra.db.create_device(&device).await {
                            tracing::error!("[DeviceService] Create channel {} failed: {}", ch.device_id, e);
                        }
                    }
                }
            }

            SignalEvent::Alarm { device_id, alarm_type, message, .. } => {
                tracing::warn!("[DeviceService] Alarm: device={} type={} msg={}", device_id, alarm_type, message);
                let _ = infra.redis.publish_event("alarm", &serde_json::json!({
                    "device_id": device_id, "alarm_type": alarm_type, "message": message,
                })).await;
                infra.ws_broadcaster.alarm(device_id.parse().unwrap_or(0), alarm_type, message);
            }

            SignalEvent::PtzControl { device_id, command, speed } => {
                Self::try_dispatch_ptz(infra, ptz_last_cmd, &device_id, &command, &speed).await;
            }
            _ => {}
        }
        Ok(())
    }

    async fn try_dispatch_ptz(
        infra: &InfraContext,
        ptz_last_cmd: &Arc<DashMap<i64, Instant>>,
        device_id: &str,
        command: &PtzCommand,
        speed: &Option<u8>,
    ) {
        let device_id_num: i64 = match device_id.parse() {
            Ok(id) => id,
            Err(_) => {
                tracing::warn!("[PTZ] Invalid device_id: {}", device_id);
                return;
            }
        };

        let device = match infra.db.get_device(device_id_num).await {
            Ok(Some(d)) => d,
            Ok(None) => {
                tracing::warn!("[PTZ] Device not found: {}", device_id);
                return;
            }
            Err(e) => {
                tracing::error!("[PTZ] DB error: {}", e);
                return;
            }
        };

        let is_continuous = matches!(
            command,
            PtzCommand::Up | PtzCommand::Down | PtzCommand::Left | PtzCommand::Right
                | PtzCommand::ZoomIn | PtzCommand::ZoomOut | PtzCommand::ContinuousMove { .. }
        );

        if is_continuous {
            let now = Instant::now();
            let last = ptz_last_cmd.get(&device_id_num);
            if let Some(entry) = last {
                if now.duration_since(*entry.value()) < std::time::Duration::from_millis(150) {
                    tracing::debug!("[PTZ] Dropped (debounce): {} {:?}", device_id, command);
                    return;
                }
            }
            ptz_last_cmd.insert(device_id_num, now);
        }

        let target_protocol = if let Some(ref parent_tag) = device.parent_device_tag {
            if let Some(parent) = infra.db.get_device_by_device_tag(parent_tag).await {
                if parent.protocol == Protocol::Onvif {
                    Protocol::Onvif
                } else {
                    device.protocol
                }
            } else {
                device.protocol
            }
        } else {
            device.protocol
        };

        match target_protocol {
            Protocol::Onvif => {
                if let Err(e) = Self::dispatch_onvif_ptz(infra, &device, command, speed).await {
                    tracing::error!("[PTZ] ONVIF failed: device={} err={}", device_id, e);
                }
            }
            Protocol::Gb28181 => {
                if let Err(e) = Self::dispatch_gb28181_ptz(infra, &device, command, speed).await {
                    tracing::error!("[PTZ] GB28181 failed: device={} err={}", device_id, e);
                }
            }
            _ => {
                tracing::warn!("[PTZ] Unsupported protocol {:?} for device {}", device.protocol, device_id);
            }
        }
    }

    async fn dispatch_onvif_ptz(
        infra: &InfraContext,
        device: &Device,
        command: &PtzCommand,
        speed: &Option<u8>,
    ) -> Result<Option<String>> {
        let (parent_tag, profile_token) = match (&device.parent_device_tag, &device.extended) {
            (Some(tag), Some(ext)) => {
                let token = ext.get("onvif_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Profile_1")
                    .to_string();
                (tag.clone(), token)
            }
            _ => {
                return Err(AppError::BadRequest(format!("ONVIF device {} has no parent/token for PTZ", device.id)));
            }
        };

        let parent = match infra.db.get_device_by_device_tag(&parent_tag).await {
            Some(p) => p,
            None => {
                return Err(AppError::NotFound(format!("ONVIF parent device {} not found", parent_tag)));
            }
        };

        let ptz_url = match &parent.extended {
            Some(ext) => ext.get("capabilities")
                .and_then(|c| c.get("ptz"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string()),
            None => None,
        };

        let ptz_url = match ptz_url {
            Some(url) if !url.is_empty() => url,
            _ => {
                return Err(AppError::NotFound(format!("No PTZ URL found for device {}", parent_tag)));
            }
        };

        let client = OnvifPtzService::new(
            ptz_url,
            parent.device_username.clone(),
            parent.device_password.clone(),
        ).into_client();

        match command {
            PtzCommand::ContinuousMove { pan, tilt, zoom } => {
                if pan.abs() < 0.01 && tilt.abs() < 0.01 && zoom.abs() < 0.01 {
                    client.with_profile(profile_token).stop().await?;
                } else {
                    client.with_profile(profile_token).continuous_move(*pan, *tilt, *zoom).await?;
                }
            }
            PtzCommand::Up => { let s = speed.as_ref().map(|v| *v as f64 / 100.0).unwrap_or(0.5); client.with_profile(profile_token).continuous_move(0.0, s, 0.0).await?; }
            PtzCommand::Down => { let s = speed.as_ref().map(|v| *v as f64 / 100.0).unwrap_or(0.5); client.with_profile(profile_token).continuous_move(0.0, -s, 0.0).await?; }
            PtzCommand::Left => { let s = speed.as_ref().map(|v| *v as f64 / 100.0).unwrap_or(0.5); client.with_profile(profile_token).continuous_move(-s, 0.0, 0.0).await?; }
            PtzCommand::Right => { let s = speed.as_ref().map(|v| *v as f64 / 100.0).unwrap_or(0.5); client.with_profile(profile_token).continuous_move(s, 0.0, 0.0).await?; }
            PtzCommand::ZoomIn => { let s = speed.as_ref().map(|v| *v as f64 / 100.0).unwrap_or(0.5); client.with_profile(profile_token).continuous_move(0.0, 0.0, s).await?; }
            PtzCommand::ZoomOut => { let s = speed.as_ref().map(|v| *v as f64 / 100.0).unwrap_or(0.5); client.with_profile(profile_token).continuous_move(0.0, 0.0, -s).await?; }
            PtzCommand::Stop => { client.with_profile(profile_token).stop().await?; }
            PtzCommand::AbsoluteMove { pan, tilt, zoom } => { client.with_profile(profile_token).absolute_move(*pan, *tilt, *zoom).await?; }
            PtzCommand::RelativeMove { pan, tilt, zoom } => { client.with_profile(profile_token).relative_move(*pan, *tilt, *zoom).await?; }
            PtzCommand::GotoPreset { preset_token } => { client.with_profile(profile_token).goto_preset(preset_token).await?; }
            PtzCommand::SetPreset { preset_name } => {
                let new_token = client.with_profile(profile_token).set_preset("", preset_name.as_deref()).await
                    .map_err(|e| AppError::Internal(format!("SetPreset failed: {}", e)))?;
                tracing::info!("[PTZ] ONVIF SetPreset: device={} token={}", device.id, new_token);
                return Ok(Some(new_token));
            }
            PtzCommand::RemovePreset { preset_token } => { client.with_profile(profile_token).remove_preset(preset_token).await?; }
            PtzCommand::FocusIn | PtzCommand::FocusOut => { return Err(AppError::BadRequest("Focus not supported for ONVIF".to_string())); }
        }
        tracing::debug!("[PTZ] ONVIF sent: device={} cmd={:?}", device.id, command);
        Ok(None)
    }

    async fn dispatch_gb28181_ptz(
        infra: &InfraContext,
        device: &Device,
        command: &PtzCommand,
        speed: &Option<u8>,
    ) -> Result<()> {
        let channel_id = match &device.device_tag {
            Some(tag) => tag.clone(),
            None => {
                return Err(AppError::BadRequest(format!("Device {} has no device_tag", device.id)));
            }
        };

        let adapter_key = if let Some(ref parent_tag) = device.parent_device_tag {
            parent_tag.clone()
        } else {
            channel_id.clone()
        };

        let adapter_arc = adapter_manager::get_adapter(&adapter_key)
            .ok_or_else(|| AppError::NotFound(format!("Device {} is offline", channel_id)))?;

        let mut inner = adapter_arc.lock().await;
        (&mut *inner).ptz_control(&channel_id, command, *speed).await?;
        tracing::debug!("[PTZ] GB28181 sent: device={} cmd={:?}", channel_id, command);
        Ok(())
    }

    pub async fn handle_ptz_control(
        &self,
        device_id: i64,
        command: PtzCommand,
        speed: Option<u8>,
    ) -> Result<PtzControlResult> {
        let device = self.infra.db.get_device(device_id).await?
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?;

        let target_protocol = if let Some(ref parent_tag) = device.parent_device_tag {
            if let Some(parent) = self.infra.db.get_device_by_device_tag(parent_tag).await {
                if parent.protocol == Protocol::Onvif {
                    Protocol::Onvif
                } else {
                    device.protocol
                }
            } else {
                device.protocol
            }
        } else {
            device.protocol
        };

        let preset_token = match target_protocol {
            Protocol::Onvif => {
                Self::dispatch_onvif_ptz(&self.infra, &device, &command, &speed).await?
            }
            Protocol::Gb28181 => {
                Self::dispatch_gb28181_ptz(&self.infra, &device, &command, &speed).await?;
                None
            }
            _ => {
                return Err(AppError::BadRequest(format!("Unsupported PTZ protocol: {:?}", device.protocol)));
            }
        };

        tracing::info!("[PTZ] Sent: device={} cmd={:?}", device_id, command);
        Ok(PtzControlResult {
            success: true,
            message: "PTZ command sent".to_string(),
            preset_token,
        })
    }

    pub async fn get_ptz_status(&self, device_id: i64) -> Result<PtzStatus> {
        let device = self.infra.db.get_device(device_id).await?
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?;

        let target_protocol = if let Some(ref parent_tag) = device.parent_device_tag {
            if let Some(parent) = self.infra.db.get_device_by_device_tag(parent_tag).await {
                if parent.protocol == Protocol::Onvif {
                    Protocol::Onvif
                } else {
                    device.protocol
                }
            } else {
                device.protocol
            }
        } else {
            device.protocol
        };

        match target_protocol {
            Protocol::Onvif => {
                let parent = if let Some(ref parent_tag) = device.parent_device_tag {
                    self.infra.db.get_device_by_device_tag(parent_tag).await
                        .ok_or_else(|| AppError::NotFound(format!("Parent device {} not found", parent_tag)))?
                } else {
                    device.clone()
                };

                let x_addr = parent.extended.as_ref()
                    .and_then(|e| e.get("x_addr"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AppError::NotFound("No ONVIF x_addr for device".to_string()))?;

                let profile_token = device.extended.as_ref()
                    .and_then(|e| e.get("onvif_token"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Profile_1")
                    .to_string();

                let mut ptz_client = crate::protocol::onvif::OnvifPtzService::new(
                    x_addr.to_string(),
                    parent.device_username.clone(),
                    parent.device_password.clone(),
                ).into_client();

                let status = ptz_client.with_profile(profile_token).get_status().await
                    .map_err(|e| AppError::Internal(format!("GetStatus failed: {}", e)))?;
                return Ok(status);
            }
            Protocol::Gb28181 => {
                return Ok(PtzStatus {
                    position_pan: Some(0.0),
                    position_tilt: Some(0.0),
                    position_zoom: Some(1.0),
                    moving: false,
                });
            }
            _ => {
                return Err(AppError::BadRequest(format!("PTZ status not supported for {:?}", device.protocol)));
            }
        }
    }

    pub async fn create_onvif_devices(
        &self,
        urn: Option<&str>,
        x_addr: &str,
        name: Option<&str>,
        manufacturer: Option<&str>,
        model: Option<&str>,
        username: &str,
        password: &str,
        capabilities: &crate::api::http::handlers::onvif_handler::OnvifCapabilityUrls,
        types: &[String],
        scopes: &[String],
        streams: &[crate::api::http::handlers::onvif_handler::OnvifStreamToCreate],
    ) -> Result<serde_json::Value> {
        let host = x_addr
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or(x_addr)
            .split(':')
            .next()
            .unwrap_or("")
            .to_string();

        let port: u16 = x_addr
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or(x_addr)
            .split(':')
            .nth(1)
            .and_then(|p| p.split('/').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(80);

        let uuid = Uuid::new_v4().to_string();
        let device_tag = match urn {
            Some(u) if !u.is_empty() => format!("{}_{}", u, uuid),
            _ => format!("urn:uuid:{}", uuid),
        };

        let parent_name = name
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                manufacturer.map(|m| {
                    model.map(|md| format!("{} {}", m, md)).unwrap_or_else(|| m.to_string())
                }).unwrap_or_else(|| "ONVIF Device".to_string())
            });

        let mut parent = Device::new(parent_name.clone(), Protocol::Onvif);
        parent.host = host.clone();
        parent.port = port;
        parent.device_username = Some(username.to_string());
        parent.device_password = Some(password.to_string());
        parent.device_tag = Some(device_tag.clone());
        parent.extended = Some(serde_json::json!({
            "onvif_urn": urn,
            "x_addr": x_addr,
            "manufacturer": manufacturer,
            "model": model,
            "types": types,
            "scopes": scopes,
            "capabilities": {
                "media": capabilities.media,
                "ptz": capabilities.ptz,
                "events": capabilities.events,
                "imaging": capabilities.imaging,
            },
        }));

        if self.infra.db.get_device_by_device_tag(&device_tag).await.is_some() {
            return Err(AppError::BadRequest(format!(
                "ONVIF device with URN '{}' already exists", device_tag
            )));
        }

        self.infra.db.create_device(&parent).await?;
        tracing::info!("[DeviceService] Created ONVIF parent device: id={}, name={}", parent.id, parent.name);

        let mut child_results = Vec::new();
        child_results.push(serde_json::json!({
            "id": parent.id,
            "name": parent.name,
            "token": null,
            "stream_key": null,
            "is_parent": true,
        }));

        for (i, stream) in streams.iter().enumerate() {
            let stream_key = Uuid::new_v4().to_string();
            let child_tag = format!("{}_ch{}", device_tag, i);
            let mut child = Device::new(stream.name.clone(), Protocol::Rtsp);
            child.host = host.clone();
            child.port = port;
            child.device_tag = Some(child_tag.clone());
            child.parent_device_tag = Some(device_tag.clone());
            child.device_username = parent.device_username.clone();
            child.device_password = parent.device_password.clone();
            child.extended = Some(serde_json::json!({
                "onvif_token": stream.token,
                "parent_device_tag": device_tag,
            }));
            child.stream_key = Some(stream_key.clone());

            let rtsp_url_with_auth = if let (Some(u), Some(p)) = (&parent.device_username, &parent.device_password) {
                Self::embed_rtsp_auth(&stream.rtsp_url, u, p)
            } else {
                stream.rtsp_url.clone()
            };

            child.pull_urls = vec![PullUrl {
                protocol: StreamProtocol::Rtsp,
                url: rtsp_url_with_auth,
                priority: 1,
            }];

            self.infra.db.create_device(&child).await?;
            tracing::info!("[DeviceService] Created ONVIF child device: id={}, name={}", child.id, child.name);

            child_results.push(serde_json::json!({
                "id": child.id,
                "name": child.name,
                "token": stream.token,
                "stream_key": stream_key,
                "is_parent": false,
            }));
        }

        Ok(serde_json::json!({
            "devices": child_results,
            "parent_id": parent.id,
        }))
    }

    // ── Device CRUD ──

    pub async fn create_device(&self, req: CreateDeviceRequest) -> Result<Device> {
        if req.name.trim().is_empty() {
            return Err(AppError::BadRequest("Device name cannot be empty".to_string()));
        }
        if req.name.len() > 255 {
            return Err(AppError::BadRequest("Device name cannot exceed 255 characters".to_string()));
        }

        if let Some(ref tag) = req.device_tag {
            if self.infra.db.get_device_by_device_tag(tag).await.is_some() {
                return Err(AppError::BadRequest(format!("device_tag '{}' already exists", tag)));
            }
        }

        let stream_key = Uuid::new_v4().to_string();

        let mut device = Device::new(req.name.clone(), req.protocol);
        device.host = req.host.clone().unwrap_or_default();
        device.port = req.port.unwrap_or(0);
        device.device_username = req.device_username.clone();
        device.device_password = req.device_password.clone();
        device.push_urls = req.push_urls.clone().unwrap_or_default();
        device.pull_urls = req.pull_urls.clone().unwrap_or_default();
        device.playback_username = req.playback_username.clone();
        device.playback_password = req.playback_password.clone();
        device.media_server_tag = req.media_server_tag.clone();
        device.stream_key = Some(stream_key);
        device.app = req.app.clone();
        device.device_tag = req.device_tag.clone();
        device.region_code = req.region_code.clone();
        device.is_public = req.is_public.unwrap_or(false);

        // 处理 extended 字段
        let mut extended = req.extended.clone().unwrap_or(serde_json::Value::Object(Default::default()));
        device.extended = Some(extended);

        self.infra.db.create_device(&device).await?;
        tracing::info!("[DeviceService] Created device: id={}, name={}", device.id, device.name);
        Ok(device)
    }

    pub async fn get_device(&self, id: i64) -> Result<Option<Device>> {
        self.infra.db.get_device(id).await
    }

    pub async fn get_device_by_device_tag(&self, device_tag: &str) -> Result<Option<Device>> {
        Ok(self.infra.db.get_device_by_device_tag(device_tag).await)
    }

    pub async fn update_device(&self, id: i64, req: UpdateDeviceRequest) -> Result<Device> {
        let mut device = self.infra.db.get_device(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", id)))?;

        if let Some(name) = req.name { device.name = name; }
        if let Some(host) = req.host { device.host = host; }
        if let Some(port) = req.port { device.port = port; }
        if let Some(v) = req.device_username { device.device_username = Some(v); }
        if let Some(v) = req.device_password { device.device_password = Some(v); }
        if let Some(v) = req.push_urls { device.push_urls = v; }
        if let Some(v) = req.pull_urls { device.pull_urls = v; }
        if let Some(v) = req.playback_username { device.playback_username = Some(v); }
        if let Some(v) = req.playback_password { device.playback_password = Some(v); }
        if let Some(v) = req.media_server_tag { device.media_server_tag = Some(v); }
        if let Some(v) = req.stream_key { device.stream_key = Some(v); }
        if let Some(v) = req.app { device.app = Some(v); }
        if let Some(v) = req.device_tag { device.device_tag = Some(v); }
        if let Some(v) = req.parent_device_tag { device.parent_device_tag = Some(v); }
        if let Some(v) = req.region_code { device.region_code = Some(v); }
        if let Some(v) = req.group_id { device.group_id = Some(v); }
        if let Some(v) = req.is_public { device.is_public = v; }
        
        let status_changed_to_maintaining = req.status.map(|s| s == DeviceStatus::Maintaining).unwrap_or(false);
        if let Some(v) = req.status { device.status = v; }

        // 处理 extended 字段 - 合并请求中的 extended
        if let Some(req_extended) = req.extended {
            let mut existing = device.extended.clone().unwrap_or(serde_json::Value::Object(Default::default()));
            if let Some(existing_obj) = existing.as_object_mut() {
                if let Some(req_obj) = req_extended.as_object() {
                    for (k, v) in req_obj {
                        existing_obj.insert(k.clone(), v.clone());
                    }
                }
            }
            device.extended = Some(existing);
        }

        self.infra.db.update_device(&device).await?;

        // 如果设备进入维护状态，停止关联的流
        if status_changed_to_maintaining {
            tracing::info!("[DeviceService] Device {} entered Maintaining status, stopping associated streams", id);
            if let Err(e) = self.stream_service.stop_streams_by_device(id).await {
                tracing::warn!("[DeviceService] Failed to stop streams for device {} in maintenance: {}", id, e);
            }
        }

        Ok(device)
    }

    pub async fn sync_device_status_from_stream(&self, device_id: i64, stream_state: StreamState) -> Result<()> {
        let mut device = match self.infra.db.get_device(device_id).await? {
            Some(d) => d,
            None => return Ok(()),
        };

        if device.status == DeviceStatus::Maintaining {
            tracing::debug!("[DeviceService] Device {} is in Maintaining status, skipping sync", device_id);
            return Ok(());
        }

        let new_status = match stream_state {
            StreamState::Active | StreamState::Starting | StreamState::Recovering => DeviceStatus::Online,
            StreamState::Error => DeviceStatus::Error,
            StreamState::Idle | StreamState::Stopping | StreamState::Stopped => DeviceStatus::Offline,
        };

        if device.status != new_status {
            tracing::info!(
                "[DeviceService] Syncing device {} status: {} -> {} (stream state: {:?})",
                device_id, device.status, new_status, stream_state
            );
            device.status = new_status;
            self.infra.db.update_device(&device).await?;
        }

        Ok(())
    }

    pub async fn delete_device(&self, id: i64) -> Result<()> {
        let device = self.infra.db.get_device(id).await?
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", id)))?;

        if let Some(ref parent_tag) = device.device_tag {
            let children = self.infra.db.get_children_by_parent_tag(parent_tag).await?;
            for child in children {
                self.infra.db.delete_streams_by_device(child.id).await?;
                self.infra.db.delete_device(child.id).await?;
                tracing::info!("[DeviceService] Cascading delete child device: id={}", child.id);
            }
        }

        self.infra.db.delete_streams_by_device(id).await?;
        self.infra.db.delete_device(id).await
    }

    pub async fn list_devices(&self) -> Vec<Device> {
        self.infra.db.list_devices().await
    }

    pub async fn list_online_devices(&self) -> Vec<Device> {
        self.infra.db.list_devices().await
            .into_iter()
            .filter(|d| d.status == DeviceStatus::Online)
            .collect()
    }

    pub async fn list_public_devices(&self) -> Vec<Device> {
        self.infra.db.list_devices().await
            .into_iter()
            .filter(|d| d.is_public)
            .collect()
    }

    pub async fn list_devices_paginated(&self, limit: usize, offset: usize, search: Option<&str>) -> Vec<Device> {
        self.infra.db.list_devices_paginated(limit, offset, search).await
    }

    pub async fn count_devices_filtered(&self, search: Option<&str>) -> usize {
        self.infra.db.count_devices_filtered(search).await
    }

    pub async fn count_devices(&self) -> usize {
        self.infra.db.count_devices().await
    }

    pub async fn count_devices_top_level(&self) -> usize {
        self.infra.db.count_devices_top_level().await
    }

    pub fn get_stats(&self) -> serde_json::Value {
        let cache = self.infra.db.devices_cache();
        let total = cache.len();
        let online = cache.iter().filter(|d| d.status == DeviceStatus::Online).count();
        let public = cache.iter().filter(|d| d.is_public && d.status == DeviceStatus::Online).count();

        serde_json::json!({
            "total": total,
            "online": online,
            "offline": total - online,
            "public": public
        })
    }

    pub async fn list_groups(&self) -> Vec<DeviceGroup> {
        self.infra.db.list_groups().await
    }

    pub async fn create_group(&self, req: CreateGroupRequest) -> Result<i64> {
        self.infra.db.create_group(&req).await
    }

    pub async fn update_group(&self, id: i64, req: UpdateGroupRequest) -> Result<()> {
        self.infra.db.update_group(id, &req).await
    }

    pub async fn delete_group(&self, id: i64) -> Result<()> {
        self.infra.db.delete_group(id).await
    }

    pub async fn get_group_tree(&self) -> Vec<DeviceGroupNode> {
        self.infra.db.get_group_tree().await
    }

    pub async fn assign_device_group(&self, device_id: i64, group_id: Option<i64>) -> Result<()> {
        self.infra.db.update_device_group(device_id, group_id).await
    }

    pub async fn list_regions(&self) -> Vec<Region> {
        self.infra.db.list_regions().await
    }

    pub async fn list_region_children(&self, parent_code: &str) -> Vec<Region> {
        self.infra.db.list_region_children(parent_code).await
    }

    pub async fn get_region_tree(&self) -> Vec<RegionNode> {
        self.infra.db.get_region_tree().await
    }

    pub async fn log_ptz_control(
        &self, user_id: Option<Uuid>, device_id: i64, command: &str,
        speed: u8, result: bool, error_message: Option<String>, call_id: Option<String>,
    ) -> Result<()> {
        self.infra.db.log_ptz_control(
            user_id.map(|u| u.to_string()), device_id, command,
            speed as f64, result, error_message, call_id,
        ).await
    }

    pub async fn log_ptz_result(
        &self, device_id: i64, call_id: Option<&str>,
        sip_code: Option<u16>, status: &str, message: Option<String>,
    ) -> Result<()> {
        self.infra.db.log_ptz_result(device_id, call_id, sip_code, status, message).await
    }

    pub fn broadcast_ptz_result(
        &self, device_id: i64, call_id: &str, command: &str,
        status: &str, sip_code: Option<u16>, message: Option<&str>,
    ) {
        self.infra.ws_broadcaster.ptz_result(device_id, call_id, command, status, sip_code, message);
    }

    pub async fn list_ptz_presets(&self, device_id: i64) -> Result<Vec<PtzPreset>> {
        let device = self.infra.db.get_device(device_id).await?
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?;

        let mut presets = self.infra.db.list_ptz_presets(device_id).await?;

        let target_protocol = if let Some(ref parent_tag) = device.parent_device_tag {
            if let Some(parent) = self.infra.db.get_device_by_device_tag(parent_tag).await {
                if parent.protocol == Protocol::Onvif {
                    Protocol::Onvif
                } else {
                    device.protocol
                }
            } else {
                device.protocol
            }
        } else {
            device.protocol
        };

        if matches!(target_protocol, Protocol::Onvif) {
            let parent = if let Some(ref parent_tag) = device.parent_device_tag {
                self.infra.db.get_device_by_device_tag(parent_tag).await
            } else {
                Some(device.clone())
            };

            if let Some(parent) = parent {
                if let Some(x_addr) = parent.extended.as_ref()
                    .and_then(|e| e.get("x_addr"))
                    .and_then(|v| v.as_str())
                {
                    let profile_token = device.extended.as_ref()
                        .and_then(|e| e.get("onvif_token"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Profile_1")
                        .to_string();

                    let mut ptz_client = crate::protocol::onvif::OnvifPtzService::new(
                        x_addr.to_string(),
                        parent.device_username.clone(),
                        parent.device_password.clone(),
                    ).into_client();

                    if let Ok(camera_presets) = ptz_client.with_profile(profile_token).get_presets().await {
                        let db_tokens: std::collections::HashSet<_> = presets.iter()
                            .map(|p| p.token.clone())
                            .collect();

                        for (token, name) in camera_presets {
                            if !db_tokens.contains(&token) {
                                presets.push(PtzPreset::new(device_id, name.unwrap_or_else(|| format!("Preset {}", token)), token.clone()));
                            }
                        }
                    }
                }
            }
        }

        Ok(presets)
    }

    pub async fn create_ptz_preset(&self, device_id: i64, name: &str) -> Result<PtzPreset> {
        let device = self.infra.db.get_device(device_id).await?
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?;

        let target_protocol = if let Some(ref parent_tag) = device.parent_device_tag {
            if let Some(parent) = self.infra.db.get_device_by_device_tag(parent_tag).await {
                if parent.protocol == Protocol::Onvif {
                    Protocol::Onvif
                } else {
                    device.protocol
                }
            } else {
                device.protocol
            }
        } else {
            device.protocol
        };

        let token = if matches!(target_protocol, Protocol::Onvif) {
            let parent = if let Some(ref parent_tag) = device.parent_device_tag {
                self.infra.db.get_device_by_device_tag(parent_tag).await
            } else {
                Some(device.clone())
            };

            if let Some(parent) = parent {
                if let Some(x_addr) = parent.extended.as_ref()
                    .and_then(|e| e.get("x_addr"))
                    .and_then(|v| v.as_str())
                {
                    let profile_token = device.extended.as_ref()
                        .and_then(|e| e.get("onvif_token"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Profile_1")
                        .to_string();

                    let mut ptz_client = crate::protocol::onvif::OnvifPtzService::new(
                        x_addr.to_string(),
                        parent.device_username.clone(),
                        parent.device_password.clone(),
                    ).into_client();

                    ptz_client.with_profile(profile_token).set_preset("", Some(name)).await
                        .map_err(|e| AppError::Internal(format!("SetPreset failed: {}", e)))?
                } else {
                    uuid::Uuid::new_v4().to_string()
                }
            } else {
                uuid::Uuid::new_v4().to_string()
            }
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        let preset = self.infra.db.create_ptz_preset_with_token(device_id, name, &token).await?;
        tracing::info!("[PTZ] Preset created: device={} name={} token={}", device_id, name, token);
        Ok(preset)
    }

    pub async fn create_ptz_preset_with_token(&self, device_id: i64, name: &str, token: &str) -> Result<PtzPreset> {
        self.infra.db.create_ptz_preset_with_token(device_id, name, token).await
    }

    pub async fn rename_ptz_preset(&self, device_id: i64, token: &str, new_name: &str) -> Result<PtzPreset> {
        let device = self.infra.db.get_device(device_id).await?
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?;

        let target_protocol = if let Some(ref parent_tag) = device.parent_device_tag {
            if let Some(parent) = self.infra.db.get_device_by_device_tag(parent_tag).await {
                if parent.protocol == Protocol::Onvif {
                    Protocol::Onvif
                } else {
                    device.protocol
                }
            } else {
                device.protocol
            }
        } else {
            device.protocol
        };

        if matches!(target_protocol, Protocol::Onvif) {
            let parent = if let Some(ref parent_tag) = device.parent_device_tag {
                self.infra.db.get_device_by_device_tag(parent_tag).await
            } else {
                Some(device.clone())
            };

            if let Some(parent) = parent {
                if let Some(x_addr) = parent.extended.as_ref()
                    .and_then(|e| e.get("x_addr"))
                    .and_then(|v| v.as_str())
                {
                    let profile_token = device.extended.as_ref()
                        .and_then(|e| e.get("onvif_token"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Profile_1")
                        .to_string();

                    let mut ptz_client = crate::protocol::onvif::OnvifPtzService::new(
                        x_addr.to_string(),
                        parent.device_username.clone(),
                        parent.device_password.clone(),
                    ).into_client();

                    ptz_client.with_profile(profile_token).set_preset(token, Some(new_name)).await
                        .map_err(|e| AppError::Internal(format!("SetPreset rename failed: {}", e)))?;
                }
            }
        }

        self.infra.db.update_ptz_preset(device_id, token, new_name).await?;
        let presets = self.list_ptz_presets(device_id).await?;
        presets.into_iter()
            .find(|p| p.token == token)
            .ok_or_else(|| AppError::Internal("Preset not found after rename".to_string()))
    }

    pub async fn delete_ptz_preset(&self, device_id: i64, token: &str) -> Result<()> {
        let device = self.infra.db.get_device(device_id).await?
            .ok_or_else(|| AppError::NotFound(format!("Device {} not found", device_id)))?;

        let target_protocol = if let Some(ref parent_tag) = device.parent_device_tag {
            if let Some(parent) = self.infra.db.get_device_by_device_tag(parent_tag).await {
                if parent.protocol == Protocol::Onvif {
                    Protocol::Onvif
                } else {
                    device.protocol
                }
            } else {
                device.protocol
            }
        } else {
            device.protocol
        };

        if matches!(target_protocol, Protocol::Onvif) {
            let parent = if let Some(ref parent_tag) = device.parent_device_tag {
                self.infra.db.get_device_by_device_tag(parent_tag).await
            } else {
                Some(device.clone())
            };

            if let Some(parent) = parent {
                if let Some(x_addr) = parent.extended.as_ref()
                    .and_then(|e| e.get("x_addr"))
                    .and_then(|v| v.as_str())
                {
                    let profile_token = device.extended.as_ref()
                        .and_then(|e| e.get("onvif_token"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Profile_1")
                        .to_string();

                    let mut ptz_client = crate::protocol::onvif::OnvifPtzService::new(
                        x_addr.to_string(),
                        parent.device_username.clone(),
                        parent.device_password.clone(),
                    ).into_client();

                    if let Err(e) = ptz_client.with_profile(profile_token).remove_preset(token).await {
                        tracing::warn!("[PTZ] RemovePreset failed (continuing to delete from DB): {}", e);
                    }
                }
            }
        }

        self.infra.db.delete_ptz_preset(device_id, token).await?;
        tracing::info!("[PTZ] Preset deleted: device={} token={}", device_id, token);
        Ok(())
    }
}

#[async_trait]
impl DeviceLookup for DeviceService {
    async fn find_by_tag(&self, tag: &str) -> Option<Device> {
        self.infra.db.get_device_by_device_tag(tag).await
    }

    async fn find_by_stream_key(&self, stream_key: &str) -> Option<Device> {
        self.infra.db.get_device_by_stream_key(stream_key).await
    }

    async fn find_by_protocol_and_host(&self, protocol: &Protocol, host: &str) -> Option<Device> {
        self.infra.db.get_device_by_protocol_and_host(protocol, host).await
    }

    async fn get_device(&self, id: i64) -> Result<Option<Device>> {
        self.infra.db.get_device(id).await
    }

    async fn set_online(&self, tag: &str) -> Result<()> {
        if let Some(mut device) = self.infra.db.get_device_by_device_tag(tag).await {
            device.set_online();
            self.infra.db.update_device(&device).await?;
            self.infra.ws_broadcaster.device_online(device.id);
        }
        Ok(())
    }

    async fn set_offline(&self, tag: &str, reason: Option<&str>) -> Result<()> {
        if let Some(mut device) = self.infra.db.get_device_by_device_tag(tag).await {
            device.set_offline();
            self.infra.db.update_device(&device).await?;
            self.infra.ws_broadcaster.device_offline(device.id, reason);
        }
        Ok(())
    }

    async fn log_ptz_control(
        &self, user_id: Option<Uuid>, device_id: i64, command: &str,
        speed: u8, result: bool, error_message: Option<String>, call_id: Option<String>,
    ) -> Result<()> {
        self.infra.db.log_ptz_control(
            user_id.map(|u| u.to_string()), device_id, command,
            speed as f64, result, error_message, call_id,
        ).await
    }

    async fn log_ptz_result(
        &self, device_id: i64, call_id: Option<&str>,
        sip_code: Option<u16>, status: &str, message: Option<String>,
    ) -> Result<()> {
        self.infra.db.log_ptz_result(device_id, call_id, sip_code, status, message).await
    }

    fn broadcast_ptz_result(
        &self, device_id: i64, call_id: &str, command: &str,
        status: &str, sip_code: Option<u16>, message: Option<&str>,
    ) {
        self.infra.ws_broadcaster.ptz_result(device_id, call_id, command, status, sip_code, message);
    }

    fn get_stats(&self) -> serde_json::Value {
        self.get_stats()
    }

    async fn list_online_devices(&self) -> Vec<Device> {
        self.list_online_devices().await
    }
}

impl Clone for DeviceService {
    fn clone(&self) -> Self {
        Self {
            infra: self.infra.clone(),
            media: self.media.clone(),
            stream_service: self.stream_service.clone(),
            shutdown_tx: Arc::new(RwLock::new(None)),
            ptz_last_cmd: self.ptz_last_cmd.clone(),
        }
    }
}