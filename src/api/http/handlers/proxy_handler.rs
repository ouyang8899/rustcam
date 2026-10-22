use std::time::Duration;
use axum::{
    extract::Query,
    response::IntoResponse,
    http::{header, StatusCode, header::HeaderValue},
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct WfsProxyQuery {
    pub url: String,
}

pub async fn wfs_proxy_handler(
    Query(query): Query<WfsProxyQuery>,
) -> impl IntoResponse {
    let url = &query.url;

    if url.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing url parameter").into_response();
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("[WfsProxy] Failed to create client: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create HTTP client").into_response();
        }
    };

    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            let content_type = response.headers().get("content-type").cloned();
            let body = response.bytes().await;

            match body {
                Ok(bytes) => {
                    let mut res = (status, bytes).into_response();
                    let headers = res.headers_mut();
                    headers.insert(
                        header::ACCESS_CONTROL_ALLOW_ORIGIN,
                        HeaderValue::from_static("*"),
                    );
                    headers.insert(
                        header::ACCESS_CONTROL_ALLOW_METHODS,
                        HeaderValue::from_static("GET, POST, OPTIONS"),
                    );
                    headers.insert(
                        header::ACCESS_CONTROL_ALLOW_HEADERS,
                        HeaderValue::from_static("*"),
                    );
                    if let Some(ct) = content_type {
                        headers.insert(header::CONTENT_TYPE, ct);
                    } else {
                        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    }
                    res
                }
                Err(e) => {
                    tracing::error!("[WfsProxy] Failed to read body: {}", e);
                    (StatusCode::BAD_GATEWAY, format!("Failed to read response: {}", e)).into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("[WfsProxy] Request failed: {}", e);
            (StatusCode::BAD_GATEWAY, format!("Request failed: {}", e)).into_response()
        }
    }
}
