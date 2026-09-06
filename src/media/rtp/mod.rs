pub mod header;
pub mod packet;

pub use header::RtpHeader;
pub use packet::RtpPacket;

pub const RTP_VERSION: u8 = 2;

pub const PAYLOAD_TYPE_PCMU: u8 = 0;
pub const PAYLOAD_TYPE_PCMA: u8 = 8;
pub const PAYLOAD_TYPE_JPEG: u8 = 26;
pub const PAYLOAD_TYPE_H264: u8 = 96;
pub const PAYLOAD_TYPE_H265: u8 = 97;
pub const PAYLOAD_TYPE_VP8: u8 = 100;
pub const PAYLOAD_TYPE_VP9: u8 = 98;
pub const PAYLOAD_TYPE_OPUS: u8 = 96;
pub const PAYLOAD_TYPE_AAC: u8 = 97;

pub const RTP_TICK_RATE_AUDIO: u32 = 8000;
pub const RTP_TIMESTAMP_INCREMENT_20MS: u32 = 160;
pub const RTP_TIMESTAMP_INCREMENT_30MS: u32 = 240;

pub fn build_audio_rtp_packet(
    payload: &[u8],
    payload_type: u8,
    timestamp: u32,
    ssrc: u32,
    sequence: u16,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(12 + payload.len());
    buf.push(0x80);
    buf.push(payload_type & 0x7F);
    buf.extend_from_slice(&sequence.to_be_bytes());
    buf.extend_from_slice(&timestamp.to_be_bytes());
    buf.extend_from_slice(&ssrc.to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}
