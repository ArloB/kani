use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

static V8_DEBUG_LOGGING: AtomicBool = AtomicBool::new(false);

pub fn set_v8_debug_logging(enabled: bool) {
    V8_DEBUG_LOGGING.store(enabled, Ordering::Relaxed);
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Timeout for V8 IPC requests that don't carry their own caller-supplied
/// timeout (context create/eval/exists/drop).
const V8_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Buffer added on top of a caller-supplied `timeout_ms` (capture_url_param /
/// capture_page_payload) so the host-side timeout only fires if the JS-side
/// timeout enforcement itself fails to fire.
const V8_TIMEOUT_BUFFER: Duration = Duration::from_secs(5);

pub type V8ProcessHandle = Arc<Mutex<Option<V8Process>>>;

/// Constructs a fresh, empty V8 process handle; the process itself is
/// spawned lazily on first use.
pub fn new_handle() -> V8ProcessHandle {
    Arc::new(Mutex::new(None))
}

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

        let mut child = Command::new("node")
            .arg(&shim_path)
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
        *guard = Some(V8Process::spawn().await?);
    }
    Ok(())
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
        let proc = guard.as_mut().ok_or("V8 process not running")?;
        proc.request(action, name, script).await
    })
    .await;

    match outcome {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => {
            let mut guard = handle.lock().await;
            *guard = None;
            Err(e)
        }
        Err(_elapsed) => {
            let mut guard = handle.lock().await;
            if let Some(mut proc) = guard.take() {
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
    let script = serde_json::json!({
        "urlPattern": opts.url_pattern,
        "paramName": opts.param_name,
        "headerName": opts.header_name,
        "timeoutMs": opts.timeout_ms,
        "forceRefresh": opts.force_refresh,
        "cacheTtlMs": opts.cache_ttl_ms,
        "verbose": verbose,
        "extraHeaders": extra_headers_obj,
    })
    .to_string();
    let timeout = Duration::from_millis(u64::from(opts.timeout_ms)) + V8_TIMEOUT_BUFFER;
    with_process(handle, timeout, "capture_token", page_url, &script).await
}

pub async fn capture_page_payload(
    handle: &V8ProcessHandle,
    page_url: &str,
    init_script: &str,
    timeout_ms: u32,
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
    let script = serde_json::json!({
        "initScript": init_script,
        "timeoutMs": timeout_ms,
        "verbose": verbose,
    })
    .to_string();
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

    #[tokio::test]
    async fn v8_context_does_not_exist_for_unknown_name() {
        // Whether or not Node.js is installed: a context with this name is never created,
        // so v8_context_exists must return false.
        let handle = null_handle();
        let exists = v8_context_exists(&handle, "no-such-ctx-test-only").await;
        assert!(!exists);
    }
}
