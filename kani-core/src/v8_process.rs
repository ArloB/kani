//! Lifecycle and newline-delimited protocol for the shared Node.js browser worker.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

static V8_DEBUG_LOGGING: AtomicBool = AtomicBool::new(false);

pub fn set_v8_debug_logging(enabled: bool) {
    V8_DEBUG_LOGGING.store(enabled, Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug)]
pub struct V8Config {
    pub max_memory_mb: u32,
    pub max_instances: u32,
    pub idle_timeout_s: u32,
}

impl Default for V8Config {
    fn default() -> Self {
        Self {
            max_memory_mb: 512,
            max_instances: 2,
            idle_timeout_s: 300,
        }
    }
}

static V8_CONFIG: RwLock<V8Config> = RwLock::new(V8Config {
    max_memory_mb: 512,
    max_instances: 2,
    idle_timeout_s: 300,
});

pub fn set_v8_config(cfg: V8Config) {
    if let Ok(mut guard) = V8_CONFIG.write() {
        *guard = cfg;
    }
}

pub fn v8_config() -> V8Config {
    V8_CONFIG.read().map(|g| *g).unwrap_or_default()
}

static V8_CALLS_TOTAL: AtomicU64 = AtomicU64::new(0);
static V8_RESTARTS_TOTAL: AtomicU64 = AtomicU64::new(0);
static V8_GRACEFUL_SHUTDOWNS_TOTAL: AtomicU64 = AtomicU64::new(0);
static V8_FORCED_TERMINATIONS_TOTAL: AtomicU64 = AtomicU64::new(0);
static BROWSER_REUSES_TOTAL: AtomicU64 = AtomicU64::new(0);
static BROWSER_RECOVERY_LAUNCHES_TOTAL: AtomicU64 = AtomicU64::new(0);
static BROWSER_CHALLENGES_TOTAL: AtomicU64 = AtomicU64::new(0);
static BROWSER_PAGE_CLOSE_TIMEOUTS_TOTAL: AtomicU64 = AtomicU64::new(0);
static BROWSER_SOLVER_ATTEMPTS_TOTAL: AtomicU64 = AtomicU64::new(0);
static BROWSER_SOLVER_SUCCESSES_TOTAL: AtomicU64 = AtomicU64::new(0);
static BROWSER_SOLVER_FAILURES_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Snapshot of browser-runtime counters plus the active resource caps, for the
/// diagnostics surface. Lives in kani-shared so kani-web can serialize it.
pub fn browser_stats() -> kani_shared::types::BrowserStats {
    let cfg = v8_config();
    kani_shared::types::BrowserStats {
        calls_total: V8_CALLS_TOTAL.load(Ordering::Relaxed),
        restarts: V8_RESTARTS_TOTAL.load(Ordering::Relaxed),
        graceful_shutdowns: V8_GRACEFUL_SHUTDOWNS_TOTAL.load(Ordering::Relaxed),
        forced_terminations: V8_FORCED_TERMINATIONS_TOTAL.load(Ordering::Relaxed),
        browser_reuses: BROWSER_REUSES_TOTAL.load(Ordering::Relaxed),
        recovery_launches: BROWSER_RECOVERY_LAUNCHES_TOTAL.load(Ordering::Relaxed),
        challenges: BROWSER_CHALLENGES_TOTAL.load(Ordering::Relaxed),
        page_close_timeouts: BROWSER_PAGE_CLOSE_TIMEOUTS_TOTAL.load(Ordering::Relaxed),
        solver_attempts: BROWSER_SOLVER_ATTEMPTS_TOTAL.load(Ordering::Relaxed),
        solver_successes: BROWSER_SOLVER_SUCCESSES_TOTAL.load(Ordering::Relaxed),
        solver_failures: BROWSER_SOLVER_FAILURES_TOTAL.load(Ordering::Relaxed),
        max_memory_mb: cfg.max_memory_mb,
        max_instances: cfg.max_instances,
        idle_timeout_s: cfg.idle_timeout_s,
    }
}

