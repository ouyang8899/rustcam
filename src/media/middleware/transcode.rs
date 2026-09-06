use std::any::Any;
use bytes::BytesMut;
use async_trait::async_trait;

use crate::media::{
    Middleware, MiddlewareResult, MediaContext, Next,
    codec::AudioCodec,
    G711,
};

pub struct TranscodeMiddleware {
    from_codec: AudioCodec,
    to_codec: AudioCodec,
}

impl TranscodeMiddleware {
    pub fn new(from: AudioCodec, to: AudioCodec) -> Self {
        Self {
            from_codec: from,
            to_codec: to,
        }
    }
}

#[async_trait]
impl Middleware for TranscodeMiddleware {
    fn name(&self) -> &str {
        "transcode"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        if ctx.payload_type != self.from_codec.payload_type() {
            return next.run(ctx).await;
        }

        tracing::debug!(
            "[Transcode] Converting PT {} ({}) -> PT {} ({})",
            self.from_codec.payload_type(),
            self.from_codec,
            self.to_codec.payload_type(),
            self.to_codec
        );

        let samples = G711::alaw_decode_slice(&ctx.data);
        let encoded = G711::ulaw_encode_slice(&samples);
        
        ctx.payload_type = self.to_codec.payload_type();
        ctx.data = BytesMut::from(&encoded[..]);

        next.run(ctx).await
    }
}

pub struct PcmaToPcmMiddleware;

impl PcmaToPcmMiddleware {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Middleware for PcmaToPcmMiddleware {
    fn name(&self) -> &str {
        "pcma_to_pcm"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        if ctx.payload_type != 8 {
            return next.run(ctx).await;
        }

        let samples = G711::alaw_decode_slice(&ctx.data);
        
        let mut pcm_bytes = Vec::with_capacity(samples.len() * 2);
        for &sample in &samples {
            pcm_bytes.extend_from_slice(&sample.to_le_bytes());
        }
        
        tracing::debug!(
            "[PcmaToPcm] Converted {} alaw bytes to {} PCM samples ({} bytes)",
            ctx.data.len(),
            samples.len(),
            pcm_bytes.len()
        );

        ctx.data = BytesMut::from(&pcm_bytes[..]);

        next.run(ctx).await
    }
}

pub struct PcmToPcmaMiddleware;

impl PcmToPcmaMiddleware {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Middleware for PcmToPcmaMiddleware {
    fn name(&self) -> &str {
        "pcm_to_pcma"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        if ctx.payload_type != 255 {
            return next.run(ctx).await;
        }

        let pcm_samples: Vec<i16> = ctx.data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        
        let encoded = G711::alaw_encode_slice(&pcm_samples);
        
        ctx.payload_type = 8;
        ctx.data = BytesMut::from(encoded.as_slice());

        tracing::debug!(
            "[PcmToPcma] Converted {} PCM samples to {} bytes",
            pcm_samples.len(),
            ctx.data.len()
        );

        next.run(ctx).await
    }
}
