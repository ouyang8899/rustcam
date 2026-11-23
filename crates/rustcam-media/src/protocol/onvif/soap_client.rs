use anyhow::anyhow;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::time::Duration;

use crate::protocol::onvif::auth::UsernameToken;
use crate::protocol::onvif::soap::xml_escape;

const SOAP_ENV_NS: &str = "http://www.w3.org/2003/05/soap-envelope";
const WSSE_NS: &str =
    "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd";
const WSU_NS: &str =
    "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd";
const PASSWORD_DIGEST_TYPE: &str =
    "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest";
const PASSWORD_TEXT_TYPE: &str =
    "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText";
const BASE64_BINARY_ENCODING: &str =
    "http://docs.oasis-open.org/wss/200401/wss-soap-message-security-1.0#Base64Binary";

const MAX_AUTH_ATTEMPTS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    Anonymous,
    HttpDigest,
    WsSecurityDigest,
    WsSecurityText,
}

#[derive(Debug, Clone)]
pub struct DigestChallenge {
    pub realm: String,
    pub nonce: String,
    pub algorithm: Option<String>,
}

impl DigestChallenge {
    pub fn parse(header: &str) -> Option<Self> {
        let realm = extract_digest_param(header, "realm")?;
        let nonce = extract_digest_param(header, "nonce")?;
        Some(Self {
            realm,
            nonce,
            algorithm: extract_digest_param(header, "algorithm"),
        })
    }

    pub fn supports_md5(&self) -> bool {
        match self.algorithm.as_deref() {
            None => true,
            Some(a) => a.eq_ignore_ascii_case("MD5"),
        }
    }
}

