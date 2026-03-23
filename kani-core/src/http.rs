use crate::{error::Result, network::ValidatingResolver};
use arc_swap::ArcSwap;
use futures::{TryStream, TryStreamExt};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};

const MAX_RETRIES: u32 = 3;
const BASE_DELAY: std::time::Duration = std::time::Duration::from_secs(5);
const CREDENTIAL_TTL_SECS: u64 = 3600;

#[derive(Clone, Default)]
pub struct CachedCredentials {
    cookies: String,
    user_agent: Option<String>,
    stored_at: Option<std::time::Instant>,
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

    pub async fn chunk(&mut self) -> Result<Option<bytes::Bytes>> {
        match self {
            SmartResponse::Normal(r) => Ok(r.chunk().await?),
            SmartResponse::Buffered { body, .. } => {
                if body.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(std::mem::take(body)))
                }
            }
        }
    }

    pub async fn bytes_limited(self, max_bytes: usize) -> Result<bytes::Bytes> {
        let content_length = self
            .headers()
            .get(rquest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok());

        let stream = Box::pin(futures::stream::unfold(self, |mut resp| async move {
            match resp.chunk().await {
                Ok(Some(bytes)) => Some((Ok(bytes), resp)),
                Ok(None) => None,
                Err(e) => Some((Err(e), resp)),
            }
        }));

        collect_bytes_limited(stream, content_length, max_bytes).await
    }
}

