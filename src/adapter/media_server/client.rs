use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::time::{timeout, Duration as TokioDuration};
use tracing::{info, error,debug, Level};
pub struct MediaServerClient {
    base_url: String,
    secret: String,
    http: Client,
}

impl MediaServerClient {
    pub fn new(base_url: &str, secret: &str) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        let base_url = base_url.trim_end_matches('/').to_string();
        Self { base_url, secret: secret.to_string(), http }
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

        let resp = timeout(TokioDuration::from_secs(15), self.http.get(&url_with_secret).send()).await
            .map_err(|_| anyhow::anyhow!("GET request timeout after 15s"))??;
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
        tracing::debug!("POST URL {}", url_with_secret);
        let resp = timeout(TokioDuration::from_secs(15), self.http.post(&url_with_secret).json(body).send()).await
            .map_err(|_| anyhow::anyhow!("request timeout after 15s"))??;

        let status = resp.status();
        let body_text = resp.text().await
            .map_err(|e| anyhow::anyhow!("read body failed: {}", e))?;
        tracing::debug!("POST RETURN DATA {}", body_text);
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

    pub fn base_url(&self) -> &str { &self.base_url }
}