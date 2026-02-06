CREATE TABLE IF NOT EXISTS settings (
    flaresolverr_url TEXT NOT NULL,
    library_path TEXT NOT NULL,
    wasm_storage_path TEXT NOT NULL,
    concurrent_page_downloads INTEGER NOT NULL DEFAULT 4,
    chapter_queue_size INTEGER NOT NULL DEFAULT 32,
    max_retries INTEGER NOT NULL DEFAULT 3,
    initial_retry_delay_ms INTEGER NOT NULL DEFAULT 100
);
