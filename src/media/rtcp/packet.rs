use crate::media::errors::{MediaError, Result};
use bytes::{BufMut, BytesMut};

#[derive(Debug, Clone)]
pub struct SenderReport {
    pub ssrc: u32,
    pub ntp_timestamp: u64,
    pub rtp_timestamp: u32,
    pub packet_count: u32,
    pub octet_count: u32,
    pub report_blocks: Vec<ReportBlock>,
}

#[derive(Debug, Clone)]
pub struct ReportBlock {
    pub ssrc: u32,
    pub fraction_lost: u8,
    pub packets_lost: i32,
    pub highest_sequence: u32,
    pub jitter: u32,
    pub lsr: u32,
    pub dlsr: u32,
}

impl SenderReport {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 20 {
            return Err(MediaError::RtcpParse("SR packet too short".into()));
        }

        let ssrc = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let ntp_timestamp = u64::from_be_bytes([
            data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
        ]);
        let rtp_timestamp = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        let packet_count = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let octet_count = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);

        let mut report_blocks = Vec::new();
        let mut offset = 24;

        while offset + 24 <= data.len() {
            let block = ReportBlock {
                ssrc: u32::from_be_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]),
                fraction_lost: data[offset + 4],
                packets_lost: i32::from_be_bytes([
                    0,
                    data[offset + 5],
                    data[offset + 6],
                    data[offset + 7],
                ]),
                highest_sequence: u32::from_be_bytes([
                    data[offset + 8],
                    data[offset + 9],
                    data[offset + 10],
                    data[offset + 11],
                ]),
                jitter: u32::from_be_bytes([
                    data[offset + 12],
                    data[offset + 13],
                    data[offset + 14],
                    data[offset + 15],
                ]),
                lsr: u32::from_be_bytes([
                    data[offset + 16],
                    data[offset + 17],
                    data[offset + 18],
                    data[offset + 19],
                ]),
                dlsr: u32::from_be_bytes([
                    data[offset + 20],
                    data[offset + 21],
                    data[offset + 22],
                    data[offset + 23],
                ]),
            };
            report_blocks.push(block);
            offset += 24;
        }

        Ok(Self {
            ssrc,
            ntp_timestamp,
            rtp_timestamp,
            packet_count,
            octet_count,
            report_blocks,
        })
    }

    pub fn build(&self) -> Result<BytesMut> {
        let mut buf = BytesMut::with_capacity(28 + self.report_blocks.len() * 24);

        buf.put_u32(self.ssrc);
        buf.put_u64(self.ntp_timestamp);
        buf.put_u32(self.rtp_timestamp);
        buf.put_u32(self.packet_count);
        buf.put_u32(self.octet_count);

        for block in &self.report_blocks {
            buf.put_u32(block.ssrc);
            buf.put_u8(block.fraction_lost);
            let pl = block.packets_lost as u32;
            buf.put_u8((pl >> 16) as u8);
            buf.put_u8((pl >> 8) as u8);
            buf.put_u8(pl as u8);
            buf.put_u32(block.highest_sequence);
            buf.put_u32(block.jitter);
            buf.put_u32(block.lsr);
            buf.put_u32(block.dlsr);
        }

        Ok(buf)
    }
}

#[derive(Debug, Clone)]
pub struct ReceiverReport {
    pub ssrc: u32,
    pub report_blocks: Vec<ReportBlock>,
}

impl ReceiverReport {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(MediaError::RtcpParse("RR packet too short".into()));
        }

        let ssrc = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);

        let mut report_blocks = Vec::new();
        let mut offset = 4;

        while offset + 24 <= data.len() {
            let block = ReportBlock {
                ssrc: u32::from_be_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]),
                fraction_lost: data[offset + 4],
                packets_lost: i32::from_be_bytes([
                    0,
                    data[offset + 5],
                    data[offset + 6],
                    data[offset + 7],
                ]),
                highest_sequence: u32::from_be_bytes([
                    data[offset + 8],
                    data[offset + 9],
                    data[offset + 10],
                    data[offset + 11],
                ]),
                jitter: u32::from_be_bytes([
                    data[offset + 12],
                    data[offset + 13],
                    data[offset + 14],
                    data[offset + 15],
                ]),
                lsr: u32::from_be_bytes([
                    data[offset + 16],
                    data[offset + 17],
                    data[offset + 18],
                    data[offset + 19],
                ]),
                dlsr: u32::from_be_bytes([
                    data[offset + 20],
                    data[offset + 21],
                    data[offset + 22],
                    data[offset + 23],
                ]),
            };
            report_blocks.push(block);
            offset += 24;
        }

        Ok(Self {
            ssrc,
            report_blocks,
        })
    }

    pub fn build(&self) -> Result<BytesMut> {
        let mut buf = BytesMut::with_capacity(4 + self.report_blocks.len() * 24);
        buf.put_u32(self.ssrc);

        for block in &self.report_blocks {
            buf.put_u32(block.ssrc);
            buf.put_u8(block.fraction_lost);
            buf.put_u8((block.packets_lost >> 16) as u8);
            buf.put_u8((block.packets_lost >> 8) as u8);
            buf.put_u8(block.packets_lost as u8);
            buf.put_u32(block.highest_sequence);
            buf.put_u32(block.jitter);
            buf.put_u32(block.lsr);
            buf.put_u32(block.dlsr);
        }

        Ok(buf)
    }
}

#[derive(Debug, Clone)]
pub struct Goodbye {
    pub ssrcs: Vec<u32>,
    pub reason: Option<String>,
}

impl Goodbye {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(MediaError::RtcpParse("BYE packet too short".into()));
        }

        let ssrc_count = data[0] & 0x1F;
        let mut ssrcs = Vec::new();
        let mut offset = 4;

        for _ in 0..ssrc_count {
            if offset + 4 > data.len() {
                break;
            }
            ssrcs.push(u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]));
            offset += 4;
        }

        let reason = if offset < data.len() && data[offset] > 0 {
            let len = data[offset] as usize;
            offset += 1;
            if offset + len <= data.len() {
                Some(String::from_utf8_lossy(&data[offset..offset + len]).to_string())
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self { ssrcs, reason })
    }

    pub fn build(&self) -> Result<BytesMut> {
        let mut buf = BytesMut::new();
        let ssrc_count = self.ssrcs.len() as u8;
        buf.put_u8(0x80 | ssrc_count);
        buf.put_u8(203);
        buf.put_u16((4 + self.ssrcs.len() * 4) as u16);

        for ssrc in &self.ssrcs {
            buf.put_u32(*ssrc);
        }

        if let Some(ref reason) = self.reason {
            let reason_bytes = reason.as_bytes();
            buf.put_u8(reason_bytes.len() as u8);
            buf.extend_from_slice(reason_bytes);
        }

        Ok(buf)
    }
}
