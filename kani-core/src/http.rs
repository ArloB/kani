use crate::error::Result;
use arc_swap::ArcSwap;
use serde_json::json;
use std::sync::Arc;

const MAX_RETRIES: u32 = 3;
const BASE_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Clone, Default)]
struct CachedCredentials {
    cookies: String,
    user_agent: Option<String>,
}

pub enum SmartResponse {
    Normal(rquest::Response),
    Buffered {
        status: rquest::StatusCode,
        url: rquest::Url,
        headers: rquest::header::HeaderMap,
        body: bytes::Bytes,
    },
}

impl SmartResponse {
    pub fn status(&self) -> rquest::StatusCode {
        match self {
            SmartResponse::Normal(r) => r.status(),
            SmartResponse::Buffered { status, .. } => *status,
        }
    }

    pub fn url(&self) -> &rquest::Url {
        match self {
            SmartResponse::Normal(r) => r.url(),
            SmartResponse::Buffered { url, .. } => url,
        }
    }

    pub fn headers(&self) -> &rquest::header::HeaderMap {
        match self {
            SmartResponse::Normal(r) => r.headers(),
            SmartResponse::Buffered { headers, .. } => headers,
        }
    }

    pub async fn bytes(self) -> Result<bytes::Bytes> {
        match self {
            SmartResponse::Normal(r) => Ok(r.bytes().await?),
            SmartResponse::Buffered { body, .. } => Ok(body),
        }
    }

    pub async fn text(self) -> Result<String> {
        match self {
            SmartResponse::Normal(r) => Ok(r.text().await?),
            SmartResponse::Buffered { body, .. } => Ok(String::from_utf8_lossy(&body).to_string()),
        }
    }
}

#[derive(Clone)]
pub struct SmartClient {
    client: rquest::Client,
    credentials: Arc<ArcSwap<CachedCredentials>>,
    solver_url: Option<String>,
}

impl SmartClient {
    pub fn new(solver_url: Option<String>) -> Result<Self> {
        let client = rquest::Client::builder()
            .emulation(rquest_util::Emulation::Chrome130)
            .redirect(rquest::redirect::Policy::limited(10))
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(100)
            .build()?;

        Ok(Self {
            client,
            credentials: Arc::new(ArcSwap::from_pointee(CachedCredentials::default())),
            solver_url,
        })
    }

