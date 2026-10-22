use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use crate::protocol::rtsp::common::{ RtcpStats };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RtpTransportMode {
    Tcp,
    Udp,
}

impl Default for RtpTransportMode {
    fn default() -> Self {
        RtpTransportMode::Tcp
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaDirection {
    RecvOnly,
    SendOnly,
    SendRecv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    Video,
    Audio,
    Application,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackState {
    Pending,
    Setup,
    Playing,
    Teardown,
}

#[derive(Clone)]
pub enum TrackTransport {
    Udp {
        rtp_port: u16,
        rtcp_port: u16,
        local_rtp_port: u16,
        local_rtcp_port: u16,
        server_addr: Option<SocketAddr>,
        server: Option<Arc<crate::media::rtp_server::UDPRtpServer>>,
    },
    Tcp {
        interleaved: (u8, u8),
    },
}

impl std::fmt::Debug for TrackTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackTransport::Udp { rtp_port, rtcp_port, local_rtp_port, local_rtcp_port, server_addr, server: _ } => {
                f.debug_struct("TrackTransport::Udp")
                    .field("rtp_port", rtp_port)
                    .field("rtcp_port", rtcp_port)
                    .field("local_rtp_port", local_rtp_port)
                    .field("local_rtcp_port", local_rtcp_port)
                    .field("server_addr", server_addr)
                    .finish()
            }
            TrackTransport::Tcp { interleaved } => {
                f.debug_struct("TrackTransport::Tcp")
                    .field("interleaved", interleaved)
                    .finish()
            }
        }
    }
}

impl Default for TrackTransport {
    fn default() -> Self {
        TrackTransport::Udp {
            rtp_port: 0,
            rtcp_port: 0,
            local_rtp_port: 0,
            local_rtcp_port: 0,
            server_addr: None,
            server: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaTrack {
    pub track_id: String,
    pub control_url: Option<String>,
    pub media_type: MediaType,
    pub codec: String,
    pub payload_type: Option<u8>,
    pub clock_rate: Option<u32>,
    pub direction: MediaDirection,
    pub fmtp: Option<String>,
    pub attributes: HashMap<String, String>,
    pub transport: TrackTransport,
    pub state: TrackState,
    pub ssrc: Option<u32>,
    pub remote_ssrc: Option<u32>,
    pub rtcp_stats: RtcpStats,
}

impl MediaTrack {
    pub fn new(track_id: String, media_type: MediaType, codec: String) -> Self {
        Self {
            track_id,
            control_url: None,
            media_type,
            codec,
            payload_type: None,
            clock_rate: None,
            direction: MediaDirection::RecvOnly,
            fmtp: None,
            attributes: HashMap::new(),
            transport: TrackTransport::default(),
            state: TrackState::Pending,
            ssrc: None,
            remote_ssrc: None,
            rtcp_stats: RtcpStats::new(),
        }
    }
}
