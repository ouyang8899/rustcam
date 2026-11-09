//! 机载链路身份：把链路地址归一化成「协议族 + 端点 + 飞控系统号」。
//!
//! 一架无人机的机载链路身份 = 端点（协议族 + 端口/串口）+ 飞控系统号。
//! 同一 adapter 下：
//! - 同一端口可以承载多架机（按飞控系统号区分）；
//! - 因此唯一性判定必须用「协议族 + 端点 + 系统号」三者组合，
//!   只看系统号会误判（不同端口的同号飞控是合法的）。

/// 传输族。**不含 in/out 方向**：同一端口的收、发方向可以并存，
/// 方向参与唯一键会把同一端口判成重复。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkFamily {
    Tcp,
    Udp,
    Serial,
}

impl LinkFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkFamily::Tcp => "tcp",
            LinkFamily::Udp => "udp",
            LinkFamily::Serial => "serial",
        }
    }

    pub fn parse(raw: &str) -> Option<LinkFamily> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "tcp" => Some(LinkFamily::Tcp),
            "udp" => Some(LinkFamily::Udp),
            "serial" => Some(LinkFamily::Serial),
            _ => None,
        }
    }
}

/// 端点：TCP/UDP 是端口号，串口是设备名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEndpoint {
    pub family: LinkFamily,
    /// 端口号（tcp/udp）或设备名（serial）。
    pub endpoint: String,
}

/// 从链路地址串解析端点。
///
/// 支持 `udpin:0.0.0.0:14540`、`udpout:10.0.0.9:14550`、`tcpin:0.0.0.0:5760`、
/// `serial:COM3:57600` 等写法；协议前缀（udpin/udpout/…）只决定方向，不进身份。
pub fn parse_endpoint(addr: &str) -> Option<LinkEndpoint> {
    let raw = addr.trim();
    if raw.is_empty() {
        return None;
    }
    let (scheme, rest) = raw.split_once(':')?;
    let rest = rest.trim_start_matches("//");
    let family = match scheme.trim().to_ascii_lowercase().as_str() {
        "tcp" | "tcpin" | "tcpout" => LinkFamily::Tcp,
        "udp" | "udpin" | "udpout" | "udpbcast" => LinkFamily::Udp,
        "serial" => LinkFamily::Serial,
        _ => return None,
    };
    if rest.is_empty() {
        return None;
    }
    let endpoint = match family {
        // serial:COM3:57600 → 设备名是第二段
        LinkFamily::Serial => rest.split(':').next().unwrap_or_default().to_string(),
        // host:port → 端口是最后一段
        _ => rest.rsplit_once(':')?.1.to_string(),
    };
    if endpoint.is_empty() {
        return None;
    }
    Some(LinkEndpoint { family, endpoint })
}

/// 链路身份键：协议族 + 端点 + 飞控系统号。
///
/// 用于同一 adapter 内的唯一性判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkIdentity {
    pub family: LinkFamily,
    pub endpoint: String,
    pub sysid: i32,
}

impl LinkIdentity {
    /// 解析「地址串 + 飞控系统号」得到链路身份。
    pub fn parse(addr: &str, sysid: i32) -> Option<LinkIdentity> {
        let endpoint = parse_endpoint(addr)?;
        Some(LinkIdentity {
            family: endpoint.family,
            endpoint: endpoint.endpoint,
            sysid,
        })
    }

    /// 归一化键：族 + 端点 + 系统号。
    pub fn key(&self) -> String {
        format!("{}:{}#{}", self.family.as_str(), self.endpoint, self.sysid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_and_tcp_endpoints_are_parsed() {
        let e = parse_endpoint("udpin:0.0.0.0:14540").expect("parse");
        assert_eq!(e.family, LinkFamily::Udp);
        assert_eq!(e.endpoint, "14540");

        let e = parse_endpoint("udpout:10.0.0.9:14550").expect("parse");
        assert_eq!(e.family, LinkFamily::Udp);
        assert_eq!(e.endpoint, "14550");

        let e = parse_endpoint("tcpin:0.0.0.0:5760").expect("parse");
        assert_eq!(e.family, LinkFamily::Tcp);
        assert_eq!(e.endpoint, "5760");
    }

    #[test]
    fn tcp_in_and_out_share_one_port() {
        // 收/发方向不属于身份：同一端口的 in/out 视为同一端点。
        let a = parse_endpoint("tcpin:0.0.0.0:5760").expect("parse");
        let b = parse_endpoint("tcpout:0.0.0.0:5760").expect("parse");
        assert_eq!(a, b);
    }

    #[test]
    fn serial_endpoint_is_the_device_name() {
        let e = parse_endpoint("serial:COM3:57600").expect("parse");
        assert_eq!(e.family, LinkFamily::Serial);
        assert_eq!(e.endpoint, "COM3");
    }

    #[test]
    fn identity_key_combines_family_endpoint_and_sysid() {
        let a = LinkIdentity::parse("udpin:0.0.0.0:14540", 7).expect("identity");
        let b = LinkIdentity::parse("udpin:0.0.0.0:14540", 8).expect("identity");
        let c = LinkIdentity::parse("udpin:0.0.0.0:14541", 7).expect("identity");
        assert_ne!(a.key(), b.key(), "同端口不同 sysid 是不同飞控");
        assert_ne!(a.key(), c.key(), "不同端口是不同端点");
        assert_eq!(
            a.key(),
            LinkIdentity::parse("udpout:0.0.0.0:14540", 7)
                .expect("identity")
                .key(),
            "方向不参与身份"
        );
    }

    #[test]
    fn invalid_addresses_are_rejected() {
        assert!(parse_endpoint("").is_none());
        assert!(parse_endpoint("0.0.0.0:14540").is_none());
        assert!(parse_endpoint("mqtt://1.2.3.4:1883").is_none());
        assert!(parse_endpoint("udpin:0.0.0.0").is_none());
    }
}
