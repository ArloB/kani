-- Insert default settings row if table is empty
INSERT INTO settings (
    flaresolverr_url,
    library_path,
    wasm_storage_path,
    concurrent_page_downloads,
    chapter_queue_size,
    max_retries,
    initial_retry_delay_ms
)
SELECT 
    'http://localhost:8191',
    './library',
    './wasm_sources',
    4,
    32,
    3,
    100
WHERE NOT EXISTS (SELECT 1 FROM settings);
