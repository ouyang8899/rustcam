//! MAVLink 连接串归一化。
//!
//! `mavlink 0.21` 只接受 `udpin:` / `udpout:` / `udpbcast:` / `tcpin:` / `tcpout:` /
//! `serial:` / `file:` 前缀；历史配置与文档里常见的 `udp://` 写法需要在这里兼容，
//! 否则飞控连接会在解析阶段直接失败。

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    UdpIn,
    UdpOut,
    UdpBroadcast,
    TcpIn,
    TcpOut,
    Serial,
    File,
}

impl Transport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Transport::UdpIn => "udpin",
            Transport::UdpOut => "udpout",
            Transport::UdpBroadcast => "udpbcast",
            Transport::TcpIn => "tcpin",
            Transport::TcpOut => "tcpout",
            Transport::Serial => "serial",
            Transport::File => "file",
        }
    }

    /// 该传输是否需要主动重连（UDP 监听不需要，丢掉的对端会在下一帧恢复）。
    #[allow(dead_code)]
    pub fn is_connection_oriented(&self) -> bool {
        matches!(self, Transport::TcpOut | Transport::Serial)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavAddress {
    normalized: String,
    transport: Transport,
}

impl MavAddress {
    pub fn parse(raw: &str) -> Result<MavAddress, AddrError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(AddrError::Empty);
        }

        let (scheme, rest) = raw.split_once(':').ok_or(AddrError::MissingScheme)?;
        let scheme = scheme.to_ascii_lowercase();
        // `0.0.0.0:14540` 这类漏写协议的地址会在这里被识别出来，而不是报“协议不支持”。
        if scheme.is_empty() || scheme.contains('.') {
            return Err(AddrError::MissingScheme);
        }
        let rest = rest.trim_start_matches("//");

        if rest.is_empty() {
            return Err(AddrError::EmptyTarget(raw.to_string()));
        }

        let (transport, target) = match scheme.as_str() {
            "udpin" => (Transport::UdpIn, rest),
            "udpout" => (Transport::UdpOut, rest),
            "udpbcast" => (Transport::UdpBroadcast, rest),
            // 兼容旧写法：`udp://` 视为本地监听，`udpout://` 才是主动外发。
            "udp" => (Transport::UdpIn, rest),
            "tcp" | "tcpout" => (Transport::TcpOut, rest),
            "tcpin" => (Transport::TcpIn, rest),
            "serial" => (Transport::Serial, rest),
            "file" => (Transport::File, rest),
            other => return Err(AddrError::UnsupportedScheme(other.to_string())),
        };

        validate_target(transport, target)?;

        Ok(MavAddress {
            normalized: format!("{}:{}", transport.as_str(), target),
            transport,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    pub fn transport(&self) -> Transport {
        self.transport
    }
}

impl fmt::Display for MavAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.normalized)
    }
}

