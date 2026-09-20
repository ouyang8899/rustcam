use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SdpDirection {
    #[default]
    RecvOnly,
    SendOnly,
    SendRecv,
    Inactive,
}

#[derive(Debug, Clone)]
pub struct SdpTrack {
    pub media: String,
    pub payload_type: u8,
    pub codec: String,
    pub clock_rate: u32,
    pub fmtp: Option<String>,
    pub control: Option<String>,
    pub direction: SdpDirection,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct SdpInfo {
    pub session_name: String,
    pub origin: Option<String>,
    pub connection_addr: Option<String>,
    pub tracks: Vec<SdpTrack>,
    pub bandwidth: Option<String>,
    pub attributes: HashMap<String, String>,
}

pub struct SdpParser;

impl SdpParser {
    pub fn parse(sdp: &str) -> anyhow::Result<SdpInfo> {
        let mut info = SdpInfo::default();
        let mut pending_track: Option<Vec<PendingTrack>> = None;

        for line in sdp.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            let (key, val) = match line.split_once('=') {
                Some((k, v)) => (k.trim(), v.trim()),
                None => continue,
            };

            match key {
                "v" => {}
                "o" => { info.origin = Some(val.to_string()); }
                "s" => { info.session_name = val.to_string(); }
                "c" => {
                    if let Some(addr) = Self::parse_connection(val) {
                        info.connection_addr = Some(addr);
                    }
                }
                "t" => {}
                "b" => { info.bandwidth = Some(val.to_string()); }
                "a" => {
                    if let Some(pending_list) = pending_track.take() {
                        if let Some(updated) = Self::parse_attribute(val, Some(pending_list)) {
                            pending_track = Some(updated);
                        }
                    }
                }
                "m" => {
                    if let Some(pending_list) = pending_track.take() {
                        for pending in pending_list {
                            if let Some(track) = pending.into_track() { info.tracks.push(track); }
                        }
                    }
                    if let Some(pending_list) = Self::parse_media_line(val) {
                        pending_track = Some(pending_list);
                    }
                }
                _ => { info.attributes.insert(key.to_string(), val.to_string()); }
            }
        }

        if let Some(pending_list) = pending_track {
            for pending in pending_list {
                if let Some(track) = pending.into_track() { info.tracks.push(track); }
            }
        }

        Ok(info)
    }

    fn parse_connection(val: &str) -> Option<String> {
        let parts: Vec<&str> = val.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "IN" && parts[1] == "IP4" {
            Some(parts[2].to_string())
        } else { None }
    }

    fn parse_media_line(val: &str) -> Option<Vec<PendingTrack>> {
        let parts: Vec<&str> = val.split_whitespace().collect();
        if parts.len() >= 4 {
            let media = parts[0].to_string();
            let mut tracks = Vec::new();
            for part in parts[3..].iter() {
                if let Ok(payload_type) = part.parse::<u8>() {
                    tracks.push(PendingTrack::new(media.clone(), payload_type));
                }
            }
            if tracks.is_empty() { None } else { Some(tracks) }
        } else { None }
    }

