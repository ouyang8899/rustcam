use super::types::MediaTrack;

#[derive(Debug, Clone)]
pub enum RtspState {
    Disconnected,
    Connecting,
    Connected,
    Described { tracks: Vec<MediaTrack> },
    Ready { tracks: Vec<MediaTrack> },
    Playing { tracks: Vec<MediaTrack> },
    Teardown,
    Error(String),
}

impl RtspState {
    pub fn is_connected(&self) -> bool {
        matches!(
            self,
            RtspState::Connected
                | RtspState::Described { .. }
                | RtspState::Ready { .. }
                | RtspState::Playing { .. }
        )
    }

    pub fn can_send_rtp(&self) -> bool {
        matches!(self, RtspState::Playing { .. })
    }
}
