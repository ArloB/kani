use crate::{error::Result, network::ValidatingResolver};
use arc_swap::ArcSwap;
use futures::{TryStream, TryStreamExt};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use serde_json::json;
use std::{collections::HashMap, num::NonZeroU32, sync::Arc};

const MAX_RETRIES: u32 = 3;
const BASE_DELAY: std::time::Duration = std::time::Duration::from_secs(5);
const CREDENTIAL_TTL_SECS: u64 = 3600;
const RETRY_AFTER_CAP_SECS: u64 = 60;
const CIRCUIT_OPEN_THRESHOLD: u32 = 5;
const CIRCUIT_COOLDOWN_SECS: u64 = 30;

pub struct RateState {
    limiter: DefaultDirectRateLimiter,
    semaphore: Arc<tokio::sync::Semaphore>,
}

pub struct HostCircuit {
    consecutive_failures: std::sync::atomic::AtomicU32,
    open_until: std::sync::Mutex<Option<std::time::Instant>>,
}

impl HostCircuit {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            consecutive_failures: std::sync::atomic::AtomicU32::new(0),
            open_until: std::sync::Mutex::new(None),
        })
    }
}

/// 429, 502, 504 are retryable. 503 is excluded — it is the Cloudflare challenge signal.
fn is_retryable(status: rquest::StatusCode) -> bool {
    matches!(
        status,
        rquest::StatusCode::TOO_MANY_REQUESTS
            | rquest::StatusCode::BAD_GATEWAY
            | rquest::StatusCode::GATEWAY_TIMEOUT
    )
}

/// Parses Retry-After (integer seconds or HTTP-date), caps at RETRY_AFTER_CAP_SECS, falls back to exponential backoff.
fn compute_delay(headers: Option<&rquest::header::HeaderMap>, attempt: u32) -> std::time::Duration {
    let backoff = BASE_DELAY * 2u32.pow(attempt);
    headers
        .and_then(|h| h.get(rquest::header::RETRY_AFTER))
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.parse::<u64>()
                .ok()
                .map(|secs| secs.min(RETRY_AFTER_CAP_SECS))
                .or_else(|| {
                    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc2822)
                        .ok()
                        .map(|dt| {
                            let now = time::OffsetDateTime::now_utc();
                            (dt - now).whole_seconds().max(0) as u64
                        })
                        .map(|secs| secs.min(RETRY_AFTER_CAP_SECS))
                })
        })
        .map(std::time::Duration::from_secs)
        .unwrap_or(backoff)
}

fn jitter() -> std::time::Duration {
    use rand::RngExt;
    std::time::Duration::from_millis(rand::rng().random_range(0u64..1000))
}

#[derive(Clone, Default)]
pub struct CachedCredentials {
    cookies: String,
    user_agent: Option<String>,
    stored_at: Option<std::time::Instant>,
    challenge_url: Option<String>,
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
    pub host_circuits: Arc<dashmap::DashMap<String, Arc<HostCircuit>>>,
    pub rate_states: Arc<dashmap::DashMap<String, Arc<RateState>>>,
}

impl SmartClient {
    pub fn new(solver_url: Option<String>) -> Result<Self> {
        let resolver = ValidatingResolver::new()?;
        let client = rquest::Client::builder()
            .emulation(rquest_util::Emulation::Chrome130)
            .redirect(rquest::redirect::Policy::limited(10))
            .dns_resolver(Arc::new(resolver))
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(100)
            .timeout(std::time::Duration::from_secs(35))
            .build()?;

        Ok(Self {
            client,
            credentials: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            solver_url: Arc::new(ArcSwap::from_pointee(solver_url)),
            solving: Arc::new(dashmap::DashMap::new()),
            host_circuits: Arc::new(dashmap::DashMap::new()),
            rate_states: Arc::new(dashmap::DashMap::new()),
        })
    }

    pub fn new_proxy(
        solver_url: Option<String>,
        credentials: Arc<ArcSwap<HashMap<String, CachedCredentials>>>,
        solving: Arc<dashmap::DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
        host_circuits: Arc<dashmap::DashMap<String, Arc<HostCircuit>>>,
    ) -> Result<Self> {
        let resolver = ValidatingResolver::new()?;
        let client = rquest::Client::builder()
            .emulation(rquest_util::Emulation::Chrome130)
            .redirect(rquest::redirect::Policy::none())
            .dns_resolver(Arc::new(resolver))
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(100)
            .timeout(std::time::Duration::from_secs(35))
            .build()?;

        Ok(Self {
            client,
            credentials,
            solver_url: Arc::new(ArcSwap::from_pointee(solver_url)),
            solving,
            host_circuits,
            rate_states: Arc::new(dashmap::DashMap::new()),
        })
    }