#[derive(Clone)]
pub struct SmartClient {
    client: rquest::Client,
    pub credentials: Arc<ArcSwap<HashMap<String, CachedCredentials>>>,
    solver_url: Arc<ArcSwap<Option<String>>>,
    pub solving: Arc<dashmap::DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
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
            credentials: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            solver_url: Arc::new(ArcSwap::from_pointee(solver_url)),
            solving: Arc::new(dashmap::DashMap::new()),
        })
    }

    pub fn new_proxy(
        solver_url: Option<String>,
        credentials: Arc<ArcSwap<HashMap<String, CachedCredentials>>>,
        solving: Arc<dashmap::DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    ) -> Result<Self> {
        let resolver = ValidatingResolver::new()?;
        let client = rquest::Client::builder()
            .emulation(rquest_util::Emulation::Chrome130)
            .redirect(rquest::redirect::Policy::none())
            .dns_resolver(Arc::new(resolver))
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(100)
            .build()?;

        Ok(Self {
            client,
            credentials,
            solver_url: Arc::new(ArcSwap::from_pointee(solver_url)),
            solving,
        })
    }

    pub async fn send_request(&self, request: rquest::Request) -> Result<SmartResponse> {
        let mut request = request;

        let domain = request.url().host_str().map(base_domain).unwrap_or_default();
        let creds_map = self.credentials.load();
        if let Some(creds) = creds_map.get(&domain) {
            let expired = creds.stored_at
                .map(|t| t.elapsed().as_secs() > CREDENTIAL_TTL_SECS)
                .unwrap_or(true);

            if expired {
                tracing::debug!("Credentials for {} have expired, dropping", domain);
                drop(creds_map);
                let mut fresh = (**self.credentials.load()).clone();
                fresh.remove(&domain);
                self.credentials.store(Arc::new(fresh));
            } else {
                if let Ok(val) = rquest::header::HeaderValue::from_str(&creds.cookies) {
                    request.headers_mut().insert(rquest::header::COOKIE, val);
                } else {
                    tracing::warn!("Stored cookies for {} contained invalid header characters, skipping", domain);
                }

                if let Some(ref ua) = creds.user_agent
                && let Ok(val) = rquest::header::HeaderValue::from_str(ua) {
                    request.headers_mut().insert(rquest::header::USER_AGENT, val);
                }
            }
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
                        && self.solver_url.load().is_some()
                        && request_clone_for_retry.is_some()
                    {
                        let url_str = url.as_str().to_string();
                        let resp = self.get_rendered_page_once(&url_str).await?;
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
                && self.solver_url.load().is_some()
                && request_clone_for_retry.is_some()
            {
                let url = resp.url().as_str().to_string();
                let domain = url.parse::<rquest::Url>()
                    .ok()
                    .and_then(|u| u.host_str().map(base_domain));

                if let Some(domain) = domain {
                    let mut creds = (**self.credentials.load()).clone();
                    if creds.remove(&domain).is_some() {
                        tracing::info!(
                            "Stored credentials for {} returned 403, clearing and re-solving",
                            domain
                        );
                        self.credentials.store(Arc::new(creds));
                    }
                }

                let (new_cookies, new_ua) = self.solve_challenge_once(&url).await?;

                if let Some(mut request) = request_clone_for_retry {
                    if let Ok(val) = rquest::header::HeaderValue::from_str(&new_cookies) {
                        request.headers_mut().insert(rquest::header::COOKIE, val);
                    }

                    if let Ok(val) = rquest::header::HeaderValue::from_str(&new_ua) {
                        request.headers_mut().insert(rquest::header::USER_AGENT, val);
                    }

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

    pub async fn safe_get(
        &self,
        initial_url: &str,
        headers: Option<rquest::header::HeaderMap>,
    ) -> Result<SmartResponse> {
        const MAX_REDIRECTS: usize = 5;

        let mut current_url = initial_url.to_string();
        let mut solver_headers = rquest::header::HeaderMap::new();
        let mut solved = false;

        if let Ok(parsed) = initial_url.parse::<rquest::Url>()
            && let Some(domain) = parsed.host_str().map(base_domain)
        {
            let creds_map = self.credentials.load();
            if let Some(creds) = creds_map.get(&domain) {
                let expired = creds.stored_at
                    .map(|t| t.elapsed().as_secs() > CREDENTIAL_TTL_SECS)
                    .unwrap_or(true);
                if expired {
                    tracing::debug!("Credentials for {} have expired, clearing", domain);
                    drop(creds_map);
                    let mut fresh = (**self.credentials.load()).clone();
                    fresh.remove(&domain);
                    self.credentials.store(Arc::new(fresh));
                } else {
                    if let Ok(val) = rquest::header::HeaderValue::from_str(&creds.cookies) {
                        solver_headers.insert(rquest::header::COOKIE, val);
                    }
                    if let Some(ref ua) = creds.user_agent
                    && let Ok(val) = rquest::header::HeaderValue::from_str(ua) {
                        solver_headers.insert(rquest::header::USER_AGENT, val);
                    }
                }
            }
        }

        for _ in 0..MAX_REDIRECTS {
            let mut req_builder = self.client.get(&current_url);
            if current_url == initial_url && let Some(ref h) = headers {
                req_builder = req_builder.headers(h.clone());
            }
            if !solver_headers.is_empty() {
                req_builder = req_builder.headers(solver_headers.clone());
            }
            let req = req_builder.build()?;

            let resp = self.client.execute(req).await?;

            if resp.status().is_redirection() {
                let location = resp
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        crate::error::Error::Other("redirect with no Location header".into())
                    })?;

                let next = resp
                    .url()
                    .join(location)
                    .map_err(|e| crate::error::Error::Other(
                        format!("invalid redirect URL '{}': {}", location, e)
                    ))?;

                match next.scheme() {
                    "http" | "https" => {}
                    s => return Err(crate::error::Error::Other(format!(
                        "redirect to forbidden scheme: {}", s
                    ))),
                }

                current_url = next.to_string();

            } else if (resp.status() == rquest::StatusCode::FORBIDDEN
                || resp.status() == rquest::StatusCode::SERVICE_UNAVAILABLE)
                && !solved
                && self.solver_url.load().is_some()
            {
                let url = resp.url().as_str().to_string();
                let domain = url.parse::<rquest::Url>()
                    .ok()
                    .and_then(|u| u.host_str().map(base_domain));

                if let Some(domain) = domain {
                    let mut creds = (**self.credentials.load()).clone();
                    if creds.remove(&domain).is_some() {
                        tracing::info!(
                            "Stored credentials for {} returned 403, clearing and re-solving",
                            domain
                        );
                        self.credentials.store(Arc::new(creds));
                    }
                }

                let (cookies, ua) = self.solve_challenge_once(&url).await?;

                if let Ok(val) = rquest::header::HeaderValue::from_str(&cookies) {
                    solver_headers.insert(rquest::header::COOKIE, val);
                }
                if let Ok(val) = rquest::header::HeaderValue::from_str(&ua) {
                    solver_headers.insert(rquest::header::USER_AGENT, val);
                }

                solved = true;

            } else {
                return Ok(SmartResponse::Normal(resp));
            }
        }

        Err(crate::error::Error::Other(format!(
            "too many redirects following '{}'",
            initial_url
        )))
    }

    pub fn inner(&self) -> &rquest::Client {
        &self.client
    }

    async fn solve_challenge(&self, url: &str) -> Result<(String, String)> {
        let guard = self.solver_url.load();
        let solver_url = guard.as_deref().ok_or_else(|| crate::error::Error::Other("No solver URL configured".into()))?;

        let client = rquest::Client::new();

        let body = json!({
          "cmd": "request.get",
          "url": url,
          "maxTimeout": 60000
        });

        let response = client
            .post(solver_url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(crate::error::Error::Other(format!(
                "FlareSolverr returned HTTP {}: check your solver URL is correct (should end in /v1)",
                response.status()
            )));
        }

        let response: serde_json::Value = response.json().await?;

        let status = response["status"].as_str().unwrap_or("");
        if status != "ok" {
            return Err(crate::error::Error::Other(format!(
                "FlareSolverr challenge failed with status '{}': {}",
                status,
                response["message"].as_str().unwrap_or("no message")
            )));
        }

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

    async fn solve_challenge_once(&self, url: &str) -> Result<(String, String)> {
        let base = url.parse::<rquest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(base_domain))
            .unwrap_or_else(|| url.to_string());

        let mutex = self.solving
            .entry(base.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        let _guard = mutex.lock().await;

        let creds = self.credentials.load();
        if let Some(c) = creds.get(&base)
            && !c.cookies.is_empty()
        {
            tracing::debug!("Challenge for {} already solved, reusing cookies", base);
            return Ok((c.cookies.clone(), c.user_agent.clone().unwrap_or_default()));
        }

        tracing::info!("Solving challenge for domain {}", base);
        let (cookies, ua) = self.solve_challenge(url).await?;
        self.store_credentials(url, &cookies, &ua);
        Ok((cookies, ua))
    }

    async fn get_rendered_page(&self, url: &str) -> Result<SmartResponse> {
        let guard = self.solver_url.load();
        let solver_url = guard.as_deref().ok_or_else(|| crate::error::Error::Other("No solver URL configured".into()))?;

        let client = rquest::Client::new();

        let body = json!({
          "cmd": "request.get",
          "url": url,
          "maxTimeout": 60000
        });

        let response = client
            .post(solver_url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(crate::error::Error::Other(format!(
                "FlareSolverr returned HTTP {}: check your solver URL is correct (should end in /v1)",
                response.status()
            )));
        }

        let response: serde_json::Value = response.json().await?;

        let status = response["status"].as_str().unwrap_or("");
        if status != "ok" {
            return Err(crate::error::Error::Other(format!(
                "FlareSolverr challenge failed with status '{}': {}",
                status,
                response["message"].as_str().unwrap_or("no message")
            )));
        }

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

    async fn get_rendered_page_once(&self, url: &str) -> Result<SmartResponse> {
        let base = url.parse::<rquest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(base_domain))
            .unwrap_or_else(|| url.to_string());

        let mutex = self.solving
            .entry(base.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        let _guard = mutex.lock().await;

        let creds = self.credentials.load();
        if let Some(c) = creds.get(&base)
            && !c.cookies.is_empty()
        {
            tracing::debug!(
                "Challenge for {} already solved, retrying with stored credentials",
                base
            );

            let mut builder = self.client.get(url);
            if let Ok(val) = rquest::header::HeaderValue::from_str(&c.cookies) {
                builder = builder.header(rquest::header::COOKIE, val);
            }

            if let Some(ref ua) = c.user_agent
            && let Ok(val) = rquest::header::HeaderValue::from_str(ua) {
                builder = builder.header(rquest::header::USER_AGENT, val);
            }

            let request = builder.build()
                .map_err(|e| crate::error::Error::Other(e.to_string()))?;
            let resp = self.client.execute(request).await?;
            return Ok(SmartResponse::Normal(resp));
        }

        tracing::info!("Solving HTML challenge for domain {}", base);
        let result = self.get_rendered_page(url).await?;

        if let Ok((cookies, ua)) = self.solve_challenge(url).await {
            self.store_credentials(url, &cookies, &ua);
        }

        Ok(result)
    }

    fn store_credentials(&self, url: &str, cookies: &str, user_agent: &str) {
        let domain = match url.parse::<rquest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(base_domain))
        {
            Some(d) => d,
            None => {
                tracing::warn!(
                    "store_credentials: could not extract domain from '{}', \
                    credentials will not be applied", url
                );
                return;
            }
        };

        let mut creds = (**self.credentials.load()).clone();
        creds.insert(domain, CachedCredentials {
            cookies: cookies.to_string(),
            user_agent: Some(user_agent.to_string()),
            stored_at: Some(std::time::Instant::now()),
        });
        self.credentials.store(Arc::new(creds));
    }

    pub fn update_solver_url(&self, url: Option<String>) {
        self.solver_url.store(Arc::new(url));
    }
}

pub async fn collect_bytes_limited<S>(
    stream: S,
    content_length_hint: Option<usize>,
    max_bytes: usize,
) -> Result<bytes::Bytes>
where
    S: TryStream<Ok = bytes::Bytes> + Unpin,
    S::Error: Into<crate::error::Error>,
{
    let mut stream = stream;

    if let Some(len) = content_length_hint
        && len > max_bytes
    {
        return Err(crate::error::Error::Other(format!(
            "Content-Length {} exceeds limit {}",
            len, max_bytes
        )));
    }

    let mut buf = bytes::BytesMut::new();
    let mut received = 0usize;

    while let Some(chunk) = stream.try_next().await.map_err(Into::into)? {
        received += chunk.len();
        if received > max_bytes {
            return Err(crate::error::Error::Other(format!(
                "body exceeded limit of {} bytes mid-stream",
                max_bytes
            )));
        }
        buf.extend_from_slice(&chunk);
    }

    Ok(buf.freeze())
}

fn base_domain(host: &str) -> String {
    use publicsuffix::{List, Psl};
    static LIST: std::sync::OnceLock<List> = std::sync::OnceLock::new();
    let list = LIST.get_or_init(List::default);
    
    list.domain(host.as_bytes())
        .and_then(|d| std::str::from_utf8(d.as_bytes()).ok().map(|s| s.to_string()))
        .unwrap_or_else(|| host.to_string())
}
