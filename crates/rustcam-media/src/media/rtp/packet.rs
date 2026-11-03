use super::header::RtpHeader;
use crate::media::errors::{MediaError, Result};
use bytes::{BufMut, BytesMut};

#[derive(Debug, Clone)]
pub struct RtpPacket {
    pub header: RtpHeader,
    pub extension_profile: Option<u16>,
    pub extension_payload: Option<BytesMut>,
    pub payload: BytesMut,
}

impl RtpPacket {
    pub fn new(header: RtpHeader) -> Self {
        Self {
            header,
            extension_profile: None,
            extension_payload: None,
            payload: BytesMut::new(),
        }
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        let header = RtpHeader::parse(data)?;
        let mut offset = header.header_size();

        let mut extension_profile = None;
        let mut extension_payload = None;

        if header.extension_flag == 1 {
            if data.len() < offset + 4 {
                return Err(MediaError::RtpParse("Extension header too short".into()));
            }
            extension_profile = Some(u16::from_be_bytes([data[offset], data[offset + 1]]));
            let ext_len = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
            offset += 4;

            if data.len() < offset + ext_len * 4 {
                return Err(MediaError::RtpParse("Extension payload too short".into()));
            }
            extension_payload = Some(BytesMut::from(&data[offset..offset + ext_len * 4]));
            offset += ext_len * 4;
        }

        let payload = if header.padding_flag == 1 {
            let padding_len = data[data.len() - 1] as usize;
            let payload_len = data.len() - offset - padding_len;
            BytesMut::from(&data[offset..offset + payload_len])
        } else {
            BytesMut::from(&data[offset..])
        };

        Ok(Self {
            header,
            extension_profile,
            extension_payload,
            payload,
        })
    }

    pub fn build(&self) -> Result<BytesMut> {
        let mut buf = self.header.build()?;

        if let Some(profile) = self.extension_profile {
            buf.put_u16(profile);
            let ext_len = self
                .extension_payload
                .as_ref()
                .map(|p| (p.len() / 4) as u16)
                .unwrap_or(0);
            buf.put_u16(ext_len);
            if let Some(ext) = &self.extension_payload {
                buf.extend_from_slice(ext);
            }
        }

        buf.extend_from_slice(&self.payload);

        Ok(buf)
    }

    pub fn set_payload(&mut self, payload: BytesMut) {
        self.payload = payload;
    }
}

impl Default for RtpPacket {
    fn default() -> Self {
        Self {
            header: RtpHeader::new(),
            extension_profile: None,
            extension_payload: None,
            payload: BytesMut::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_parse_build_roundtrip() {
        let mut packet = RtpPacket::default();
        packet.header.payload_type = 8;
        packet.header.sequence_number = 100;
        packet.header.timestamp = 1000;
        packet.header.ssrc = 0xABCD1234;
        packet.header.marker = 0;
        packet.payload = BytesMut::from(&b"\x01\x02\x03\x04"[..]);

        let built = packet.build().unwrap();
        let parsed = RtpPacket::parse(&built).unwrap();

        assert_eq!(parsed.header.payload_type, packet.header.payload_type);
        assert_eq!(parsed.header.sequence_number, packet.header.sequence_number);
        assert_eq!(parsed.header.timestamp, packet.header.timestamp);
        assert_eq!(parsed.payload, packet.payload);
    }
}
