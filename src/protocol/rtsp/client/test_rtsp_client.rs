use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use rustcam_media::protocol::rtsp::client::client::RtspClient;
use rustcam_media::protocol::rtsp::client::auth::AuthCredentials;
use rustcam_media::protocol::rtsp::client::types::{MediaDirection, RtpTransportMode};
use rustcam_media::protocol::rtsp::common::RtspError;

struct TestCallback {
    rtp_count: Arc<RwLock<usize>>,
    rtcp_count: Arc<RwLock<usize>>,
}

impl TestCallback {
    fn new() -> Self {
        Self {
            rtp_count: Arc::new(RwLock::new(0)),
            rtcp_count: Arc::new(RwLock::new(0)),
        }
    }
}

impl rustcam_media::protocol::rtsp::client::client::RtspClientCallback for TestCallback {
    fn on_state_change(&self, state: &rustcam_media::protocol::rtsp::common::RtspState) {
    }

    fn on_rtp_packet(&self, track_id: &str, data: &[u8], ssrc: u32) {
        let len = data.len();
    }

    fn on_rtcp_packet(
        &self,
        track_id: &str,
        data: &[u8],
        ssrc: u32,
        report_type: &rustcam_media::protocol::rtsp::common::RtcpReportType,
    ) {
    }

    fn on_error(&self, error: &RtspError) {
    }

    fn on_teardown_complete(&self) {
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let url = "rtsp://192.168.1.211:554/Streaming/Channels/101?transportmode=unicast&profile=Profile_1";
    let credentials = AuthCredentials {
        username: "admin".to_string(),
        password: "<PASSWORD>".to_string(),
    };

    let callback = Arc::new(TestCallback::new());

    let client = RtspClient::new(
        url.to_string(),
        credentials,
        RtpTransportMode::Tcp,
        callback.clone(),
    )?;
    let client = Arc::new(client);

    client.connect().await?;

    client.start_rtsp_receive().await?;

    match client.options().await {
        Ok(resp) => {
        }
        Err(e) => {
            return Err(Box::new(e));
        }
    }

    match client.describe().await {
        Ok(tracks) => {
            for track in &tracks {
            }
        }
        Err(e) => {
            return Err(Box::new(e));
        }
    }

    let tracks = client.get_tracks().await;
    if tracks.is_empty() {
        return Err("No tracks".into());
    }

    for track in &tracks {
        match client.setup(&track.track_id, MediaDirection::RecvOnly).await {
            Ok(_) => {}
            Err(e) => {
            }
        }
    }

    match client.play().await {
        Ok(_) => {
        }
        Err(e) => {
            return Err(Box::new(e));
        }
    }

    client.start().await?;

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        print!(".");
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }

    match client.teardown().await {
        Ok(_) => {}
        Err(e) => {}
    }

    Ok(())
}
