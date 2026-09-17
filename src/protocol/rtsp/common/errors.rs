use thiserror::Error;

#[derive(Debug, Error)]
pub enum RtspError {
    #[error("连接失败: {0}")]
    ConnectionFailed(String),

    #[error("认证失败: {0}")]
    AuthFailed(String),

    #[error("服务器错误: {0}")]
    ServerError(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("协议错误: {0}")]
    ProtocolError(String),

    #[error("会话不存在")]
    SessionNotFound,

    #[error("轨道不存在: {0}")]
    TrackNotFound(String),

    #[error("RTP 发送失败: {0}")]
    RtpSendFailed(String),
}
