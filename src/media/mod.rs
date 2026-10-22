pub mod errors;
pub mod rtp;
pub mod rtcp;
pub mod transport;
pub mod codec;
pub mod rtp_server;
pub mod rtp_server_manager;
pub mod tcp_rtsp_rtp_server;
pub mod node;

use std::sync::Arc;
use once_cell::sync::OnceCell;
use crate::Metrics;
use crate::protocol::traits::ProtocolDeps;

static TCP_RTSP_RTP_SERVER: OnceCell<Arc<TcpRtspRtpServer>> = OnceCell::new();

pub async fn init_tcp_rtsp_rtp_server(
    config: crate::config::TcpRtpServerConfig,
    event_bus: Arc<crate::infrastructure::EventBus>,
    metrics: Arc<Metrics>,
    protocol_deps: ProtocolDeps,
) -> Result<Arc<TcpRtspRtpServer>> {
    let server = TcpRtspRtpServer::new(
        TcpRtspRtpServerConfig {
            bind_addr: config.bind_addr,
            max_connections: config.max_connections,
            connection_timeout: config.connection_timeout,
            keepalive_interval_secs: config.keepalive_interval_secs,
            rtsp_port: 554,
        },
        event_bus,
        metrics,
        protocol_deps,
    ).await.map_err(|e| MediaError::Transport(e.to_string()))?;
    let server = Arc::new(server);
    TCP_RTSP_RTP_SERVER.set(server.clone()).ok();
    Ok(server)
}

pub fn get_tcp_rtsp_rtp_server() -> Option<&'static Arc<TcpRtspRtpServer>> {
    TCP_RTSP_RTP_SERVER.get()
}

pub use errors::{MediaError, Result};
pub use rtp::{RtpHeader, RtpPacket, PAYLOAD_TYPE_PCMU, PAYLOAD_TYPE_PCMA};
pub use rtcp::{SenderReport, ReceiverReport, RtcpGoodbye, RtcpPacket};
pub use transport::{DataTransport, Gb28181TcpTransport, GB28181BroadcastVoiceTransport, WebSocketMediaTransport, WebSocketMediaNodeTransport, DataTransportAdapter, NullTransport, OnvifBackchannelTransport};
pub use codec::{G711, AudioCodec};
pub use tcp_rtsp_rtp_server::{TcpRtspRtpServer, TcpRtspRtpServerConfig, TcpRtpEvent};
pub use node::{MediaNodeType, MediaNodeId, MediaNode, BasicMediaNode, MediaSession as NodeMediaSession, MediaSessionKey, MediaSessionManager, MediaTransport as MediaNodeTransport, NodeMiddleware, BasicMiddleware, G711ToLinearPcmMiddleware, LinearPcmToG711Middleware, RtpSendMiddleware, SendToReceiversMiddleware, NodeMediaContext, init_media_session_manager, get_media_session_manager, DataType};

pub use rtp_server::{RtpPortRange, RtpSession, UDPRtpServer, UdpRtpPacket};
pub use rtp_server_manager::{RtpServerManager, init_rtp_server_manager, get_rtp_server_manager};
