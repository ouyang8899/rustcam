#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{RwLock, mpsc};

    use crate::protocol::rtsp::client::auth::AuthCredentials;
    use crate::protocol::rtsp::client::types::{MediaDirection, RtpTransportMode};
    use crate::protocol::rtsp::client::client::RtspClient;
    use crate::protocol::rtsp::client::state::RtspState;
    use crate::protocol::rtsp::common::RtspError;
    use crate::protocol::rtsp::common::RtcpReportType;

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

    impl crate::protocol::rtsp::client::client::RtspClientCallback for TestCallback {
        fn on_state_change(&self, state: &RtspState) {
            println!("[Callback] State changed: {:?}", state);
        }

        fn on_rtp_packet(&self, track_id: &str, data: &[u8], ssrc: u32) {
            let len = data.len();
            println!(
                "[Callback] RTP: track={}, len={}, ssrc=0x{:08x}",
                track_id, len, ssrc
            );
        }

        fn on_rtcp_packet(
            &self,
            track_id: &str,
            data: &[u8],
            ssrc: u32,
            report_type: &RtcpReportType,
        ) {
            println!(
                "[Callback] RTCP: track={}, ssrc=0x{:08x}, type={:?}",
                track_id, ssrc, report_type
            );
        }

        fn on_error(&self, error: &RtspError) {
            eprintln!("[Callback] Error: {:?}", error);
        }

        fn on_teardown_complete(&self) {
            println!("[Callback] Teardown complete");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_rtsp_client_full_session() {
        let url = "rtsp://192.168.1.211:554/Streaming/Channels/101?transportmode=unicasttransportmode=unicast&profile=Profile_1";
        let credentials = AuthCredentials {
            username: "admin".to_string(),
            password: "sjzl8888".to_string(),
        };

        let callback = Arc::new(TestCallback::new());

        println!("=== RTSP Client Test ===");
        println!("URL: {}", url);
        println!();

        println!("[1] Creating RTSP client...");
        let client = RtspClient::new(
            url.to_string(),
            credentials,
            RtpTransportMode::Tcp,
            callback.clone(),
        )
        .expect("Failed to create RtspClient");
        let client = Arc::new(client);

        println!("[2] Connecting to server...");
        client.connect().await.expect("connect failed");
        println!("[2] Connected!");

        println!("[3] Starting RTSP receive loop...");
        client
            .start_rtsp_receive()
            .await
            .expect("start_rtsp_receive failed");
        println!("[3] Receive loop started!");

        println!("[4] Sending OPTIONS...");
        match client.options().await {
            Ok(resp) => {
                println!("[4] OPTIONS success!");
                println!("    Response:\n{}", resp);
            }
            Err(e) => {
                eprintln!("[4] OPTIONS failed: {:?}", e);
                panic!("OPTIONS failed: {:?}", e);
            }
        }

        println!("\n[5] Sending DESCRIBE...");
        let tracks = match client.describe().await {
            Ok(tracks) => {
                println!("[5] DESCRIBE success! Found {} tracks:", tracks.len());
                for track in &tracks {
                    println!("    - {}: {:?}", track.track_id, track.media_type);
                }
                tracks
            }
            Err(e) => {
                eprintln!("[5] DESCRIBE failed: {:?}", e);
                eprintln!("[5] Keeping connection alive for debugging...");
                return;
            }
        };

        if tracks.is_empty() {
            eprintln!("[5] No tracks found!");
            return;
        }

        println!("\n[6] Setting up tracks...");
        for track in &tracks {
            println!("    Setting up: {}", track.track_id);
            match client.setup(&track.track_id, MediaDirection::RecvOnly).await {
                Ok(_) => println!("    {} SETUP success!", track.track_id),
                Err(e) => {
                    eprintln!("    {} SETUP failed: {:?}", track.track_id, e);
                }
            }
        }

        println!("\n[7] Sending PLAY...");
        match client.play().await {
            Ok(_) => {
                println!("[7] PLAY success!");
            }
            Err(e) => {
                eprintln!("[7] PLAY failed: {:?}", e);
                panic!("PLAY failed: {:?}", e);
            }
        }

        println!("\n[8] Starting data plane...");
        client.start().await.expect("start failed");
        println!("[8] Data plane started!");

        println!("\n=== Receiving RTP for 10 seconds ===");
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            tokio::time::sleep(Duration::from_secs(1)).await;
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
        println!();

        println!("\n[9] Sending TEARDOWN...");
        match client.teardown().await {
            Ok(_) => println!("[9] TEARDOWN success!"),
            Err(e) => eprintln!("[9] TEARDOWN failed: {:?}", e),
        }

        println!("\n=== Test completed ===");
    }

    #[tokio::test]
    #[ignore]
    async fn test_rtsp_backchannel() {
        let url = "rtsp://192.168.1.211:554/Streaming/Channels/101?transportmode=unicasttransportmode=unicast&profile=Profile_1";
        let credentials = AuthCredentials {
            username: "admin".to_string(),
            password: "sjzl8888".to_string(),
        };

        let callback = Arc::new(TestCallback::new());

        println!("=== RTSP Backchannel Test ===");
        println!("URL: {}", url);
        println!();

        println!("[1] Creating RTSP client...");
        let client = RtspClient::new(
            url.to_string(),
            credentials,
            RtpTransportMode::Tcp,
            callback.clone(),
        )
        .expect("Failed to create RtspClient");
        let client = Arc::new(client);

        println!("[2] Connecting to server...");
        client.connect().await.expect("connect failed");
        println!("[2] Connected!");

        println!("[3] Starting RTSP receive loop...");
        client
            .start_rtsp_receive()
            .await
            .expect("start_rtsp_receive failed");
        println!("[3] Receive loop started!");

        println!("\n[4] Sending OPTIONS...");
        match client.options().await {
            Ok(resp) => {
                println!("[4] OPTIONS success!");
                println!("    Response:\n{}", resp);
            }
            Err(e) => {
                eprintln!("[4] OPTIONS failed: {:?}", e);
                panic!("OPTIONS failed: {:?}", e);
            }
        }

        println!("\n[5] Sending DESCRIBE with backchannel require...");
        let tracks = match client.describe_with_backchannel().await {
            Ok(tracks) => {
                println!("[5] DESCRIBE (backchannel) success! Found {} tracks:", tracks.len());
                for track in &tracks {
                    println!("    - {}: {:?} codec={} direction={:?}",
                        track.track_id, track.media_type, track.codec, track.direction);
                    if let Some(ref ctrl) = track.control_url {
                        println!("      control_url: {}", ctrl);
                    }
                    println!("      attributes: {:?}", track.attributes.keys().collect::<Vec<_>>());
                }
                tracks
            }
            Err(e) => {
                eprintln!("[5] DESCRIBE (backchannel) failed: {:?}", e);
                if matches!(e, RtspError::BackchannelNotSupported) {
                    println!("[5] Device does not support backchannel!");
                }
                return;
            }
        };

        println!("\n[6] Looking for backchannel track (Audio + SendOnly)...");
        match client.get_backchannel_track().await {
            Some(track) => {
                println!("[6] Found backchannel track:");
                println!("    - track_id: {}", track.track_id);
                println!("    - codec: {}", track.codec);
                println!("    - control_url: {:?}", track.control_url);
            }
            None => {
                println!("[6] No backchannel track found (no Audio/SendOnly track)");
            }
        }

        println!("\n[7] Sending TEARDOWN...");
        match client.teardown().await {
            Ok(_) => println!("[7] TEARDOWN success!"),
            Err(e) => eprintln!("[7] TEARDOWN failed: {:?}", e),
        }

        println!("\n=== Test completed ===");
    }

    #[tokio::test]
    async fn test_rtsp_url_parsing() {
        let url = "rtsp://192.168.1.211:554/Streaming/Channels/101?transportmode=unicast&profile=Profile_1";

        let rest = url
            .strip_prefix("rtsp://")
            .unwrap();

        println!("URL after rtsp://: {}", rest);

        let rest_without_auth = if let Some(at) = rest.find('@') {
            println!("Has auth, stripped: {}", &rest[at + 1..]);
            &rest[at + 1..]
        } else {
            rest
        };

        let (host_port, path) = if let Some(slash) = rest_without_auth.find('/') {
            (&rest_without_auth[..slash], &rest_without_auth[slash..])
        } else {
            (rest_without_auth, "/")
        };

        println!("host_port: {}, path: {}", host_port, path);

        let (host, port) = if let Some(colon) = host_port.rfind(':') {
            let port_str = &host_port[colon + 1..];
            let port: u16 = port_str.parse().unwrap();
            (&host_port[..colon], port)
        } else {
            (host_port, 554)
        };

        println!("Parsed: host={}, port={}, path={}", host, port, path);
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 554);
    }

    #[tokio::test]
    async fn test_rtsp_url_with_auth_parsing() {
        let url = "rtsp://admin:sjzl8888@192.168.1.211:554/Streaming/Channels/101";

        let rest = url.strip_prefix("rtsp://").unwrap();

        let (auth, rest_without_auth) = if let Some(at) = rest.find('@') {
            (&rest[..at], &rest[at + 1..])
        } else {
            ("", rest)
        };

        println!("Auth: {}", auth);
        println!("Rest: {}", rest_without_auth);

        let (host_port, path) = if let Some(slash) = rest_without_auth.find('/') {
            (&rest_without_auth[..slash], &rest_without_auth[slash..])
        } else {
            (rest_without_auth, "/")
        };

        let (host, port) = if let Some(colon) = host_port.rfind(':') {
            let port_str = &host_port[colon + 1..];
            let port: u16 = port_str.parse().unwrap();
            (&host_port[..colon], port)
        } else {
            (host_port, 554)
        };

        println!("Parsed: host={}, port={}, path={}", host, port, path);
        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 554);
        assert_eq!(path, "/Streaming/Channels/101");
    }
}
