use crate::api::FullState;
use crate::application::ws_broadcaster::WsBroadcaster;
use crate::domain::device::Protocol;

use crate::auth::middleware::CurrentUser;
use crate::media::{
    AudioCodec, BasicMediaNode, LinearPcmToG711Middleware, MediaNode, MediaNodeId,
    MediaNodeType, SendToReceiversMiddleware, WebSocketMediaNodeTransport,
};
use crate::protocol::adapter_manager::{ self };

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{ConnectInfo, Extension, Path, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::{ BytesMut };
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{ broadcast, mpsc };
use uuid::Uuid;

pub struct WsTicketStore {
    ttl: Duration,
    tickets: DashMap<String, WsTicketData>,
}

struct WsTicketData {
    user_id: Uuid,
    sid: Uuid,
    expires_at: Instant,
}

pub struct WsTicket {
    pub user_id: Uuid,
    pub sid: Uuid,
}

impl WsTicketStore {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_seconds),
            tickets: DashMap::new(),
        }
    }

    pub fn issue(&self, user_id: Uuid, sid: Uuid) -> String {
        let ticket = Uuid::new_v4().to_string();
        self.tickets.insert(
            ticket.clone(),
            WsTicketData {
                user_id,
                sid,
                expires_at: Instant::now() + self.ttl,
            },
        );
        ticket
    }

    fn redeem(&self, ticket: &str) -> Option<WsTicket> {
        let (_, data) = self.tickets.remove(ticket)?;
        if data.expires_at < Instant::now() {
            return None;
        }
        Some(WsTicket {
            user_id: data.user_id,
            sid: data.sid,
        })
    }
}

impl axum::extract::FromRequestParts<Arc<FullState>> for WsTicket {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &Arc<FullState>,
    ) -> Result<Self, Self::Rejection> {
        let ticket = parts
            .headers
            .get("x-ws-ticket")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing ws ticket").into_response())?;

        let store = parts
            .extensions
            .get::<Arc<WsTicketStore>>()
            .cloned()
            .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "ws ticket store missing").into_response())?;

        store
            .redeem(ticket)
            .ok_or_else(|| (StatusCode::FORBIDDEN, "invalid ws ticket").into_response())
    }
}

fn extract_ws_ticket(headers: &HeaderMap) -> Option<(String, bool)> {
    if let Some(v) = headers.get("x-ws-ticket").and_then(|v| v.to_str().ok()) {
        let v = v.trim();
        if !v.is_empty() {
            return Some((v.to_string(), false));
        }
    }
    headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| (v, true))
}

fn extract_ws_ticket_from_query(uri: &axum::http::Uri) -> Option<String> {
    let query = uri.query()?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");
        if key == "ticket" {
            let decoded = percent_decode(value);
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
    }
    None
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub async fn issue_ws_ticket(
    State(_state): State<Arc<FullState>>,
    Extension(tickets): Extension<Arc<WsTicketStore>>,
    Extension(current_user): Extension<CurrentUser>,
) -> axum::Json<crate::api::response::ApiResponse<serde_json::Value>> {
    let ticket = tickets.issue(current_user.user.id, current_user.sid);
    axum::Json(crate::api::response::ApiResponse::success(
        serde_json::json!({ "ticket": ticket }),
    ))
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Extension(tickets): Extension<Arc<WsTicketStore>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<FullState>>,
) -> Response {
    let (ticket_text, via_protocol) = match extract_ws_ticket(&headers) {
        Some(v) => v,
        None => match extract_ws_ticket_from_query(&uri) {
            Some(v) => (v, false),
            None => return (StatusCode::UNAUTHORIZED, "missing ws ticket").into_response(),
        },
    };
    let ticket = match tickets.redeem(&ticket_text) {
        Some(t) => t,
        None => return (StatusCode::FORBIDDEN, "invalid ws ticket").into_response(),
    };

    match state.auth.is_session_active(ticket.sid).await {
        Ok(true) => {}
        _ => return (StatusCode::UNAUTHORIZED, "session revoked").into_response(),
    }

    let ws = if via_protocol {
        ws.protocols([ticket_text])
    } else {
        ws
    };

    let broadcaster = state.app.registry.ws_broadcaster.clone();
    let revoker = state.auth.revoker.subscribe();
    tracing::info!(
        "[WebSocket] client connected addr={} user={}",
        addr,
        ticket.user_id
    );

    ws.on_upgrade(move |socket| handle_socket(socket, broadcaster, ticket.sid, revoker))
}

async fn handle_socket(
    socket: WebSocket,
    broadcaster: Arc<WsBroadcaster>,
    sid: Uuid,
    mut revoker: broadcast::Receiver<Uuid>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut broadcast_rx = broadcaster.subscribe();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(_text))) => {
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("[WebSocket] Error: {}", e);
                        break;
                    }
                    None => {
                        break;
                    }
                    _ => {}
                }
            }

            broadcast_msg = broadcast_rx.recv() => {
                match broadcast_msg {
                    Ok(msg) => {
                        let text = serde_json::to_string(&msg).unwrap_or_default();
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            tracing::warn!("[WebSocket] Failed to send message, closing");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("[WebSocket] Lagged {} messages", n);
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            revoke = revoker.recv() => {
                match revoke {
                    Ok(revoked) if revoked == sid => {
                        let _ = sender
                            .send(Message::Close(Some(CloseFrame {
                                code: 4401,
                                reason: "session revoked".into(),
                            })))
                            .await;
                        break;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => {}
                }
            }
        }
    }

}

pub async fn audio_talk_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Extension(tickets): Extension<Arc<WsTicketStore>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path((device_tag, channel_id)): Path<(String, String)>,
    State(state): State<Arc<FullState>>,
) -> Response {
    let (ticket_text, via_protocol) = match extract_ws_ticket(&headers) {
        Some(v) => v,
        None => match extract_ws_ticket_from_query(&uri) {
            Some(v) => (v, false),
            None => return (StatusCode::UNAUTHORIZED, "missing ws ticket").into_response(),
        },
    };
    let ticket = match tickets.redeem(&ticket_text) {
        Some(t) => t,
        None => return (StatusCode::FORBIDDEN, "invalid ws ticket").into_response(),
    };

    match state.auth.is_session_active(ticket.sid).await {
        Ok(true) => {}
        _ => return (StatusCode::UNAUTHORIZED, "session revoked").into_response(),
    }

    let ws = if via_protocol {
        ws.protocols([ticket_text])
    } else {
        ws
    };

    let revoker = state.auth.revoker.subscribe();
    tracing::info!(
        "[AudioTalk] client connected addr={} user={} device={}",
        addr,
        ticket.user_id,
        device_tag
    );

    ws.on_upgrade(move |socket| {
        handle_audio_talk(socket, device_tag, channel_id, state, ticket.sid, revoker)
    })
}

