use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use crate::protocol::rtsp::common::{ReceivedData, RtcpReportType, RtcpStats, RtpUpdateResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RtpTransportMode {
    TcpInterleaved,
    Udp,
}

impl Default for RtpTransportMode {
    fn default() -> Self {
        RtpTransportMode::TcpInterleaved
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

#[derive(Debug, Clone)]
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
    pub media_type: MediaType,
    pub codec: String,
    pub direction: MediaDirection,
    pub fmtp: Option<String>,
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
            media_type,
            codec,
            direction: MediaDirection::RecvOnly,
            fmtp: None,
            transport: TrackTransport::default(),
            state: TrackState::Pending,
            ssrc: None,
            remote_ssrc: None,
            rtcp_stats: RtcpStats::new(),
        }
    }
}
