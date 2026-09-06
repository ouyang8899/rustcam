use std::any::Any;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use bytes::BytesMut;
use async_trait::async_trait;

use crate::media::{Middleware, MiddlewareResult, MediaContext, Next};
use crate::media::codec::{G711, AudioCodec};

pub struct TalkMiddleware {
    device_tag: String,
    channel_tag: String,
    ssrc: AtomicU32,
    codec: Arc<Mutex<AudioCodec>>,
}

impl TalkMiddleware {
    pub fn new() -> Self {
        Self {
            device_tag: String::new(),
            channel_tag: String::new(),
            ssrc: AtomicU32::new(0),
            codec: Arc::new(Mutex::new(AudioCodec::Pcma)),
        }
    }
    
    pub fn set_device_tag(&mut self, tag: String) {
        self.device_tag = tag;
    }
    
    pub fn set_channel_tag(&mut self, tag: String) {
        self.channel_tag = tag;
    }
    
    pub fn set_ssrc(&mut self, ssrc: u32) {
        self.ssrc.store(ssrc, Ordering::Relaxed);
    }
    
    pub fn set_codec(&self, codec: AudioCodec) {
        *self.codec.lock().unwrap() = codec;
    }
    
    pub fn device_tag(&self) -> &str {
        &self.device_tag
    }
    
    pub fn channel_tag(&self) -> &str {
        &self.channel_tag
    }
    
    pub fn ssrc(&self) -> u32 {
        self.ssrc.load(Ordering::Relaxed)
    }
    
    pub fn codec(&self) -> AudioCodec {
        *self.codec.lock().unwrap()
    }
}

impl Default for TalkMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for TalkMiddleware {
    fn name(&self) -> &str {
        "talk"
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn process(&self, ctx: &mut MediaContext, next: Next<'_>) -> MiddlewareResult {
        let codec = self.codec();
        match ctx.direction {
            crate::media::DataDirection::Recv => {
                let samples = match codec {
                    AudioCodec::Pcma => G711::alaw_decode_slice(&ctx.data),
                    AudioCodec::Pcmu => G711::ulaw_decode_slice(&ctx.data),
                    AudioCodec::Aac => {
                        tracing::warn!("[TalkMiddleware] AAC not supported, skipping");
                        return next.run(ctx).await;
                    }
                };
                
                let mut pcm_data = Vec::with_capacity(samples.len() * 2);
                for sample in samples {
                    pcm_data.extend_from_slice(&sample.to_le_bytes());
                }
                
                ctx.data = BytesMut::from(pcm_data.as_slice());
                ctx.payload_type = 0;
            }
            crate::media::DataDirection::Send => {
                let samples: Vec<i16> = ctx.data
                    .chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();
                
                let encoded = match codec {
                    AudioCodec::Pcma => G711::alaw_encode_slice(&samples),
                    AudioCodec::Pcmu => G711::ulaw_encode_slice(&samples),
                    AudioCodec::Aac => {
                        tracing::warn!("[TalkMiddleware] AAC not supported, skipping");
                        return next.run(ctx).await;
                    }
                };
                
                ctx.data = BytesMut::from(encoded.as_slice());
                ctx.payload_type = codec.payload_type();
            }
        }
        
        next.run(ctx).await
    }
}
