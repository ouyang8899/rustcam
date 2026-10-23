use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::time::{timeout, Duration as TokioDuration};

/// Timeout of a single API call against a media server.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 4;

pub struct MediaServerClient {
    base_url: String,
    secret: String,
    http: Client,
    timeout_secs: u64,
}

impl MediaServerClient {
    pub fn new(base_url: &str, secret: &str) -> Self {
        Self::with_timeout(base_url, secret, DEFAULT_REQUEST_TIMEOUT_SECS)
    }

    pub fn with_timeout(base_url: &str, secret: &str, timeout_secs: u64) -> Self {
        // The transport timeout is a safety net that has to outlive the
        // request timeout below, otherwise reqwest aborts the request first
        // and the caller only sees a generic "error sending request".
        let http = Client::builder()
            .timeout(Duration::from_secs(timeout_secs + 2))
            .build()
            .expect("Failed to create HTTP client");

        let base_url = base_url.trim_end_matches('/').to_string();
        Self { base_url, secret: secret.to_string(), http, timeout_secs }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.get_with_params(path, &[]).await
    }

    pub async fn get_with_params<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &String)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut url_with_secret = format!("{}?secret={}", url, self.secret);
        for (k, v) in params {
            url_with_secret.push_str(&format!("&{}={}", k, v));
        }

        let start = std::time::Instant::now();
        let resp = timeout(
            TokioDuration::from_secs(self.timeout_secs),
            self.http.get(&url_with_secret).send(),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "GET request timeout after {}s",
                self.timeout_secs
            )
        })??;
        let _elapsed = start.elapsed();

        let status = resp.status();
        let body_text = resp.text().await
            .map_err(|e| anyhow::anyhow!("read body failed: {}", e))?;

        if !status.is_success() {
            anyhow::bail!("API error: {} - {}", status, body_text);
        }

        let body: serde_json::Value = serde_json::from_str(&body_text)
            .context("Deserialize response")?;
        serde_json::from_value(body).context("Deserialize response")
    }

    pub async fn post<T: DeserializeOwned, B: serde::Serialize>(
        &self, path: &str, body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let url_with_secret = format!("{}?secret={}", url, self.secret);
        let _body_json = serde_json::to_string(body).unwrap_or_default();

        let start = std::time::Instant::now();
        let resp = timeout(
            TokioDuration::from_secs(self.timeout_secs),
            self.http.post(&url_with_secret).json(body).send(),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "POST request timeout after {}s",
                self.timeout_secs
            )
        })??;
        let _elapsed = start.elapsed();

        let status = resp.status();
        let body_text = resp.text().await
            .map_err(|e| anyhow::anyhow!("read body failed: {}", e))?;

        if !status.is_success() {
            anyhow::bail!("API error: {} - {}", status, body_text);
        }

        let body: serde_json::Value = serde_json::from_str(&body_text)
            .context("Deserialize response")?;
        serde_json::from_value(body).context("Deserialize response")
    }

    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.post(path, &serde_json::Value::Null).await
    }

    pub async fn post_with_params<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &String)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut url_with_secret = format!("{}?secret={}", url, self.secret);
        for (k, v) in params {
            url_with_secret.push_str(&format!("&{}={}", k, v));
        }

        let start = std::time::Instant::now();
        let resp = timeout(
            TokioDuration::from_secs(self.timeout_secs),
            self.http.post(&url_with_secret).send(),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "POST request timeout after {}s",
                self.timeout_secs
            )
        })??;
        let _elapsed = start.elapsed();

        let status = resp.status();
        let body_text = resp.text().await
            .map_err(|e| anyhow::anyhow!("read body failed: {}", e))?;

        if !status.is_success() {
            anyhow::bail!("API error: {} - {}", status, body_text);
        }

        let body: serde_json::Value = serde_json::from_str(&body_text)
            .context("Deserialize response")?;
        serde_json::from_value(body).context("Deserialize response")
    }

    pub fn base_url(&self) -> &str { &self.base_url }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A server that accepts the connection but never answers: the client has
    /// to give up after the configured timeout and report that timeout.
    #[tokio::test]
    async fn test_request_timeout_error_reports_the_configured_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => held.push(stream),
                    Err(_) => break,
                }
            }
        });

        let client = MediaServerClient::with_timeout(&format!("http://{}", addr), "secret", 1);
        let err = client
            .get::<serde_json::Value>("/index/api/getStatistic")
            .await
            .expect_err("the request must time out");

        assert!(
            err.to_string().contains("timeout after 1s"),
            "the error must report the configured timeout, got: {}",
            err
        );
    }
}
