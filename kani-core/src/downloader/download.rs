//! HTTP client for downloading with challenge solving support.

use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::Result;

/// Cached credentials from challenge solver
#[derive(Clone, Default)]
struct CachedCredentials {
    cookies: String,
    user_agent: Option<String>,
}

#[derive(Clone)]
pub struct DownloadClient {
    client: rquest::Client,
    credentials: Arc<Mutex<CachedCredentials>>,
    solver_url: String,
}

impl DownloadClient {
    pub fn new(solver_url: &str) -> Result<Self> {
        let client = rquest::Client::builder()
            .emulation(rquest_util::Emulation::Chrome130)
            .build()?;

        Ok(Self {
            client,
            credentials: Arc::new(Mutex::new(CachedCredentials::default())),
            solver_url: solver_url.to_string(),
        })
    }

    pub async fn get(&self, url: &str) -> Result<rquest::Response> {
        let creds = self.credentials.lock().await.clone();

        let mut request = self.client.get(url).header("Cookie", &creds.cookies);

        // Use cached user agent if available
        if let Some(ref ua) = creds.user_agent {
            request = request.header("User-Agent", ua);
        }

        let resp = request.send().await?;

        if resp.status().is_success() {
            return Ok(resp);
        }

        if resp.status() == 403 || resp.status() == 503 {
            let (new_cookies, new_ua) = self.solve_challenge(url).await?;

            // Cache both cookies and user agent for future requests
            {
                let mut creds = self.credentials.lock().await;
                creds.cookies = new_cookies.clone();
                creds.user_agent = Some(new_ua.clone());
            }

            let resp = self
                .client
                .get(url)
                .header("Cookie", new_cookies)
                .header("User-Agent", new_ua)
                .send()
                .await?;

            return Ok(resp);
        }

        Ok(resp)
    }

    async fn solve_challenge(&self, url: &str) -> Result<(String, String)> {
        let client = rquest::Client::new();

        let body = json!({
          "cmd": "request.get",
          "url": url,
          "maxTimeout": 60000
        });

        let response: serde_json::Value = client
            .post(&self.solver_url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?
            .json()
            .await?;

        let ua = response["solution"]["userAgent"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let cookies = response["solution"]["cookies"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|c| {
                format!(
                    "{}={}",
                    c["name"].as_str().unwrap_or_default(),
                    c["value"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<String>>()
            .join("; ");

        Ok((cookies, ua))
    }
}