    /// Registers a per-domain rate limit config. Called at source load time.
    pub fn register_rate_limit(&self, domain: &str, cfg: &kani_shared::extension::RateLimitConfig) {
        let period_ns = (1_000_000_000.0 / cfg.requests_per_second.max(0.001)) as u64;
        let burst = NonZeroU32::new(cfg.burst.max(1)).unwrap_or(NonZeroU32::MIN);
        let quota = Quota::with_period(std::time::Duration::from_nanos(period_ns))
            .expect("rate limit period must be > 0")
            .allow_burst(burst);
        let limiter = RateLimiter::direct(quota);
        let max_concurrent = cfg.max_concurrent.max(1) as usize;
        let state = Arc::new(RateState {
            limiter,
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
        });
        self.rate_states.insert(domain.to_string(), state);
    }

    /// Removes the rate limit state for a domain. Called when a source is removed or reloaded.
    pub fn deregister_rate_limit(&self, domain: &str) {
        self.rate_states.remove(domain);
    }

    fn circuit_for(&self, domain: &str) -> Arc<HostCircuit> {
        self.host_circuits
            .entry(domain.to_string())
            .or_insert_with(HostCircuit::new)
            .clone()
    }

    fn is_circuit_open(&self, domain: &str) -> bool {
        let Some(circuit) = self.host_circuits.get(domain) else {
            return false;
        };
        let guard = circuit.open_until.lock().expect("circuit mutex poisoned");
        guard
            .map(|until| std::time::Instant::now() < until)
            .unwrap_or(false)
    }

    fn record_success(&self, domain: &str) {
        let Some(circuit) = self.host_circuits.get(domain) else {
            return;
        };
        circuit
            .consecutive_failures
            .store(0, std::sync::atomic::Ordering::Relaxed);
        *circuit.open_until.lock().expect("circuit mutex poisoned") = None;
    }

    fn record_failure(&self, domain: &str) {
        let circuit = self.circuit_for(domain);
        let prev = circuit
            .consecutive_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if prev + 1 >= CIRCUIT_OPEN_THRESHOLD {
            let until =
                std::time::Instant::now() + std::time::Duration::from_secs(CIRCUIT_COOLDOWN_SECS);
            *circuit.open_until.lock().expect("circuit mutex poisoned") = Some(until);
            tracing::warn!(
                "Circuit opened for {} after {} consecutive failures (cooldown {}s)",
                domain,
                prev + 1,
                CIRCUIT_COOLDOWN_SECS,
            );
        }
    }

