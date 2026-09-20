use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum ReceivedData {
    RtspResponse(String),
    RtpData { track_id: String, data: Vec<u8>, ssrc: u32 },
    RtcpData { track_id: String, data: Vec<u8>, ssrc: u32, report_type: RtcpReportType },
}

#[derive(Debug, Clone)]
pub enum RtcpReportType {
    SenderReport {
        ntp_timestamp: u64,
        rtp_timestamp: u32,
        sender_packet_count: u32,
        sender_octet_count: u32,
    },
    ReceiverReport {
        fraction_lost: u8,
        lost_count: u32,
        jitter: u32,
    },
    Bye {
        reason: Option<String>,
    },
    Unknown,
}

#[derive(Debug, Clone, Default)]
pub struct RtcpStats {
    pub last_sr_ntp: u64,
    pub last_sr_time: Option<std::time::Instant>,
    pub last_sr_delay: u32,
    pub fraction_lost: u8,
    pub lost_count: u32,
    pub last_seq: u16,
    pub jitter: u32,
    pub last_rtp_time: u32,
    pub packet_count: u32,
    pub octet_count: u32,
    pub last_remote_ssrc: Option<u32>,
}

impl RtcpStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_from_rtp(&mut self, seq: u16, timestamp: u32, arrival: std::time::Instant) -> Option<RtpUpdateResult> {
        let mut result = None;

        if self.last_seq != 0 {
            let diff = (seq as i32 - self.last_seq as i32) & 0xFFFF;

            if diff == 0 {
                tracing::warn!("Duplicate RTP packet detected, seq={}", seq);
            } else if diff > 0x8000 {
                tracing::warn!("RTP sequence number wrap or severe reordering detected, prev={} curr={}", self.last_seq, seq);
                result = Some(RtpUpdateResult::SequenceWraparound);
            } else if diff > 1000 {
                tracing::debug!("RTP sequence jump detected, prev={} curr={}, possible packet loss or reordering", self.last_seq, seq);
                self.lost_count = self.lost_count.wrapping_add(diff as u32);
                result = Some(RtpUpdateResult::LargeJump(diff as u16));
            } else {
                let fraction = ((self.lost_count as f64 / diff as f64) * 256.0).min(255.0) as u8;
                self.fraction_lost = fraction;
            }

            let last_arrival = self.last_sr_time.unwrap_or(arrival);
            let transit = (arrival.duration_since(last_arrival).as_secs_f64() * 1000.0) as i32;
            let d = transit.abs();
            let dj = d as f64 - (self.jitter as f64 / 16.0);
            self.jitter = (self.jitter as f64 + dj / 16.0) as u32;
        }

        self.last_seq = seq;
        self.last_rtp_time = timestamp;
        self.packet_count += 1;

        result
    }

    pub fn update_from_sr(&mut self, ntp_timestamp: u64, rtp_timestamp: u32, packet_count: u32, octet_count: u32) {
        self.last_sr_ntp = ntp_timestamp;
        self.last_sr_time = Some(std::time::Instant::now());
        self.last_rtp_time = rtp_timestamp;

        if self.packet_count > 0 && packet_count < self.packet_count {
            tracing::info!("SSRC may have changed: packet_count decreased from {} to {}", self.packet_count, packet_count);
        }
        self.packet_count = packet_count;
        self.octet_count = octet_count;
    }

    pub fn calc_delay_since_last_sr(&self) -> u32 {
        if let Some(last_time) = self.last_sr_time {
            let elapsed = last_time.elapsed().as_millis() as f64 * 65536.0 / 1000.0;
            elapsed as u32
        } else {
            0
        }
    }
}

#[derive(Debug, Clone)]
pub enum RtpUpdateResult {
    SequenceWraparound,
    LargeJump(u16),
    SSRCConflict,
}

pub struct RtcpPacket;

impl RtcpPacket {
    pub fn parse(data: &[u8]) -> Option<(u32, RtcpReportType)> {
        if data.len() < 8 {
            return None;
        }

        let version = (data[0] >> 6) & 0x3;
        if version != 2 {
            return None;
        }

        let payload_type = data[1];
        let length = u16::from_be_bytes([data[2], data[3]]) as usize;
        let length_bytes = (length + 1) * 4;

        if data.len() < length_bytes {
            return None;
        }

        let ssrc = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

        let report_type = match payload_type {
            200 => {
                if data.len() < 28 {
                    return Some((ssrc, RtcpReportType::Unknown));
                }
                let ntp_timestamp = u64::from_be_bytes([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]);
                let rtp_timestamp = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
                let sender_packet_count = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
                let sender_octet_count = u32::from_be_bytes([data[24], data[25], data[26], data[27]]);

                RtcpReportType::SenderReport {
                    ntp_timestamp,
                    rtp_timestamp,
                    sender_packet_count,
                    sender_octet_count,
                }
            }
            201 => {
                if data.len() < 24 {
                    return Some((ssrc, RtcpReportType::Unknown));
                }
                let fraction_lost = data[8];
                let lost_count = u32::from_be_bytes([0, data[9], data[10], data[11]]) & 0x7FFFFF;
                let jitter = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);

                RtcpReportType::ReceiverReport {
                    fraction_lost,
                    lost_count,
                    jitter,
                }
            }
            203 => {
                let mut reason_len = 0;
                if data.len() > 8 {
                    reason_len = data[8] as usize;
                }
                let reason = if reason_len > 0 && data.len() > 9 + reason_len {
                    Some(String::from_utf8_lossy(&data[9..9 + reason_len]).to_string())
                } else {
                    None
                };
                RtcpReportType::Bye { reason }
            }
            _ => RtcpReportType::Unknown,
        };

        Some((ssrc, report_type))
    }

    pub fn build_receiver_report(
        ssrc_receiver: u32,
        ssrc_sender: u32,
        fraction_lost: u8,
        lost_count: u32,
        jitter: u32,
        last_sr: u32,
        delay_since_last_sr: u32,
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(32);

        packet.push(0x81);
        packet.push(201);
        packet.push(0x00);
        packet.push(0x07);

        packet.extend_from_slice(&ssrc_receiver.to_be_bytes());

        packet.extend_from_slice(&ssrc_sender.to_be_bytes());

        packet.push(fraction_lost);
        packet.extend_from_slice(&lost_count.to_be_bytes()[1..4]);
        packet.extend_from_slice(&jitter.to_be_bytes());
        packet.extend_from_slice(&last_sr.to_be_bytes());
        packet.extend_from_slice(&delay_since_last_sr.to_be_bytes());

        let length = (packet.len() / 4) as u16 - 1;
        packet[2] = (length >> 8) as u8;
        packet[3] = (length & 0xFF) as u8;

        packet
    }

    pub fn extract_ssrc_from_rtp(data: &[u8]) -> Option<u32> {
        if data.len() < 12 {
            return None;
        }
        Some(u32::from_be_bytes([data[8], data[9], data[10], data[11]]))
    }
}