pub fn record_browser_solver_result(success: bool) {
    BROWSER_SOLVER_ATTEMPTS_TOTAL.fetch_add(1, Ordering::Relaxed);
    if success {
        BROWSER_SOLVER_SUCCESSES_TOTAL.fetch_add(1, Ordering::Relaxed);
    } else {
        BROWSER_SOLVER_FAILURES_TOTAL.fetch_add(1, Ordering::Relaxed);
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static V8_SHIM_PATH: OnceLock<Result<std::path::PathBuf, String>> = OnceLock::new();

/// Timeout for V8 IPC requests that don't carry their own caller-supplied
/// timeout (context create/eval/exists/drop).
const V8_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Buffer added on top of a caller-supplied `timeout_ms` (capture_url_param /
/// capture_page_payload) so the host-side timeout only fires if the JS-side
/// timeout enforcement itself fails to fire.
const V8_TIMEOUT_BUFFER: Duration = Duration::from_secs(5);
const V8_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

pub type V8ProcessHandle = Arc<Mutex<V8Slot>>;

#[derive(Debug)]
pub enum V8Slot {
    Empty,
    Running(Box<V8Process>, Instant),
    Retired,
}

/// Constructs a fresh, empty V8 process handle; the process itself is
/// spawned lazily on first use.
pub fn new_handle() -> V8ProcessHandle {
    Arc::new(Mutex::new(V8Slot::Empty))
}

#[derive(Debug)]
pub struct V8Process {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Debug)]
enum V8RequestError {
    Action(V8ActionError),
    Transport(String),
}

#[derive(Debug, Clone)]
struct V8ActionError {
    code: String,
    message: String,
    url: Option<String>,
    status: Option<u16>,
}

impl V8RequestError {
    fn into_message(self) -> String {
        match self {
            Self::Action(error) => error.message,
            Self::Transport(message) => message,
        }
    }
}

impl V8Process {
    async fn spawn() -> Result<Self, String> {
        let shim_path = V8_SHIM_PATH.get_or_init(|| {
            let path = std::env::temp_dir().join(format!("kani_v8_shim_{}.js", std::process::id()));
            std::fs::write(&path, include_str!("v8_shim.js"))
                .map(|()| path)
                .map_err(|e| format!("Failed to write v8 shim: {e}"))
        });
        let shim_path = shim_path.as_ref().map_err(|error| error.clone())?;

        let cfg = v8_config();
        let mut command = Command::new("node");
        command
            .arg(format!("--max-old-space-size={}", cfg.max_memory_mb))
            .arg(shim_path)
            .env(
                "BROWSER_IDLE_TIMEOUT_MS",
                (cfg.idle_timeout_s * 1000).to_string(),
            )
            .env("BROWSER_MAX_INSTANCES", cfg.max_instances.to_string())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .map_err(|e| format!("Failed to spawn node: {e}. Is Node.js installed?"))?;

        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout_raw = child.stdout.take().ok_or("no stdout")?;
        let mut stdout = BufReader::new(stdout_raw);

        let mut line = String::new();
        tokio::time::timeout(V8_REQUEST_TIMEOUT, stdout.read_line(&mut line))
            .await
            .map_err(|_| "V8 shim startup timed out".to_string())?
            .map_err(|e| format!("V8 shim startup error: {e}"))?;
        if !line.contains("ready") {
            return Err(format!("V8 shim did not signal ready: {line}"));
        }
        if V8_DEBUG_LOGGING.load(Ordering::Relaxed) {
            eprintln!("[v8] worker spawned pid={:?}", child.id());
        }

        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    async fn request(
        &mut self,
        action: &str,
        name: &str,
        script: &str,
    ) -> Result<String, V8RequestError> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let msg = serde_json::json!({ "id": id, "action": action, "name": name, "script": script });
        let mut line = serde_json::to_string(&msg)
            .map_err(|e| V8RequestError::Transport(format!("V8 request encode error: {e}")))?;
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| V8RequestError::Transport(format!("V8 write error: {e}")))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| V8RequestError::Transport(format!("V8 flush error: {e}")))?;

        let mut resp = String::new();
        self.stdout
            .read_line(&mut resp)
            .await
            .map_err(|e| V8RequestError::Transport(format!("V8 read error: {e}")))?;

        if resp.is_empty() {
            return Err(V8RequestError::Transport(
                "V8 worker closed its output stream".to_string(),
            ));
        }

        let val: serde_json::Value = serde_json::from_str(resp.trim())
            .map_err(|e| V8RequestError::Transport(format!("V8 bad response: {e}")))?;

        if val.get("id").and_then(serde_json::Value::as_u64) != Some(id) {
            return Err(V8RequestError::Transport(
                "V8 response id did not match the request".to_string(),
            ));
        }

        if let Some(metrics) = val.get("metrics") {
            BROWSER_REUSES_TOTAL.fetch_add(
                metrics
                    .get("browserReuses")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                Ordering::Relaxed,
            );
            BROWSER_RECOVERY_LAUNCHES_TOTAL.fetch_add(
                metrics
                    .get("recoveryLaunches")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                Ordering::Relaxed,
            );
            BROWSER_CHALLENGES_TOTAL.fetch_add(
                metrics
                    .get("challenges")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                Ordering::Relaxed,
            );
            BROWSER_PAGE_CLOSE_TIMEOUTS_TOTAL.fetch_add(
                metrics
                    .get("pageCloseTimeouts")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                Ordering::Relaxed,
            );
            V8_GRACEFUL_SHUTDOWNS_TOTAL.fetch_add(
                metrics
                    .get("gracefulShutdowns")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                Ordering::Relaxed,
            );
            V8_FORCED_TERMINATIONS_TOTAL.fetch_add(
                metrics
                    .get("forcedTerminations")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                Ordering::Relaxed,
            );
        }

        match val.get("ok").and_then(serde_json::Value::as_bool) {
            Some(true) => Ok(val["value"].as_str().unwrap_or("").to_string()),
            Some(false) => {
                let error = val.get("error");
                let message = error
                    .and_then(|value| value.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| error.and_then(serde_json::Value::as_str))
                    .unwrap_or("unknown V8 error")
                    .to_string();
                Err(V8RequestError::Action(V8ActionError {
                    code: error
                        .and_then(|value| value.get("code"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("action_error")
                        .to_string(),
                    url: error
                        .and_then(|value| value.get("url"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    status: error
                        .and_then(|value| value.get("status"))
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|status| u16::try_from(status).ok()),
                    message,
                }))
            }
            None => Err(V8RequestError::Transport(
                "V8 response did not contain a boolean 'ok' field".to_string(),
            )),
        }
    }

    async fn force_terminate(&mut self, reason: &str) {
        if V8_DEBUG_LOGGING.load(Ordering::Relaxed) {
            eprintln!(
                "[v8] force terminating worker pid={:?} reason={reason}",
                self.child.id()
            );
        }
        V8_FORCED_TERMINATIONS_TOTAL.fetch_add(1, Ordering::Relaxed);

        #[cfg(windows)]
        if let Some(pid) = self.child.id() {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        }

        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        }

        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }

    async fn shutdown(self, reason: &str) {
        self.shutdown_with_timeout(reason, V8_SHUTDOWN_TIMEOUT)
            .await;
    }

    async fn shutdown_with_timeout(mut self, reason: &str, timeout: Duration) -> bool {
        if V8_DEBUG_LOGGING.load(Ordering::Relaxed) {
            eprintln!(
                "[v8] shutting down worker pid={:?} reason={reason}",
                self.child.id()
            );
        }
        if timeout.is_zero() {
            self.force_terminate(reason).await;
            return false;
        }
        let graceful = tokio::time::timeout(timeout, async {
            self.request("shutdown", reason, "")
                .await
                .map_err(V8RequestError::into_message)?;
            self.child
                .wait()
                .await
                .map_err(|e| format!("V8 worker wait failed: {e}"))?;
            Ok::<(), String>(())
        })
        .await;

        if matches!(graceful, Ok(Ok(()))) {
            V8_GRACEFUL_SHUTDOWNS_TOTAL.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            self.force_terminate(reason).await;
            false
        }
    }
}

async fn ensure_running(handle: &V8ProcessHandle) -> Result<(), String> {
    let mut guard = handle.lock().await;
    match &mut *guard {
        V8Slot::Running(_, last_used) => {
            *last_used = Instant::now();
            return Ok(());
        }
        V8Slot::Retired => return Err("V8 worker owner has been retired".to_string()),
        V8Slot::Empty => {}
    }
    *guard = V8Slot::Running(Box::new(V8Process::spawn().await?), Instant::now());
    Ok(())
}

/// Gracefully closes and clears the subprocess if it has been idle for at least `idle_for`.
/// Returns `true` when a process was reaped. Safe to call on a never-spawned or
/// already-cleared handle (returns `false`).
pub async fn reap_if_idle(handle: &V8ProcessHandle, idle_for: Duration) -> bool {
    let mut guard = handle.lock().await;
    let idle = match &*guard {
        V8Slot::Running(_, last_used) => last_used.elapsed() >= idle_for,
        V8Slot::Empty | V8Slot::Retired => return false,
    };
    if idle {
        if let V8Slot::Running(proc, _) = std::mem::replace(&mut *guard, V8Slot::Empty) {
            proc.shutdown("idle-reap").await;
        }
        true
    } else {
        false
    }
}

pub async fn shutdown(handle: &V8ProcessHandle, reason: &str) -> bool {
    let mut guard = handle.lock().await;
    match std::mem::replace(&mut *guard, V8Slot::Empty) {
        V8Slot::Running(proc, _) => {
            proc.shutdown(reason).await;
            true
        }
        V8Slot::Retired => {
            *guard = V8Slot::Retired;
            false
        }
        V8Slot::Empty => false,
    }
}

pub async fn retire(handle: &V8ProcessHandle, reason: &str) -> bool {
    let mut guard = handle.lock().await;
    match std::mem::replace(&mut *guard, V8Slot::Retired) {
        V8Slot::Running(proc, _) => {
            proc.shutdown(reason).await;
            true
        }
        V8Slot::Empty | V8Slot::Retired => false,
    }
}

/// Sends one request to the live V8 process, bounded by `timeout`.
///
/// Timing out drops the in-flight write/read futures immediately (tokio's
/// async I/O cancellation is cooperative and safe, unlike a blocking-thread
/// approach which cannot be interrupted once stuck) and additionally kills
/// the subprocess so a genuinely CPU-bound guest script doesn't linger. On
/// Action errors keep the worker alive because the newline protocol remains in
/// sync. Transport failures and timeouts retire it before the next request.
async fn with_process_detailed(
    handle: &V8ProcessHandle,
    timeout: Duration,
    action: &str,
    name: &str,
    script: &str,
) -> Result<String, V8RequestError> {
    ensure_running(handle)
        .await
        .map_err(V8RequestError::Transport)?;

    let outcome = tokio::time::timeout(timeout, async {
        let mut guard = handle.lock().await;
        let V8Slot::Running(proc, last_used) = &mut *guard else {
            return Err(V8RequestError::Transport(
                "V8 process not running".to_string(),
            ));
        };
        *last_used = Instant::now();
        proc.request(action, name, script).await
    })
    .await;

    match outcome {
        Ok(Ok(v)) => {
            V8_CALLS_TOTAL.fetch_add(1, Ordering::Relaxed);
            Ok(v)
        }
        Ok(Err(V8RequestError::Action(error))) => Err(V8RequestError::Action(error)),
        Ok(Err(V8RequestError::Transport(message))) => {
            V8_RESTARTS_TOTAL.fetch_add(1, Ordering::Relaxed);
            let mut guard = handle.lock().await;
            match std::mem::replace(&mut *guard, V8Slot::Empty) {
                V8Slot::Running(mut proc, _) => {
                    proc.force_terminate("transport-failure").await;
                }
                V8Slot::Retired => *guard = V8Slot::Retired,
                V8Slot::Empty => {}
            }
            Err(V8RequestError::Transport(message))
        }
        Err(_elapsed) => {
            V8_RESTARTS_TOTAL.fetch_add(1, Ordering::Relaxed);
            let mut guard = handle.lock().await;
            match std::mem::replace(&mut *guard, V8Slot::Empty) {
                V8Slot::Running(mut proc, _) => {
                    proc.force_terminate("request-timeout").await;
                }
                V8Slot::Retired => *guard = V8Slot::Retired,
                V8Slot::Empty => {}
            }
            Err(V8RequestError::Transport(format!(
                "V8 request timed out after {timeout:?}"
            )))
        }
    }
}

async fn with_process(
    handle: &V8ProcessHandle,
    timeout: Duration,
    action: &str,
    name: &str,
    script: &str,
) -> Result<String, String> {
    with_process_detailed(handle, timeout, action, name, script)
        .await
        .map_err(V8RequestError::into_message)
}

pub async fn v8_context_exists(handle: &V8ProcessHandle, name: &str) -> bool {
    with_process(handle, V8_REQUEST_TIMEOUT, "exists", name, "")
        .await
        .map(|v| v == "true")
        .unwrap_or(false)
}

pub async fn v8_context_create(
    handle: &V8ProcessHandle,
    name: &str,
    init_script: &str,
) -> Result<(), String> {
    with_process(handle, V8_REQUEST_TIMEOUT, "create", name, init_script)
        .await
        .map(|_| ())
}

pub async fn v8_context_eval(
    handle: &V8ProcessHandle,
    name: &str,
    script: &str,
) -> Result<String, String> {
    with_process(handle, V8_REQUEST_TIMEOUT, "eval", name, script).await
}

pub async fn v8_context_drop(handle: &V8ProcessHandle, name: &str) {
    let _ = with_process(handle, V8_REQUEST_TIMEOUT, "drop", name, "").await;
}

/// Root directory under which per-source Chromium profiles are stored. Prefers
/// the explicit `KANI_BROWSER_PROFILES_DIR`, then `$KANI_DATA_DIR/.browser-profiles`,
/// falling back to a temp-dir subfolder.
fn profiles_root() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("KANI_BROWSER_PROFILES_DIR")
        && !p.is_empty()
    {
        return std::path::PathBuf::from(p);
    }
    if let Ok(d) = std::env::var("KANI_DATA_DIR")
        && !d.is_empty()
    {
        return std::path::PathBuf::from(d).join(".browser-profiles");
    }
    std::env::temp_dir().join("kani-browser-profiles")
}

/// Maps a source key to a dedicated Chromium `userDataDir`, sanitizing the key
/// to a single safe path component so login/cookie state never leaks between
/// sources and no key can escape the profiles root.
pub fn profile_dir_for(source_key: &str) -> std::path::PathBuf {
    let mut sanitized: String = source_key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() || sanitized.chars().all(|c| c == '.') {
        sanitized = "__default__".to_string();
    }
    let dir = profiles_root().join(sanitized);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub struct CaptureUrlParamOpts<'a> {
    pub url_pattern: &'a str,
    pub param_name: Option<&'a str>,
    pub header_name: Option<&'a str>,
    pub timeout_ms: u32,
    pub force_refresh: bool,
    pub cache_ttl_ms: Option<u32>,
    pub extra_headers: &'a [(String, String)],
}

pub async fn capture_url_param(
    handle: &V8ProcessHandle,
    page_url: &str,
    opts: &CaptureUrlParamOpts<'_>,
    source_key: Option<&str>,
) -> Result<String, String> {
    let enabled = std::env::var("KANI_BROWSER_ENABLED")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    if !enabled {
        return Err(
            "Browser features are disabled (KANI_BROWSER_ENABLED=false). \
             Set KANI_BROWSER_ENABLED=true and ensure chromium is installed."
                .to_string(),
        );
    }
    let verbose = V8_DEBUG_LOGGING.load(Ordering::Relaxed);
    let extra_headers_obj: serde_json::Map<String, serde_json::Value> = opts
        .extra_headers
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    let mut params = serde_json::json!({
        "urlPattern": opts.url_pattern,
        "paramName": opts.param_name,
        "headerName": opts.header_name,
        "timeoutMs": opts.timeout_ms,
        "forceRefresh": opts.force_refresh,
        "cacheTtlMs": opts.cache_ttl_ms,
        "verbose": verbose,
        "extraHeaders": extra_headers_obj,
    });
    if let Some(key) = source_key {
        params["profileDir"] =
            serde_json::Value::String(profile_dir_for(key).to_string_lossy().into_owned());
    }
    let script = params.to_string();
    let timeout = Duration::from_millis(u64::from(opts.timeout_ms)) + V8_TIMEOUT_BUFFER;
    with_process(handle, timeout, "capture_token", page_url, &script).await
}

pub struct BrowserPageCredentials<'a> {
    pub cookie_header: &'a str,
    pub user_agent: &'a str,
}