    pub async fn send_request(&self, request: rquest::Request) -> Result<SmartResponse> {
        let mut request = request;

        let domain = request
            .url()
            .host_str()
            .map(base_domain)
            .unwrap_or_default();
        let creds_map = self.credentials.load();
        if let Some(creds) = creds_map.get(&domain) {
            let expired = creds
                .stored_at
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
                    tracing::warn!(
                        "Stored cookies for {} contained invalid header characters, skipping",
                        domain
                    );
                }

                if let Some(ref ua) = creds.user_agent
                    && let Ok(val) = rquest::header::HeaderValue::from_str(ua)
                {
                    request
                        .headers_mut()
                        .insert(rquest::header::USER_AGENT, val);
                }
            }
        }

        if self.is_circuit_open(&domain) {
            return Err(crate::error::Error::Other(format!(
                "Circuit open for {domain}: host temporarily unavailable"
            )));
        }

        let rate_state = self.rate_states.get(&domain).map(|r| Arc::clone(&*r));
        if let Some(ref state) = rate_state {
            state.limiter.until_ready().await;
        }
        let _semaphore_permit = if let Some(ref state) = rate_state {
            state.semaphore.clone().acquire_owned().await.ok()
        } else {
            None
        };

        let mut current_request = request;
        let mut attempt = 0;

        loop {
            let request_clone_for_retry = current_request.try_clone();

            let resp = match self.client.execute(current_request).await {
                Ok(r) => r,
                Err(e) if attempt < MAX_RETRIES && request_clone_for_retry.is_some() => {
                    let delay = compute_delay(None, attempt) + jitter();
                    tracing::warn!(
                        "HTTP request failed ({}), retrying in {:?} (attempt {}/{})",
                        e,
                        delay,
                        attempt + 1,
                        MAX_RETRIES,
                    );
                    self.record_failure(&domain);
                    tokio::time::sleep(delay).await;
                    current_request = request_clone_for_retry.unwrap();
                    attempt += 1;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            let status = resp.status();

            if is_retryable(status) {
                if attempt < MAX_RETRIES
                    && let Some(next_req) = request_clone_for_retry
                {
                    let delay = compute_delay(Some(resp.headers()), attempt) + jitter();
                    tracing::warn!(
                        "Upstream returned {}, retrying in {:?} (attempt {}/{})",
                        status.as_u16(),
                        delay,
                        attempt + 1,
                        MAX_RETRIES,
                    );
                    tokio::time::sleep(delay).await;
                    current_request = next_req;
                    attempt += 1;
                    continue;
                }
                self.record_failure(&domain);
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
                        self.record_success(&domain);
                        return Ok(resp);
                    } else {
                        self.record_success(&domain);
                        return Ok(SmartResponse::Buffered {
                            status,
                            url,
                            headers,
                            body: bytes,
                        });
                    }
                } else {
                    self.record_success(&domain);
                    return Ok(SmartResponse::Normal(resp));
                }
            }

            if (status == rquest::StatusCode::FORBIDDEN
                || status == rquest::StatusCode::SERVICE_UNAVAILABLE)
                && self.solver_url.load().is_some()
                && request_clone_for_retry.is_some()
            {
                let url = resp.url().as_str().to_string();
                let cf_domain = url
                    .parse::<rquest::Url>()
                    .ok()
                    .and_then(|u| u.host_str().map(base_domain));

                if let Some(ref d) = cf_domain {
                    let mut creds = (**self.credentials.load()).clone();
                    if creds.remove(d).is_some() {
                        tracing::info!(
                            "Stored credentials for {} returned 403, clearing and re-solving",
                            d
                        );
                        self.credentials.store(Arc::new(creds));
                    }
                }

                let req_headers = request_clone_for_retry.as_ref().map(|r| r.headers());
                let (new_cookies, new_ua) = self.solve_challenge_once(&url, req_headers).await?;

                if let Some(mut request) = request_clone_for_retry {
                    if let Ok(val) = rquest::header::HeaderValue::from_str(&new_cookies) {
                        request.headers_mut().insert(rquest::header::COOKIE, val);
                    }

                    if let Ok(val) = rquest::header::HeaderValue::from_str(&new_ua) {
                        request
                            .headers_mut()
                            .insert(rquest::header::USER_AGENT, val);
                    }

                    let resp = self.client.execute(request).await?;
                    self.record_success(&domain);
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

        let circuit_domain = initial_url
            .parse::<rquest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(base_domain))
            .unwrap_or_default();

        if let Ok(parsed) = initial_url.parse::<rquest::Url>()
            && let Some(domain) = parsed.host_str().map(base_domain)
        {
            let creds_map = self.credentials.load();
            if let Some(creds) = creds_map.get(&domain) {
                let expired = creds
                    .stored_at
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
                        && let Ok(val) = rquest::header::HeaderValue::from_str(ua)
                    {
                        solver_headers.insert(rquest::header::USER_AGENT, val);
                    }
                }
            }
        }

        if self.is_circuit_open(&circuit_domain) {
            return Err(crate::error::Error::Other(format!(
                "Circuit open for {circuit_domain}: host temporarily unavailable"
            )));
        }

        let mut redirect_count = 0usize;
        let mut retry_count = 0u32;

        loop {
            let mut req_builder = self.client.get(&current_url);
            if current_url == initial_url
                && let Some(ref h) = headers
            {
                req_builder = req_builder.headers(h.clone());
            }
            if !solver_headers.is_empty() {
                req_builder = req_builder.headers(solver_headers.clone());
            }
            let req = req_builder.build()?;

            let current_headers = req.headers().clone();

            let resp = match self.client.execute(req).await {
                Ok(r) => r,
                Err(e) if retry_count < MAX_RETRIES => {
                    let delay = compute_delay(None, retry_count) + jitter();
                    tracing::warn!(
                        "safe_get network error ({}), retrying in {:?} (attempt {}/{})",
                        e,
                        delay,
                        retry_count + 1,
                        MAX_RETRIES,
                    );
                    self.record_failure(&circuit_domain);
                    retry_count += 1;
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            if is_retryable(resp.status()) && retry_count < MAX_RETRIES {
                let delay = compute_delay(Some(resp.headers()), retry_count) + jitter();
                tracing::warn!(
                    "safe_get got {}, retrying in {:?} (attempt {}/{})",
                    resp.status().as_u16(),
                    delay,
                    retry_count + 1,
                    MAX_RETRIES,
                );
                self.record_failure(&circuit_domain);
                retry_count += 1;
                tokio::time::sleep(delay).await;
                continue;
            }

            if resp.status().is_redirection() {
                let location = resp
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        crate::error::Error::Other("redirect with no Location header".into())
                    })?;

                let next = resp.url().join(location).map_err(|e| {
                    crate::error::Error::Other(format!(
                        "invalid redirect URL '{}': {}",
                        location, e
                    ))
                })?;

                match next.scheme() {
                    "http" | "https" => {}
                    s => {
                        return Err(crate::error::Error::Other(format!(
                            "redirect to forbidden scheme: {}",
                            s
                        )));
                    }
                }

                redirect_count += 1;
                if redirect_count >= MAX_REDIRECTS {
                    return Err(crate::error::Error::Other(format!(
                        "too many redirects following '{}'",
                        initial_url
                    )));
                }
                current_url = next.to_string();
            } else if (resp.status() == rquest::StatusCode::FORBIDDEN
                || resp.status() == rquest::StatusCode::SERVICE_UNAVAILABLE)
                && !solved
                && self.solver_url.load().is_some()
            {
                let url = resp.url().as_str().to_string();
                let domain = url
                    .parse::<rquest::Url>()
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

                let (cookies, ua) = self
                    .solve_challenge_once(&url, Some(&current_headers))
                    .await?;

                if let Ok(val) = rquest::header::HeaderValue::from_str(&cookies) {
                    solver_headers.insert(rquest::header::COOKIE, val);
                }
                if let Ok(val) = rquest::header::HeaderValue::from_str(&ua) {
                    solver_headers.insert(rquest::header::USER_AGENT, val);
                }

                solved = true;
            } else {
                self.record_success(&circuit_domain);
                return Ok(SmartResponse::Normal(resp));
            }
        }
    }

    pub fn inner(&self) -> &rquest::Client {
        &self.client
    }

    async fn solve_challenge(
        &self,
        url: &str,
        headers: Option<&rquest::header::HeaderMap>,
    ) -> Result<(String, String)> {
        let guard = self.solver_url.load();
        let solver_url = guard
            .as_deref()
            .ok_or_else(|| crate::error::Error::Other("No solver URL configured".into()))?;

        let client = rquest::Client::new();

        let mut body = json!({
          "cmd": "request.get",
          "url": url,
          "maxTimeout": 60000
        });

        if let Some(h) = headers {
            let mut header_map = serde_json::Map::new();
            for (k, v) in h.iter() {
                if let Ok(v_str) = v.to_str() {
                    header_map.insert(k.as_str().to_string(), json!(v_str));
                }
            }
            if !header_map.is_empty() {
                body.as_object_mut()
                    .unwrap()
                    .insert("headers".to_string(), json!(header_map));
            }
        }

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

    async fn solve_challenge_once(
        &self,
        url: &str,
        headers: Option<&rquest::header::HeaderMap>,
    ) -> Result<(String, String)> {
        let base = url
            .parse::<rquest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(base_domain))
            .unwrap_or_else(|| url.to_string());

        let mutex = self
            .solving
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
        let (cookies, ua) = self.solve_challenge(url, headers).await?;
        self.store_credentials(url, &cookies, &ua);
        Ok((cookies, ua))
    }

    async fn get_rendered_page(&self, url: &str) -> Result<SmartResponse> {
        let guard = self.solver_url.load();
        let solver_url = guard
            .as_deref()
            .ok_or_else(|| crate::error::Error::Other("No solver URL configured".into()))?;

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
        let base = url
            .parse::<rquest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(base_domain))
            .unwrap_or_else(|| url.to_string());

        let mutex = self
            .solving
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
                && let Ok(val) = rquest::header::HeaderValue::from_str(ua)
            {
                builder = builder.header(rquest::header::USER_AGENT, val);
            }

            let request = builder
                .build()
                .map_err(|e| crate::error::Error::Other(e.to_string()))?;
            let resp = self.client.execute(request).await?;
            return Ok(SmartResponse::Normal(resp));
        }

        tracing::info!("Solving HTML challenge for domain {}", base);
        let result = self.get_rendered_page(url).await?;

        if let Ok((cookies, ua)) = self.solve_challenge(url, None).await {
            self.store_credentials(url, &cookies, &ua);
        }

        Ok(result)
    }

    fn store_credentials(&self, url: &str, cookies: &str, user_agent: &str) {
        let domain = match url
            .parse::<rquest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(base_domain))
        {
            Some(d) => d,
            None => {
                tracing::warn!(
                    "store_credentials: could not extract domain from '{}', \
                    credentials will not be applied",
                    url
                );
                return;
            }
        };

        let mut creds = (**self.credentials.load()).clone();
        creds.insert(
            domain,
            CachedCredentials {
                cookies: cookies.to_string(),
                user_agent: Some(user_agent.to_string()),
                stored_at: Some(std::time::Instant::now()),
                challenge_url: Some(url.to_string()),
            },
        );
        self.credentials.store(Arc::new(creds));
    }

    pub async fn refresh_expiring_credentials(&self) {
        if self.solver_url.load().is_none() {
            return;
        }

        const REFRESH_THRESHOLD_SECS: u64 = 300;

        let domains_to_refresh: Vec<(String, String)> = {
            let creds = self.credentials.load();
            creds
                .iter()
                .filter_map(|(domain, cred)| {
                    let age = cred.stored_at?.elapsed().as_secs();
                    let expiring_soon = age + REFRESH_THRESHOLD_SECS >= CREDENTIAL_TTL_SECS;
                    let url = cred.challenge_url.clone()?;
                    if expiring_soon {
                        Some((domain.clone(), url))
                    } else {
                        None
                    }
                })
                .collect()
        };

        for (domain, url) in domains_to_refresh {
            tracing::info!("Proactively refreshing credentials for {}", domain);
            match self.solve_challenge_once(&url, None).await {
                Ok((cookies, ua)) => self.store_credentials(&url, &cookies, &ua),
                Err(e) => {
                    tracing::warn!("Proactive credential refresh failed for {}: {}", domain, e)
                }
            }
        }
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
        .and_then(|d| {
            std::str::from_utf8(d.as_bytes())
                .ok()
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| host.to_string())
}

/// Test-only constructor: uses Chrome emulation (for HTTP/1.1 compatibility with wiremock) but
/// omits ValidatingResolver (which blocks 127.0.0.1) and the per-request timeout (which
/// interacts badly with `#[tokio::test(start_paused = true)]`).
#[cfg(test)]
impl SmartClient {
    pub fn new_for_test() -> Result<Self> {
        let client = rquest::Client::builder()
            .emulation(rquest_util::Emulation::Chrome130)
            .redirect(rquest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            credentials: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            solver_url: Arc::new(ArcSwap::from_pointee(None)),
            solving: Arc::new(dashmap::DashMap::new()),
            host_circuits: Arc::new(dashmap::DashMap::new()),
            rate_states: Arc::new(dashmap::DashMap::new()),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    // ── is_retryable ─────────────────────────────────────────────────────────

    #[test]
    fn retryable_on_429() {
        assert!(is_retryable(rquest::StatusCode::TOO_MANY_REQUESTS));
    }

    #[test]
    fn retryable_on_502() {
        assert!(is_retryable(rquest::StatusCode::BAD_GATEWAY));
    }

    #[test]
    fn retryable_on_504() {
        assert!(is_retryable(rquest::StatusCode::GATEWAY_TIMEOUT));
    }

    #[test]
    fn not_retryable_on_200() {
        assert!(!is_retryable(rquest::StatusCode::OK));
    }

    #[test]
    fn not_retryable_on_400() {
        assert!(!is_retryable(rquest::StatusCode::BAD_REQUEST));
    }

    #[test]
    fn not_retryable_on_403() {
        assert!(!is_retryable(rquest::StatusCode::FORBIDDEN));
    }

    #[test]
    fn not_retryable_on_503() {
        // 503 is deliberately excluded (Cloudflare challenge signal)
        assert!(!is_retryable(rquest::StatusCode::SERVICE_UNAVAILABLE));
    }

    // ── compute_delay ────────────────────────────────────────────────────────

    #[test]
    fn no_header_gives_exponential_backoff() {
        // attempt 0 → BASE_DELAY * 2^0 = 5s; attempt 1 → 10s; attempt 2 → 20s
        let d0 = compute_delay(None, 0);
        let d1 = compute_delay(None, 1);
        let d2 = compute_delay(None, 2);
        assert_eq!(d0, BASE_DELAY);
        assert_eq!(d1, BASE_DELAY * 2);
        assert_eq!(d2, BASE_DELAY * 4);
    }

    #[test]
    fn retry_after_integer_seconds_used() {
        let mut headers = rquest::header::HeaderMap::new();
        headers.insert(
            rquest::header::RETRY_AFTER,
            rquest::header::HeaderValue::from_static("10"),
        );
        let d = compute_delay(Some(&headers), 0);
        assert_eq!(d, std::time::Duration::from_secs(10));
    }

    #[test]
    fn retry_after_capped_at_max() {
        let mut headers = rquest::header::HeaderMap::new();
        // 9999s > RETRY_AFTER_CAP_SECS (60)
        headers.insert(
            rquest::header::RETRY_AFTER,
            rquest::header::HeaderValue::from_static("9999"),
        );
        let d = compute_delay(Some(&headers), 0);
        assert_eq!(d, std::time::Duration::from_secs(RETRY_AFTER_CAP_SECS));
    }

    #[test]
    fn empty_headers_falls_back_to_backoff() {
        let headers = rquest::header::HeaderMap::new();
        let d = compute_delay(Some(&headers), 0);
        assert_eq!(d, BASE_DELAY);
    }

    // ── base_domain ──────────────────────────────────────────────────────────

    #[test]
    fn base_domain_strips_subdomain() {
        let d = base_domain("sub.example.com");
        assert_eq!(d, "example.com");
    }

    #[test]
    fn base_domain_preserves_apex() {
        let d = base_domain("example.com");
        assert_eq!(d, "example.com");
    }

    #[test]
    fn base_domain_is_deterministic_for_ips() {
        // publicsuffix parses IP octets as domain labels; the exact output is
        // library-defined — just verify the call doesn't panic and is stable.
        let first = base_domain("8.8.8.8");
        let second = base_domain("8.8.8.8");
        assert_eq!(first, second);
    }

    // ── SmartResponse (Buffered variant) ─────────────────────────────────────

    #[tokio::test]
    async fn smart_response_buffered_accessors() {
        let mut headers = rquest::header::HeaderMap::new();
        headers.insert(
            rquest::header::CONTENT_TYPE,
            rquest::header::HeaderValue::from_static("application/json"),
        );
        let resp = SmartResponse::Buffered {
            status: rquest::StatusCode::OK,
            url: rquest::Url::parse("https://example.com/test").unwrap(),
            headers,
            body: bytes::Bytes::from("hello world"),
        };
        assert_eq!(resp.status(), rquest::StatusCode::OK);
        assert_eq!(resp.url().as_str(), "https://example.com/test");
        assert!(resp.headers().contains_key(rquest::header::CONTENT_TYPE));
    }

    #[tokio::test]
    async fn smart_response_buffered_text() {
        let resp = SmartResponse::Buffered {
            status: rquest::StatusCode::OK,
            url: rquest::Url::parse("https://example.com/").unwrap(),
            headers: rquest::header::HeaderMap::new(),
            body: bytes::Bytes::from("test body"),
        };
        let text = resp.text().await.unwrap();
        assert_eq!(text, "test body");
    }

    #[tokio::test]
    async fn smart_response_buffered_bytes() {
        let resp = SmartResponse::Buffered {
            status: rquest::StatusCode::NOT_FOUND,
            url: rquest::Url::parse("https://example.com/").unwrap(),
            headers: rquest::header::HeaderMap::new(),
            body: bytes::Bytes::from_static(b"\x01\x02\x03"),
        };
        let b = resp.bytes().await.unwrap();
        assert_eq!(&b[..], b"\x01\x02\x03");
    }

    // ── Circuit breaker ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn circuit_starts_closed() {
        let client = SmartClient::new(None).unwrap();
        assert!(!client.is_circuit_open("example.com"));
    }

    #[tokio::test]
    async fn circuit_opens_after_threshold_failures() {
        let client = SmartClient::new(None).unwrap();
        for _ in 0..CIRCUIT_OPEN_THRESHOLD {
            client.record_failure("example.com");
        }
        assert!(client.is_circuit_open("example.com"));
    }

    #[tokio::test]
    async fn record_success_resets_failure_counter() {
        let client = SmartClient::new(None).unwrap();
        // record failures but not enough to trip the breaker, then clear
        for _ in 0..CIRCUIT_OPEN_THRESHOLD - 1 {
            client.record_failure("example.com");
        }
        client.record_success("example.com");
        // after reset, one more failure should not open the circuit
        client.record_failure("example.com");
        assert!(!client.is_circuit_open("example.com"));
    }

    #[tokio::test]
    async fn record_success_on_unknown_domain_is_noop() {
        let client = SmartClient::new(None).unwrap();
        // should not panic or insert an entry
        client.record_success("never-seen.com");
        assert!(!client.is_circuit_open("never-seen.com"));
    }

    #[tokio::test]
    async fn collect_bytes_limited_rejects_large_content_length() {
        use futures::stream;
        let small_chunk = bytes::Bytes::from("hello");
        // The content-length hint says 1000 but limit is 10 → should reject immediately
        let s = stream::iter(vec![Ok::<_, crate::error::Error>(small_chunk)]);
        let err = collect_bytes_limited(s, Some(1000), 10).await.unwrap_err();
        assert!(err.to_string().contains("exceeds limit"));
    }

    #[tokio::test]
    async fn collect_bytes_limited_rejects_oversized_body() {
        use futures::stream;
        let big = bytes::Bytes::from(vec![0u8; 20]);
        let s = stream::iter(vec![Ok::<_, crate::error::Error>(big)]);
        let err = collect_bytes_limited(s, None, 10).await.unwrap_err();
        assert!(err.to_string().contains("exceeded limit"));
    }

    #[tokio::test]
    async fn collect_bytes_limited_succeeds_within_limit() {
        use futures::stream;
        let data = bytes::Bytes::from("hello");
        let s = stream::iter(vec![Ok::<_, crate::error::Error>(data)]);
        let result = collect_bytes_limited(s, None, 100).await.unwrap();
        assert_eq!(&result[..], b"hello");
    }

    // ── SmartClient integration (wiremock) ───────────────────────────────────

    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn get_request_reaches_server() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(ResponseTemplate::new(200).set_body_string("world"))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/hello", server.uri()))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status(), rquest::StatusCode::OK);
        let body = resp.text().await.unwrap();
        assert_eq!(body, "world");
    }

    #[tokio::test]
    async fn post_request_json_body_reaches_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api"))
            .and(body_partial_json(serde_json::json!({"k": "v"})))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .post(format!("{}/api", server.uri()))
            .json(&serde_json::json!({"k": "v"}))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status().as_u16(), 201);
    }

    #[tokio::test]
    async fn post_request_form_body_reaches_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/form"))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .post(format!("{}/form", server.uri()))
            .form(&[("field", "val")])
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status(), rquest::StatusCode::OK);
    }

    #[tokio::test]
    async fn custom_header_forwarded_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/h"))
            .and(header("x-custom", "test-val"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/h", server.uri()))
            .header("x-custom", "test-val")
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status(), rquest::StatusCode::OK);
    }

    #[tokio::test]
    async fn query_params_forwarded_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "manga"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/search?q=manga", server.uri()))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status(), rquest::StatusCode::OK);
    }

    #[tokio::test]
    async fn html_response_is_buffered() {
        let server = MockServer::start().await;
        // Use set_body_raw so we control the mime type directly.
        // set_body_string always sets mime="text/plain" which generate_response
        // later inserts as Content-Type, overriding any insert_header call.
        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(b"<html><body>hello</body></html>".to_vec(), "text/html"),
            )
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/page", server.uri()))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert!(
            matches!(resp, SmartResponse::Buffered { .. }),
            "expected Buffered for text/html"
        );
        let body = resp.text().await.unwrap();
        assert!(body.contains("hello"));
    }

    #[tokio::test]
    async fn json_response_is_normal() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/data"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(r#"{"ok":true}"#),
            )
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/data", server.uri()))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert!(matches!(resp, SmartResponse::Normal(_)));
    }

    #[tokio::test]
    async fn not_found_404_returns_normal_not_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/missing", server.uri()))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status(), rquest::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn server_error_500_returned_as_normal() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/err"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/err", server.uri()))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status(), rquest::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_on_429_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/retry"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/retry"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/retry", server.uri()))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status(), rquest::StatusCode::OK);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_exhausted_returns_last_429() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/always429"))
            .respond_with(ResponseTemplate::new(429))
            .expect((1 + MAX_RETRIES) as u64)
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let req = client
            .inner()
            .get(format!("{}/always429", server.uri()))
            .build()
            .unwrap();
        let resp = client.send_request(req).await.unwrap();
        assert_eq!(resp.status().as_u16(), 429);
    }

    #[tokio::test]
    async fn safe_get_follows_redirect() {
        let server = MockServer::start().await;
        let dest = format!("{}/dest", server.uri());
        Mock::given(method("GET"))
            .and(path("/redir"))
            .respond_with(ResponseTemplate::new(301).insert_header("location", dest.as_str()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/dest"))
            .respond_with(ResponseTemplate::new(200).set_body_string("final"))
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let resp = client
            .safe_get(&format!("{}/redir", server.uri()), None)
            .await
            .unwrap();
        assert_eq!(resp.status(), rquest::StatusCode::OK);
    }

    #[tokio::test]
    async fn safe_get_too_many_redirects_returns_error() {
        let server = MockServer::start().await;
        // 5 sequential redirects triggers the MAX_REDIRECTS guard
        for i in 1..=5 {
            let next = format!("{}/r{}", server.uri(), i + 1);
            Mock::given(method("GET"))
                .and(path(format!("/r{}", i)))
                .respond_with(ResponseTemplate::new(301).insert_header("location", next.as_str()))
                .mount(&server)
                .await;
        }

        let client = SmartClient::new_for_test().unwrap();
        let Err(err) = client.safe_get(&format!("{}/r1", server.uri()), None).await else {
            panic!("expected error for too many redirects");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("too many redirects") || msg.contains("redirect"),
            "{msg}"
        );
    }

    #[tokio::test]
    async fn safe_get_redirect_to_non_http_scheme_returns_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/bad"))
            .respond_with(
                ResponseTemplate::new(301).insert_header("location", "ftp://example.com/file"),
            )
            .mount(&server)
            .await;

        let client = SmartClient::new_for_test().unwrap();
        let Err(err) = client
            .safe_get(&format!("{}/bad", server.uri()), None)
            .await
        else {
            panic!("expected error for bad redirect scheme");
        };
        assert!(err.to_string().contains("forbidden scheme"), "{err}");
    }

    // ── Rate limiting ────────────────────────────────────────────────────────

    #[test]
    fn register_and_deregister_rate_limit() {
        let client = SmartClient::new(None).unwrap();
        let cfg = kani_shared::extension::RateLimitConfig {
            requests_per_second: 10.0,
            burst: 5,
            max_concurrent: 3,
        };
        client.register_rate_limit("example.com", &cfg);
        assert!(client.rate_states.contains_key("example.com"));
        client.deregister_rate_limit("example.com");
        assert!(!client.rate_states.contains_key("example.com"));
    }

    #[tokio::test]
    async fn rate_limiter_shapes_burst() {
        let client = SmartClient::new(None).unwrap();
        let cfg = kani_shared::extension::RateLimitConfig {
            requests_per_second: 100.0,
            burst: 3,
            max_concurrent: 10,
        };
        client.register_rate_limit("test.local", &cfg);
        let state = client
            .rate_states
            .get("test.local")
            .map(|r| Arc::clone(&*r))
            .unwrap();

        let mut passed = 0u32;
        let mut blocked = 0u32;
        for _ in 0..6 {
            match state.limiter.check() {
                Ok(_) => passed += 1,
                Err(_) => blocked += 1,
            }
        }
        assert_eq!(passed, 3, "only burst={} requests pass immediately", 3);
        assert_eq!(blocked, 3, "remaining requests are rate-limited");
    }

    #[tokio::test]
    async fn semaphore_caps_concurrency() {
        let client = SmartClient::new(None).unwrap();
        let cfg = kani_shared::extension::RateLimitConfig {
            requests_per_second: 100.0,
            burst: 100,
            max_concurrent: 2,
        };
        client.register_rate_limit("sem.local", &cfg);
        let state = client
            .rate_states
            .get("sem.local")
            .map(|r| Arc::clone(&*r))
            .unwrap();

        let p1 = state.semaphore.clone().try_acquire_owned().ok();
        let p2 = state.semaphore.clone().try_acquire_owned().ok();
        let p3 = state.semaphore.clone().try_acquire_owned();

        assert!(p1.is_some(), "first permit acquired");
        assert!(p2.is_some(), "second permit acquired");
        assert!(p3.is_err(), "third permit denied (max_concurrent=2)");
        drop(p1);
        let p4 = state.semaphore.clone().try_acquire_owned();
        assert!(p4.is_ok(), "permit available after release");
    }
}
