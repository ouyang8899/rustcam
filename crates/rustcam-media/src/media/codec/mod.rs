pub mod g711;

pub use g711::G711;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Pcma,
    Pcmu,
    Aac,
}

impl AudioCodec {
    pub fn payload_type(&self) -> u8 {
        match self {
            AudioCodec::Pcma => 8,
            AudioCodec::Pcmu => 0,
            AudioCodec::Aac => 97,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        match self {
            AudioCodec::Pcma | AudioCodec::Pcmu => 8000,
            AudioCodec::Aac => 44100,
        }
    }
}

impl std::fmt::Display for AudioCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioCodec::Pcma => write!(f, "PCMA"),
            AudioCodec::Pcmu => write!(f, "PCMU"),
            AudioCodec::Aac => write!(f, "AAC"),
        }
    }
}
