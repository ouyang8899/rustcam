use async_trait::async_trait;

use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::{ broadcast };
use crate::protocol::event::{SignalEvent, EventHandler, FnEventHandler};
use crate::domain::traits::EventPublisher;
use crate::error::AppError;

#[derive(Clone)]
pub struct EventBus {
    tx: Arc<broadcast::Sender<SignalEvent>>,
    handlers: Arc<DashMap<String, Vec<Arc<dyn EventHandler>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            tx: Arc::new(broadcast::channel(1024).0),
            handlers: Arc::new(DashMap::new()),
        }
    }

    pub async fn publish(&self, event: SignalEvent) -> anyhow::Result<()> {
        let _event_type = event.event_type();
        self.tx.send(event.clone()).map_err(|e| anyhow::anyhow!("broadcast send: {}", e))?;

        let event_type = event.event_type();

        let exact_handlers: Vec<Arc<dyn EventHandler>> = self
            .handlers
            .get(&event_type)
            .map(|h| h.value().clone())
            .unwrap_or_default();
        for handler in exact_handlers {
            let event = event.clone();
            tokio::spawn(async move {
                handler.handle(event).await;
            });
        }

        let wildcard_handlers: Vec<Arc<dyn EventHandler>> = self
            .handlers
            .get("*")
            .map(|h| h.value().clone())
            .unwrap_or_default();
        for handler in wildcard_handlers {
            let event = event.clone();
            tokio::spawn(async move {
                handler.handle(event).await;
            });
        }

        Ok(())
    }

    pub fn on(&self, event_type: &str, handler: Arc<dyn EventHandler>) {

        self.handlers
            .entry(event_type.to_string())
            .or_default()
            .push(handler);

    }
    pub fn on_fn<F, Fut>(&self, event_type: &str, f: F)
    where
        F: Fn(SignalEvent) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.on(event_type, Arc::new(FnEventHandler::new(f)));
    }

    pub fn off(&self, event_type: &str) {
        self.handlers.remove(event_type);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SignalEvent> {
        self.tx.subscribe()
    }

    pub fn subscribe_with_handler<F, Fut>(&self, mut f: F)
    where
        F: FnMut(SignalEvent) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut rx = self.tx.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => f(event).await,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("EventBus lagged {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl EventPublisher for EventBus {
    async fn publish(&self, event: SignalEvent) -> Result<(), AppError> {
        self.tx.send(event).map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<SignalEvent> {
        self.tx.subscribe()
    }
}
