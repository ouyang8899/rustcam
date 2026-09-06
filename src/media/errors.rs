use thiserror::Error;

#[derive(Error, Debug)]
pub enum MediaError {
    #[error("RTP parse error: {0}")]
    RtpParse(String),

    #[error("RTP build error: {0}")]
    RtpBuild(String),

    #[error("RTCP parse error: {0}")]
    RtcpParse(String),

    #[error("RTCP build error: {0}")]
    RtcpBuild(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Transport send error: {0}")]
    TransportSend(String),

    #[error("Transport recv error: {0}")]
    TransportRecv(String),

    #[error("Codec error: {0}")]
    Codec(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Middleware error: {0}")]
    Middleware(String),

    #[error("Port allocation error: {0}")]
    PortAllocation(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),
}

pub type Result<T> = std::result::Result<T, MediaError>;
