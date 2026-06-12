use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::adapter::media_server::{MediaServerAdapter, Protocol, RecordingInfo, RecordingFile, RtpTransport, ServerStatus, StreamInfo};
use crate::config::MediaServerConfig;

pub struct SrsAdapter {
    config: MediaServerConfig,
    base_url: String,
}

impl SrsAdapter {
    pub fn new(config: MediaServerConfig) -> Self {
        let base_url = config.url.trim_end_matches('/').to_string();
        Self { config, base_url }
    }

    fn base_host(&self) -> String {
        self.config.url.trim_start_matches("http://").trim_start_matches("https://").to_string()
    }
}

#[async_trait]
impl MediaServerAdapter for SrsAdapter {
    fn name(&self) -> &str { &self.config.name }
    fn tag(&self) -> &str { &self.config.server_tag }
    fn server_type(&self) -> &str { "srs" }

    async fn is_online(&self) -> bool {
        let url = format!("{}/api/v1/summaries?api={}", self.base_url, self.config.api_key);
        reqwest::get(&url).await.map(|r| r.status().is_success()).unwrap_or(false)
    }

    async fn get_status(&self) -> anyhow::Result<ServerStatus> {
        #[derive(Deserialize)]
        struct Resp { code: i32, data: Option<serde_json::Value> }
        let url = format!("{}/api/v1/summaries?api={}", self.base_url, self.config.api_key);
        let resp: Resp = reqwest::get(&url).await?.json().await?;

        let data = resp.data.unwrap_or_default();
        Ok(ServerStatus {
            name: self.config.name.clone(), server_type: "srs".to_string(), online: true,
            session_count: data.get("session_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            cpu_usage: data.get("cpu").and_then(|v| v.as_f64()).unwrap_or(0.0),
            memory_usage: data.get("mem").and_then(|v| v.as_f64()).unwrap_or(0.0),
            bandwidth_in: 0, bandwidth_out: 0, last_heartbeat: None,
        })
    }

    async fn add_stream_proxy(&self, _app: &str, stream_key: &str, rtsp_url: &str) -> anyhow::Result<StreamInfo> {
        let base = self.base_host();
        let rtsp = format!("rtsp://{}/{}", base, stream_key);
        let rtmp = format!("rtmp://{}/live/{}", base, stream_key);
        let hls = format!("http://{}/live/{}.m3u8", base, stream_key);

        Ok(StreamInfo {
            stream_key: stream_key.to_string(),
            play_url: format!("http://{}/{}", base, stream_key),
            rtsp_url: rtsp, rtmp_url: rtmp, hls_url: hls,
            webrtc_url: String::new(), flv_url: None, web_flv_url: None,
            media_server_id: self.config.name.clone(), media_server_name: self.config.name.clone(),
        })
    }

    async fn remove_stream_proxy(&self, _app: &str, _stream_key: &str) -> anyhow::Result<()> { Ok(()) }
    async fn get_play_url(&self, _app: &str, stream_key: &str, protocol: Protocol) -> anyhow::Result<String> {
        let base = self.base_host();
        match protocol {
            Protocol::Rtsp => Ok(format!("rtsp://{}/{}", base, stream_key)),
            Protocol::Rtmp => Ok(format!("rtmp://{}/live/{}", base, stream_key)),
            Protocol::Hls => Ok(format!("http://{}/live/{}.m3u8", base, stream_key)),
            _ => Ok(String::new()),
        }
    }
    async fn get_session_count(&self) -> anyhow::Result<u32> { Ok(0) }
    async fn get_sessions(&self) -> anyhow::Result<Vec<serde_json::Value>> { Ok(vec![]) }
    async fn is_stream_online(&self, _app: &str, _stream_key: &str) -> anyhow::Result<bool> { Ok(false) }
    async fn ptz_control(&self, _stream_key: &str, _command: &str, _channel: u8) -> anyhow::Result<()> { Ok(()) }
    async fn start_recording(&self, _app: &str, _stream_key: &str, _format: &str, _output_path: Option<&str>) -> anyhow::Result<RecordingInfo> {
        Ok(RecordingInfo { stream_key: String::new(), output_path: String::new(), started_at: chrono::Utc::now().timestamp() })
    }
    async fn stop_recording(&self, _app: &str, _stream_key: &str, _format: &str) -> anyhow::Result<()> { Ok(()) }
    async fn is_recording(&self, _stream_key: &str, _format: &str) -> anyhow::Result<bool> { Ok(false) }
    async fn list_recordings(&self, _app: &str, _stream_key: &str) -> anyhow::Result<Vec<RecordingFile>> { Ok(vec![]) }
    async fn open_rtp_server(&self, _stream_id: &str, _port: u16, _transport: RtpTransport) -> anyhow::Result<(u16, String)> {
        Err(anyhow::anyhow!("RTP server not supported for SRS"))
    }
    async fn close_rtp_server(&self, _stream_id: &str) -> anyhow::Result<()> { Ok(()) }
    async fn get_media_info(&self, _app: &str, _stream_key: &str) -> anyhow::Result<Option<serde_json::Value>> { Ok(None) }
}