use std::sync::Arc;
use std::future::Future;
use async_trait::async_trait;
use bytes::{Bytes, BufMut, BytesMut};
use tokio::sync::{broadcast, mpsc, Mutex};
use futures_util::SinkExt;
use std::collections::HashMap;
use crate::media::errors::{MediaError, Result};
use crate::media::node::{DataType, MediaNode, MediaTransportCallback};
use axum::extract::ws::{Message, WebSocket};

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

pub struct WebSocketMediaNodeTransport {
    sender: broadcast::Sender<BytesMut>,
}

impl WebSocketMediaNodeTransport {
    pub fn new(sender: broadcast::Sender<BytesMut>) -> Self {
        Self { sender }
    }

    pub fn sender(&self) -> broadcast::Sender<BytesMut> {
        self.sender.clone()
    }

    pub async fn send(&self, data: &[u8], _data_type: DataType) -> Result<()> {
        let msg_data = BytesMut::from(data);
        self.sender
            .send(msg_data)
            .map_err(|e| MediaError::TransportSend(format!("WebSocket send error: {}", e)))?;
        Ok(())
    }

    pub async fn send_text(&self, text: &str) -> Result<()> {
        let msg_data = BytesMut::from(text.as_bytes());
        self.sender
            .send(msg_data)
            .map_err(|e| MediaError::TransportSend(format!("WebSocket send error: {}", e)))?;
        Ok(())
    }

    pub fn run<R>(
        self: Arc<Self>,
        socket: WebSocket,
        on_media_data: impl Fn(HashMap<DataType, BytesMut>) -> R + Send + Sync + 'static,
        on_text: impl Fn(String) + Send + Sync + 'static,
        shutdown_tx: Option<mpsc::Sender<()>>,
    )
    where
        R: Future<Output = ()> + Send + 'static,
    {
        use futures_util::StreamExt;
        let sender = self.sender.clone();
        let (mut ws_sender, mut ws_receiver) = socket.split();
        let ws_sender = Arc::new(Mutex::new(ws_sender));
        let ws_sender_clone = ws_sender.clone();
        let mut rx = sender.subscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = ws_receiver.next() => {
                        match msg {
                            Some(Ok(Message::Binary(data))) => {
                                tracing::debug!("[WebSocket] Binary message received, len={}", data.len());
                                let mut result = HashMap::new();
                                result.insert(DataType::OrgData, BytesMut::from(data));
                                let f = on_media_data(result);
                                tokio::spawn(async move {
                                    f.await;
                                });
                            }
                            Some(Ok(Message::Text(text))) => {
                                on_text(text.to_string());
                            }
                            Some(Ok(Message::Ping(data))) => {
                                let mut s = ws_sender.lock().await;
                                let _ = s.send(Message::Pong(data)).await;
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                tracing::info!("[WebSocketMediaNodeTransport] WebSocket closed");
                                if let Some(tx) = shutdown_tx.clone() {
                                    let _ = tx.send(()).await;
                                }
                                break;
                            }
                            Some(Err(e)) => {
                                tracing::warn!("[WebSocketMediaNodeTransport] Error: {}", e);
                                if let Some(tx) = shutdown_tx.clone() {
                                    let _ = tx.send(()).await;
                                }
                                break;
                            }
                            _ => {}
                        }
                    }
                    text = rx.recv() => {
                        match text {
                            Ok(data) => {
                                let msg = Message::Text(axum::extract::ws::Utf8Bytes::from(String::from_utf8_lossy(&data).to_string()));
                                let mut s = ws_sender_clone.lock().await;
                                if let Err(e) = s.send(msg).await {
                                    tracing::warn!("[WebSocketMediaNodeTransport] Failed to send text: {}", e);
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!("[WebSocketMediaNodeTransport] Receiver lagged {} messages", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                tracing::info!("[WebSocketMediaNodeTransport] Channel closed");
                                if let Some(tx) = shutdown_tx.clone() {
                                    let _ = tx.send(()).await;
                                }
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn run_text_only(self: Arc<Self>, socket: WebSocket, on_text: impl Fn(String) + Send + Sync + 'static) {
        use futures_util::StreamExt;
        tokio::spawn(async move {
            let (mut ws_sender, mut ws_receiver) = socket.split();
            loop {
                tokio::select! {
                    msg = ws_receiver.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                on_text(text.to_string());
                            }
                            Some(Ok(Message::Ping(data))) => {
                                let _ = ws_sender.send(Message::Pong(data)).await;
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                tracing::info!("[WebSocketMediaNodeTransport] WebSocket closed");
                                break;
                            }
                            Some(Err(e)) => {
                                tracing::warn!("[WebSocketMediaNodeTransport] Error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }
}

#[async_trait]
impl crate::media::node::MediaTransport for WebSocketMediaNodeTransport {
    fn local_addr(&self) -> Result<std::net::SocketAddr> {
        Ok("0.0.0.0:0".parse().unwrap())
    }

    fn peer_addr(&self) -> Result<std::net::SocketAddr> {
        Ok("0.0.0.0:0".parse().unwrap())
    }

    async fn send(&self, data: &[u8], data_type: DataType) -> Result<()> {
        WebSocketMediaNodeTransport::send(self, data, data_type).await
    }

    async fn recv(&self) -> Result<HashMap<DataType, BytesMut>> {
        Err(MediaError::TransportRecv("WebSocketMediaNodeTransport does not support recv".into()))
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
