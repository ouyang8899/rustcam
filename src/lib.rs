pub mod config;
pub mod common_types;
pub mod error;
pub mod domain;
pub mod context;
pub mod application;
pub mod adapter;
pub mod infrastructure;
pub mod api;
pub mod auth;
pub mod protocol;
pub mod transport;
pub mod monitoring;
pub mod sql_mappers;
pub mod media;

pub use config::{AppConfig, UdpRtpServerConfig, TcpRtpServerConfig};
pub use common_types::TransportType;
pub use context::ServiceRegistry;
pub use infrastructure::DbRepository;
pub use infrastructure::EventBus;
pub use infrastructure::cluster::ClusterManager;
pub use application::{DeviceService, StreamService, SessionService, MediaServerService};
pub use auth::{AuthState, CasbinManager, JwtConfig};
pub use auth::auth_db::PostgresAuthRepository;
pub use protocol::{AdapterRegistry, ProtocolMatcher, ProtocolType};
pub use protocol::gb28181::Gb28181Adapter;
pub use protocol::onvif::OnvifAdapter;
pub use protocol::websocket::WebSocketAdapter;
pub use transport::{SipTcpServer, SipUdpServer, SipTcpServerConfig, SipUdpServerConfig};
pub use monitoring::Metrics;
pub use media::{
    MediaSession, MediaType, Direction, SessionState,
    Middleware, MiddlewareChain, MiddlewareResult, MediaContext, DataDirection,
    RtpHeader, RtpPacket,
    SenderReport, ReceiverReport, RtcpGoodbye as RtcpGoodbye, RtcpPacket,
    DataTransport,
    G711, AudioCodec,
    MediaError,
    RtpServerManager, RtpPortRange,
    init_rtp_server_manager, init_tcp_rtsp_rtp_server, get_tcp_rtsp_rtp_server,
    TcpRtspRtpServer, TcpRtpEvent,
    MediaNodeType, MediaNodeId, MediaNode, BasicMediaNode,
    MediaSessionKey, MediaSessionManager,
};