'use strict';
const vm = require('vm');
const readline = require('readline');
const contexts = new Map();

process.on('uncaughtException', (err) => {
    process.stderr.write('v8_shim uncaughtException: ' + String(err) + '\n');
});

function makeContext() {
    const { webcrypto } = require('crypto');
    const sandbox = {
        crypto: webcrypto,
        TextEncoder, TextDecoder, URL, URLSearchParams,
        setTimeout, clearTimeout, setInterval, clearInterval, queueMicrotask,
        Promise, Uint8Array, Uint16Array, Uint32Array, Int8Array, Int16Array, Int32Array,
        Float32Array, Float64Array, DataView, ArrayBuffer, SharedArrayBuffer,
        JSON, Math, Date, RegExp, Error, Object, Array, String, Number, Boolean,
        Map, Set, WeakMap, WeakSet, Symbol, BigInt,
        parseInt, parseFloat, isNaN, isFinite, encodeURIComponent, decodeURIComponent,
        encodeURI, decodeURI, atob, btoa, console,
        Proxy, Reflect,
    };
    const ctx = vm.createContext(sandbox);
    sandbox.globalThis = sandbox;
    sandbox.global = sandbox;
    sandbox.window = sandbox;
    sandbox.self = sandbox;
    return ctx;
}

const browsers = new Map();

const BROWSER_IDLE_MS = parseInt(process.env.BROWSER_IDLE_TIMEOUT_MS || '300000', 10);
const MAX_INSTANCES = Math.max(1, parseInt(process.env.BROWSER_MAX_INSTANCES || '2', 10));

const tokenCache = new Map();

function getCachedToken(key) {
    const entry = tokenCache.get(key);
    if (!entry) return null;
    if (entry.expiresAt !== null && Date.now() > entry.expiresAt) {
        tokenCache.delete(key);
        return null;
    }
    return entry.token;
}
function setCachedToken(key, token, ttlMs) {
    tokenCache.set(key, { token, expiresAt: ttlMs != null ? Date.now() + ttlMs : null });
}
function deleteCachedToken(key) { tokenCache.delete(key); }

function loadPuppeteer() {
    try { return require('puppeteer-core'); } catch (_) {}
    for (const p of ['/usr/local/lib/node_modules/puppeteer-core', '/usr/lib/node_modules/puppeteer-core']) {
        try { return require(p); } catch (_) {}
    }
    try {
        const { execSync } = require('child_process');
        const root = execSync('npm root -g', { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], timeout: 5000 }).trim();
        return require(require('path').join(root, 'puppeteer-core'));
    } catch (_) {}
    return null;
}

function findChromium() {
    if (process.env.CHROMIUM_PATH) return process.env.CHROMIUM_PATH;
    if (process.platform === 'win32') {
        const fs = require('fs');
        const winCandidates = [
            process.env.LOCALAPPDATA && `${process.env.LOCALAPPDATA}\\Google\\Chrome\\Application\\chrome.exe`,
            'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
            'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
            'C:\\Program Files\\Chromium\\Application\\chromium.exe',
        ].filter(Boolean);
        for (const p of winCandidates) {
            try { if (fs.existsSync(p)) return p; } catch (_) {}
        }
    }
    return '/usr/bin/chromium';
}

function browserKey(profileDir) { return profileDir || '__default__'; }

// Evicts the least-recently-used browser with no active pages so a new one can
// launch under the instance cap. The global serial queue guarantees at most one
// entry has activePages > 0, so an idle victim always exists when size >= 1.
async function evictLruIdle() {
    let victimKey = null;
    let victimUsed = Infinity;
    for (const [k, e] of browsers) {
        if (e.activePages === 0 && e.lastUsed < victimUsed) {
            victimUsed = e.lastUsed;
            victimKey = k;
        }
    }
    if (victimKey === null) return;
    const victim = browsers.get(victimKey);
    browsers.delete(victimKey);
    if (victim.idleTimer) clearTimeout(victim.idleTimer);
    if (victim.browser) await victim.browser.close().catch(() => {});
}

