use crate::protocol::rtsp::common::RtspError;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AuthCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    Basic,
    Digest { realm: String, nonce: String },
}

pub struct AuthHandler {
    credentials: AuthCredentials,
    auth_method: Option<AuthMethod>,
}

impl AuthHandler {
    pub fn new(credentials: AuthCredentials) -> Self {
        Self {
            credentials,
            auth_method: None,
        }
    }

    pub fn set_basic_auth(&mut self) {
        self.auth_method = Some(AuthMethod::Basic);
    }

    pub fn build_basic_authorization(&self, _uri: &str) -> Result<String, RtspError> {
        use base64::Engine;
        let credentials = format!(
            "{}:{}",
            self.credentials.username, self.credentials.password
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        Ok(format!("Basic {}", encoded))
    }

    pub fn parse_www_authenticate(&mut self, header: &str) -> Result<(), RtspError> {
        let header = header.trim();
        if header.starts_with("Digest ") {
            let params = Self::parse_digest_params(&header[7..]);
            let realm = params
                .get("realm")
                .ok_or_else(|| RtspError::AuthFailed("Missing realm in Digest auth".into()))?;
            let nonce = params
                .get("nonce")
                .ok_or_else(|| RtspError::AuthFailed("Missing nonce in Digest auth".into()))?;
            let _qop = params.get("_qop").map(|s| s.as_str());
            self.auth_method = Some(AuthMethod::Digest {
                realm: realm.clone(),
                nonce: nonce.clone(),
            });
        } else if header.starts_with("Basic ") {
            self.auth_method = Some(AuthMethod::Basic);
        } else {
            return Err(RtspError::AuthFailed(format!(
                "Unknown auth method: {}",
                header
            )));
        }
        Ok(())
    }

    fn parse_digest_params(input: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();
        let input = input.trim();
        for part in input.split(',') {
            let part = part.trim();
            if let Some((key, value)) = part.split_once('=') {
                let key = key.trim();
                let mut value = value.trim();
                if value.starts_with('"') && value.ends_with('"') {
                    value = &value[1..value.len() - 1];
                }
                params.insert(key.to_string(), value.to_string());
            }
        }
        params
    }

    pub fn build_authorization(&self, method: &str, uri: &str) -> Result<String, RtspError> {
        match &self.auth_method {
            Some(AuthMethod::Basic) => {
                use base64::Engine;
                let credentials = format!(
                    "{}:{}",
                    self.credentials.username, self.credentials.password
                );
                let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
                Ok(format!("Basic {}", encoded))
            }
            Some(AuthMethod::Digest { realm, nonce }) => {

                let response = Self::calc_digest_response(
                    &self.credentials.username,
                    realm,
                    &self.credentials.password,
                    uri,
                    nonce,
                    method,
                );
                let auth_header = format!(
                    "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
                    self.credentials.username, realm, nonce, uri, response
                );
                Ok(auth_header)
            }
            None => Err(RtspError::AuthFailed(
                "No auth method selected".into(),
            )),
        }
    }

    fn calc_digest_response(
        username: &str,
        realm: &str,
        password: &str,
        uri: &str,
        nonce: &str,
        method: &str,
    ) -> String {
        use md5::Context;
        let ha1_input = format!("{}:{}:{}", username, realm, password);
        let ha1 = format!("{:x}", {
            let mut ctx = Context::new();
            ctx.consume(ha1_input.as_bytes());
            ctx.compute()
        });

        let ha2_input = format!("{}:{}", method, uri);
        let ha2 = format!("{:x}", {
            let mut ctx = Context::new();
            ctx.consume(ha2_input.as_bytes());
            ctx.compute()
        });

        let response_input = format!("{}:{}:{}", ha1, nonce, ha2);
        let response = format!("{:x}", {
            let mut ctx = Context::new();
            ctx.consume(response_input.as_bytes());
            ctx.compute()
        });
        response
    }

    pub fn has_auth_method(&self) -> bool {
        self.auth_method.is_some()
    }
}
