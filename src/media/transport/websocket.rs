use std::sync::Arc;
use async_trait::async_trait;
use bytes::BytesMut;
use tokio::sync::{mpsc, Mutex};
use std::collections::HashMap;
use crate::media::errors::{MediaError, Result};
use crate::media::node::{DataType, MediaNode, MediaTransportCallback};

pub struct WebSocketMediaTransport {
    receiver: Arc<Mutex<mpsc::Receiver<BytesMut>>>,
}

impl WebSocketMediaTransport {
    pub fn new(receiver: mpsc::Receiver<BytesMut>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub fn start_loop(self: Arc<Self>, node: Arc<dyn MediaNode>) {
        tokio::spawn(async move {
            loop {
                let data = {
                    let mut receiver = self.receiver.lock().await;
                    match receiver.recv().await {
                        Some(data) => data,
                        None => {
                            tracing::debug!("[WebSocket] Channel closed, stopping loop");
                            break;
                        }
                    }
                };

                let mut result = HashMap::new();
                result.insert(DataType::OrgData, data);
                let _ = node.on_media_data(result);
            }
        });
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for WebSocketMediaTransport {
    fn local_addr(&self) -> Result<std::net::SocketAddr> {
        Ok("0.0.0.0:0".parse().unwrap())
    }

    fn peer_addr(&self) -> Result<std::net::SocketAddr> {
        Ok("0.0.0.0:0".parse().unwrap())
    }

    async fn send(&self, _data: &[u8], _data_type: DataType) -> Result<()> {
        Ok(())
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        let mut receiver = self.receiver.lock().await;
        match receiver.recv().await {
            Some(data) => {
                let mut result = HashMap::new();
                result.insert(DataType::OrgData, data);
                Ok(result)
            }
            None => Err(MediaError::TransportRecv("WebSocket channel closed".into())),
        }
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
