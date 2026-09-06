use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum TransportType {
    Udp,
    Tcp,
    Http,
    WebSocket,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::Udp => write!(f, "UDP"),
            TransportType::Tcp => write!(f, "TCP"),
            TransportType::Http => write!(f, "HTTP"),
            TransportType::WebSocket => write!(f, "WebSocket"),
        }
    }
}