    pub async fn send_request(&self, request: rquest::Request) -> Result<SmartResponse> {
        let mut request = request;

        let creds = self.credentials.load();
        if !creds.cookies.is_empty() {
            request.headers_mut().insert(
                rquest::header::COOKIE,
                rquest::header::HeaderValue::from_str(&creds.cookies).unwrap(),
            );
        }
        if let Some(ref ua) = creds.user_agent {
            request.headers_mut().insert(
                rquest::header::USER_AGENT,
                rquest::header::HeaderValue::from_str(ua).unwrap(),
            );
        }

        let mut current_request = request;
        let mut attempt = 0;

        loop {
            let request_clone_for_retry = current_request.try_clone();

            let resp = self.client.execute(current_request).await?;
            let status = resp.status();

            if status == rquest::StatusCode::TOO_MANY_REQUESTS {
                if attempt < MAX_RETRIES
                    && let Some(next_req) = request_clone_for_retry
                {
                    let delay = if let Some(retry_after) =
                        resp.headers().get(rquest::header::RETRY_AFTER)
                    {
                        retry_after
                            .to_str()
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok())
                            .map(std::time::Duration::from_secs)
                            .unwrap_or_else(|| BASE_DELAY * 2u32.pow(attempt))
                    } else {
                        BASE_DELAY * 2u32.pow(attempt)
                    };

                    tracing::warn!(
                        "Received 429 Too Many Requests, retrying in {:?} (attempt {}/{})",
                        delay,
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(delay).await;

                    current_request = next_req;
                    attempt += 1;
                    continue;
                }

                return Ok(SmartResponse::Normal(resp));
            }

            if status.is_success() {
                let is_html = resp
                    .headers()
                    .get(rquest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|ct| ct.contains("text/html"))
                    .unwrap_or(false);

                if is_html {
                    let status = resp.status();
                    let url = resp.url().clone();
                    let headers = resp.headers().clone();
                    let bytes = resp.bytes().await?;
                    let body_str = String::from_utf8_lossy(&bytes);
                    let body_lower = body_str.to_lowercase();

                    let is_challenge = body_lower.contains("just a moment...")
                        || body_lower.contains("enable javascript");

                    if is_challenge
                        && self.solver_url.is_some()
                        && request_clone_for_retry.is_some()
                    {
                        let url_str = url.as_str().to_string();
                        let resp = self.get_rendered_page(&url_str).await?;
                        return Ok(resp);
                    } else {
                        return Ok(SmartResponse::Buffered {
                            status,
                            url,
                            headers,
                            body: bytes,
                        });
                    }
                } else {
                    return Ok(SmartResponse::Normal(resp));
                }
            }

            if (status == rquest::StatusCode::FORBIDDEN
                || status == rquest::StatusCode::SERVICE_UNAVAILABLE)
                && self.solver_url.is_some()
                && request_clone_for_retry.is_some()
            {
                let url = resp.url().as_str().to_string();

                let (new_cookies, new_ua) = self.solve_challenge(&url).await?;

                let new_creds = CachedCredentials {
                    cookies: new_cookies.clone(),
                    user_agent: Some(new_ua.clone()),
                };

                self.credentials.store(Arc::new(new_creds));

                if let Some(mut request) = request_clone_for_retry {
                    request.headers_mut().insert(
                        rquest::header::COOKIE,
                        rquest::header::HeaderValue::from_str(&new_cookies).unwrap(),
                    );

                    request.headers_mut().insert(
                        rquest::header::USER_AGENT,
                        rquest::header::HeaderValue::from_str(&new_ua).unwrap(),
                    );

                    let resp = self.client.execute(request).await?;
                    return Ok(SmartResponse::Normal(resp));
                }
            }

            return Ok(SmartResponse::Normal(resp));
        }
    }

    pub async fn get(&self, url: &str) -> Result<SmartResponse> {
        let request = self.client.get(url).build()?;
        self.send_request(request).await
    }

    pub fn inner(&self) -> &rquest::Client {
        &self.client
    }

    async fn solve_challenge(&self, url: &str) -> Result<(String, String)> {
        let solver_url = self
            .solver_url
            .as_ref()
            .ok_or_else(|| crate::error::Error::Other("No solver URL configured".into()))?;

        let client = rquest::Client::new();

        let body = json!({
          "cmd": "request.get",
          "url": url,
          "maxTimeout": 60000
        });

        let response: serde_json::Value = client
            .post(solver_url)
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

    async fn get_rendered_page(&self, url: &str) -> Result<SmartResponse> {
        let solver_url = self
            .solver_url
            .as_ref()
            .ok_or_else(|| crate::error::Error::Other("No solver URL configured".into()))?;

        let client = rquest::Client::new();

        let body = json!({
          "cmd": "request.get",
          "url": url,
          "maxTimeout": 60000
        });

        let response: serde_json::Value = client
            .post(solver_url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?
            .json()
            .await?;

        let html = response["solution"]["response"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let mut headers = rquest::header::HeaderMap::new();
        headers.insert(
            rquest::header::CONTENT_TYPE,
            rquest::header::HeaderValue::from_static("text/html"),
        );

        Ok(SmartResponse::Buffered {
            status: rquest::StatusCode::OK,
            url: rquest::Url::parse(url).map_err(|e| crate::error::Error::Other(e.to_string()))?,
            headers,
            body: bytes::Bytes::from(html),
        })
    }
}
