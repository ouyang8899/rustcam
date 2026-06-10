use regex::Regex;

#[derive(Debug, Clone)]
pub enum ProtocolMatcher {
    FirstBytes(Vec<u8>),
    FirstBytesCaseless(Vec<u8>),
    Regex(Regex),
    Port(u16),
    PortRange(u16, u16),
    TlsSni(String),
    All(Vec<ProtocolMatcher>),
    Any(Vec<ProtocolMatcher>),
}

impl ProtocolMatcher {
    pub fn matches(&self, data: &[u8]) -> bool {
        match self {
            ProtocolMatcher::FirstBytes(pattern) => {
                data.len() >= pattern.len() && &data[..pattern.len()] == pattern.as_slice()
            }
            ProtocolMatcher::FirstBytesCaseless(pattern) => {
                data.len() >= pattern.len()
                    && data[..pattern.len()].iter().zip(pattern.iter()).all(|(a, b)| a.eq_ignore_ascii_case(b))
            }
            ProtocolMatcher::Regex(re) => std::str::from_utf8(data).map(|s| re.is_match(s)).unwrap_or(false),
            ProtocolMatcher::Port(_) => false,
            ProtocolMatcher::PortRange(_, _) => false,
            ProtocolMatcher::TlsSni(_) => false,
            ProtocolMatcher::All(matchers) => matchers.iter().all(|m| m.matches(data)),
            ProtocolMatcher::Any(matchers) => matchers.iter().any(|m| m.matches(data)),
        }
    }

    pub fn gb28181() -> Self {
        ProtocolMatcher::FirstBytesCaseless(b"REGISTER sip:".to_vec())
    }

    pub fn onvif() -> Self {
        ProtocolMatcher::Any(vec![
            ProtocolMatcher::FirstBytesCaseless(b"GET /onvif".to_vec()),
            ProtocolMatcher::FirstBytesCaseless(b"POST /onvif".to_vec()),
        ])
    }

    pub fn rtsp() -> Self {
        ProtocolMatcher::FirstBytesCaseless(b"OPTIONS rtsp".to_vec())
    }

    pub fn websocket() -> Self {
        ProtocolMatcher::FirstBytesCaseless(b"GET /ws".to_vec())
    }

    pub fn http() -> Self {
        ProtocolMatcher::FirstBytesCaseless(b"GET ".to_vec())
    }
}