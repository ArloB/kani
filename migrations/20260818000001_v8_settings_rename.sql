-- The Puppeteer browser pool these named was removed when capture moved into the
-- solver. What survives is the Node/V8 worker, so the columns take its name.
ALTER TABLE settings RENAME COLUMN browser_max_memory_mb TO v8_max_memory_mb;
ALTER TABLE settings RENAME COLUMN browser_idle_timeout_s TO v8_idle_timeout_s;
ALTER TABLE settings RENAME COLUMN browser_debug_logging TO v8_debug_logging;

-- browser_max_instances reached V8Config::max_instances, which was only ever
-- echoed back through diagnostics; nothing enforced a limit.
ALTER TABLE settings DROP COLUMN browser_max_instances;
