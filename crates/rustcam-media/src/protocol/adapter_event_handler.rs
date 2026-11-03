use crate::protocol::event::{SignalEvent, TalkResponseStatus};
use crate::protocol::adapter_manager::get_adapter;
use crate::domain::traits::EventPublisher;
use std::sync::Arc;

pub async fn handle_adapter_event(event: SignalEvent, event_bus: Arc<dyn EventPublisher>) {
    let _event_type = event.event_type();
    
    match event {
        SignalEvent::StartAudioTalk { node_id } => {

            let device_tag = &node_id.device_tag;
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
            
            let adapter_key = device_tag.clone();
            let node_id_clone = node_id.clone();
            let event_bus_clone = event_bus.clone();
            tokio::spawn(async move {
                let mut guard = adapter_entry.lock().await;
                
                match guard.start_audio_talk_internal(&adapter_key, node_id_clone.clone()).await {
                    Ok(()) => {
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

            if let Some(adapter_entry) = get_adapter(&node_id.device_tag) {
                let adapter_key = node_id.device_tag.clone();
                tokio::spawn(async move {
                    let mut guard = adapter_entry.lock().await;
                    
                    if let Err(e) = guard.stop_audio_talk_internal(node_id.clone()).await {
                        tracing::error!("[AdapterEvent] Failed to stop audio talk for device {}: {}", adapter_key, e);
                    } else {
                    }
                });
            }
        }
        SignalEvent::AudioTalkReady { node_id } => {
            let resp = crate::protocol::event::SignalEvent::AudioTalkResponse {
                node_id: node_id.clone(),
                status: TalkResponseStatus::Ready,
                message: Some("可以对讲".to_string()),
            };
            let _ = event_bus.publish(resp).await;
        }
        SignalEvent::DeviceOffline { device_tag, reason, .. } => {
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
                    let _adapter_key = device_tag.clone();
                    let event_clone = event.clone();
                    let event_bus_clone = event_bus.clone();
                    tokio::spawn(async move {
                        let mut guard = adapter_entry.lock().await;
                        guard.handle_event(event_clone, event_bus_clone).await;
                    });
                }
            } else {
            }
        }
    }
}
