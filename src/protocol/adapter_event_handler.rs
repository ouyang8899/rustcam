use crate::protocol::event::{SignalEvent, TalkResponseStatus};
use crate::protocol::adapter_manager::get_adapter;
use crate::domain::traits::EventPublisher;
use std::sync::Arc;

pub async fn handle_adapter_event(event: SignalEvent, event_bus: Arc<dyn EventPublisher>) {
    let event_type = event.event_type();
    tracing::info!("[AdapterEvent] handle_adapter_event called: event_type={}", event_type);
    
    match event {
        SignalEvent::StartAudioTalk { node_id } => {
            tracing::info!("[AdapterEvent] StartAudioTalk matched: node_id={}", node_id.id);

            let device_tag = &node_id.device_tag;
            tracing::info!("[AdapterEvent] Looking up adapter for device={}", device_tag);
            let Some(adapter_entry) = get_adapter(device_tag) else {
                tracing::warn!("[AdapterEvent] No adapter found for device {}", device_tag);
                let resp = SignalEvent::AudioTalkResponse {
                    node_id: node_id.clone(),
                    status: TalkResponseStatus::Error,
                    message: Some("设备未注册".to_string()),
                };
                let _ = event_bus.publish(resp).await;
                return;
            };
            
            tracing::info!("[AdapterEvent] Adapter found, spawning task to call start_audio_talk_internal");
            let adapter_key = device_tag.clone();
            let node_id_clone = node_id.clone();
            let event_bus_clone = event_bus.clone();
            tokio::spawn(async move {
                tracing::info!("[AdapterEvent] Task started, acquiring adapter lock");
                let mut guard = adapter_entry.lock().await;
                tracing::info!("[AdapterEvent] Lock acquired, calling start_audio_talk_internal");
                
                match guard.start_audio_talk_internal(&adapter_key, node_id_clone.clone()).await {
                    Ok(()) => {
                        tracing::info!("[AdapterEvent] start_audio_talk_internal completed OK for device {}", adapter_key);
                    }
                    Err(e) => {
                        tracing::error!("[AdapterEvent] start_audio_talk_internal failed for device {}: {}", adapter_key, e);
                        let resp = SignalEvent::AudioTalkResponse {
                            node_id: node_id_clone.clone(),
                            status: TalkResponseStatus::Error,
                            message: Some(format!("启动失败: {}", e)),
                        };
                        let _ = event_bus_clone.publish(resp).await;
                    }
                }
            });
        }
        SignalEvent::StopAudioTalk { node_id } => {
            tracing::info!("[AdapterEvent] StopAudioTalk for node_id={}", node_id.id);

            if let Some(adapter_entry) = get_adapter(&node_id.device_tag) {
                let adapter_key = node_id.device_tag.clone();
                tokio::spawn(async move {
                    let mut guard = adapter_entry.lock().await;
                    
                    if let Err(e) = guard.stop_audio_talk_internal(node_id.clone()).await {
                        tracing::error!("[AdapterEvent] Failed to stop audio talk for device {}: {}", adapter_key, e);
                    } else {
                        tracing::info!("[AdapterEvent] Audio talk stopped for device {}", adapter_key);
                    }
                });
            }
        }
        SignalEvent::AudioTalkReady { node_id } => {
            tracing::info!("[AdapterEvent] AudioTalkReady for node_id={}", node_id.id);
            let resp = crate::protocol::event::SignalEvent::AudioTalkResponse {
                node_id: node_id.clone(),
                status: TalkResponseStatus::Ready,
                message: Some("可以对讲".to_string()),
            };
            let _ = event_bus.publish(resp).await;
            tracing::info!("[AdapterEvent] Sent AudioTalkResponse::Ready for node_id={}", node_id.id);
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
