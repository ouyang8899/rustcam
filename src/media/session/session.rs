use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU16, Ordering};
use bytes::BytesMut;

use crate::media::errors::{MediaError, Result};
use crate::media::rtp::{RtpHeader, RtpPacket};
use crate::media::transport::DataTransport;
use super::middleware::{Middleware, MediaContext, DataDirection};
use super::chain::MiddlewareChain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Video,
    Audio,
    Talk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Send,
    Recv,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Active,
    Paused,
    Closed,
}

pub struct MediaSession {
    pub id: String,
    pub channel_id: String,
    pub media_type: MediaType,
    pub direction: Direction,
    pub state: SessionState,
    
    transport: Box<dyn DataTransport>,
    middlewares: MiddlewareChain,
    
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
    
    timestamp: AtomicU32,
    sequence: AtomicU16,
    ssrc: u32,
    payload_type: u8,
}

impl MediaSession {
    pub fn new(
        id: String,
        channel_id: String,
        media_type: MediaType,
        direction: Direction,
        transport: Box<dyn DataTransport>,
    ) -> Result<Self> {
        let local_addr = transport.local_addr()?;
        let peer_addr = transport.peer_addr()?;
        
        Ok(Self {
            id,
            channel_id,
            media_type,
            direction,
            state: SessionState::Idle,
            transport,
            middlewares: MiddlewareChain::new(),
            local_addr,
            peer_addr,
            timestamp: AtomicU32::new(0),
            sequence: AtomicU16::new(0),
            ssrc: rand_u32(),
            payload_type: 8,
        })
    }
    
    pub fn with_middlewares(mut self, middlewares: MiddlewareChain) -> Self {
        self.middlewares = middlewares;
        self
    }
    
    pub fn middlewares_mut(&mut self) -> &mut MiddlewareChain {
        &mut self.middlewares
    }
    
    pub fn ssrc(&self) -> u32 {
        self.ssrc
    }
    
    pub fn set_payload_type(&mut self, pt: u8) {
        self.payload_type = pt;
    }
    
    pub fn set_ssrc(&mut self, ssrc: u32) {
        self.ssrc = ssrc;
    }
    
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
    
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }
    
    pub async fn start(&mut self) -> Result<()> {
        if self.state != SessionState::Idle {
            return Err(MediaError::InvalidState("Session already started".into()));
        }
        self.state = SessionState::Active;
        Ok(())
    }
    
    pub async fn stop(&mut self) -> Result<()> {
        self.state = SessionState::Closed;
        self.transport.close().await?;
        Ok(())
    }
    
    pub async fn pause(&mut self) -> Result<()> {
        if self.state != SessionState::Active {
            return Err(MediaError::InvalidState("Session not active".into()));
        }
        self.state = SessionState::Paused;
        Ok(())
    }
    
    pub async fn resume(&mut self) -> Result<()> {
        if self.state != SessionState::Paused {
            return Err(MediaError::InvalidState("Session not paused".into()));
        }
        self.state = SessionState::Active;
        Ok(())
    }
    
    pub async fn send(&self, data: BytesMut) -> Result<()> {
        if self.state != SessionState::Active {
            return Err(MediaError::InvalidState("Session not active".into()));
        }
        
        if self.direction == Direction::Recv {
            return Err(MediaError::InvalidState("Session is recv-only".into()));
        }
        
        let timestamp = self.timestamp.fetch_add(160, Ordering::Relaxed);
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        
        let mut header = RtpHeader::new();
        header.ssrc = self.ssrc;
        header.payload_type = self.payload_type;
        header.sequence_number = sequence;
        header.timestamp = timestamp;
        header.marker = 1;
        
        let mut packet = RtpPacket::new(header);
        packet.set_payload(data);
        
        let built = packet.build()?;
        self.transport.send_raw(&built).await?;
        
        Ok(())
    }
    
    pub async fn recv(&self) -> Result<MediaContext> {
        if self.state != SessionState::Active {
            return Err(MediaError::InvalidState("Session not active".into()));
        }
        
        if self.direction == Direction::Send {
            return Err(MediaError::InvalidState("Session is send-only".into()));
        }
        
        let data = self.transport.recv_raw().await?;
        
        let packet = RtpPacket::parse(&data)
            .map_err(|e| MediaError::RtpParse(e.to_string()))?;
        
        let direction = DataDirection::Recv;
        let mut ctx = MediaContext::new(
            direction,
            packet.header.timestamp,
            packet.header.ssrc,
            packet.header.payload_type,
            packet.header.marker == 1,
            packet.payload,
        );
        
        self.middlewares.execute(&mut ctx).await?;
        
        Ok(ctx)
    }
    
    pub async fn send_with_middleware(&self, mut ctx: MediaContext) -> Result<()> {
        if self.state != SessionState::Active {
            return Err(MediaError::InvalidState("Session not active".into()));
        }
        
        ctx.direction = DataDirection::Send;
        self.middlewares.execute(&mut ctx).await?;
        
        let timestamp = self.timestamp.fetch_add(160, Ordering::Relaxed);
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        
        let mut header = RtpHeader::new();
        header.ssrc = self.ssrc;
        header.payload_type = self.payload_type;
        header.sequence_number = sequence;
        header.timestamp = timestamp;
        header.marker = ctx.marker as u8;
        
        let mut packet = RtpPacket::new(header);
        packet.set_payload(ctx.data);
        
        let built = packet.build()?;
        self.transport.send_raw(&built).await?;
        
        Ok(())
    }
}

fn rand_u32() -> u32 {
    rand::random()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::transport::UdpTransport;

    #[tokio::test]
    async fn test_session_create() {
        let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        
        let session = MediaSession::new(
            "test-1".into(),
            "ch-1".into(),
            MediaType::Audio,
            Direction::Both,
            Box::new(transport),
        ).unwrap();
        
        assert_eq!(session.state, SessionState::Idle);
    }
}
