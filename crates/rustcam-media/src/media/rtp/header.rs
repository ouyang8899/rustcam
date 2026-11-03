use crate::media::errors::{MediaError, Result};
use bytes::{BufMut, BytesMut};

#[derive(Debug, Clone, Default)]
pub struct RtpHeader {
    pub version: u8,
    pub padding_flag: u8,
    pub extension_flag: u8,
    pub csrc_count: u8,
    pub marker: u8,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub csrcs: Vec<u32>,
}

impl RtpHeader {
    pub fn new() -> Self {
        Self {
            version: 2,
            ..Default::default()
        }
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 12 {
            return Err(MediaError::RtpParse("Header too short".into()));
        }

        let byte_1st = data[0];
        let version = (byte_1st >> 6) & 0x03;
        let padding_flag = (byte_1st >> 5) & 0x01;
        let extension_flag = (byte_1st >> 4) & 0x01;
        let csrc_count = byte_1st & 0x0F;

        let byte_2nd = data[1];
        let marker = (byte_2nd >> 7) & 0x01;
        let payload_type = byte_2nd & 0x7F;

        let sequence_number = u16::from_be_bytes([data[2], data[3]]);
        let timestamp = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ssrc = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);

        let mut header = RtpHeader {
            version,
            padding_flag,
            extension_flag,
            csrc_count,
            marker,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            csrcs: Vec::new(),
        };

        let mut offset = 12;
        for _ in 0..csrc_count {
            if data.len() < offset + 4 {
                return Err(MediaError::RtpParse("CSRC data too short".into()));
            }
            let csrc = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            header.csrcs.push(csrc);
            offset += 4;
        }

        Ok(header)
    }

    pub fn build(&self) -> Result<BytesMut> {
        let mut buf = BytesMut::with_capacity(12 + self.csrc_count as usize * 4);

        let byte_1st = (self.version << 6)
            | (self.padding_flag << 5)
            | (self.extension_flag << 4)
            | (self.csrc_count & 0x0F);
        buf.put_u8(byte_1st);

        let byte_2nd = (self.marker << 7) | (self.payload_type & 0x7F);
        buf.put_u8(byte_2nd);

        buf.put_u16(self.sequence_number);
        buf.put_u32(self.timestamp);
        buf.put_u32(self.ssrc);

        for csrc in &self.csrcs {
            buf.put_u32(*csrc);
        }

        Ok(buf)
    }

    pub fn header_size(&self) -> usize {
        12 + self.csrc_count as usize * 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_parse_build_roundtrip() {
        let original = RtpHeader {
            version: 2,
            padding_flag: 0,
            extension_flag: 0,
            csrc_count: 0,
            marker: 1,
            payload_type: 8,
            sequence_number: 1234,
            timestamp: 160,
            ssrc: 0x12345678,
            csrcs: vec![],
        };

        let built = original.build().unwrap();
        let parsed = RtpHeader::parse(&built).unwrap();

        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.marker, original.marker);
        assert_eq!(parsed.payload_type, original.payload_type);
        assert_eq!(parsed.sequence_number, original.sequence_number);
        assert_eq!(parsed.timestamp, original.timestamp);
        assert_eq!(parsed.ssrc, original.ssrc);
    }
}