async fn handle_audio_talk(
    socket: WebSocket,
    device_tag: String,
    channel_id: String,
    state: Arc<FullState>,
    sid: Uuid,
    mut revoker: broadcast::Receiver<Uuid>,
) {
    let (tx, _rx) = broadcast::channel::<BytesMut>(100);

    let transport = Arc::new(WebSocketMediaNodeTransport::new(tx));

    let device_protocol = {
        match state
            .app
            .registry
            .device_service
            .get_device_by_device_tag(&device_tag)
        {
            Ok(Some(device)) => {
                Some(device.protocol)
            }
            Ok(None) => {
                tracing::warn!("[AudioTalk] Device {} not found", device_tag);
                None
            }
            Err(e) => {
                tracing::error!("[AudioTalk] Failed to get device {}: {}", device_tag, e);
                None
            }
        }
    };

    let device_protocol = match device_protocol {
        Some(p) => p,
        None => {
            tracing::error!(
                "[AudioTalk] Device {} not found or error, closing connection",
                device_tag
            );
            let resp =
                serde_json::json!({"status": "error", "message": "设备不存在或无法获取设备信息"});
            let _ = transport
                .send_text(&serde_json::to_string(&resp).unwrap())
                .await;
            return;
        }
    };

    let node_id = MediaNodeId::new(
        MediaNodeType::WebSocket,
        &device_tag,
        Some(channel_id.clone()),
    );
    let node_id_for_delete = node_id.clone();
    let ws_node: Arc<dyn MediaNode> =
        Arc::new(BasicMediaNode::new(node_id.clone(), transport.clone()));

    match device_protocol {
        Protocol::Gb28181 => {
            ws_node.add_send_chain(
                MediaNodeType::GB28181_BROADCAST_VOICE,
                Box::new(LinearPcmToG711Middleware::new(AudioCodec::Pcma)),
            );
            ws_node.add_send_chain(
                MediaNodeType::GB28181_BROADCAST_VOICE,
                Box::new(SendToReceiversMiddleware::new(
                    Arc::downgrade(&ws_node),
                    MediaNodeType::GB28181_BROADCAST_VOICE,
                )),
            );
        }
        Protocol::Onvif => {

            if adapter_manager::get_or_create_adapter(&Protocol::Onvif, &device_tag).is_none() {
                tracing::error!("[AudioTalk] ONVIF adapter factory not registered");
                let resp = serde_json::json!({"status": "error", "message": "ONVIF 适配器未注册"});
                let _ = transport
                    .send_text(&serde_json::to_string(&resp).unwrap())
                    .await;
                return;
            }

            ws_node.add_send_chain(
                MediaNodeType::ONVIF_BROADCAST_VOICE,
                Box::new(LinearPcmToG711Middleware::new(AudioCodec::Pcma)),
            );
            ws_node.add_send_chain(
                MediaNodeType::ONVIF_BROADCAST_VOICE,
                Box::new(SendToReceiversMiddleware::new(
                    Arc::downgrade(&ws_node),
                    MediaNodeType::ONVIF_BROADCAST_VOICE,
                )),
            );
        }
        other => {
            tracing::error!(
                "[AudioTalk] Unsupported protocol {:?} for device {}, closing connection",
                other,
                device_tag
            );
            let resp = serde_json::json!({"status": "error", "message": format!("设备协议 {:?} 不支持语音广播", other)});
            let _ = transport
                .send_text(&serde_json::to_string(&resp).unwrap())
                .await;
            return;
        }
    }

    let manager = match crate::media::get_media_session_manager() {
        Some(m) => m,
        None => {
            tracing::error!("[AudioTalk] No MediaSessionManager");
            return;
        }
    };
    let session = manager.get_or_create(&device_tag);

    if let Err(e) = session.add_node(ws_node.clone()) {
        tracing::error!("[AudioTalk] Failed to add ws_node to session: {}", e);
        return;
    }

    let node_id_clone = node_id_for_delete.clone();
    let state_clone = state.clone();
    let ws_node_clone = ws_node.clone();

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (close_tx, close_rx) = mpsc::channel::<u16>(1);
    let transport_clone = transport.clone();
    transport.run(
        socket,
        move |data| {
            let ws_node = ws_node_clone.clone();
            async move {
                ws_node.on_media_data(data).await;
            }
        },
        move |text| {
            let node_id = node_id_clone.clone();
            let state = state_clone.clone();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                let action = json.get("action").and_then(|v| v.as_str());
                if action == Some("start_talk") {
                    let event = crate::protocol::event::SignalEvent::StartAudioTalk { node_id };
                    let event_bus = state.app.registry.infra.event_bus.clone();
                    tokio::spawn(async move {
                        if let Err(e) = event_bus.publish(event).await {
                            tracing::error!(
                                "[AudioTalk] Failed to publish StartAudioTalk event: {}",
                                e
                            );
                        } else {
                        }
                    });
                } else if action == Some("stop_talk") {
                    let event = crate::protocol::event::SignalEvent::StopAudioTalk { node_id };
                    let event_bus = state.app.registry.infra.event_bus.clone();
                    tokio::spawn(async move {
                        if let Err(e) = event_bus.publish(event).await {
                            tracing::error!(
                                "[AudioTalk] Failed to publish StopAudioTalk event: {}",
                                e
                            );
                        } else {
                        }
                    });
                }
            }
        },
        Some(shutdown_tx.clone()),
        Some(close_rx),
    );

    let mut event_rx = state.app.registry.infra.event_bus.subscribe();
    let node_id_for_compare = node_id_for_delete.clone();
    let device_tag_clone = device_tag.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    match event {
                        Ok(crate::protocol::event::SignalEvent::AudioTalkResponse { node_id: ref resp_node_id, status, message }) => {
                            if *resp_node_id == node_id_for_compare {
                                match status {
                                    crate::protocol::event::TalkResponseStatus::Ready => {
                                        let resp = serde_json::json!({"status": "ready", "message": message.as_deref().unwrap_or("你可以说话了")});
                                        let _ = transport_clone.send_text(&serde_json::to_string(&resp).unwrap()).await;
                                    }
                                    crate::protocol::event::TalkResponseStatus::Error => {
                                        let resp = serde_json::json!({"status": "error", "message": message.as_deref().unwrap_or("启动失败")});
                                        let _ = transport_clone.send_text(&serde_json::to_string(&resp).unwrap()).await;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!("[AudioTalk] Event error: {:?}", e),
                    }
                }
                _ = shutdown_rx.recv() => {
                    let event = crate::protocol::event::SignalEvent::StopAudioTalk {
                        node_id: node_id_for_delete.clone(),
                    };
                    let _ = state.app.registry.infra.event_bus.publish(event).await;
                    if let Some(mgr) = crate::media::get_media_session_manager() {
                        if let Some(session) = mgr.get(&device_tag_clone) {
                            session.remove_node(&node_id_for_delete);
                        }
                    }
                    break;
                }
                revoke = revoker.recv() => {
                    match revoke {
                        Ok(revoked) if revoked == sid => {
                            let _ = close_tx.send(4401).await;
                            break;
                        }
                        Ok(_) => {}
                        Err(_) => {}
                    }
                }
            }
        }
    });
}
