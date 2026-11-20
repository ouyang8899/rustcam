pub mod codec;
pub mod errors;
pub mod node;
pub mod rtcp;
pub mod rtp;
pub mod rtp_server;
pub mod rtp_server_manager;
pub mod tcp_rtsp_rtp_server;
pub mod transport;

use crate::protocol::traits::ProtocolDeps;
use crate::Metrics;
use once_cell::sync::OnceCell;
use std::sync::Arc;

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
    )
    .await
    .map_err(|e| MediaError::Transport(e.to_string()))?;
    let server = Arc::new(server);
    TCP_RTSP_RTP_SERVER.set(server.clone()).ok();
    Ok(server)
}

pub fn get_tcp_rtsp_rtp_server() -> Option<&'static Arc<TcpRtspRtpServer>> {
    TCP_RTSP_RTP_SERVER.get()
}

pub use codec::{AudioCodec, G711};
pub use errors::{MediaError, Result};
pub use node::{
    get_media_session_manager, init_media_session_manager, BasicMediaNode, BasicMiddleware,
    DataType, G711ToLinearPcmMiddleware, LinearPcmToG711Middleware, MediaNode, MediaNodeId,
    MediaNodeType, MediaSession as NodeMediaSession, MediaSessionKey, MediaSessionManager,
    MediaTransport as MediaNodeTransport, NodeMediaContext, NodeMiddleware, RtpSendMiddleware,
    SendToReceiversMiddleware,
};
pub use rtcp::{ReceiverReport, RtcpGoodbye, RtcpPacket, SenderReport};
pub use rtp::{RtpHeader, RtpPacket, PAYLOAD_TYPE_PCMA, PAYLOAD_TYPE_PCMU};
pub use tcp_rtsp_rtp_server::{TcpRtpEvent, TcpRtspRtpServer, TcpRtspRtpServerConfig};
pub use transport::{
    DataTransport, DataTransportAdapter, GB28181BroadcastVoiceTransport, Gb28181TcpTransport,
    NullTransport, OnvifBackchannelTransport, WebSocketMediaNodeTransport, WebSocketMediaTransport,
};

pub use rtp_server::{RtpPortRange, RtpSession, UDPRtpServer, UdpRtpPacket};
pub use rtp_server_manager::{get_rtp_server_manager, init_rtp_server_manager, RtpServerManager};
