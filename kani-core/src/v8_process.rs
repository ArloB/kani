use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
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

/// Snapshot of browser-runtime counters plus the active resource caps, for the
/// diagnostics surface. Lives in kani-shared so kani-web can serialize it.
pub fn browser_stats() -> kani_shared::types::BrowserStats {
    let cfg = v8_config();
    kani_shared::types::BrowserStats {
        calls_total: V8_CALLS_TOTAL.load(Ordering::Relaxed),
        restarts: V8_RESTARTS_TOTAL.load(Ordering::Relaxed),
        max_memory_mb: cfg.max_memory_mb,
        max_instances: cfg.max_instances,
        idle_timeout_s: cfg.idle_timeout_s,
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Timeout for V8 IPC requests that don't carry their own caller-supplied
/// timeout (context create/eval/exists/drop).
const V8_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Buffer added on top of a caller-supplied `timeout_ms` (capture_url_param /
/// capture_page_payload) so the host-side timeout only fires if the JS-side
/// timeout enforcement itself fails to fire.
const V8_TIMEOUT_BUFFER: Duration = Duration::from_secs(5);

pub type V8ProcessHandle = Arc<Mutex<Option<(V8Process, Instant)>>>;

/// Constructs a fresh, empty V8 process handle; the process itself is
/// spawned lazily on first use.
pub fn new_handle() -> V8ProcessHandle {
    Arc::new(Mutex::new(None))
}

#[derive(Debug)]
pub struct V8Process {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl V8Process {
    async fn spawn() -> Result<Self, String> {
        let shim_path = std::env::temp_dir().join("kani_v8_shim.js");
        std::fs::write(&shim_path, include_str!("v8_shim.js"))
            .map_err(|e| format!("Failed to write v8 shim: {e}"))?;

        let cfg = v8_config();
        let mut child = Command::new("node")
            .arg(format!("--max-old-space-size={}", cfg.max_memory_mb))
            .arg(&shim_path)
            .env(
                "BROWSER_IDLE_TIMEOUT_MS",
                (cfg.idle_timeout_s * 1000).to_string(),
            )
            .env("BROWSER_MAX_INSTANCES", cfg.max_instances.to_string())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
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

        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    async fn request(&mut self, action: &str, name: &str, script: &str) -> Result<String, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let msg = serde_json::json!({ "id": id, "action": action, "name": name, "script": script });
        let mut line =
            serde_json::to_string(&msg).map_err(|e| format!("V8 request encode error: {e}"))?;
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("V8 write error: {e}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("V8 flush error: {e}"))?;

        let mut resp = String::new();
        self.stdout
            .read_line(&mut resp)
            .await
            .map_err(|e| format!("V8 read error: {e}"))?;

        let val: serde_json::Value =
            serde_json::from_str(resp.trim()).map_err(|e| format!("V8 bad response: {e}"))?;

        if val["ok"].as_bool().unwrap_or(false) {
            Ok(val["value"].as_str().unwrap_or("").to_string())
        } else {
            Err(val["error"]
                .as_str()
                .unwrap_or("unknown V8 error")
                .to_string())
        }
    }

    /// Kills the subprocess. Called after a request times out so a wedged or
    /// CPU-bound guest script doesn't linger as an orphaned node process.
    async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }
}

async fn ensure_running(handle: &V8ProcessHandle) -> Result<(), String> {
    let mut guard = handle.lock().await;
    if guard.is_none() {
        *guard = Some((V8Process::spawn().await?, Instant::now()));
    }
    Ok(())
}

/// Kills and clears the subprocess if it has been idle for at least `idle_for`.
/// Returns `true` when a process was reaped. Safe to call on a never-spawned or
/// already-cleared handle (returns `false`).
pub async fn reap_if_idle(handle: &V8ProcessHandle, idle_for: Duration) -> bool {
    let mut guard = handle.lock().await;
    let idle = match guard.as_ref() {
        Some((_, last_used)) => last_used.elapsed() >= idle_for,
        None => return false,
    };
    if idle {
        if let Some((mut proc, _)) = guard.take() {
            proc.kill().await;
        }
        true
    } else {
        false
    }
}

/// Sends one request to the live V8 process, bounded by `timeout`.
///
/// Timing out drops the in-flight write/read futures immediately (tokio's
/// async I/O cancellation is cooperative and safe, unlike a blocking-thread
/// approach which cannot be interrupted once stuck) and additionally kills
/// the subprocess so a genuinely CPU-bound guest script doesn't linger. On
/// any error (request-level failure or timeout) the process is torn down so
/// the next call respawns a fresh one rather than reusing a possibly-wedged
/// process.
async fn with_process(
    handle: &V8ProcessHandle,
    timeout: Duration,
    action: &str,
    name: &str,
    script: &str,
) -> Result<String, String> {
    ensure_running(handle).await?;

    let outcome = tokio::time::timeout(timeout, async {
        let mut guard = handle.lock().await;
        let (proc, last_used) = guard.as_mut().ok_or("V8 process not running")?;
        *last_used = Instant::now();
        proc.request(action, name, script).await
    })
    .await;

    match outcome {
        Ok(Ok(v)) => {
            V8_CALLS_TOTAL.fetch_add(1, Ordering::Relaxed);
            Ok(v)
        }
        Ok(Err(e)) => {
            V8_RESTARTS_TOTAL.fetch_add(1, Ordering::Relaxed);
            let mut guard = handle.lock().await;
            *guard = None;
            Err(e)
        }
        Err(_elapsed) => {
            V8_RESTARTS_TOTAL.fetch_add(1, Ordering::Relaxed);
            let mut guard = handle.lock().await;
            if let Some((mut proc, _)) = guard.take() {
                proc.kill().await;
            }
            Err(format!("V8 request timed out after {timeout:?}"))
        }
    }
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

pub async fn capture_page_payload(
    handle: &V8ProcessHandle,
    page_url: &str,
    init_script: &str,
    timeout_ms: u32,
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
    let mut params = serde_json::json!({
        "initScript": init_script,
        "timeoutMs": timeout_ms,
        "verbose": verbose,
    });
    if let Some(key) = source_key {
        params["profileDir"] =
            serde_json::Value::String(profile_dir_for(key).to_string_lossy().into_owned());
    }
    let script = params.to_string();
    let timeout = Duration::from_millis(u64::from(timeout_ms)) + V8_TIMEOUT_BUFFER;
    with_process(handle, timeout, "capture_page_payload", page_url, &script).await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn null_handle() -> V8ProcessHandle {
        new_handle()
    }

    // Serialise tests that mutate KANI_BROWSER_ENABLED to prevent races. An
    // async-aware mutex since the guard must stay held across the `.await`
    // calls below.
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    /// Both "false" and "0" values are tested in a single serialised block to
    /// avoid parallel tests racing on the process-wide env var.
    #[tokio::test]
    async fn capture_url_param_disabled_by_env_var() {
        let _guard = ENV_LOCK.lock().await;
        let handle = null_handle();

        for val in &["false", "0"] {
            // SAFETY: tests hold ENV_LOCK; no other test mutates this var concurrently.
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
        // Whether or not Node.js is installed: a context with this name is never created,
        // so v8_context_exists must return false.
        let handle = null_handle();
        let exists = v8_context_exists(&handle, "no-such-ctx-test-only").await;
        assert!(!exists);
    }
}