async function getPuppeteerBrowser(profileDir) {
    const key = browserKey(profileDir);
    let entry = browsers.get(key);
    if (entry && entry.browser) {
        try {
            if (entry.browser.isConnected()) {
                if (entry.idleTimer) { clearTimeout(entry.idleTimer); entry.idleTimer = null; }
                entry.lastUsed = Date.now();
                return entry;
            }
        } catch (_) {}
        browsers.delete(key);
    }

    if (browsers.size >= MAX_INSTANCES) await evictLruIdle();

    const puppeteer = loadPuppeteer();
    if (!puppeteer) throw new Error(
        'puppeteer-core not found. Install it with: npm install -g puppeteer-core\n' +
        'On Windows, also set CHROMIUM_PATH to your Chrome executable.'
    );

    const launchOpts = {
        executablePath: findChromium(),
        args: [
            '--no-sandbox',
            '--disable-setuid-sandbox',
            '--disable-dev-shm-usage',
            '--disable-gpu',
            '--no-zygote',
            '--disable-extensions',
            '--disable-background-networking',
            '--disable-background-timer-throttling',
            '--disable-backgrounding-occluded-windows',
            '--disable-renderer-backgrounding',
            '--disable-default-apps',
            '--disable-sync',
            '--disable-translate',
            '--no-first-run',
            '--disable-blink-features=AutomationControlled',
            '--js-flags=--max-old-space-size=64',
        ],
        headless: true,
    };
    if (profileDir) launchOpts.userDataDir = profileDir;

    const browser = await puppeteer.launch(launchOpts);
    entry = { browser, idleTimer: null, activePages: 0, lastUsed: Date.now() };
    browsers.set(key, entry);
    return entry;
}

async function openPage(entry) {
    entry.activePages++;
    if (entry.idleTimer) { clearTimeout(entry.idleTimer); entry.idleTimer = null; }
    return entry.browser.newPage();
}

async function closePage(entry, page) {
    await page.close().catch(() => {});
    entry.activePages = Math.max(0, entry.activePages - 1);
    if (entry.activePages === 0 && entry.browser && BROWSER_IDLE_MS > 0) {
        entry.idleTimer = setTimeout(async () => {
            for (const [k, e] of browsers) {
                if (e === entry) { browsers.delete(k); break; }
            }
            if (entry.browser) {
                await entry.browser.close().catch(() => {});
                entry.browser = null;
                process.stderr.write('v8_shim: browser closed after idle timeout\n');
            }
        }, BROWSER_IDLE_MS);
    }
}

