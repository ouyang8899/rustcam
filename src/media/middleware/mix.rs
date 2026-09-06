use std::any::Any;
use std::sync::Arc;
use bytes::BytesMut;
use async_trait::async_trait;
use dashmap::DashMap;

use crate::media::{
    Middleware, MiddlewareResult, MediaContext, Next,
};

pub struct AudioMixer {
    buffers: Arc<DashMap<String, BytesMut>>,
    sample_rate: u32,
    channel_count: u8,
}

impl AudioMixer {
    pub fn new(sample_rate: u32, channel_count: u8) -> Self {
        Self {
            buffers: Arc::new(DashMap::new()),
            sample_rate,
            channel_count,
        }
    }
    
    pub fn add_audio(&self, stream_id: &str, data: &[u8]) {
        self.buffers.insert(stream_id.to_string(), BytesMut::from(data));
    }
    
    pub fn mix(&self) -> BytesMut {
        if self.buffers.is_empty() {
            return BytesMut::new();
        }
        
        let max_len = self.buffers
            .iter()
            .map(|e| e.value().len())
            .max()
            .unwrap_or(0);
        
        if max_len == 0 {
            return BytesMut::new();
        }
        
        let samples_count = max_len / 2;
        let mut output = vec![0i32; samples_count];
        
        for buffer in self.buffers.iter() {
            let data = buffer.value();
            for (i, chunk) in data.chunks_exact(2).enumerate() {
                if i < samples_count {
                    let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
                    output[i] += sample;
                }
            }
        }
        
        let count = self.buffers.len() as i32;
        let mixed: Vec<i16> = output
            .iter()
            .map(|&s| (s / count).clamp(i16::MIN as i32, i16::MAX as i32) as i16)
            .collect();
        
        let mut result = BytesMut::with_capacity(mixed.len() * 2);
        for sample in mixed {
            result.extend_from_slice(&sample.to_le_bytes());
        }
        
        self.buffers.clear();
        
        result
    }
}

pub struct MixMiddleware {
    mixer: Arc<AudioMixer>,
    stream_id: String,
}

impl MixMiddleware {
    pub fn new(mixer: Arc<AudioMixer>, stream_id: &str) -> Self {
        Self {
            mixer,
            stream_id: stream_id.to_string(),
        }
    }
}

#[async_trait]
impl Middleware for MixMiddleware {
    fn name(&self) -> &str {
        "mix"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        self.mixer.add_audio(&self.stream_id, &ctx.data);
        
        tracing::debug!(
            "[Mix] Added audio from {}: {} bytes",
            self.stream_id,
            ctx.data.len()
        );
        
        next.run(ctx).await
    }
}

pub struct MixOutputMiddleware {
    mixer: Arc<AudioMixer>,
}

impl MixOutputMiddleware {
    pub fn new(mixer: Arc<AudioMixer>) -> Self {
        Self { mixer }
    }
}

#[async_trait]
impl Middleware for MixOutputMiddleware {
    fn name(&self) -> &str {
        "mix_output"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        ctx.data = self.mixer.mix();
        
        tracing::debug!(
            "[MixOutput] Mixed result: {} bytes",
            ctx.data.len()
        );
        
        next.run(ctx).await
    }
}