#[derive(Debug, Clone)]
pub enum CapturePagePayloadError {
    Challenge {
        message: String,
        url: Option<String>,
        status: Option<u16>,
    },
    Action {
        code: String,
        message: String,
    },
    Transport(String),
}

impl std::fmt::Display for CapturePagePayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Challenge {
                message, status, ..
            } => match status {
                Some(status) => write!(f, "{message} (HTTP {status})"),
                None => f.write_str(message),
            },
            Self::Action { code, message } => write!(f, "{message} [{code}]"),
            Self::Transport(message) => f.write_str(message),
        }
    }
}

pub async fn capture_page_payload_detailed(
    handle: &V8ProcessHandle,
    page_url: &str,
    init_script: &str,
    timeout_ms: u32,
    source_key: Option<&str>,
    auto_scroll: bool,
    credentials: Option<&BrowserPageCredentials<'_>>,
) -> Result<String, CapturePagePayloadError> {
    let enabled = std::env::var("KANI_BROWSER_ENABLED")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    if !enabled {
        return Err(CapturePagePayloadError::Action {
            code: "browser_disabled".to_string(),
            message: "Browser features are disabled (KANI_BROWSER_ENABLED=false). \
             Set KANI_BROWSER_ENABLED=true and ensure chromium is installed."
                .to_string(),
        });
    }
    let verbose = V8_DEBUG_LOGGING.load(Ordering::Relaxed);
    let mut params = serde_json::json!({
        "initScript": init_script,
        "timeoutMs": timeout_ms,
        "verbose": verbose,
        "autoScroll": auto_scroll,
    });
    if let Some(credentials) = credentials {
        params["cookieHeader"] = serde_json::Value::String(credentials.cookie_header.to_string());
        params["userAgent"] = serde_json::Value::String(credentials.user_agent.to_string());
    }
    if let Some(key) = source_key {
        params["profileDir"] =
            serde_json::Value::String(profile_dir_for(key).to_string_lossy().into_owned());
    }
    let script = params.to_string();
    let timeout = Duration::from_millis(u64::from(timeout_ms)) + V8_TIMEOUT_BUFFER;
    with_process_detailed(handle, timeout, "capture_page_payload", page_url, &script)
        .await
        .map_err(|error| match error {
            V8RequestError::Action(error) if error.code == "browser_challenge" => {
                CapturePagePayloadError::Challenge {
                    message: error.message,
                    url: error.url,
                    status: error.status,
                }
            }
            V8RequestError::Action(error) => CapturePagePayloadError::Action {
                code: error.code,
                message: error.message,
            },
            V8RequestError::Transport(message) => CapturePagePayloadError::Transport(message),
        })
}

