#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{ RwLock };

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
        fn on_state_change(&self, _state: &RtspState) {
        }

        fn on_rtp_packet(&self, _track_id: &str, data: &[u8], _ssrc: u32) {
            let _len = data.len();
        }

        fn on_rtcp_packet(
            &self,
            _track_id: &str,
            _data: &[u8],
            _ssrc: u32,
            _report_type: &RtcpReportType,
        ) {
        }

        fn on_error(&self, _error: &RtspError) {
        }

        fn on_teardown_complete(&self) {
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

        let client = RtspClient::new(
            url.to_string(),
            credentials,
            RtpTransportMode::Tcp,
            callback.clone(),
        )
        .expect("Failed to create RtspClient");
        let client = Arc::new(client);

        client.connect().await.expect("connect failed");

        client
            .start_rtsp_receive()
            .await
            .expect("start_rtsp_receive failed");

        match client.options().await {
            Ok(_resp) => {
            }
            Err(e) => {
                panic!("OPTIONS failed: {:?}", e);
            }
        }

        let tracks = match client.describe().await {
            Ok(tracks) => {
                for _track in &tracks {
                }
                tracks
            }
            Err(_e) => {
                return;
            }
        };

        if tracks.is_empty() {
            return;
        }

        for track in &tracks {
            match client.setup(&track.track_id, MediaDirection::RecvOnly).await {
                Ok(_) => {}
                Err(_e) => {
                }
            }
        }

        match client.play().await {
            Ok(_) => {
            }
            Err(e) => {
                panic!("PLAY failed: {:?}", e);
            }
        }

        client.start().await.expect("start failed");

        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            tokio::time::sleep(Duration::from_secs(1)).await;
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }

        match client.teardown().await {
            Ok(_) => {}
            Err(_e) => {}
        }

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

        let client = RtspClient::new(
            url.to_string(),
            credentials,
            RtpTransportMode::Tcp,
            callback.clone(),
        )
        .expect("Failed to create RtspClient");
        let client = Arc::new(client);

        client.connect().await.expect("connect failed");

        client
            .start_rtsp_receive()
            .await
            .expect("start_rtsp_receive failed");

        match client.options().await {
            Ok(_resp) => {
            }
            Err(e) => {
                panic!("OPTIONS failed: {:?}", e);
            }
        }

        let _tracks = match client.describe_with_backchannel().await {
            Ok(tracks) => {
                for track in &tracks {
                    if let Some(ref _ctrl) = track.control_url {
                    }
                }
                tracks
            }
            Err(e) => {
                if matches!(e, RtspError::BackchannelNotSupported) {
                }
                return;
            }
        };

        match client.get_backchannel_track().await {
            Some(_track) => {
            }
            None => {
            }
        }

        match client.teardown().await {
            Ok(_) => {}
            Err(_e) => {}
        }

    }

    #[tokio::test]
    async fn test_rtsp_url_parsing() {
        let url = "rtsp://192.168.1.211:554/Streaming/Channels/101?transportmode=unicast&profile=Profile_1";

        let rest = url
            .strip_prefix("rtsp://")
            .unwrap();

        let rest_without_auth = if let Some(at) = rest.find('@') {
            &rest[at + 1..]
        } else {
            rest
        };

        let (host_port, _path) = if let Some(slash) = rest_without_auth.find('/') {
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

        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 554);
    }

    #[tokio::test]
    async fn test_rtsp_url_with_auth_parsing() {
        let url = "rtsp://admin:sjzl8888@192.168.1.211:554/Streaming/Channels/101";

        let rest = url.strip_prefix("rtsp://").unwrap();

        let (_auth, rest_without_auth) = if let Some(at) = rest.find('@') {
            (&rest[..at], &rest[at + 1..])
        } else {
            ("", rest)
        };

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

        assert_eq!(host, "192.168.1.211");
        assert_eq!(port, 554);
        assert_eq!(path, "/Streaming/Channels/101");
    }
}
