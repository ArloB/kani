use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static BROWSER_DEBUG_LOGGING: AtomicBool = AtomicBool::new(false);

pub fn set_browser_debug_logging(enabled: bool) {
    BROWSER_DEBUG_LOGGING.store(enabled, Ordering::Relaxed);
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub type V8ProcessHandle = Arc<Mutex<Option<V8Process>>>;

pub struct V8Process {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl V8Process {
    pub fn spawn() -> Result<Self, String> {
        let shim_path = std::env::temp_dir().join("kani_v8_shim.js");
        std::fs::write(&shim_path, include_str!("v8_shim.js"))
            .map_err(|e| format!("Failed to write v8 shim: {e}"))?;

        let mut child = Command::new("node")
            .arg(&shim_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Failed to spawn node: {e}. Is Node.js installed?"))?;

        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout_raw = child.stdout.take().ok_or("no stdout")?;
        let mut stdout = BufReader::new(stdout_raw);

        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .map_err(|e| format!("V8 shim startup error: {e}"))?;
        if !line.contains("ready") {
            return Err(format!("V8 shim did not signal ready: {line}"));
        }

        Ok(Self { _child: child, stdin, stdout })
    }

    fn request(&mut self, action: &str, name: &str, script: &str) -> Result<String, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let msg =
            serde_json::json!({ "id": id, "action": action, "name": name, "script": script });
        let mut line = serde_json::to_string(&msg).unwrap();
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .map_err(|e| format!("V8 write error: {e}"))?;
        self.stdin.flush().map_err(|e| format!("V8 flush error: {e}"))?;

        let mut resp = String::new();
        self.stdout
            .read_line(&mut resp)
            .map_err(|e| format!("V8 read error: {e}"))?;

        let val: serde_json::Value = serde_json::from_str(resp.trim())
            .map_err(|e| format!("V8 bad response: {e}"))?;

        if val["ok"].as_bool().unwrap_or(false) {
            Ok(val["value"].as_str().unwrap_or("").to_string())
        } else {
            Err(val["error"].as_str().unwrap_or("unknown V8 error").to_string())
        }
    }
}

fn ensure_running(handle: &V8ProcessHandle) -> Result<(), String> {
    let mut guard = handle.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(V8Process::spawn()?);
    }
    Ok(())
}

fn with_process<F, T>(handle: &V8ProcessHandle, f: F) -> Result<T, String>
where
    F: FnOnce(&mut V8Process) -> Result<T, String>,
{
    ensure_running(handle)?;
    let mut guard = handle.lock().unwrap_or_else(|e| e.into_inner());
    let proc = guard.as_mut().ok_or("V8 process not running")?;
    match f(proc) {
        Ok(v) => Ok(v),
        Err(e) => {
            *guard = None;
            Err(e)
        }
    }
}

pub fn v8_context_exists(handle: &V8ProcessHandle, name: &str) -> bool {
    with_process(handle, |p| p.request("exists", name, ""))
        .map(|v| v == "true")
        .unwrap_or(false)
}

pub fn v8_context_create(
    handle: &V8ProcessHandle,
    name: &str,
    init_script: &str,
) -> Result<(), String> {
    with_process(handle, |p| p.request("create", name, init_script)).map(|_| ())
}

pub fn v8_context_eval(
    handle: &V8ProcessHandle,
    name: &str,
    script: &str,
) -> Result<String, String> {
    with_process(handle, |p| p.request("eval", name, script))
}

pub fn v8_context_drop(handle: &V8ProcessHandle, name: &str) {
    let _ = with_process(handle, |p| p.request("drop", name, ""));
}

pub fn capture_url_param(
    handle: &V8ProcessHandle,
    page_url: &str,
    url_pattern: &str,
    param: &str,
    timeout_ms: u32,
    force_refresh: bool,
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
    let verbose = BROWSER_DEBUG_LOGGING.load(Ordering::Relaxed);
    with_process(handle, |p| {
        p.request(
            "capture_token",
            page_url,
            &format!("{}|{}|{}|{}|{}",
                url_pattern, param, timeout_ms,
                if force_refresh { 1 } else { 0 },
                if verbose { 1 } else { 0 },
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn null_handle() -> V8ProcessHandle {
        Arc::new(Mutex::new(None))
    }

    // Serialise tests that mutate KANI_BROWSER_ENABLED to prevent races.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Both "false" and "0" values are tested in a single serialised block to
    /// avoid parallel tests racing on the process-wide env var.
    #[test]
    fn capture_url_param_disabled_by_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        let handle = null_handle();

        for val in &["false", "0"] {
            // SAFETY: tests hold ENV_LOCK; no other test mutates this var concurrently.
            unsafe { std::env::set_var("KANI_BROWSER_ENABLED", val) };
            let result =
                capture_url_param(&handle, "http://example.com", ".*", "token", 100, false);
            let err = result.unwrap_err();
            assert!(
                err.contains("disabled") || err.contains("KANI_BROWSER_ENABLED"),
                "value={val}: expected disabled message, got: {err}"
            );
        }
        unsafe { std::env::remove_var("KANI_BROWSER_ENABLED") };
    }

    #[test]
    fn set_browser_debug_logging_toggles_flag() {
        set_browser_debug_logging(true);
        assert!(BROWSER_DEBUG_LOGGING.load(Ordering::Relaxed));
        set_browser_debug_logging(false);
        assert!(!BROWSER_DEBUG_LOGGING.load(Ordering::Relaxed));
    }

    #[test]
    fn v8_context_does_not_exist_for_unknown_name() {
        // Whether or not Node.js is installed: a context with this name is never created,
        // so v8_context_exists must return false.
        let handle = null_handle();
        let exists = v8_context_exists(&handle, "no-such-ctx-test-only");
        assert!(!exists);
    }
}