pub async fn capture_page_payload_resilient(
    handle: &V8ProcessHandle,
    http: &crate::http::SmartClient,
    page_url: &str,
    init_script: &str,
    timeout_ms: u32,
    source_key: Option<&str>,
    auto_scroll: bool,
) -> Result<String, CapturePagePayloadError> {
    let first = capture_page_payload_detailed(
        handle,
        page_url,
        init_script,
        timeout_ms,
        source_key,
        auto_scroll,
        None,
    )
    .await;
    if !matches!(first, Err(CapturePagePayloadError::Challenge { .. })) {
        return first;
    }
    if !http.solver_configured() {
        return Err(CapturePagePayloadError::Challenge {
            message: "Cloudflare managed challenge blocked the browser page; configure the existing FlareSolverr URL setting and ensure it shares Kani's public egress IP".to_string(),
            url: Some(page_url.to_string()),
            status: None,
        });
    }

    let credentials = match http.browser_challenge_credentials(page_url, false).await {
        Ok(credentials) => {
            record_browser_solver_result(true);
            credentials
        }
        Err(error) => {
            record_browser_solver_result(false);
            return Err(CapturePagePayloadError::Action {
                code: "solver_error".to_string(),
                message: format!("FlareSolverr challenge solve failed: {error}"),
            });
        }
    };
    let browser_credentials = BrowserPageCredentials {
        cookie_header: &credentials.cookie_header,
        user_agent: &credentials.user_agent,
    };
    let retry = capture_page_payload_detailed(
        handle,
        page_url,
        init_script,
        timeout_ms,
        source_key,
        auto_scroll,
        Some(&browser_credentials),
    )
    .await;
    if !matches!(retry, Err(CapturePagePayloadError::Challenge { .. })) || !credentials.from_cache {
        return retry;
    }

    let refreshed = match http.browser_challenge_credentials(page_url, true).await {
        Ok(credentials) => {
            record_browser_solver_result(true);
            credentials
        }
        Err(error) => {
            record_browser_solver_result(false);
            return Err(CapturePagePayloadError::Action {
                code: "solver_error".to_string(),
                message: format!("FlareSolverr challenge refresh failed: {error}"),
            });
        }
    };
    let browser_credentials = BrowserPageCredentials {
        cookie_header: &refreshed.cookie_header,
        user_agent: &refreshed.user_agent,
    };
    capture_page_payload_detailed(
        handle,
        page_url,
        init_script,
        timeout_ms,
        source_key,
        auto_scroll,
        Some(&browser_credentials),
    )
    .await
}

