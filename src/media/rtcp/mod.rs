pub mod packet;

pub use packet::Goodbye as RtcpGoodbye;
pub use packet::{Goodbye, ReceiverReport, ReportBlock, SenderReport};

#[derive(Debug, Clone)]
pub enum RtcpPacket {
    SenderReport(SenderReport),
    ReceiverReport(ReceiverReport),
    Goodbye(Goodbye),
}

impl RtcpPacket {
    pub fn parse(data: &[u8]) -> Result<Self, crate::media::errors::MediaError> {
        if data.len() < 4 {
            return Err(crate::media::errors::MediaError::RtcpParse(
                "Packet too short".into(),
            ));
        }

        let pt = data[1];
        match pt {
            200 => Ok(RtcpPacket::SenderReport(SenderReport::parse(data)?)),
            201 => Ok(RtcpPacket::ReceiverReport(ReceiverReport::parse(data)?)),
            203 => Ok(RtcpPacket::Goodbye(Goodbye::parse(data)?)),
            _ => Err(crate::media::errors::MediaError::RtcpParse(format!(
                "Unknown RTCP type: {}",
                pt
            ))),
        }
    }
}

pub const RTCP_SR: u8 = 200;
pub const RTCP_RR: u8 = 201;
pub const RTCP_SDES: u8 = 202;
pub const RTCP_BYE: u8 = 203;
pub const RTCP_APP: u8 = 204;
