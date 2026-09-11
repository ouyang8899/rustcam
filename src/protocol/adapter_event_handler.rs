use crate::protocol::event::{SignalEvent, TalkResponseStatus};
use crate::protocol::adapter_manager::get_adapter;
use crate::domain::traits::EventPublisher;
use std::sync::Arc;

pub async fn handle_adapter_event(event: SignalEvent, event_bus: Arc<dyn EventPublisher>) {
    let event_type = event.event_type();
    
    match event {
        SignalEvent::StartAudioTalk { device_tag, connection_id } => {
            tracing::info!("[AdapterEvent] StartAudioTalk for device {} conn {}", device_tag, connection_id);

            let Some(adapter_entry) = get_adapter(&device_tag) else {
                tracing::warn!("[AdapterEvent] No adapter found for device {}", device_tag);
                let resp = SignalEvent::AudioTalkResponse {
                    device_tag: device_tag.clone(),
                    connection_id,
                    status: TalkResponseStatus::Error,
                    message: Some("设备未注册".to_string()),
                };
                let _ = event_bus.publish(resp).await;
                return;
            };
            
            let adapter_key = device_tag.clone();
            let event_bus_clone = event_bus.clone();
            tokio::spawn(async move {
                let mut guard = adapter_entry.lock().await;
                
                match guard.start_audio_talk_internal(&adapter_key, connection_id).await {
                    Ok(()) => {
                        tracing::info!("[AdapterEvent] Audio talk started for device {}", adapter_key);
                    }
                    Err(e) => {
                        tracing::error!("[AdapterEvent] Failed to start audio talk for device {}: {}", adapter_key, e);
                        let resp = SignalEvent::AudioTalkResponse {
                            device_tag: adapter_key.clone(),
                            connection_id,
                            status: TalkResponseStatus::Error,
                            message: Some(format!("启动失败: {}", e)),
                        };
                        let _ = event_bus_clone.publish(resp).await;
                    }
                }
            });
        }
        SignalEvent::StopAudioTalk { device_tag, connection_id } => {
            tracing::info!("[AdapterEvent] StopAudioTalk for device {} conn {}", device_tag, connection_id);

            if let Some(adapter_entry) = get_adapter(&device_tag) {
                let adapter_key = device_tag.clone();
                tokio::spawn(async move {
                    let mut guard = adapter_entry.lock().await;
                    
                    if let Err(e) = guard.stop_audio_talk_internal(connection_id).await {
                        tracing::error!("[AdapterEvent] Failed to stop audio talk for device {}: {}", adapter_key, e);
                    } else {
                        tracing::info!("[AdapterEvent] Audio talk stopped for device {}", adapter_key);
                    }
                });
            }
        }
        SignalEvent::DeviceOffline { device_tag, reason, .. } => {
            tracing::info!("[AdapterEvent] DeviceOffline for {} reason {:?}", device_tag.as_deref().unwrap_or("unknown"), reason);
            if let Some(tag) = device_tag {
                if let Some(adapter_entry) = get_adapter(&tag) {
                    let adapter_key = tag.clone();
                    tokio::spawn(async move {
                        let mut guard = adapter_entry.lock().await;
                        guard.handle_event(
                            SignalEvent::DeviceOffline { 
                                device_id: 0, 
                                device_tag: Some(adapter_key.clone()), 
                                reason 
                            },
                            event_bus,
                        ).await;
                    });
                }
            }
        }
        _ => {
            if let Some(device_tag) = event.device_tag() {
                if let Some(adapter_entry) = get_adapter(&device_tag) {
                    let adapter_key = device_tag.clone();
                    let event_clone = event.clone();
                    let event_bus_clone = event_bus.clone();
                    tokio::spawn(async move {
                        let mut guard = adapter_entry.lock().await;
                        guard.handle_event(event_clone, event_bus_clone).await;
                    });
                }
            } else {
                tracing::debug!("[AdapterEvent] Event {} has no device_tag, skipping", event_type);
            }
        }
    }
}