pub async fn capture_page_payload(
    handle: &V8ProcessHandle,
    page_url: &str,
    init_script: &str,
    timeout_ms: u32,
    source_key: Option<&str>,
) -> Result<String, String> {
    capture_page_payload_detailed(
        handle,
        page_url,
        init_script,
        timeout_ms,
        source_key,
        true,
        None,
    )
    .await
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn null_handle() -> V8ProcessHandle {
        new_handle()
    }

    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    fn mock_puppeteer_module() -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("kani-puppeteer-mock-{}.js", std::process::id()));
        std::fs::write(
            &path,
            r#"const { EventEmitter } = require('events');
let nextPid = 50000;
let challengeRemaining = process.env.KANI_MOCK_CHALLENGE === '1' ? 1 : 0;
module.exports.launch = async function launch(options) {
  const profile = options.userDataDir || '';
  if (process.env.KANI_MOCK_LOCK_CANONICAL === '1' && !profile.includes('-recovery-')) {
    throw new Error('The browser is already running for ' + profile + '. Use a different userDataDir');
  }
  const browser = new EventEmitter();
  const child = { pid: nextPid++, exitCode: null, kill() { this.exitCode = 1; } };
  let connected = true;
  browser.isConnected = () => connected;
  browser.process = () => child;
  browser.version = async () => 'Chrome/140.0.0.0';
  browser.newPage = async () => {
    const page = new EventEmitter();
    const mainFrame = { url: () => 'https://example.test/' };
    const exposed = new Map();
    page.setUserAgent = async () => {};
    page.setViewport = async (viewport) => {
      if (viewport.width !== 1365 || viewport.height !== 768) throw new Error('unexpected viewport');
    };
    page.cookies = async () => [];
    page.deleteCookie = async () => {};
    page.setCookie = async () => {};
    page.exposeFunction = async (name, callback) => exposed.set(name, callback);
    page.evaluateOnNewDocument = async () => {};
    page.evaluate = async () => false;
    page.mainFrame = () => mainFrame;
    page.url = () => 'https://example.test/';
    page.goto = async (url) => {
      const challenged = challengeRemaining > 0;
      if (challenged) challengeRemaining--;
      const request = { isNavigationRequest: () => true };
      const response = {
        request: () => request,
        frame: () => mainFrame,
        status: () => challenged ? 403 : 200,
        url: () => url,
      };
      page.emit('response', response);
      if (!challenged) setImmediate(() => exposed.get('passPayload')?.('{"ok":true}'));
    };
    page.close = async () => {
      if (process.env.KANI_MOCK_HANG_PAGE_CLOSE === '1') await new Promise(() => {});
    };
    return page;
  };
  browser.close = async () => { connected = false; child.exitCode = 0; browser.emit('disconnected'); };
  return browser;
};
"#,
        )
        .expect("write mock puppeteer module");
        path
    }

    async fn worker_pid(handle: &V8ProcessHandle) -> Option<u32> {
        match &*handle.lock().await {
            V8Slot::Running(process, _) => process.child.id(),
            V8Slot::Empty | V8Slot::Retired => None,
        }
    }

    /// Both "false" and "0" values are tested in a single serialised block to
    /// avoid parallel tests racing on the process-wide env var.
    #[tokio::test]
    async fn capture_url_param_disabled_by_env_var() {
        let _guard = ENV_LOCK.lock().await;
        let handle = null_handle();

        for val in &["false", "0"] {
            unsafe { std::env::set_var("KANI_BROWSER_ENABLED", val) };
            let result = capture_url_param(
                &handle,
                "http://example.com",
                &CaptureUrlParamOpts {
                    url_pattern: ".*",
                    param_name: Some("token"),
                    header_name: None,
                    timeout_ms: 100,
                    force_refresh: false,
                    cache_ttl_ms: None,
                    extra_headers: &[],
                },
                None,
            )
            .await;
            let err = result.unwrap_err();
            assert!(
                err.contains("disabled") || err.contains("KANI_BROWSER_ENABLED"),
                "value={val}: expected disabled message, got: {err}"
            );
        }
        unsafe { std::env::remove_var("KANI_BROWSER_ENABLED") };
    }

    #[test]
    fn set_v8_debug_logging_toggles_flag() {
        set_v8_debug_logging(true);
        assert!(V8_DEBUG_LOGGING.load(Ordering::Relaxed));
        set_v8_debug_logging(false);
        assert!(!V8_DEBUG_LOGGING.load(Ordering::Relaxed));
    }

    #[test]
    fn profile_dir_for_sanitizes_path_traversal() {
        let root = profiles_root();
        for key in ["../evil", "..", ".", "a/b/c", "..\\..\\x"] {
            let dir = profile_dir_for(key);
            assert_eq!(
                dir.parent(),
                Some(root.as_path()),
                "key {key:?} escaped the profiles root: {dir:?}"
            );
            let last = dir.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                !last.contains('/') && !last.contains('\\') && last != ".." && last != ".",
                "key {key:?} produced unsafe component {last:?}"
            );
        }
    }

    #[tokio::test]
    async fn reap_if_idle_returns_false_on_empty_handle() {
        let handle = null_handle();
        assert!(!reap_if_idle(&handle, Duration::from_secs(0)).await);
    }

    #[tokio::test]
    async fn action_error_keeps_worker_reusable() {
        let handle = null_handle();
        assert!(!v8_context_exists(&handle, "missing-context").await);
        let before = worker_pid(&handle).await.expect("worker should be running");

        let error = v8_context_eval(&handle, "missing-context", "1 + 1")
            .await
            .expect_err("missing context should be an action error");
        assert!(error.contains("not found"));
        assert_eq!(worker_pid(&handle).await, Some(before));

        v8_context_create(&handle, "reusable-context", "globalThis.answer = 42")
            .await
            .expect("worker should accept a later request");
        assert_eq!(
            v8_context_eval(&handle, "reusable-context", "answer")
                .await
                .expect("eval after action error"),
            "42"
        );
        assert!(shutdown(&handle, "test-complete").await);
    }

    #[tokio::test]
    async fn browser_probe_reuses_one_entry_across_pages() {
        let _guard = ENV_LOCK.lock().await;
        let module = mock_puppeteer_module();
        unsafe { std::env::set_var("KANI_PUPPETEER_MODULE", &module) };
        unsafe { std::env::remove_var("KANI_MOCK_LOCK_CANONICAL") };
        let handle = null_handle();
        let params = serde_json::json!({
            "profileDir": std::env::temp_dir().join("kani-browser-probe-profile"),
            "verbose": false,
        })
        .to_string();

        let mut entry_ids = Vec::new();
        for page in 1..=3 {
            if page == 2 {
                let error = with_process(
                    &handle,
                    V8_REQUEST_TIMEOUT,
                    "unknown-test-action",
                    "expected-error",
                    "",
                )
                .await
                .expect_err("action error should be returned");
                assert!(error.contains("Unknown action"));
            }
            let raw = with_process(
                &handle,
                V8_REQUEST_TIMEOUT,
                "browser_probe",
                &format!("page-{page}"),
                &params,
            )
            .await
            .expect("browser probe");
            let value: serde_json::Value = serde_json::from_str(&raw).expect("probe response");
            entry_ids.push(value["entryId"].as_u64().expect("entry id"));
        }

        assert_eq!(entry_ids, vec![1, 1, 1]);
        assert!(shutdown(&handle, "test-complete").await);
        unsafe {
            std::env::remove_var("KANI_BROWSER_CHALLENGE_GRACE_MS");
            std::env::remove_var("KANI_PUPPETEER_MODULE");
        }
        let _ = std::fs::remove_file(module);
    }

    #[tokio::test]
    async fn challenge_is_an_action_error_and_worker_remains_reusable() {
        let _guard = ENV_LOCK.lock().await;
        let module = mock_puppeteer_module();
        unsafe {
            std::env::set_var("KANI_PUPPETEER_MODULE", &module);
            std::env::set_var("KANI_MOCK_CHALLENGE", "1");
            std::env::set_var("KANI_BROWSER_CHALLENGE_GRACE_MS", "5");
        }
        let handle = null_handle();
        let profile = std::env::temp_dir().join("kani-browser-challenge-profile");
        let probe = serde_json::json!({ "profileDir": profile, "verbose": false }).to_string();
        let before: serde_json::Value = serde_json::from_str(
            &with_process(
                &handle,
                V8_REQUEST_TIMEOUT,
                "browser_probe",
                "before",
                &probe,
            )
            .await
            .expect("initial probe"),
        )
        .expect("probe response");
        let worker_before = worker_pid(&handle).await;
        let capture = serde_json::json!({
            "profileDir": profile,
            "timeoutMs": 200,
            "challengeGraceMs": 5,
            "autoScroll": false,
        })
        .to_string();

        let error = with_process_detailed(
            &handle,
            Duration::from_secs(1),
            "capture_page_payload",
            "https://example.test/",
            &capture,
        )
        .await
        .expect_err("challenge should be reported");
        assert!(matches!(
            error,
            V8RequestError::Action(V8ActionError { ref code, .. }) if code == "browser_challenge"
        ));

        unsafe { std::env::remove_var("KANI_MOCK_CHALLENGE") };
        let payload = with_process(
            &handle,
            Duration::from_secs(1),
            "capture_page_payload",
            "https://example.test/",
            &capture,
        )
        .await
        .expect("capture after challenge");
        assert_eq!(payload, r#"{"ok":true}"#);
        let after: serde_json::Value = serde_json::from_str(
            &with_process(
                &handle,
                V8_REQUEST_TIMEOUT,
                "browser_probe",
                "after",
                &probe,
            )
            .await
            .expect("final probe"),
        )
        .expect("probe response");
        assert_eq!(worker_pid(&handle).await, worker_before);
        assert_eq!(before["entryId"], after["entryId"]);

        assert!(shutdown(&handle, "test-complete").await);
        unsafe {
            std::env::remove_var("KANI_BROWSER_CHALLENGE_GRACE_MS");
            std::env::remove_var("KANI_PUPPETEER_MODULE");
        }
        let _ = std::fs::remove_file(module);
    }

    #[tokio::test]
    async fn resilient_capture_solves_once_then_reuses_the_browser_worker() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let _guard = ENV_LOCK.lock().await;
        let module = mock_puppeteer_module();
        unsafe {
            std::env::set_var("KANI_PUPPETEER_MODULE", &module);
            std::env::set_var("KANI_MOCK_CHALLENGE", "1");
            std::env::set_var("KANI_BROWSER_CHALLENGE_GRACE_MS", "5");
        }
        let solver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok",
                "solution": {
                    "userAgent": "solver-agent",
                    "cookies": [{"name": "cf_clearance", "value": "opaque"}]
                }
            })))
            .mount(&solver)
            .await;
        let http = crate::http::SmartClient::new(Some(solver.uri())).expect("smart client");
        let handle = null_handle();

        let payload = capture_page_payload_resilient(
            &handle,
            &http,
            "https://example.test/",
            "",
            200,
            Some("solver-source"),
            false,
        )
        .await
        .expect("solver-assisted capture");
        assert_eq!(payload, r#"{"ok":true}"#);
        assert_eq!(solver.received_requests().await.unwrap().len(), 1);
        assert!(worker_pid(&handle).await.is_some());

        assert!(shutdown(&handle, "test-complete").await);
        unsafe {
            std::env::remove_var("KANI_MOCK_CHALLENGE");
            std::env::remove_var("KANI_BROWSER_CHALLENGE_GRACE_MS");
            std::env::remove_var("KANI_PUPPETEER_MODULE");
        }
        let _ = std::fs::remove_file(module);
    }

    #[tokio::test]
    async fn resilient_capture_reports_missing_solver_without_resetting_worker() {
        let _guard = ENV_LOCK.lock().await;
        let module = mock_puppeteer_module();
        unsafe {
            std::env::set_var("KANI_PUPPETEER_MODULE", &module);
            std::env::set_var("KANI_MOCK_CHALLENGE", "1");
            std::env::set_var("KANI_BROWSER_CHALLENGE_GRACE_MS", "5");
        }
        let http = crate::http::SmartClient::new(None).expect("smart client");
        let handle = null_handle();

        let error = capture_page_payload_resilient(
            &handle,
            &http,
            "https://example.test/",
            "",
            200,
            Some("missing-solver-source"),
            false,
        )
        .await
        .expect_err("missing solver should be actionable");
        assert!(error.to_string().contains("FlareSolverr"));
        assert!(worker_pid(&handle).await.is_some());

        assert!(shutdown(&handle, "test-complete").await);
        unsafe {
            std::env::remove_var("KANI_MOCK_CHALLENGE");
            std::env::remove_var("KANI_BROWSER_CHALLENGE_GRACE_MS");
            std::env::remove_var("KANI_PUPPETEER_MODULE");
        }
        let _ = std::fs::remove_file(module);
    }

    #[tokio::test]
    async fn hanging_page_close_retires_browser_but_not_worker() {
        let _guard = ENV_LOCK.lock().await;
        let module = mock_puppeteer_module();
        unsafe {
            std::env::set_var("KANI_PUPPETEER_MODULE", &module);
            std::env::set_var("KANI_MOCK_HANG_PAGE_CLOSE", "1");
        }
        let handle = null_handle();
        let profile = std::env::temp_dir().join("kani-browser-hanging-close-profile");
        let capture = serde_json::json!({
            "profileDir": profile,
            "timeoutMs": 200,
            "autoScroll": false,
        })
        .to_string();
        let error = with_process_detailed(
            &handle,
            Duration::from_secs(3),
            "capture_page_payload",
            "https://example.test/",
            &capture,
        )
        .await
        .expect_err("stuck cleanup should be an action error");
        assert!(matches!(
            error,
            V8RequestError::Action(V8ActionError { ref code, .. }) if code == "page_cleanup_timeout"
        ));
        let worker_before = worker_pid(&handle).await;
        unsafe { std::env::remove_var("KANI_MOCK_HANG_PAGE_CLOSE") };
        let probe = serde_json::json!({ "profileDir": profile, "verbose": false }).to_string();
        let after: serde_json::Value = serde_json::from_str(
            &with_process(
                &handle,
                V8_REQUEST_TIMEOUT,
                "browser_probe",
                "after",
                &probe,
            )
            .await
            .expect("replacement probe"),
        )
        .expect("probe response");
        assert_eq!(worker_pid(&handle).await, worker_before);
        assert_eq!(after["entryId"], 2);

        assert!(shutdown(&handle, "test-complete").await);
        unsafe { std::env::remove_var("KANI_PUPPETEER_MODULE") };
        let _ = std::fs::remove_file(module);
    }

    #[tokio::test]
    async fn solver_user_agent_is_reused_and_a_change_relaunches_browser() {
        let _guard = ENV_LOCK.lock().await;
        let module = mock_puppeteer_module();
        unsafe { std::env::set_var("KANI_PUPPETEER_MODULE", &module) };
        let handle = null_handle();
        let profile = std::env::temp_dir().join("kani-browser-user-agent-profile");
        let capture = |user_agent: &str| {
            serde_json::json!({
                "profileDir": profile,
                "timeoutMs": 200,
                "autoScroll": false,
                "cookieHeader": "cf_clearance=opaque",
                "userAgent": user_agent,
            })
            .to_string()
        };
        let probe = serde_json::json!({ "profileDir": profile, "verbose": false }).to_string();

        for user_agent in ["solver-agent-one", "solver-agent-one"] {
            with_process(
                &handle,
                Duration::from_secs(1),
                "capture_page_payload",
                "https://example.test/",
                &capture(user_agent),
            )
            .await
            .expect("cleared capture");
        }
        let first: serde_json::Value = serde_json::from_str(
            &with_process(
                &handle,
                V8_REQUEST_TIMEOUT,
                "browser_probe",
                "first",
                &probe,
            )
            .await
            .expect("first probe"),
        )
        .expect("probe response");

        with_process(
            &handle,
            Duration::from_secs(1),
            "capture_page_payload",
            "https://example.test/",
            &capture("solver-agent-two"),
        )
        .await
        .expect("capture after UA change");
        let second: serde_json::Value = serde_json::from_str(
            &with_process(
                &handle,
                V8_REQUEST_TIMEOUT,
                "browser_probe",
                "second",
                &probe,
            )
            .await
            .expect("second probe"),
        )
        .expect("probe response");
        assert_ne!(first["entryId"], second["entryId"]);

        assert!(shutdown(&handle, "test-complete").await);
        unsafe { std::env::remove_var("KANI_PUPPETEER_MODULE") };
        let _ = std::fs::remove_file(module);
    }

    #[tokio::test]
    async fn disconnected_browser_is_replaced_once() {
        let _guard = ENV_LOCK.lock().await;
        let module = mock_puppeteer_module();
        unsafe { std::env::set_var("KANI_PUPPETEER_MODULE", &module) };
        unsafe { std::env::remove_var("KANI_MOCK_LOCK_CANONICAL") };
        let handle = null_handle();
        let params = serde_json::json!({
            "profileDir": std::env::temp_dir().join("kani-browser-disconnect-profile"),
            "verbose": false,
        })
        .to_string();

        let first = with_process(
            &handle,
            V8_REQUEST_TIMEOUT,
            "browser_probe",
            "page-1",
            &params,
        )
        .await
        .expect("first probe");
        assert_eq!(
            with_process(
                &handle,
                V8_REQUEST_TIMEOUT,
                "browser_disconnect",
                "disconnect",
                &params,
            )
            .await
            .expect("disconnect browser"),
            "true"
        );
        let second = with_process(
            &handle,
            V8_REQUEST_TIMEOUT,
            "browser_probe",
            "page-2",
            &params,
        )
        .await
        .expect("replacement probe");
        let first: serde_json::Value = serde_json::from_str(&first).expect("first response");
        let second: serde_json::Value = serde_json::from_str(&second).expect("second response");
        assert_ne!(first["entryId"], second["entryId"]);

        assert!(shutdown(&handle, "test-complete").await);
        unsafe { std::env::remove_var("KANI_PUPPETEER_MODULE") };
        let _ = std::fs::remove_file(module);
    }

    #[tokio::test]
    async fn reap_live_worker_allows_clean_restart() {
        let handle = null_handle();
        assert!(!v8_context_exists(&handle, "first-worker").await);
        assert!(reap_if_idle(&handle, Duration::ZERO).await);
        assert!(worker_pid(&handle).await.is_none());
        assert!(!v8_context_exists(&handle, "second-worker").await);
        assert!(worker_pid(&handle).await.is_some());
        assert!(shutdown(&handle, "test-complete").await);
    }

    #[tokio::test]
    async fn retired_worker_cannot_respawn() {
        let handle = null_handle();
        assert!(!v8_context_exists(&handle, "retired-worker").await);
        assert!(retire(&handle, "test-retire").await);
        let error = v8_context_create(&handle, "must-not-start", "globalThis.value = 1")
            .await
            .expect_err("retired owner must reject new workers");
        assert!(error.contains("retired"));
        assert!(worker_pid(&handle).await.is_none());
    }

    #[tokio::test]
    async fn broken_ipc_forces_cleanup_and_resets_handle() {
        let handle = null_handle();
        assert!(!v8_context_exists(&handle, "broken-worker").await);
        {
            let mut guard = handle.lock().await;
            let V8Slot::Running(process, _) = &mut *guard else {
                panic!("worker should be running");
            };
            process.child.kill().await.expect("kill mock worker");
        }

        let error = with_process(&handle, V8_REQUEST_TIMEOUT, "exists", "after-kill", "")
            .await
            .expect_err("closed IPC must be a transport failure");
        assert!(error.contains("write") || error.contains("read") || error.contains("closed"));
        assert!(matches!(*handle.lock().await, V8Slot::Empty));
    }

    #[tokio::test]
    async fn shutdown_timeout_forces_cleanup() {
        let handle = null_handle();
        assert!(!v8_context_exists(&handle, "shutdown-timeout").await);
        let process = {
            let mut guard = handle.lock().await;
            match std::mem::replace(&mut *guard, V8Slot::Empty) {
                V8Slot::Running(process, _) => process,
                V8Slot::Empty | V8Slot::Retired => panic!("worker should be running"),
            }
        };

        assert!(
            !process
                .shutdown_with_timeout("test-timeout", Duration::ZERO)
                .await
        );
        assert!(worker_pid(&handle).await.is_none());
    }

    #[tokio::test]
    async fn locked_profile_uses_one_reusable_recovery_entry() {
        let _guard = ENV_LOCK.lock().await;
        let module = mock_puppeteer_module();
        unsafe { std::env::set_var("KANI_PUPPETEER_MODULE", &module) };
        unsafe { std::env::set_var("KANI_MOCK_LOCK_CANONICAL", "1") };
        let handle = null_handle();
        let params = serde_json::json!({
            "profileDir": std::env::temp_dir().join("kani-browser-locked-profile"),
            "verbose": false,
        })
        .to_string();

        let first = with_process(
            &handle,
            V8_REQUEST_TIMEOUT,
            "browser_probe",
            "page-1",
            &params,
        )
        .await
        .expect("recovery probe");
        let second = with_process(
            &handle,
            V8_REQUEST_TIMEOUT,
            "browser_probe",
            "page-2",
            &params,
        )
        .await
        .expect("reused recovery probe");
        let first: serde_json::Value = serde_json::from_str(&first).expect("first response");
        let second: serde_json::Value = serde_json::from_str(&second).expect("second response");
        assert_eq!(first["entryId"], second["entryId"]);
        assert_eq!(first["recovery"], true);

        assert!(shutdown(&handle, "test-complete").await);
        unsafe { std::env::remove_var("KANI_MOCK_LOCK_CANONICAL") };
        unsafe { std::env::remove_var("KANI_PUPPETEER_MODULE") };
        let _ = std::fs::remove_file(module);
    }

    #[test]
    fn profile_dir_for_is_stable_and_distinct() {
        assert_eq!(profile_dir_for("source-a"), profile_dir_for("source-a"));
        assert_ne!(profile_dir_for("source-a"), profile_dir_for("source-b"));
    }

    #[test]
    fn v8_config_default_values() {
        let cfg = V8Config::default();
        assert_eq!(cfg.max_memory_mb, 512);
        assert_eq!(cfg.max_instances, 2);
        assert_eq!(cfg.idle_timeout_s, 300);
    }

    #[test]
    fn set_v8_config_roundtrips() {
        let restore = v8_config();
        set_v8_config(V8Config {
            max_memory_mb: 1024,
            max_instances: 4,
            idle_timeout_s: 120,
        });
        let cfg = v8_config();
        assert_eq!(cfg.max_memory_mb, 1024);
        assert_eq!(cfg.max_instances, 4);
        assert_eq!(cfg.idle_timeout_s, 120);
        let stats = browser_stats();
        assert_eq!(stats.max_memory_mb, 1024);
        assert_eq!(stats.max_instances, 4);
        assert_eq!(stats.idle_timeout_s, 120);
        set_v8_config(restore);
    }

    #[tokio::test]
    async fn v8_context_does_not_exist_for_unknown_name() {
        let handle = null_handle();
        let exists = v8_context_exists(&handle, "no-such-ctx-test-only").await;
        assert!(!exists);
    }
}
