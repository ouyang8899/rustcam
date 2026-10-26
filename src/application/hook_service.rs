// use serde::{Deserialize, Serialize};
// use std::sync::Arc;
// use std::collections::HashMap;

// use crate::context::{InfraContext, MediaContext};
// use crate::infrastructure::DbRepository;
// use crate::infrastructure::RedisStore;
// use crate::infrastructure::cluster::ClusterManager;
// use crate::infrastructure::EventBus;
// use crate::error::{AppError, Result};
// use crate::application::RecordingService;
// use crate::application::StreamService;
// use crate::protocol::event::SignalEvent;

// #[derive(Debug, Clone)]
// pub enum HookAction {

//     Allow,

//     Deny(String),
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(tag = "event")]
// pub enum HookEvent {

//     #[serde(rename = "on_play")]
//     OnPlay {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: String,
//         #[serde(rename = "ip")]
//         ip: Option<String>,
//         #[serde(rename = "port")]
//         port: Option<u16>,
//         #[serde(rename = "schema")]
//         schema: Option<String>,
//         #[serde(rename = "params", default)]
//         params: Option<String>,
//         #[serde(rename = "app")]
//         app: Option<String>,
//         #[serde(rename = "vhost")]
//         vhost: Option<String>,
//     },

//     #[serde(rename = "on_keepalive")]
//     OnKeepalive {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: Option<String>,
//         #[serde(rename = "ip")]
//         ip: Option<String>,
//     },

//     #[serde(rename = "on_stop")]
//     OnStop {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: String,
//         #[serde(rename = "ip")]
//         ip: Option<String>,
//         #[serde(rename = "port")]
//         port: Option<u16>,
//         #[serde(rename = "schema")]
//         schema: Option<String>,
//         #[serde(rename = "params", default)]
//         params: Option<String>,
//         #[serde(rename = "app")]
//         app: Option<String>,
//         #[serde(rename = "vhost")]
//         vhost: Option<String>,
//     },

//     #[serde(rename = "on_publish")]
//     OnPublish {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: String,
//         #[serde(rename = "ip")]
//         ip: Option<String>,
//         #[serde(rename = "app")]
//         app: Option<String>,
//         #[serde(rename = "vhost")]
//         vhost: Option<String>,
//         #[serde(rename = "params", default)]
//         params: Option<String>,
//     },

//     #[serde(rename = "on_rtcp_stats")]
//     OnRtcpStats {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: String,
//         #[serde(rename = "bytesSent")]
//         bytes_sent: Option<u64>,
//         #[serde(rename = "bytesReceived")]
//         bytes_received: Option<u64>,
//     },

//     #[serde(rename = "on_record_mp4")]
//     #[serde(alias = "on_record_ts")]
//     #[serde(alias = "on_record_flv")]
//     #[serde(alias = "on_record_hls")]
//     OnRecordDone {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: String,
//         #[serde(rename = "app")]
//         app: String,
//         #[serde(rename = "file_path")]
//         file_path: Option<String>,
//         #[serde(rename = "file_size")]
//         file_size: Option<u64>,
//         #[serde(rename = "time_len")]
//         time_len: Option<f64>,
//         /// 媒体服务器推送的相对录像路径
//         #[serde(rename = "url")]
//         url: Option<String>,
//         /// 录像文件真正的开始时间（秒级时间戳）
//         #[serde(rename = "start_time")]
//         start_time: Option<u64>,
//         #[serde(rename = "file_name")]
//         file_name: Option<String>,
//     },

//     #[serde(rename = "on_flow_report")]
//     OnFlowReport {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "app")]
//         app: String,
//         #[serde(rename = "stream")]
//         stream: String,
//         #[serde(rename = "ip")]
//         ip: Option<String>,
//         #[serde(rename = "port")]
//         port: Option<u32>,
//         #[serde(rename = "params")]
//         params: Option<String>,
//         #[serde(rename = "duration")]
//         duration: Option<u32>,
//         #[serde(rename = "totalBytes")]
//         total_bytes: Option<u64>,
//         #[serde(rename = "player")]
//         player: Option<bool>,
//         #[serde(rename = "schema")]
//         schema: Option<String>,
//         #[serde(rename = "vhost")]
//         vhost: Option<String>,
//     },

