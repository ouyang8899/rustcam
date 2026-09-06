use bytes::BytesMut;
use std::any::{Any, TypeId};
use async_trait::async_trait;
use crate::media::errors::{MediaError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDirection {
    Send,
    Recv,
}

#[derive(Debug, Clone)]
pub struct MediaContext {
    pub direction: DataDirection,
    pub timestamp: u32,
    pub ssrc: u32,
    pub payload_type: u8,
    pub marker: bool,
    pub data: BytesMut,
}

impl MediaContext {
    pub fn new(direction: DataDirection, timestamp: u32, ssrc: u32, payload_type: u8, marker: bool, data: BytesMut) -> Self {
        Self {
            direction,
            timestamp,
            ssrc,
            payload_type,
            marker,
            data,
        }
    }
}

#[derive(Debug, Clone)]
pub enum MiddlewareResult {
    Continue,
    Stop,
    Response(Vec<u8>),
}

#[async_trait]
pub trait Middleware: Send + Sync {
    fn name(&self) -> &str;
    
    fn as_any(&self) -> &dyn Any;
    
    async fn process(
        &self,
        ctx: &mut MediaContext,
        next: Next<'_>,
    ) -> MiddlewareResult;
}

pub struct Next<'a> {
    middlewares: &'a [(String, Box<dyn Middleware>)],
    index: usize,
}

impl<'a> Next<'a> {
    pub fn new(middlewares: &'a [(String, Box<dyn Middleware>)]) -> Self {
        Self {
            middlewares,
            index: 0,
        }
    }
    
    pub async fn run(&self, ctx: &mut MediaContext) -> MiddlewareResult {
        if let Some((_, mw)) = self.middlewares.get(self.index) {
            let next = Next {
                middlewares: self.middlewares,
                index: self.index + 1,
            };
            mw.process(ctx, next).await
        } else {
            MiddlewareResult::Continue
        }
    }
}
