pub mod bridge;
pub mod transcode;
pub mod mix;
pub mod talk;

pub use bridge::{BridgeMiddleware, MultiBridgeMiddleware};
pub use transcode::{TranscodeMiddleware, PcmaToPcmMiddleware, PcmToPcmaMiddleware};
pub use mix::{AudioMixer, MixMiddleware, MixOutputMiddleware};
pub use talk::TalkMiddleware;

use std::any::Any;
use bytes::BytesMut;
use async_trait::async_trait;
use crate::media::{Middleware, MiddlewareResult, MediaContext, Next};

pub struct LogMiddleware {
    name: String,
}

impl LogMiddleware {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl Middleware for LogMiddleware {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        tracing::info!(
            "[{}] direction={:?}, ssrc=0x{:08x}, ts={}, pt={}, size={}",
            self.name,
            ctx.direction,
            ctx.ssrc,
            ctx.timestamp,
            ctx.payload_type,
            ctx.data.len()
        );
        next.run(ctx).await
    }
}

pub struct DropMiddleware;

impl DropMiddleware {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Middleware for DropMiddleware {
    fn name(&self) -> &str {
        "drop"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, _ctx: &mut MediaContext, _next: Next<'_>) -> MiddlewareResult {
        MiddlewareResult::Stop
    }
}