//     #[serde(rename = "on_http_access")]
//     OnHttpAccess {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "ip")]
//         ip: Option<String>,
//         #[serde(rename = "port")]
//         port: Option<u16>,
//         #[serde(rename = "params")]
//         params: Option<String>,
//         #[serde(rename = "path")]
//         path: Option<String>,
//         #[serde(rename = "is_dir")]
//         is_dir: Option<bool>,
//         #[serde(flatten)]
//         headers: HashMap<String, String>,
//     },

//     #[serde(rename = "on_rtsp_realm")]
//     OnRtspRealm {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: String,
//         app: Option<String>,
//         ip: Option<String>,
//         port: Option<u16>,
//         params: Option<String>,
//         schema: Option<String>,
//         vhost: Option<String>,
//     },

//     #[serde(rename = "on_rtsp_auth")]
//     OnRtspAuth {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: String,
//         app: Option<String>,
//         ip: Option<String>,
//         port: Option<u16>,
//         params: Option<String>,
//         schema: Option<String>,
//         vhost: Option<String>,
//         realm: Option<String>,
//         user_name: Option<String>,
//         must_no_encrypt: Option<bool>,
//     },

//     #[serde(rename = "on_shell_login")]
//     OnShellLogin {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         ip: Option<String>,
//         port: Option<u16>,
//         passwd: Option<String>,
//         user_name: Option<String>,
//     },

//     #[serde(rename = "on_stream_changed")]
//     OnStreamChanged {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         app: String,
//         stream: String,
//         vhost: Option<String>,
//         schema: Option<String>,
//         regist: bool,
//         origin_type: Option<u8>,
//         origin_type_str: Option<String>,
//         origin_url: Option<String>,
//         reader_count: Option<u32>,
//         total_reader_count: Option<u32>,
//         alive_second: Option<u32>,
//         bytes_speed: Option<u64>,
//         tracks: Option<Vec<StreamTrackInfo>>,
//     },

//     #[serde(rename = "on_stream_none_reader")]
//     OnStreamNoneReader {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         app: String,
//         stream: String,
//         vhost: Option<String>,
//         schema: Option<String>,
//     },

//     #[serde(rename = "on_stream_not_found")]
//     OnStreamNotFound {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(rename = "stream")]
//         stream_id: String,
//         app: Option<String>,
//         ip: Option<String>,
//         port: Option<u16>,
//         params: Option<String>,
//         schema: Option<String>,
//         vhost: Option<String>,
//     },

//     #[serde(rename = "on_server_started")]
//     OnServerStarted {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         #[serde(flatten)]
//         config: HashMap<String, serde_json::Value>,
//     },

//     #[serde(rename = "on_server_keepalive")]
//     OnServerKeepalive {
//         #[serde(rename = "mediaServerId")]
//         media_server_id: String,
//         data: Option<ServerKeepaliveData>,
//     },