let queue = Promise.resolve();
function enqueue(fn) { queue = queue.then(fn).catch(() => {}); }

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
rl.on('line', (line) => {
    enqueue(async () => {
        let req;
        try { req = JSON.parse(line.trim()); } catch { return; }
        const { id, action, name, script } = req;
        try {
            if (action === 'exists') {
                process.stdout.write(JSON.stringify({ id, ok: true, value: contexts.has(name) ? 'true' : 'false' }) + '\n');

            } else if (action === 'create') {
                if (!contexts.has(name)) {
                    const ctx = makeContext();
                    const result = vm.runInContext(script, ctx, { filename: name });
                    if (result && typeof result.then === 'function') await result;
                    contexts.set(name, ctx);
                }
                process.stdout.write(JSON.stringify({ id, ok: true, value: '' }) + '\n');

            } else if (action === 'eval') {
                const ctx = contexts.get(name);
                if (!ctx) throw new Error(`V8 context '${name}' not found`);
                let result = vm.runInContext(script, ctx, { filename: `${name}:eval` });
                if (result && typeof result.then === 'function') result = await result;
                process.stdout.write(JSON.stringify({ id, ok: true, value: String(result ?? '') }) + '\n');

            } else if (action === 'drop') {
                contexts.delete(name);
                process.stdout.write(JSON.stringify({ id, ok: true, value: '' }) + '\n');

            } else if (action === 'capture_token') {
                // name   = page URL to load
                // script = JSON with fields: urlPattern, paramName, headerName, timeoutMs,
                //          forceRefresh, verbose, cacheTtlMs, extraHeaders
                const params      = JSON.parse(script);
                const urlPattern  = params.urlPattern;
                const paramName   = params.paramName  ?? null;
                const headerName  = params.headerName ?? null;
                const timeoutMs   = params.timeoutMs  || 30000;
                const forceRefresh = !!params.forceRefresh;
                const verbose     = !!params.verbose;
                const cacheTtlMs  = params.cacheTtlMs ?? null;
                const extraHeaders = params.extraHeaders || {};

                if (!paramName && !headerName) {
                    throw new Error('capture_token: one of paramName or headerName must be provided');
                }

                const dbg = (msg) => { if (verbose) process.stderr.write(`[capture_token] ${msg}\n`); };

                const cacheKey = name + '|' + urlPattern + '|' + (paramName ?? '') + '|' + (headerName ?? '');
                if (!forceRefresh) {
                    const cached = getCachedToken(cacheKey);
                    if (cached) {
                        dbg(`cache hit for ${name}`);
                        process.stdout.write(JSON.stringify({ id, ok: true, value: cached }) + '\n');
                        return;
                    }
                } else {
                    deleteCachedToken(cacheKey);
                }

                const pageHost = (() => { try { return new URL(name).hostname; } catch (_) { return ''; } })();

                dbg(`loading page: ${name}`);
                dbg(`waiting for pattern: ${urlPattern}`);
                if (headerName) dbg(`extracting header: ${headerName}`);
                else dbg(`extracting param: ${paramName}`);

                const entry = await getPuppeteerBrowser(params.profileDir);
                const page = await openPage(entry);

                try {
                    page.on('console', msg => dbg(`page console [${msg.type()}]: ${msg.text()}`));
                    page.on('pageerror', err => dbg(`page error: ${err}`));

                    const extraHeaderKeys = Object.keys(extraHeaders);
                    if (extraHeaderKeys.length > 0) {
                        await page.setExtraHTTPHeaders(extraHeaders);
                        dbg(`set ${extraHeaderKeys.length} extra request header(s)`);
                    }

                    await page.setRequestInterception(true);
                    let resolved = false;
                    const seenUrls = [];

                    const tokenPromise = new Promise((resolve, reject) => {
                        const timer = setTimeout(() => {
                            if (!resolved) {
                                resolved = true;
                                dbg(`timeout after ${timeoutMs}ms — URLs seen (${seenUrls.length}):`);
                                seenUrls.forEach(u => dbg(`  ${u}`));
                                reject(new Error(`Token not captured within ${timeoutMs}ms from: ${name}`));
                            }
                        }, timeoutMs);

                        page.on('request', req => {
                            if (resolved) { req.abort().catch(() => {}); return; }

                            const type = req.resourceType();
                            if (type === 'image' || type === 'font' || type === 'media' || type === 'stylesheet') {
                                req.abort().catch(() => {});
                                return;
                            }

                            const u = req.url();
                            seenUrls.push(u);

                            if (u.includes(urlPattern)) {
                                dbg(`PATTERN MATCH: ${u}`);
                                try {
                                    let token = null;
                                    if (headerName) {
                                        token = req.headers()[headerName.toLowerCase()] ?? null;
                                        if (token) dbg(`header '${headerName}' captured (len=${token.length})`);
                                        else dbg(`pattern matched but header '${headerName}' missing`);
                                    } else {
                                        token = new URL(u).searchParams.get(paramName);
                                        if (token) dbg(`param '${paramName}' captured (len=${token.length})`);
                                        else dbg(`pattern matched but param '${paramName}' missing in: ${u}`);
                                    }
                                    if (token) {
                                        resolved = true;
                                        clearTimeout(timer);
                                        resolve(token);
                                        req.abort().catch(() => {});
                                        return;
                                    }
                                } catch (_) {}
                            }

                            // Allow requests to the same host as the page; block everything else
                            // to reduce unnecessary network traffic.
                            let allow = false;
                            try {
                                const host = new URL(u).hostname;
                                if (pageHost && (host === pageHost || host.endsWith('.' + pageHost))) allow = true;
                            } catch (_) {}
                            if (!allow) dbg(`blocking off-origin request: ${u}`);
                            (allow ? req.continue() : req.abort()).catch(() => {});
                        });
                    });

                    // Start navigation but don't await it — we only need the first
                    // matching API request, which fires well before the page fully loads.
                    page.goto(name, { timeout: timeoutMs + 5000 }).catch(e => dbg(`goto error: ${e}`));

                    const token = await tokenPromise;
                    dbg(`success, caching token`);
                    setCachedToken(cacheKey, token, cacheTtlMs);
                    process.stdout.write(JSON.stringify({ id, ok: true, value: token }) + '\n');
                } finally {
                    await closePage(entry, page);
                }

            } else if (action === 'capture_page_payload') {
                const params = JSON.parse(script);
                const initScript = params.initScript || '';
                const timeoutMs  = params.timeoutMs  || 30000;
                const verbose    = !!params.verbose;

                const dbg = (msg) => { if (verbose) process.stderr.write(`[capture_page_payload] ${msg}\n`); };

                dbg(`loading page: ${name}`);
                dbg(`timeout: ${timeoutMs}ms`);
                dbg(`init script length: ${initScript.length} chars`);

                const entry = await getPuppeteerBrowser(params.profileDir);
                const page = await openPage(entry);
                let scrollInterval = null;

                try {
                    await page.setViewport({ width: 1280, height: 8000 });

                    if (verbose) {
                        page.on('console', msg => dbg(`page console [${msg.type()}]: ${msg.text()}`));
                        page.on('pageerror', err => dbg(`page error: ${err}`));
                    }

                    let payloadResolve, payloadReject;
                    const payloadPromise = new Promise((res, rej) => {
                        payloadResolve = res;
                        payloadReject  = rej;
                    });

                    let timer = null;
                    const armTimer = () => {
                        if (timer) clearTimeout(timer);
                        timer = setTimeout(() => {
                            payloadReject(new Error(`Payload not captured within ${timeoutMs}ms from: ${name}`));
                        }, timeoutMs);
                    };
                    armTimer();

                    await page.exposeFunction('passPayload', (data) => {
                        dbg(`passPayload called (${String(data).length} chars)`);
                        if (timer) clearTimeout(timer);
                        timer = null;
                        payloadResolve(String(data));
                    });

                    await page.exposeFunction('resetPayloadTimer', () => {
                        dbg('resetPayloadTimer called');
                        if (timer) armTimer();
                    });

                    if (initScript) {
                        await page.evaluateOnNewDocument(initScript);
                        dbg('evaluateOnNewDocument registered');
                    }

                    await page.setRequestInterception(true);
                    page.on('request', req => {
                        const type = req.resourceType();
                        if (type === 'image' || type === 'font' || type === 'media') {
                            req.abort().catch(() => {});
                        } else {
                            dbg(`request [${type}]: ${req.url().slice(0, 120)}`);
                            req.continue().catch(() => {});
                        }
                    });

                    dbg('starting navigation');
                    page.goto(name, { timeout: timeoutMs + 5000 }).catch(e => {
                        dbg(`goto error: ${e}`);
                    });

                    scrollInterval = setInterval(async () => {
                        try {
                            await page.evaluate(() => {
                                window.scrollTo(0, document.body.scrollHeight);
                                window.dispatchEvent(new Event('scroll', { bubbles: true }));
                            });
                        } catch (_) {}
                    }, 1500);

                    const payload = await payloadPromise;
                    dbg(`payload captured (${payload.length} chars)`);
                    process.stdout.write(JSON.stringify({ id, ok: true, value: payload }) + '\n');
                } finally {
                    if (scrollInterval) clearInterval(scrollInterval);
                    await closePage(entry, page);
                }

            } else {
                throw new Error(`Unknown action: ${action}`);
            }
        } catch (e) {
            process.stdout.write(JSON.stringify({ id, ok: false, error: String(e) }) + '\n');
        }
    });
});

process.stdout.write(JSON.stringify({ ready: true }) + '\n');
