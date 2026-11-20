pub mod adapter;
pub mod adapter_event_handler;
pub mod adapter_manager;
pub mod event;
pub mod gb28181;
pub mod matcher;
pub mod onvif;
pub mod rtsp;
pub mod traits;
pub mod websocket;

pub use adapter::{AdapterRegistry, SignalAdapter};
pub use adapter_manager::{
    cleanup_expired_subscriptions_all, clear_adapters, get_adapter,
    refresh_expiring_subscriptions_all, remove_adapter, set_adapter, AdapterEntry,
};
pub use event::ProtocolType;
pub use matcher::ProtocolMatcher;