fn extract_digest_param(header: &str, param: &str) -> Option<String> {
    let quoted = format!("{}=\"", param);
    if let Some(start) = header.find(&quoted) {
        let rest = &header[start + quoted.len()..];
        let end = rest.find('"').unwrap_or(rest.len());
        return Some(rest[..end].to_string());
    }
    let bare = format!("{}=", param);
    let start = header.find(&bare)? + bare.len();
    let rest = &header[start..];
    let end = rest.find([',', ' ']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

struct AuthFailure {
    status: Option<reqwest::StatusCode>,
    body: String,
    challenge: Option<String>,
}

impl AuthFailure {
    fn next_method(&self, tried: &[AuthMethod]) -> Option<AuthMethod> {
        let candidates: Vec<AuthMethod> = match self.status {
            Some(reqwest::StatusCode::UNAUTHORIZED) => match self.challenge.as_deref() {
                Some(c) if c.contains("Digest") => vec![AuthMethod::HttpDigest],
                _ => vec![AuthMethod::WsSecurityDigest, AuthMethod::WsSecurityText],
            },
            Some(reqwest::StatusCode::BAD_REQUEST)
                if self.body.contains("NotAuthorized")
                    || self.body.contains("ter:NotAuthorized") =>
            {
                vec![
                    AuthMethod::WsSecurityDigest,
                    AuthMethod::WsSecurityText,
                    AuthMethod::HttpDigest,
                ]
            }
            _ => Vec::new(),
        };
        candidates.into_iter().find(|m| !tried.contains(m))
    }
}

pub struct OnvifSoapClient {
    endpoint: String,
    username: Option<String>,
    password: Option<String>,
    http: reqwest::Client,
    auth_method: AuthMethod,
    digest: Option<DigestChallenge>,
}

impl OnvifSoapClient {
    pub fn new(
        endpoint: impl Into<String>,
        username: Option<String>,
        password: Option<String>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("ONVIF HTTP client build failed");
        let auth_method = if username.is_some() && password.is_some() {
            AuthMethod::WsSecurityDigest
        } else {
            AuthMethod::Anonymous
        };
        Self {
            endpoint: endpoint.into(),
            username,
            password,
            http,
            auth_method,
            digest: None,
        }
    }

    pub fn with_credentials(mut self, username: &str, password: &str) -> Self {
        self.username = Some(username.to_string());
        self.password = Some(password.to_string());
        self.auth_method = AuthMethod::WsSecurityDigest;
        self
    }

    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn auth_method(&self) -> AuthMethod {
        self.auth_method
    }

    pub async fn post(&mut self, action: Option<&str>, body: &str) -> anyhow::Result<String> {
        let mut tried: Vec<AuthMethod> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for _ in 0..MAX_AUTH_ATTEMPTS {
            tried.push(self.auth_method);
            match self.send_once(action, body).await {
                Ok(text) => return Ok(text),
                Err(failure) => {
                    if let Some(challenge) =
                        failure.challenge.as_deref().and_then(DigestChallenge::parse)
                    {
                        self.digest = Some(challenge);
                    }
                    let detail = match failure.status {
                        Some(status) => match failure.challenge.as_deref() {
                            Some(c) => format!("status={} challenge={} body={}", status, c, failure.body),
                            None => format!("status={} body={}", status, failure.body),
                        },
                        None => failure.body.clone(),
                    };
                    errors.push(detail);
                    match failure.next_method(&tried) {
                        Some(next) => {
                            tracing::debug!(
                                "[ONVIF] auth negotiation for {}: {:?} -> {:?}",
                                self.endpoint,
                                self.auth_method,
                                next
                            );
                            self.auth_method = next;
                        }
                        None => break,
                    }
                }
            }
        }

        Err(anyhow!(
            "ONVIF SOAP request to {} failed: {}",
            self.endpoint,
            errors.join(" | ")
        ))
    }

    async fn send_once(&self, action: Option<&str>, body: &str) -> Result<String, AuthFailure> {
        let envelope = self.build_envelope(body);
        let mut req = self
            .http
            .post(&self.endpoint)
            .header("Content-Type", "application/soap+xml; charset=utf-8");
        if let Some(a) = action {
            req = req.header("SOAPAction", format!("\"{}\"", a));
        }
        if let Some(auth) = self.http_digest_authorization() {
            req = req.header("Authorization", auth);
        }

        let resp = match req.body(envelope).send().await {
            Ok(r) => r,
            Err(e) => {
                return Err(AuthFailure {
                    status: None,
                    body: format!("transport error: {}", e),
                    challenge: None,
                })
            }
        };

        let status = resp.status();
        let challenge = resp
            .headers()
            .get("www-authenticate")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let text = resp.text().await.unwrap_or_default();

        if status.is_success() {
            return Ok(text);
        }
        Err(AuthFailure {
            status: Some(status),
            body: text,
            challenge,
        })
    }

    fn build_envelope(&self, body: &str) -> String {
        self.build_envelope_with(self.auth_method, body)
    }

    fn build_envelope_with(&self, method: AuthMethod, body: &str) -> String {
        let header = match self.security_block(method) {
            Some(sec) => format!("<s:Header>{}</s:Header>", sec),
            None => "<s:Header/>".to_string(),
        };
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<s:Envelope xmlns:s=\"{}\" xmlns:wsse=\"{}\" xmlns:wsu=\"{}\">\n{}\n<s:Body>{}</s:Body>\n</s:Envelope>",
            SOAP_ENV_NS, WSSE_NS, WSU_NS, header, body
        )
    }

    fn security_block(&self, method: AuthMethod) -> Option<String> {
        let (username, password) = (self.username.as_ref()?, self.password.as_ref()?);
        match method {
            AuthMethod::WsSecurityDigest => {
                let (nonce, created, digest) = UsernameToken::build_digest(username, password);
                Some(format!(
                    "<wsse:Security s:mustUnderstand=\"1\"><wsse:UsernameToken><wsse:Username>{}</wsse:Username><wsse:Password Type=\"{}\">{}</wsse:Password><wsse:Nonce EncodingType=\"{}\">{}</wsse:Nonce><wsu:Created>{}</wsu:Created></wsse:UsernameToken></wsse:Security>",
                    xml_escape(username),
                    PASSWORD_DIGEST_TYPE,
                    digest,
                    BASE64_BINARY_ENCODING,
                    nonce,
                    created
                ))
            }
            AuthMethod::WsSecurityText => Some(format!(
                "<wsse:Security s:mustUnderstand=\"1\"><wsse:UsernameToken><wsse:Username>{}</wsse:Username><wsse:Password Type=\"{}\">{}</wsse:Password></wsse:UsernameToken></wsse:Security>",
                xml_escape(username),
                PASSWORD_TEXT_TYPE,
                xml_escape(password)
            )),
            AuthMethod::Anonymous | AuthMethod::HttpDigest => None,
        }
    }

    fn http_digest_authorization(&self) -> Option<String> {
        let challenge = self.digest.as_ref()?;
        if !challenge.supports_md5() {
            return None;
        }
        let username = self.username.as_deref()?;
        let password = self.password.as_deref()?;

        let cnonce_bytes: [u8; 16] = rand::random();
        let cnonce = STANDARD.encode(cnonce_bytes);
        let uri = self.request_uri();

        let ha1 = format!("{}:{}:{}", username, challenge.realm, password);
        let ha2 = format!("POST:{}", uri);
        let ha1_md5 = format!("{:x}", md5::compute(ha1.as_bytes()));
        let ha2_md5 = format!("{:x}", md5::compute(ha2.as_bytes()));
        let response = format!(
            "{}:{}:00000001:{}:auth:{}",
            ha1_md5, challenge.nonce, cnonce, ha2_md5
        );
        let response_md5 = format!("{:x}", md5::compute(response.as_bytes()));

        Some(format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", qop=auth, nc=00000001, cnonce=\"{}\", response=\"{}\", algorithm=MD5",
            username,
            challenge.realm,
            challenge.nonce,
            uri,
            cnonce,
            response_md5
        ))
    }

    fn request_uri(&self) -> String {
        match url::Url::parse(&self.endpoint) {
            Ok(u) => {
                let mut path = u.path().to_string();
                if let Some(q) = u.query() {
                    path.push('?');
                    path.push_str(q);
                }
                if path.is_empty() {
                    path.push('/');
                }
                path
            }
            Err(_) => "/onvif/device_service".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roxmltree::Document;

    fn body_element() -> String {
        "<GetDeviceInformation xmlns=\"http://www.onvif.org/ver10/device/wsdl\"/>".to_string()
    }

    #[test]
    fn digest_challenge_parses_realm_and_nonce() {
        let c = DigestChallenge::parse(
            "Digest realm=\"IP Camera\", nonce=\"abc123\", algorithm=MD5, qop=\"auth\"",
        )
        .expect("challenge should parse");
        assert_eq!(c.realm, "IP Camera");
        assert_eq!(c.nonce, "abc123");
        assert!(c.supports_md5());
    }

    #[test]
    fn digest_challenge_rejects_non_md5() {
        let c = DigestChallenge::parse("Digest realm=\"r\", nonce=\"n\", algorithm=MD5-sess")
            .expect("challenge should parse");
        assert!(!c.supports_md5());
    }

    #[test]
    fn anonymous_envelope_is_namespace_wellformed() {
        let client = OnvifSoapClient::new("http://127.0.0.1:1/onvif/device_service", None, None);
        let envelope = client.build_envelope(&body_element());
        let doc = Document::parse(&envelope).expect("envelope must be well-formed XML");
        let ns = doc.root_element().tag_name().namespace().unwrap();
        assert_eq!(ns, "http://www.w3.org/2003/05/soap-envelope");
        assert!(envelope.contains("<GetDeviceInformation"));
    }

    #[test]
    fn ws_security_envelope_declares_every_prefix() {
        let client = OnvifSoapClient::new(
            "http://127.0.0.1:1/onvif/ptz_service",
            Some("admin".into()),
            Some("secret".into()),
        );
        let envelope = client.build_envelope(&body_element());
        Document::parse(&envelope).expect("envelope must be well-formed XML");
        assert!(!envelope.contains("SOAP-ENV:"), "no undeclared SOAP-ENV prefix");
        assert!(envelope.contains("s:mustUnderstand"));
        assert!(envelope.contains("<wsu:Created>"));
        assert!(envelope.contains("#PasswordDigest"));
    }

    #[test]
    fn ws_security_text_uses_plaintext_password() {
        let client = OnvifSoapClient::new(
            "http://127.0.0.1:1/onvif/ptz_service",
            Some("admin".into()),
            Some("secret".into()),
        );
        let envelope = client.build_envelope_with(AuthMethod::WsSecurityText, &body_element());
        assert!(envelope.contains("#PasswordText"));
        assert!(envelope.contains(">secret<"));
    }

    #[test]
    fn http_digest_authorization_matches_rfc_2617() {
        let mut client = OnvifSoapClient::new(
            "http://cam.local/onvif/ptz_service",
            Some("admin".into()),
            Some("secret".into()),
        );
        client.digest = Some(DigestChallenge {
            realm: "IP Camera".into(),
            nonce: "NONCE".into(),
            algorithm: Some("MD5".into()),
        });
        let header = client
            .http_digest_authorization()
            .expect("authorization header should be built");
        assert!(header.starts_with("Digest username=\"admin\""));
        assert!(header.contains("realm=\"IP Camera\""));
        assert!(header.contains("uri=\"/onvif/ptz_service\""));
        assert!(header.contains("algorithm=MD5"));

        let ha1 = format!("{:x}", md5::compute(b"admin:IP Camera:secret"));
        let ha2 = format!("{:x}", md5::compute(b"POST:/onvif/ptz_service"));
        let cnonce = header
            .split("cnonce=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("cnonce present");
        let expected = format!(
            "{:x}",
            md5::compute(format!("{}:NONCE:00000001:{}:auth:{}", ha1, cnonce, ha2).as_bytes())
        );
        assert!(
            header.contains(&expected),
            "header must contain the RFC 2617 response hash"
        );
    }
}
