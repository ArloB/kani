//! Named tuneable constants for the service layer. Change here to affect all usages.
//!
//! Downloader defaults live in `kani_core::downloader` (the crate that owns
//! `DownloaderConfig`); kani-core cannot depend on this crate.

/// Capacity of the SSE refresh-event broadcast channel in production. Sized so a
/// briefly-stalled client can fall behind a burst of refresh events without the
/// sender lagging; slow receivers are dropped past this bound rather than blocking.
pub const SSE_BROADCAST_CAPACITY: usize = 256;

/// Delay between cover-retry sweeps for manga whose cover download previously failed.
pub const COVER_RETRY_INTERVAL_SECS: u64 = 30;

/// Interval between background refreshes of cached source credentials.
pub const CREDENTIAL_REFRESH_INTERVAL_SECS: u64 = 20 * 60;

/// Period of the wasmtime epoch ticker; bounds how long a runaway extension can
/// run before an epoch-deadline interrupt fires.
pub const WASM_EPOCH_TICK_MS: u64 = 10;

/// Maximum simultaneous background jobs across all types.
pub const DEFAULT_MAX_CONCURRENT_JOBS: usize = 10;

/// Maximum simultaneous chapter downloads across all sources.
pub const DEFAULT_DOWNLOAD_CONCURRENCY: usize = 3;

/// Maximum simultaneous chapter downloads from a single source.
pub const DEFAULT_PER_SOURCE_DOWNLOAD_CONCURRENCY: usize = 1;

/// Maximum simultaneous pages fetched within one chapter download.
pub const DEFAULT_IMAGE_FETCH_CONCURRENCY: usize = 4;

/// Maximum simultaneous source scans (library-wide refresh).
pub const DEFAULT_SCAN_CONCURRENCY: usize = 2;

/// How many completed/failed/cancelled jobs to retain before pruning.
pub const DEFAULT_JOB_MAX_HISTORY: usize = 1000;

/// Seconds to wait for running jobs to drain on graceful shutdown.
pub const DEFAULT_JOB_SHUTDOWN_TIMEOUT_SECS: u64 = 30;