//     #[serde(rename = "on_rtp_server_timeout")]
//     OnRtpServerTimeout {
//         media_server_id: String,
//         local_port: Option<u16>,
//         re_use_port: Option<bool>,
//         ssrc: Option<u32>,
//         stream_id: Option<String>,
//         tcp_mode: Option<u8>,
//     },
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct StreamTrackInfo {
//     pub codec_type: Option<u8>,
//     pub codec_id: Option<u8>,
//     pub codec_id_name: Option<String>,
//     pub ready: Option<bool>,
//     pub width: Option<u32>,
//     pub height: Option<u32>,
//     pub fps: Option<f64>,
//     pub sample_rate: Option<u32>,
//     pub sample_bit: Option<u16>,
//     pub channels: Option<u8>,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct ServerKeepaliveData {
//     pub buffer: Option<u32>,
//     pub buffer_like_string: Option<u32>,
//     pub buffer_list: Option<u32>,
//     pub buffer_raw: Option<u32>,
//     pub frame: Option<u32>,
//     pub frame_imp: Option<u32>,
//     pub media_source: Option<u32>,
//     pub multi_media_source_muxer: Option<u32>,
//     pub rtmp_packet: Option<u32>,
//     pub rtp_packet: Option<u32>,
//     pub socket: Option<u32>,
//     pub tcp_client: Option<u32>,
//     pub tcp_server: Option<u32>,
//     pub tcp_session: Option<u32>,
//     pub udp_server: Option<u32>,
//     pub udp_session: Option<u32>,
// }

// pub struct HookService {

//     repo: Arc<DbRepository>,

//     redis: Arc<RedisStore>,

//     cluster: Arc<ClusterManager>,

//     event_bus: Arc<EventBus>,

//     session_expiry_secs: i64,

//     recording_service: Arc<RecordingService>,

//     stream_service: Arc<StreamService>,
// }

// impl HookService {

//     pub fn new(
//         repo: Arc<DbRepository>,
//         redis: Arc<RedisStore>,
//         cluster: Arc<ClusterManager>,
//         event_bus: Arc<EventBus>,
//         session_expiry_secs: i64,
//         recording_service: Arc<RecordingService>,
//         stream_service: Arc<StreamService>,
//     ) -> Self {
//         Self {
//             repo,
//             redis,
//             cluster,
//             event_bus,
//             session_expiry_secs,
//             recording_service,
//             stream_service,
//         }
//     }

//     #[allow(dead_code)]
//     pub fn from_context(
//         infra: &InfraContext,
//         media: &MediaContext,
//         recording_service: Arc<RecordingService>,
//     ) -> Self {
//         let stream_service = Arc::new(StreamService::new(
//             infra.clone(),
//             media.clone(),
//             recording_service.clone(),
//         ));
//         Self {
//             repo: infra.db.clone(),
//             redis: infra.redis.clone(),
//             cluster: media.cluster.clone(),
//             event_bus: infra.event_bus.clone(),
//             session_expiry_secs: infra.config().session.expiration_secs,
//             recording_service,
//             stream_service,
//         }
//     }

//     pub async fn handle(&self, event: HookEvent) -> HookAction {
//         match event {
//             HookEvent::OnPlay {
//                 media_server_id,
//                 stream_id,
//                 ip,
//                 port: _,
//                 schema: _,
//                 params,
//                 app,
//                 vhost: _,
//             } => {
//                 self.on_play(&media_server_id, &stream_id, ip.as_deref(), params.as_deref(), app.as_deref())
//                     .await
//             }

//             HookEvent::OnKeepalive {
//                 media_server_id,
//                 stream_id,
//                 ip,
//             } => {
//                 self.on_keepalive(&media_server_id, stream_id.as_deref(), ip.as_deref())
//                     .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnStop {
//                 media_server_id,
//                 stream_id,
//                 ip,
//                 port: _,
//                 schema: _,
//                 params,
//                 app: _,
//                 vhost: _,
//             } => {
//                 self.on_stop(&media_server_id, &stream_id, ip.as_deref(), params.as_deref())
//                     .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnPublish {
//                 media_server_id,
//                 stream_id,
//                 ip,
//                 app,
//                 vhost: _,
//                 params,
//             } => {
//                 self.on_publish(&media_server_id, &stream_id, ip.as_deref(), app.as_deref(), params.as_deref())
//                     .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnRtcpStats {
//                 media_server_id,
//                 stream_id,
//                 bytes_sent,
//                 bytes_received,
//             } => {
//                 self.on_rtcp_stats(&media_server_id, &stream_id, bytes_sent, bytes_received)
//                     .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnRecordDone {
//                 stream_id,
//                 media_server_id,
//                 file_path,
//                 file_size,
//                 time_len,
//                 url,
//                 start_time,
//                 file_name,
//                 ..
//             } => {
//                 self.on_record_done(
//                     &media_server_id,
//                     &stream_id,
//                     file_path.as_deref(),
//                     file_size,
//                     time_len,
//                     url.as_deref(),
//                     start_time,
//                     file_name.as_deref(),
//                 )
//                 .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnFlowReport {
//                 media_server_id,
//                 stream,
//                 ip,
//                 port,
//                 params,
//                 duration,
//                 total_bytes,
//                 player,
//                 ..
//             } => {
//                 self.on_flow_report(
//                     &media_server_id,
//                     &stream,
//                     ip.as_deref(),
//                     port,
//                     params.as_deref(),
//                     duration,
//                     total_bytes,
//                     player,
//                 )
//                 .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnHttpAccess {
//                 media_server_id,
//                 ip,
//                 port,
//                 path,
//                 is_dir,
//                 ..
//             } => {
//                 self.on_http_access(
//                     &media_server_id,
//                     ip.as_deref(),
//                     port,
//                     path.as_deref(),
//                     is_dir,
//                 )
//                 .await
//             }

//             HookEvent::OnRtspRealm {
//                 media_server_id,
//                 stream_id,
//                 ip,
//                 ..
//             } => {
//                 self.on_rtsp_realm(&media_server_id, &stream_id, ip.as_deref())
//                     .await
//             }

//             HookEvent::OnRtspAuth {
//                 media_server_id,
//                 stream_id,
//                 ip,
//                 user_name,
//                 must_no_encrypt,
//                 ..
//             } => {
//                 self.on_rtsp_auth(
//                     &media_server_id,
//                     &stream_id,
//                     ip.as_deref(),
//                     user_name.as_deref(),
//                     must_no_encrypt.unwrap_or(false),
//                 )
//                 .await
//             }

//             HookEvent::OnShellLogin {
//                 media_server_id,
//                 ip,
//                 passwd,
//                 user_name,
//                 ..
//             } => {
//                 self.on_shell_login(
//                     &media_server_id,
//                     ip.as_deref(),
//                     passwd.as_deref(),
//                     user_name.as_deref(),
//                 )
//                 .await
//             }

//             HookEvent::OnStreamChanged {
//                 media_server_id,
//                 app,
//                 stream,
//                 regist,
//                 origin_type,
//                 origin_type_str,
//                 reader_count,
//                 total_reader_count,
//                 alive_second,
//                 bytes_speed,
//                 tracks,
//                 ..
//             } => {
//                 self.on_stream_changed(
//                     &media_server_id,
//                     &app,
//                     &stream,
//                     regist,
//                     origin_type,
//                     origin_type_str.as_deref(),
//                     reader_count,
//                     total_reader_count,
//                     alive_second,
//                     bytes_speed,
//                     tracks.as_ref(),
//                 )
//                 .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnStreamNoneReader {
//                 media_server_id,
//                 app,
//                 stream,
//                 ..
//             } => {
//                 self.on_stream_none_reader(&media_server_id, &app, &stream)
//                     .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnStreamNotFound {
//                 media_server_id,
//                 stream_id,
//                 ip,
//                 ..
//             } => {
//                 self.on_stream_not_found(&media_server_id, &stream_id, ip.as_deref())
//                     .await
//             }

//             HookEvent::OnServerStarted {
//                 media_server_id,
//                 config,
//             } => {
//                 self.on_server_started(&media_server_id, &config).await;
//                 HookAction::Allow
//             }

//             HookEvent::OnServerKeepalive {
//                 media_server_id,
//                 data,
//             } => {
//                 self.on_server_keepalive(&media_server_id, data.as_ref())
//                     .await;
//                 HookAction::Allow
//             }

//             HookEvent::OnRtpServerTimeout {
//                 media_server_id,
//                 stream_id,
//                 ..
//             } => {
//                 self.on_rtp_server_timeout(&media_server_id, stream_id.as_deref())
//                     .await;
//                 HookAction::Allow
//             }
//         }
//     }

//     async fn on_play(
//         &self,
//         media_server_id: &str,
//         stream_id: &str,
//         client_ip: Option<&str>,
//         params: Option<&str>,
//         _app: Option<&str>,
//     ) -> HookAction {

//         let params_map: HashMap<String, String> = params
//             .map(|p| {
//                 p.split('&')
//                     .filter_map(|pair| {
//                         let mut parts = pair.split('=');
//                         match (parts.next(), parts.next()) {
//                             (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
//                             _ => None,
//                         }
//                     })
//                     .collect()
//             })
//             .unwrap_or_default();

//         let token = params_map.get("token").map(|s| s.as_str());

//         let _token = match token {
//             Some(t) => t,
//             None => {
//                 tracing::warn!("[Hook] on_play: no token provided for stream={}", stream_id);
//                 return HookAction::Deny("Missing token".to_string());
//             }
//         };

//         let _stream = match self.get_stream_by_token(_token).await {
//             Ok(Some(s)) => s,
//             Ok(None) => {
//                 tracing::warn!("[Hook] on_play: invalid token for stream={}", stream_id);
//                 return HookAction::Deny("Invalid token".to_string());
//             }
//             Err(e) => {
//                 tracing::error!("[Hook] on_play: failed to get stream by token: {}", e);
//                 return HookAction::Deny("Internal error".to_string());
//             }
//         };

//         let event = SignalEvent::HookPlay {
//             media_server_tag: media_server_id.to_string(),
//             stream_id: stream_id.to_string(),
//             client_ip: client_ip.map(String::from),
//         };
//         if let Err(e) = self.event_bus.publish(event).await {
//             tracing::warn!("[Hook] on_play: failed to publish event: {}", e);
//         }

//         HookAction::Allow
//     }

//     async fn on_stop(
//         &self,
//         _media_server_id: &str,
//         stream_id: &str,
//         _client_ip: Option<&str>,
//         _params: Option<&str>,
//     ) {

//         let sessions = self.find_sessions_by_stream(stream_id).await;
//         let count = sessions.len();
//         for session in sessions {
//             let mut s = session;
//             s.terminate();
//             self.repo.sessions_cache().insert(s.id, s);
//         }

//         if count > 0 {
//         }

//         if let Err(e) = self.decrement_stream_viewers(stream_id).await {
//             tracing::warn!("[Hook] on_stop: failed to decrement viewers: {}", e);
//         }
//     }

//     async fn on_keepalive(
//         &self,
//         _media_server_id: &str,
//         stream_id: Option<&str>,
//         _client_ip: Option<&str>,
//     ) {

//         let Some(stream_id) = stream_id else {
//             return;
//         };

//         let sessions = self.find_sessions_by_stream(stream_id).await;
//         for session in sessions {
//             let mut s = session;
//             s.touch();
//             s.refresh_expiry(self.session_expiry_secs);
//             self.repo.sessions_cache().insert(s.id, s);
//         }

//         if let Err(_e) = self.touch_stream_keepalive(stream_id).await {
//         }

//         const STREAM_TTL_SECS: u64 = 86400;
//         if let Err(_e) = self.redis.expire_key(&format!("stream:{}", stream_id), STREAM_TTL_SECS).await {
//         }

//         if let Some(stream) = self.find_stream_by_stream_key(stream_id) {
//             if let Some(ref device_tag) = stream.device_tag {

//                 let cached_device = self.repo.device_cached(device_tag);
//                 if let Some(mut device) = cached_device {
//                     device.set_online();
//                     if let Err(_e) = self.repo.update_device(&device).await {
//                     }
//                 }
//             }
//         }
//     }

//     async fn on_publish(
//         &self,
//         media_server_id: &str,
//         stream_id: &str,
//         client_ip: Option<&str>,
//         _app: Option<&str>,
//         params: Option<&str>,
//     ) {
//         let params_map: std::collections::HashMap<String, String> = params
//             .map(|p| {
//                 p.split('&')
//                     .filter_map(|pair| {
//                         let mut parts = pair.split('=');
//                         match (parts.next(), parts.next()) {
//                             (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
//                             _ => None,
//                         }
//                     })
//                     .collect()
//             })
//             .unwrap_or_default();

//         let protocol = params_map.get("protocol").map(|s| s.to_lowercase()).unwrap_or_default();
//         let _is_rtmp = protocol == "rtmp" || protocol.is_empty();
//         let device_tag = params_map.get("device_tag").cloned();
//         let channel_tag = params_map.get("channel_tag").cloned();

//         let event = SignalEvent::HookPublish {
//             media_server_tag: media_server_id.to_string(),
//             stream_id: stream_id.to_string(),
//             client_ip: client_ip.map(String::from),
//             device_tag,
//             channel_tag,
//         };
//         if let Err(e) = self.event_bus.publish(event).await {
//             tracing::warn!("[Hook] on_publish: failed to publish event: {}", e);
//         }
//     }

//     async fn on_rtcp_stats(
//         &self,
//         _media_server_id: &str,
//         stream_id: &str,
//         bytes_sent: Option<u64>,
//         bytes_received: Option<u64>,
//     ) {

//         let sessions = self.find_sessions_by_stream(stream_id).await;
//         for session in sessions {
//             let mut s = session;
//             s.update_stats(
//                 bytes_sent.unwrap_or(0),
//                 bytes_received.unwrap_or(0),
//             );
//             self.repo.sessions_cache().insert(s.id, s);
//         }

//         if let Err(_e) = self.update_stream_bandwidth(
//             stream_id,
//             bytes_received.unwrap_or(0),
//             bytes_sent.unwrap_or(0),
//         ).await {
//         }
//     }

//     async fn on_record_done(
//         &self,
//         media_server_id: &str,
//         stream_id: &str,
//         file_path: Option<&str>,
//         file_size: Option<u64>,
//         time_len: Option<f64>,
//         url: Option<&str>,
//         start_time: Option<u64>,
//         file_name: Option<&str>,
//     ) {
//         let size = file_size.unwrap_or(0);
//         let duration = time_len.map(|t| t as u64).unwrap_or(0);

//         if let Err(e) = self
//             .recording_service
//             .finalize_recording(
//                 stream_id,
//                 Some(media_server_id.to_string()),
//                 0,//url,
//                 11,//file_path
//             )
//             .await
//         {
//             tracing::error!("[Hook] on_record_done: failed to record segment: {}", e);
//         }
//     }

//     async fn get_stream_by_token(
//         &self,
//         token: &str,
//     ) -> std::result::Result<Option<crate::domain::Stream>, AppError> {
//         self.repo.get_stream_by_token(token).await
//     }

//     fn find_stream_by_stream_key(&self, stream_key: &str) -> Option<crate::domain::Stream> {
//         let (device_tag, channel_tag) = crate::common::device_utils::parse_stream_key(stream_key)?;
//         let cache_key = format!("{}/{}", device_tag, channel_tag);
//         self.repo.streams_cache()
//             .get(&cache_key)
//             .map(|s| s.clone())
//     }

//     async fn increment_stream_viewers(&self, stream_key: &str) -> Result<()> {
//         if let Some(s) = self.find_stream_by_stream_key(stream_key) {
//             let mut s = s;
//             s.increment_viewers();
//             self.repo.update_stream(&s).await?;
//         }
//         Ok(())
//     }

//     async fn decrement_stream_viewers(&self, stream_key: &str) -> Result<()> {
//         if let Some(s) = self.find_stream_by_stream_key(stream_key) {
//             let mut s = s;
//             s.decrement_viewers();
//             self.repo.update_stream(&s).await?;
//         }
//         Ok(())
//     }

//     async fn touch_stream_keepalive(&self, stream_key: &str) -> Result<()> {
//         if let Some(s) = self.find_stream_by_stream_key(stream_key) {
//             let mut s = s;
//             s.update_keepalive();
//             self.repo.update_stream(&s).await?;
//         }
//         Ok(())
//     }

//     async fn update_stream_bandwidth(&self, stream_key: &str, bytes_in: u64, bytes_out: u64) -> Result<()> {
//         if let Some(s) = self.find_stream_by_stream_key(stream_key) {
//             let mut s = s;
//             s.update_bandwidth(bytes_in, bytes_out);
//             self.repo.update_stream(&s).await?;
//         }
//         Ok(())
//     }

//     async fn register_push_stream(&self, media_server_id: &str, stream_id: &str, device_tag_from_params: Option<&str>, channel_tag_from_params: Option<&str>) -> Result<()> {
//         let (final_device_tag, final_channel_tag) = if let (Some(dt), Some(ct)) = (device_tag_from_params, channel_tag_from_params) {
//             (dt.to_string(), ct.to_string())
//         } else {
//             crate::common::device_utils::parse_stream_key(stream_id)
//                 .unwrap_or_else(|| (stream_id.to_string(), stream_id.to_string()))
//         };

//         let mut stream = match self.stream_service.find_stream(&final_device_tag, &final_channel_tag) {
//             Some(s) => s,
//             None => {
//                 tracing::warn!("[Hook] on_publish: push stream not found for device_tag={}, channel_tag={}", final_device_tag, final_channel_tag);
//                 return Ok(());
//             }
//         };

//         stream.update_keepalive();
//         if stream.state == crate::domain::StreamState::Idle || stream.state == crate::domain::StreamState::Error {
//             stream.start();
//         }
//         stream.media_server_tag = media_server_id.to_string();
//         self.repo.update_stream(&stream).await?;

//         Ok(())
//     }

//     fn find_device_by_push_stream(&self, stream_id: &str) -> Option<String> {
//         let (device_tag_from_stream, _channel_tag_from_stream) = crate::common::device_utils::parse_stream_key(stream_id)
//             .or_else(|| {
//                 if let Some(idx) = stream_id.rfind('/') {
//                     let dt = stream_id[..idx].to_string();
//                     let ct = stream_id[idx + 1..].to_string();
//                     if !dt.is_empty() && !ct.is_empty() {
//                         return Some((dt, ct));
//                     }
//                 }
//                 None
//             })
//             .unwrap_or_else(|| (stream_id.to_string(), stream_id.to_string()));

//         let device_tag_str = device_tag_from_stream.clone();

//         if self.repo.devices_cache().get(&device_tag_str).is_some() {
//             return Some(device_tag_str);
//         }

//         let cache_key = stream_id.replace('_', "/");
//         if let Some(s) = self.repo.streams_cache().get(&cache_key) {
//             return s.device_tag.clone();
//         }

//         for entry in self.repo.devices_cache().iter() {
//             let dt_str = entry.key().clone();
//             if stream_id.contains(&dt_str) && !dt_str.is_empty() {
//                 return Some(dt_str);
//             }
//             for pu in &entry.push_urls {
//                 if !pu.url.is_empty() && pu.url.contains(stream_id) {
//                     return Some(dt_str);
//                 }
//             }
//         }

//         None
//     }

//     async fn find_sessions_by_stream(
//         &self,
//         stream_id: &str,
//     ) -> Vec<crate::domain::Session> {
//         use crate::domain::SessionState;

//         self.repo.sessions_cache()
//             .iter()
//             .filter(|s| {
//                 s.stream_key() == stream_id
//                     && s.state != SessionState::Terminated
//                     && s.state != SessionState::Terminating
//             })
//             .map(|s| s.clone())
//             .collect()
//     }

//     async fn on_flow_report(
//         &self,
//         _media_server_id: &str,
//         _stream: &str,
//         _ip: Option<&str>,
//         _port: Option<u32>,
//         _params: Option<&str>,
//         _duration: Option<u32>,
//         _total_bytes: Option<u64>,
//         _player: Option<bool>,
//     ) {
//     }

//     async fn on_http_access(
//         &self,
//         _media_server_id: &str,
//         _ip: Option<&str>,
//         _port: Option<u16>,
//         _path: Option<&str>,
//         _is_dir: Option<bool>,
//     ) -> HookAction {
//         HookAction::Allow
//     }

//     async fn on_rtsp_realm(
//         &self,
//         _media_server_id: &str,
//         _stream_id: &str,
//         _ip: Option<&str>,
//     ) -> HookAction {
//         HookAction::Allow
//     }

//     async fn on_rtsp_auth(
//         &self,
//         _media_server_id: &str,
//         _stream_id: &str,
//         _ip: Option<&str>,
//         _user_name: Option<&str>,
//         _must_no_encrypt: bool,
//     ) -> HookAction {
//         HookAction::Allow
//     }

//     async fn on_shell_login(
//         &self,
//         media_server_id: &str,
//         ip: Option<&str>,
//         _passwd: Option<&str>,
//         user_name: Option<&str>,
//     ) -> HookAction {
//         tracing::warn!(
//             "[Hook] on_shell_login: server={}, ip={:?}, user={:?} - denying shell login",
//             media_server_id, ip, user_name
//         );
//         HookAction::Deny("Shell login not allowed".to_string())
//     }

//     async fn on_stream_changed(
//         &self,
//         media_server_id: &str,
//         _app: &str,
//         stream: &str,
//         regist: bool,
//         _origin_type: Option<u8>,
//         _origin_type_str: Option<&str>,
//         reader_count: Option<u32>,
//         total_reader_count: Option<u32>,
//         _alive_second: Option<u32>,
//         bytes_speed: Option<u64>,
//         _tracks: Option<&Vec<StreamTrackInfo>>,
//     ) {

//         if !regist {
//             let event = SignalEvent::HookUnpublish {
//                 media_server_tag: media_server_id.to_string(),
//                 stream_id: stream.to_string(),
//             };
//             if let Err(e) = self.event_bus.publish(event).await {
//                 tracing::warn!("[Hook] on_stream_changed: failed to publish unpublish event: {}", e);
//             }
//         } else {
//             let event = SignalEvent::HookStreamUpdate {
//                 media_server_tag: media_server_id.to_string(),
//                 stream_id: stream.to_string(),
//                 regist: true,
//                 reader_count,
//                 total_reader_count,
//                 bytes_speed,
//             };
//             if let Err(e) = self.event_bus.publish(event).await {
//                 tracing::warn!("[Hook] on_stream_changed: failed to publish stream update event: {}", e);
//             }
//         }
//     }

//     async fn on_stream_none_reader(
//         &self,
//         _media_server_id: &str,
//         _app: &str,
//         _stream: &str,
//     ) {
//     }

//     async fn on_stream_not_found(
//         &self,
//         _media_server_id: &str,
//         _stream_id: &str,
//         _ip: Option<&str>,
//     ) -> HookAction {
//         HookAction::Allow
//     }

//     async fn on_server_started(
//         &self,
//         _media_server_id: &str,
//         _config: &std::collections::HashMap<String, serde_json::Value>,
//     ) {
//     }

//     async fn on_server_keepalive(
//         &self,
//         _media_server_id: &str,
//         _data: Option<&ServerKeepaliveData>,
//     ) {
//     }

//     async fn on_rtp_server_timeout(
//         &self,
//         _media_server_id: &str,
//         _stream_id: Option<&str>,
//     ) {
//     }
// }