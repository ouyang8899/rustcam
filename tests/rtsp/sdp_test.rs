// tests/rtsp/sdp_test.rs
// Note: integration tests go here since lib tests need the module exported

#[cfg(test)]
mod tests {
    use rustcam_media::protocol::rtsp::sdp::{SdpParser, SdpTrack};

    #[test]
    fn test_parse_h264_video_sdp() {
        let sdp = r#"v=0
o=- 1234567890 1234567890 IN IP4 192.168.1.100
s=Media Server
c=IN IP4 0.0.0.0
t=0 0
m=video 0 RTP/AVP 96
a=rtpmap:96 H264/90000
a=fmtp:96 profile-level-id=4D001E; packetization-mode=1
"#;
        let parsed = SdpParser::parse(sdp).unwrap();
        assert_eq!(parsed.session_name, "Media Server");
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].codec, "H264");
        assert_eq!(parsed.tracks[0].payload_type, 96);
        assert_eq!(parsed.tracks[0].clock_rate, 90000);
    }

    #[test]
    fn test_parse_audio_pcmu() {
        let sdp = r#"v=0
o=- 1 1 IN IP4 127.0.0.1
s=Test
c=IN IP4 0.0.0.0
t=0 0
m=audio 0 RTP/AVP 0
a=rtpmap:0 PCMU/8000
"#;
        let parsed = SdpParser::parse(sdp).unwrap();
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].codec, "PCMU");
        assert_eq!(parsed.tracks[0].payload_type, 0);
        assert_eq!(parsed.tracks[0].clock_rate, 8000);
    }

    #[test]
    fn test_parse_video_and_audio() {
        let sdp = r#"v=0
o=- 1 1 IN IP4 127.0.0.1
s=Dual Track
c=IN IP4 0.0.0.0
t=0 0
m=video 0 RTP/AVP 96
a=rtpmap:96 H264/90000
m=audio 0 RTP/AVP 97
a=rtpmap:97 MPEG4-generic/44100
"#;
        let parsed = SdpParser::parse(sdp).unwrap();
        assert_eq!(parsed.tracks.len(), 2);
        assert_eq!(parsed.tracks[0].media, "video");
        assert_eq!(parsed.tracks[1].media, "audio");
    }

    #[test]
    fn test_build_sdp() {
        let tracks = vec![SdpTrack {
            media: "video".to_string(),
            payload_type: 96,
            codec: "H264".to_string(),
            clock_rate: 90000,
            fmtp: Some("profile-level-id=4D001E; packetization-mode=1".to_string()),
            control: None,
        }];
        let sdp = SdpParser::build_sdp("live/test", "0.0.0.0", &tracks);
        assert!(sdp.contains("H264/90000"));
        assert!(sdp.contains("m=video"));
        assert!(sdp.contains("96"));
        assert!(sdp.contains("live/test"));
    }

    #[test]
    fn test_parse_h265() {
        let sdp = r#"v=0
o=- 1 1 IN IP4 127.0.0.1
s=H265 Camera
c=IN IP4 0.0.0.0
t=0 0
m=video 0 RTP/AVP 98
a=rtpmap:98 H265/90000
a=fmtp:98 profile-space=0; tier-flag=0; level-idc=93
"#;
        let parsed = SdpParser::parse(sdp).unwrap();
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].codec, "H265");
        assert_eq!(parsed.tracks[0].payload_type, 98);
    }

    #[test]
    fn test_parse_aac() {
        let sdp = r#"v=0
o=- 1 1 IN IP4 127.0.0.1
s=Audio
c=IN IP4 0.0.0.0
t=0 0
m=audio 0 RTP/AVP 97
a=rtpmap:97 mpeg4-generic/44100/2
a=fmtp:97 profile-level-id=1; config=1208; mode=AAC-hbr
"#;
        let parsed = SdpParser::parse(sdp).unwrap();
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].codec, "mpeg4-generic");
        assert_eq!(parsed.tracks[0].clock_rate, 44100);
    }

    #[test]
    fn test_parse_empty_sdp() {
        let sdp = "";
        let parsed = SdpParser::parse(sdp).unwrap();
        assert_eq!(parsed.tracks.len(), 0);
    }
}
