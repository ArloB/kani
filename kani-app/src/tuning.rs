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