    fn parse_attribute(val: &str, mut pending: Option<Vec<PendingTrack>>) -> Option<Vec<PendingTrack>> {
        if let Some(ref mut pending_list) = pending {
            if val.starts_with("rtpmap:") {
                let after_prefix = &val[7..];
                let parts: Vec<&str> = after_prefix.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(pt) = parts[0].parse::<u8>() {
                        if let Some(track) = pending_list.iter_mut().find(|t| t.payload_type == pt) {
                            if let Some((codec, rest)) = parts[1].split_once('/') {
                                track.codec = Some(codec.to_string());
                                track.clock_rate = rest.split('/').next().and_then(|s| s.parse().ok());
                            }
                        }
                    }
                }
            } else if val.starts_with("fmtp:") {
                let fmtp_part = &val[5..];
                if let Some((pt_str, params)) = fmtp_part.split_once(' ') {
                    if let Ok(pt) = pt_str.parse::<u8>() {
                        if let Some(track) = pending_list.iter_mut().find(|t| t.payload_type == pt) {
                            track.fmtp = Some(params.to_string());
                        }
                    }
                }
            } else if val.starts_with("control:") {
                for track in pending_list.iter_mut() {
                    track.control = Some(val[8..].to_string());
                }
            } else if val == "sendonly" {
                for track in pending_list.iter_mut() {
                    track.direction = SdpDirection::SendOnly;
                }
            } else if val == "recvonly" {
                for track in pending_list.iter_mut() {
                    track.direction = SdpDirection::RecvOnly;
                }
            } else if val == "sendrecv" {
                for track in pending_list.iter_mut() {
                    track.direction = SdpDirection::SendRecv;
                }
            } else if val == "inactive" {
                for track in pending_list.iter_mut() {
                    track.direction = SdpDirection::Inactive;
                }
            } else {
                for track in pending_list.iter_mut() {
                    track.attributes.insert(val.to_string(), String::new());
                }
            }
        }
        pending
    }

    pub fn build_sdp(stream_url: &str, bind_addr: &str, tracks: &[SdpTrack]) -> String {
        let mut lines = vec![
            "v=0".to_string(),
            format!("o=- 0 0 IN IP4 {}", bind_addr),
            format!("s={}", stream_url),
            format!("c=IN IP4 {}", bind_addr),
            "t=0 0".to_string(),
        ];

        for track in tracks {
            lines.push(format!("m={} 0 RTP/AVP {}", track.media, track.payload_type));
            lines.push(format!("a=rtpmap:{} {}/{}", track.payload_type, track.codec, track.clock_rate));
            if let Some(ref fmtp) = track.fmtp {
                lines.push(format!("a=fmtp:{} {}", track.payload_type, fmtp));
            }
            if let Some(ref ctrl) = track.control {
                lines.push(format!("a=control:{}", ctrl));
            }
        }

        lines.join("\r\n")
    }
}

#[derive(Default)]
struct PendingTrack {
    media: String,
    payload_type: u8,
    codec: Option<String>,
    clock_rate: Option<u32>,
    fmtp: Option<String>,
    control: Option<String>,
    direction: SdpDirection,
    attributes: HashMap<String, String>,
}

impl PendingTrack {
    fn new(media: String, payload_type: u8) -> Self {
        Self { media, payload_type, ..Default::default() }
    }

    fn into_track(self) -> Option<SdpTrack> {
        Some(SdpTrack {
            media: self.media,
            payload_type: self.payload_type,
            codec: self.codec.unwrap_or_else(|| "H264".to_string()),
            clock_rate: self.clock_rate.unwrap_or(90000),
            fmtp: self.fmtp,
            control: self.control,
            direction: self.direction,
            attributes: self.attributes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_h264_video() {
        let sdp = r#"v=0
o=- 1234567890 1234567890 IN IP4 192.168.1.100
s=Media Server
c=IN IP4 0.0.0.0
t=0 0
m=video 0 RTP/AVP 96
a=rtpmap:96 H264/90000
a=fmtp:96 packetization-mode=1"#;
        let parsed = SdpParser::parse(sdp).unwrap();
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].codec, "H264");
        assert_eq!(parsed.tracks[0].payload_type, 96);
        assert_eq!(parsed.tracks[0].clock_rate, 90000);
    }

    #[test]
    fn test_parse_dual_track() {
        let sdp = r#"v=0
o=- 1 1 IN IP4 127.0.0.1
s=Dual
c=IN IP4 0.0.0.0
t=0 0
m=video 0 RTP/AVP 96
a=rtpmap:96 H264/90000
m=audio 0 RTP/AVP 97
a=rtpmap:97 MPEG4/44100"#;
        let parsed = SdpParser::parse(sdp).unwrap();
        assert_eq!(parsed.tracks.len(), 2);
        assert_eq!(parsed.tracks[0].media, "video");
        assert_eq!(parsed.tracks[1].media, "audio");
    }

    #[test]
    fn test_build_sdp() {
        let tracks = vec![SdpTrack {
            media: "video".to_string(), payload_type: 96, codec: "H264".to_string(),
            clock_rate: 90000, fmtp: Some("packetization-mode=1".to_string()), control: None,
            direction: SdpDirection::RecvOnly,
            attributes: std::collections::HashMap::new(),
        }];
        let sdp = SdpParser::build_sdp("live/test", "0.0.0.0", &tracks);
        assert!(sdp.contains("H264/90000"));
        assert!(sdp.contains("m=video"));
    }
}