fn validate_target(transport: Transport, target: &str) -> Result<(), AddrError> {
    match transport {
        Transport::Serial => {
            let (port, baud) = target
                .rsplit_once(':')
                .ok_or_else(|| AddrError::InvalidTarget(target.to_string()))?;
            if port.is_empty() {
                return Err(AddrError::InvalidTarget(target.to_string()));
            }
            baud.parse::<u32>()
                .map_err(|_| AddrError::InvalidBaud(baud.to_string()))?;
        }
        Transport::UdpIn
        | Transport::UdpOut
        | Transport::UdpBroadcast
        | Transport::TcpIn
        | Transport::TcpOut => {
            let (host, port) = target
                .rsplit_once(':')
                .ok_or_else(|| AddrError::InvalidTarget(target.to_string()))?;
            if host.is_empty() {
                return Err(AddrError::InvalidTarget(target.to_string()));
            }
            let port: u16 = port
                .parse()
                .map_err(|_| AddrError::InvalidPort(port.to_string()))?;
            if port == 0 {
                return Err(AddrError::InvalidPort("0".into()));
            }
        }
        Transport::File => {
            if target.trim().is_empty() {
                return Err(AddrError::InvalidTarget(target.to_string()));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddrError {
    #[error("mavlink address must not be empty")]
    Empty,
    #[error("mavlink address is missing a transport scheme (e.g. udpin:0.0.0.0:14540)")]
    MissingScheme,
    #[error("mavlink address `{0}` has an empty target")]
    EmptyTarget(String),
    #[error("unsupported mavlink transport `{0}`")]
    UnsupportedScheme(String),
    #[error("invalid mavlink target `{0}`")]
    InvalidTarget(String),
    #[error("invalid mavlink port `{0}`")]
    InvalidPort(String),
    #[error("invalid serial baud rate `{0}`")]
    InvalidBaud(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_udp_url_binds_locally() {
        let a = MavAddress::parse("udp://0.0.0.0:14540").unwrap();
        assert_eq!(a.as_str(), "udpin:0.0.0.0:14540");
        assert_eq!(a.transport(), Transport::UdpIn);
    }

    #[test]
    fn udpout_and_tcpout_are_preserved() {
        assert_eq!(
            MavAddress::parse("udpout://127.0.0.1:14550")
                .unwrap()
                .as_str(),
            "udpout:127.0.0.1:14550"
        );
        assert_eq!(
            MavAddress::parse("tcp://192.168.1.10:5760")
                .unwrap()
                .as_str(),
            "tcpout:192.168.1.10:5760"
        );
        assert_eq!(
            MavAddress::parse("tcpin:0.0.0.0:5760").unwrap().as_str(),
            "tcpin:0.0.0.0:5760"
        );
    }

    #[test]
    fn udpbcast_is_preserved() {
        let a = MavAddress::parse("udpbcast://192.168.1.255:14550").unwrap();
        assert_eq!(a.as_str(), "udpbcast:192.168.1.255:14550");
        assert_eq!(a.transport(), Transport::UdpBroadcast);
    }

    #[test]
    fn serial_com_and_unix_ports_are_normalized() {
        assert_eq!(
            MavAddress::parse("serial://COM3:57600").unwrap().as_str(),
            "serial:COM3:57600"
        );
        assert_eq!(
            MavAddress::parse("serial:/dev/ttyUSB0:921600")
                .unwrap()
                .as_str(),
            "serial:/dev/ttyUSB0:921600"
        );
        assert_eq!(
            MavAddress::parse("serial:COM7:115200").unwrap().transport(),
            Transport::Serial
        );
    }

    #[test]
    fn canonical_addresses_pass_through() {
        let a = MavAddress::parse("udpin:0.0.0.0:14540").unwrap();
        assert_eq!(a.as_str(), "udpin:0.0.0.0:14540");
    }

    #[test]
    fn invalid_addresses_are_rejected() {
        assert_eq!(MavAddress::parse("  ").unwrap_err(), AddrError::Empty);
        assert_eq!(
            MavAddress::parse("0.0.0.0:14540").unwrap_err(),
            AddrError::MissingScheme
        );
        assert!(matches!(
            MavAddress::parse("mqtt://127.0.0.1:1883").unwrap_err(),
            AddrError::UnsupportedScheme(_)
        ));
        assert!(matches!(
            MavAddress::parse("udpin:127.0.0.1:notaport").unwrap_err(),
            AddrError::InvalidPort(_)
        ));
        assert!(matches!(
            MavAddress::parse("serial:COM3:fast").unwrap_err(),
            AddrError::InvalidBaud(_)
        ));
        assert!(matches!(
            MavAddress::parse("udpin:127.0.0.1:0").unwrap_err(),
            AddrError::InvalidPort(_)
        ));
    }

    #[test]
    fn only_active_transports_need_reconnect() {
        assert!(Transport::TcpOut.is_connection_oriented());
        assert!(Transport::Serial.is_connection_oriented());
        assert!(!Transport::UdpIn.is_connection_oriented());
        assert!(!Transport::UdpOut.is_connection_oriented());
    }
}
