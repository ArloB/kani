CREATE TABLE IF NOT EXISTS settings (
    id TEXT NOT NULL DEFAULT 'singleton' PRIMARY KEY CHECK (id = 'singleton'),
    flaresolverr_url TEXT NOT NULL DEFAULT 'http://localhost:8191',
    library_path TEXT NOT NULL DEFAULT './library',
    wasm_storage_path TEXT NOT NULL DEFAULT './wasm_sources',
    concurrent_page_downloads INTEGER NOT NULL DEFAULT 4,
    chapter_queue_size INTEGER NOT NULL DEFAULT 32,
    max_retries INTEGER NOT NULL DEFAULT 3,
    initial_retry_delay_ms INTEGER NOT NULL DEFAULT 100,
    max_wasm_instances INTEGER NOT NULL DEFAULT 1000,
    auto_scan BOOLEAN NOT NULL DEFAULT 0,
    scan_interval_minutes INTEGER NOT NULL DEFAULT 60,
    concurrent_manga_downloads INTEGER NOT NULL DEFAULT 2
);

INSERT INTO settings DEFAULT VALUES;
