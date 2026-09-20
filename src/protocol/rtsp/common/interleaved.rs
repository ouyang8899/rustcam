use crate::protocol::rtsp::common::{RtcpPacket, RtcpReportType, RtcpStats};

pub const INTERLEAVED_MAGIC: u8 = 0x24;
pub const MAX_INTERLEAVED_BUFFER: usize = 1024 * 1024;
pub const MAX_INTERLEAVED_PAYLOAD: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct InterleavedFrame {
    pub channel: u8,
    pub payload: Vec<u8>,
    pub is_rtp: bool,
}

pub struct InterleavedParser {
    buffer: Vec<u8>,
}

impl InterleavedParser {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(8192),
        }
    }

    pub fn parse(&mut self, data: &[u8]) -> (Vec<InterleavedFrame>, bool) {
        if self.buffer.len() + data.len() > MAX_INTERLEAVED_BUFFER {
            tracing::error!(
                "[InterleavedParser] Buffer overflow ({} + {} > {}), clearing",
                self.buffer.len(),
                data.len(),
                MAX_INTERLEAVED_BUFFER
            );
            self.buffer.clear();
            return (vec![], false);
        }

        self.buffer.extend_from_slice(data);
        let mut frames = Vec::new();
        let mut offset = 0;

        while offset + 4 <= self.buffer.len() {
            if self.buffer[offset] != INTERLEAVED_MAGIC {
                tracing::warn!(
                    "[InterleavedParser] Invalid magic byte: 0x{:02x} at offset {}",
                    self.buffer[offset],
                    offset
                );
                self.buffer.clear();
                return (frames, false);
            }

            let channel = self.buffer[offset + 1];
            let payload_len = u16::from_be_bytes([
                self.buffer[offset + 2],
                self.buffer[offset + 3],
            ]) as usize;

            if payload_len > MAX_INTERLEAVED_PAYLOAD {
                tracing::warn!(
                    "[InterleavedParser] Payload too large: {} > {}",
                    payload_len,
                    MAX_INTERLEAVED_PAYLOAD
                );
                self.buffer.drain(..offset + 4);
                return (frames, true);
            }

            let total_len = 4 + payload_len;

            if offset + total_len > self.buffer.len() {
                break;
            }

            let payload = self.buffer[offset + 4..offset + total_len].to_vec();
            let is_rtp = channel % 2 == 0;

            frames.push(InterleavedFrame {
                channel,
                payload,
                is_rtp,
            });

            offset += total_len;
        }

        if offset > 0 {
            self.buffer.drain(..offset);
        }

        (frames, true)
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for InterleavedParser {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InterleavedBuilder;

impl InterleavedBuilder {
    pub fn build_frame(channel: u8, payload: &[u8]) -> Vec<u8> {
        let len = payload.len() as u16;
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.push(INTERLEAVED_MAGIC);
        frame.push(channel);
        frame.push((len >> 8) as u8);
        frame.push((len & 0xff) as u8);
        frame.extend_from_slice(payload);
        frame
    }

    pub fn build_rtp_frame(channel: u8, rtp_data: &[u8]) -> Vec<u8> {
        Self::build_frame(channel, rtp_data)
    }

    pub fn build_rtcp_frame(channel: u8, rtcp_data: &[u8]) -> Vec<u8> {
        Self::build_frame(channel, rtcp_data)
    }
}

pub fn parse_interleaved_channels(transport: &str) -> Option<(u8, u8)> {
    for part in transport.split(';') {
        let part = part.trim();
        if part.starts_with("interleaved=") {
            let val = &part[12..];
            let ports: Vec<&str> = val.split('-').collect();
            if ports.len() == 2 {
                let rtp_channel: u8 = ports[0].parse().ok()?;
                let rtcp_channel: u8 = ports[1].parse().ok()?;
                return Some((rtp_channel, rtcp_channel));
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct InterleavedStats {
    pub rtcp_stats: RtcpStats,
    pub last_frame_time: Option<std::time::Instant>,
    pub frame_count: u64,
    pub rtcp_frame_count: u64,
    pub rtp_frame_count: u64,
    pub total_bytes: u64,
}

impl InterleavedStats {
    pub fn new() -> Self {
        Self {
            rtcp_stats: RtcpStats::new(),
            last_frame_time: None,
            frame_count: 0,
            rtcp_frame_count: 0,
            rtp_frame_count: 0,
            total_bytes: 0,
        }
    }

    pub fn process_frame(&mut self, frame: &InterleavedFrame) {
        self.frame_count += 1;
        self.total_bytes += frame.payload.len() as u64;
        self.last_frame_time = Some(std::time::Instant::now());

        if frame.is_rtp {
            self.rtp_frame_count += 1;
            if frame.payload.len() >= 12 {
                let seq = u16::from_be_bytes([frame.payload[2], frame.payload[3]]);
                let timestamp = u32::from_be_bytes([frame.payload[4], frame.payload[5], frame.payload[6], frame.payload[7]]);
                if let Some(ssrc) = RtcpPacket::extract_ssrc_from_rtp(&frame.payload) {
                    self.rtcp_stats.last_remote_ssrc = Some(ssrc);
                }
                self.rtcp_stats.update_from_rtp(seq, timestamp, std::time::Instant::now());
            }
        } else {
            self.rtcp_frame_count += 1;
            if let Some((ssrc, report_type)) = RtcpPacket::parse(&frame.payload) {
                if let RtcpReportType::SenderReport {
                    ntp_timestamp,
                    rtp_timestamp,
                    sender_packet_count,
                    sender_octet_count,
                } = report_type {
                    self.rtcp_stats.update_from_sr(ntp_timestamp, rtp_timestamp, sender_packet_count, sender_octet_count);
                }
            }
        }
    }
}

impl Default for InterleavedStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_frame() {
        let payload = vec![1, 2, 3, 4];
        let frame = InterleavedBuilder::build_frame(0, &payload);
        assert_eq!(frame[0], INTERLEAVED_MAGIC);
        assert_eq!(frame[1], 0);
        assert_eq!(frame[2], 0);
        assert_eq!(frame[3], 4);
        assert_eq!(&frame[4..], &payload[..]);
    }

    #[test]
    fn test_parse_channels() {
        assert_eq!(parse_interleaved_channels("RTP/AVP/TCP;unicast;interleaved=0-1"), Some((0, 1)));
        assert_eq!(parse_interleaved_channels("RTP/AVP/TCP;unicast;interleaved=2-3"), Some((2, 3)));
        assert_eq!(parse_interleaved_channels("RTP/AVP/UDP"), None);
    }
}
