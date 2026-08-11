'use strict';
const vm = require('vm');
const readline = require('readline');
const fs = require('fs');
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
let nextBrowserEntryId = 1;
let browserReusesPending = 0;
let recoveryLaunchesPending = 0;
let challengesPending = 0;
let pageCloseTimeoutsPending = 0;
let gracefulShutdownsPending = 0;
let forcedTerminationsPending = 0;

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
    if (process.env.KANI_PUPPETEER_MODULE) {
        try { return require(process.env.KANI_PUPPETEER_MODULE); } catch (_) {}
    }
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

function browserLog(enabled, message) {
    if (enabled) process.stderr.write(`[browser] node=${process.pid} ${message}\n`);
}

function browserPid(browser) {
    try { return browser.process()?.pid ?? null; } catch (_) { return null; }
}

function browserMajor(version) {
    const match = String(version || '').match(/(?:Chrome|Chromium)\/(\d+)/i);
    return match ? Number(match[1]) : null;
}

function actionError(code, message, details = {}) {
    const error = new Error(message);
    error.kaniActionError = { code, message, ...details };
    return error;
}

async function settleWithin(promise, timeoutMs) {
    let timer;
    const timeout = new Promise(resolve => {
        timer = setTimeout(() => resolve(false), timeoutMs);
    });
    try {
        return await Promise.race([
            Promise.resolve(promise).then(() => 'resolved', () => 'rejected'),
            timeout,
        ]);
    } finally {
        if (timer) clearTimeout(timer);
    }
}

async function waitForProcessExit(processHandle, timeoutMs) {
    if (!processHandle || processHandle.exitCode !== null) return true;
    if (typeof processHandle.once !== 'function') return false;
    const status = await settleWithin(new Promise(resolve => {
        processHandle.once('exit', resolve);
        processHandle.once('close', resolve);
    }), timeoutMs);
    return status === 'resolved' || processHandle.exitCode !== null;
}

async function cleanupRecoveryProfile(entry) {
    if (!entry.recovery || !entry.effectiveProfileDir || !entry.canonicalProfileDir) return;
    if (!entry.processStopped) {
        browserLog(entry.debug, `entry=${entry.id} recovery cleanup deferred because Chromium exit is unconfirmed`);
        return;
    }
    const expectedPath = `${entry.canonicalProfileDir}-recovery-${process.pid}`;
    if (entry.effectiveProfileDir !== expectedPath) return;
    try {
        await fs.promises.rm(entry.effectiveProfileDir, { recursive: true, force: true, maxRetries: 2 });
        browserLog(entry.debug, `entry=${entry.id} recovery profile removed path=${entry.effectiveProfileDir}`);
    } catch (err) {
        browserLog(entry.debug, `entry=${entry.id} recovery profile cleanup failed: ${String(err)}`);
    }
}

async function retireBrowserEntry(key, entry, reason) {
    if (!entry) return;
    if (entry.retirePromise) return entry.retirePromise;
    entry.retirePromise = (async () => {
        if (browsers.get(key) === entry) browsers.delete(key);
        if (entry.idleTimer) { clearTimeout(entry.idleTimer); entry.idleTimer = null; }
        browserLog(entry.debug, `entry=${entry.id} chromium=${entry.chromiumPid ?? 'unknown'} closing reason=${reason}`);
        if (entry.browser) {
            const closeStatus = await settleWithin(entry.browser.close(), 2000);
            const processHandle = (() => { try { return entry.browser.process(); } catch (_) { return null; } })();
            let stopped = await waitForProcessExit(processHandle, closeStatus === 'resolved' ? 250 : 0);
            if (!stopped && processHandle) {
                forcedTerminationsPending++;
                try { processHandle.kill('SIGKILL'); } catch (_) {}
                stopped = await waitForProcessExit(processHandle, 500);
            } else if (closeStatus === 'resolved') {
                gracefulShutdownsPending++;
            }
            entry.processStopped = stopped;
        }
        entry.browser = null;
        await cleanupRecoveryProfile(entry);
    })();
    return entry.retirePromise;
}

async function closeAllBrowsers(reason) {
    const entries = Array.from(browsers.entries());
    for (const [key, entry] of entries) await retireBrowserEntry(key, entry, reason);
    browsers.clear();
}

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
    await retireBrowserEntry(victimKey, victim, 'lru-eviction');
}

async function getPuppeteerBrowser(profileDir, verbose = false, userAgent = null) {
    const key = browserKey(profileDir);
    let entry = browsers.get(key);
    if (entry && entry.browser) {
        entry.debug = entry.debug || verbose;
        try {
            if (entry.browser.isConnected()) {
                if (entry.userAgentOverride && userAgent && entry.userAgentOverride !== userAgent) {
                    await retireBrowserEntry(key, entry, 'user-agent-change');
                    entry = null;
                }
            }
            if (entry?.browser?.isConnected()) {
                if (entry.idleTimer) { clearTimeout(entry.idleTimer); entry.idleTimer = null; }
                entry.lastUsed = Date.now();
                if (userAgent) entry.userAgentOverride = userAgent;
                browserReusesPending++;
                browserLog(entry.debug, `entry=${entry.id} chromium=${entry.chromiumPid ?? 'unknown'} reused profile=${key}`);
                return entry;
            }
        } catch (_) {}
        await retireBrowserEntry(key, entry, 'disconnected');
    }

    if (browsers.size >= MAX_INSTANCES) await evictLruIdle();

    const puppeteer = loadPuppeteer();
    if (!puppeteer) throw new Error(
        'puppeteer-core not found. Install it with: npm install -g puppeteer-core\n' +
        'On Windows, also set CHROMIUM_PATH to your Chrome executable.'
    );

    const launchArgs = [
        '--disable-blink-features=AutomationControlled',
        '--js-flags=--max-old-space-size=64',
        '--window-size=1365,768',
    ];
    if (process.platform === 'linux') {
        launchArgs.push('--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage');
    }
    const launchOpts = {
        executablePath: findChromium(),
        args: launchArgs,
        headless: true,
    };
    if (profileDir) launchOpts.userDataDir = profileDir;

    let browser;
    let effectiveProfileDir = profileDir || null;
    let recovery = false;
    try {
        browser = await puppeteer.launch(launchOpts);
    } catch (err) {
        // A previous worker can leave Chromium alive while its profile lock is
        // still held. Keep the stable profile as the first choice, then use a
        // process-scoped recovery profile instead of failing the request.
        const message = String(err && err.message || err);
        if (!profileDir || !/already running|user.?data.?dir/i.test(message)) throw err;
        const recoveryDir = `${profileDir}-recovery-${process.pid}`;
        launchOpts.userDataDir = recoveryDir;
        effectiveProfileDir = recoveryDir;
        recovery = true;
        recoveryLaunchesPending++;
        browserLog(true, `canonical profile locked profile=${profileDir}; recovery=${recoveryDir}`);
        browser = await puppeteer.launch(launchOpts);
    }
    entry = {
        id: nextBrowserEntryId++, browser, idleTimer: null, activePages: 0,
        lastUsed: Date.now(), canonicalProfileDir: profileDir || null,
        effectiveProfileDir, recovery, chromiumPid: browserPid(browser),
        retirePromise: null, debug: verbose, userAgentOverride: userAgent || null,
    };
    const productVersion = typeof browser.version === 'function'
        ? await browser.version().catch(() => 'unknown')
        : 'unknown';
    entry.productVersion = productVersion;
    const actualMajor = browserMajor(productVersion);
    const expectedMajor = Number(String(puppeteer.PUPPETEER_REVISIONS?.chrome || '').split('.')[0]) || null;
    if (actualMajor && expectedMajor && expectedMajor - actualMajor >= 4) {
        browserLog(true, `entry=${entry.id} browser=${productVersion} is materially older than Puppeteer's expected Chrome ${expectedMajor}`);
    }
    browser.on('disconnected', () => {
        browserLog(entry.debug, `entry=${entry.id} chromium=${entry.chromiumPid ?? 'unknown'} disconnected`);
        enqueue(() => retireBrowserEntry(key, entry, 'disconnected'));
    });
    browsers.set(key, entry);
    browserLog(entry.debug, `entry=${entry.id} chromium=${entry.chromiumPid ?? 'unknown'} product=${productVersion} launched profile=${effectiveProfileDir ?? '__default__'} recovery=${recovery}`);
    return entry;
}

async function openPage(entry) {
    if (entry.idleTimer) { clearTimeout(entry.idleTimer); entry.idleTimer = null; }
    const page = await entry.browser.newPage();
    if (entry.userAgentOverride) await page.setUserAgent(entry.userAgentOverride);
    entry.activePages++;
    browserLog(entry.debug, `entry=${entry.id} page opened active=${entry.activePages}`);
    return page;
}

async function closePage(entry, page) {
    const closeStatus = await settleWithin(page.close(), 1000);
    entry.activePages = Math.max(0, entry.activePages - 1);
    entry.lastUsed = Date.now();
    browserLog(entry.debug, `entry=${entry.id} page closed active=${entry.activePages}`);
    if (closeStatus !== 'resolved') {
        if (closeStatus === false) pageCloseTimeoutsPending++;
        const key = Array.from(browsers.entries()).find(([, e]) => e === entry)?.[0];
        if (key !== undefined) await retireBrowserEntry(
            key,
            entry,
            closeStatus === false ? 'page-close-timeout' : 'page-close-error'
        );
        return closeStatus === false ? 'timeout' : 'error';
    }
    if (entry.activePages === 0 && entry.browser && BROWSER_IDLE_MS > 0) {
        entry.idleTimer = setTimeout(() => enqueue(async () => {
            entry.idleTimer = null;
            if (entry.activePages !== 0 || Date.now() - entry.lastUsed < BROWSER_IDLE_MS) return;
            const key = Array.from(browsers.entries()).find(([, e]) => e === entry)?.[0];
            if (key !== undefined) await retireBrowserEntry(key, entry, 'browser-idle-timeout');
        }), BROWSER_IDLE_MS);
    }
    return 'closed';
}

let queue = Promise.resolve();
function enqueue(fn) { queue = queue.then(fn).catch(() => {}); }

function respond(id, ok, value, error, callback) {
    const metrics = {
        browserReuses: browserReusesPending,
        recoveryLaunches: recoveryLaunchesPending,
        challenges: challengesPending,
        pageCloseTimeouts: pageCloseTimeoutsPending,
        gracefulShutdowns: gracefulShutdownsPending,
        forcedTerminations: forcedTerminationsPending,
    };
    browserReusesPending = 0;
    recoveryLaunchesPending = 0;
    challengesPending = 0;
    pageCloseTimeoutsPending = 0;
    gracefulShutdownsPending = 0;
    forcedTerminationsPending = 0;
    const response = ok ? { id, ok, value, metrics } : { id, ok, error, metrics };
    process.stdout.write(JSON.stringify(response) + '\n', callback);
}

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
rl.on('line', (line) => {
    enqueue(async () => {
        let req;
        try { req = JSON.parse(line.trim()); } catch { return; }
        const { id, action, name, script } = req;
        try {
            if (action === 'shutdown') {
                await closeAllBrowsers(`worker-shutdown:${name || 'unspecified'}`);
                contexts.clear();
                tokenCache.clear();
                respond(id, true, '', null, () => rl.close());

            } else if (action === 'exists') {
                respond(id, true, contexts.has(name) ? 'true' : 'false', null);

            } else if (action === 'create') {
                if (!contexts.has(name)) {
                    const ctx = makeContext();
                    const result = vm.runInContext(script, ctx, { filename: name });
                    if (result && typeof result.then === 'function') await result;
                    contexts.set(name, ctx);
                }
                respond(id, true, '', null);

            } else if (action === 'eval') {
                const ctx = contexts.get(name);
                if (!ctx) throw new Error(`V8 context '${name}' not found`);
                let result = vm.runInContext(script, ctx, { filename: `${name}:eval` });
                if (result && typeof result.then === 'function') result = await result;
                respond(id, true, String(result ?? ''), null);

            } else if (action === 'drop') {
                contexts.delete(name);
                respond(id, true, '', null);

            } else if (action === 'browser_probe') {
                const params = JSON.parse(script);
                const entry = await getPuppeteerBrowser(params.profileDir, !!params.verbose);
                const page = await openPage(entry);
                await closePage(entry, page);
                respond(id, true, JSON.stringify({
                    entryId: entry.id,
                    chromiumPid: entry.chromiumPid,
                    recovery: entry.recovery,
                }), null);

            } else if (action === 'browser_disconnect') {
                const params = JSON.parse(script);
                const key = browserKey(params.profileDir);
                const entry = browsers.get(key);
                if (entry) await retireBrowserEntry(key, entry, 'test-disconnect');
                respond(id, true, entry ? 'true' : 'false', null);

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
                        respond(id, true, cached, null);
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

                const entry = await getPuppeteerBrowser(params.profileDir, verbose);
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
                    respond(id, true, token, null);
                } finally {
                    await closePage(entry, page);
                }

            } else if (action === 'capture_page_payload') {
                const params = JSON.parse(script);
                const initScript = params.initScript || '';
                const timeoutMs  = params.timeoutMs  || 30000;
                const verbose    = !!params.verbose;
                const autoScroll = params.autoScroll !== false;
                const cookieHeader = params.cookieHeader || '';
                const userAgent = params.userAgent || null;
                const challengeGraceMs = params.challengeGraceMs
                    || parseInt(process.env.KANI_BROWSER_CHALLENGE_GRACE_MS || '8000', 10);

                const dbg = (msg) => { if (verbose) process.stderr.write(`[capture_page_payload] ${msg}\n`); };

                dbg(`loading page: ${name}`);
                dbg(`timeout: ${timeoutMs}ms`);
                dbg(`init script length: ${initScript.length} chars`);

                const entry = await getPuppeteerBrowser(params.profileDir, verbose, userAgent);
                const page = await openPage(entry);
                let scrollInterval = null;
                let challengeTimer = null;
                let timer = null;
                let settled = false;
                let capturedPayload = null;
                let cleanupStatus = 'closed';
                let disconnectedHandler = null;
                const connectedBrowser = entry.browser;

                try {
                    await page.setViewport({ width: 1365, height: 768, deviceScaleFactor: 1 });

                    if (cookieHeader) {
                        const stale = await page.cookies(name).catch(() => []);
                        const cloudflareCookies = stale.filter(cookie => /^(cf_|__cf|cf_clearance)/i.test(cookie.name));
                        if (cloudflareCookies.length) await page.deleteCookie(...cloudflareCookies).catch(() => {});
                        const origin = new URL(name).origin;
                        const cookies = cookieHeader.split(';').map(part => part.trim()).filter(Boolean).map(part => {
                            const separator = part.indexOf('=');
                            if (separator <= 0) return null;
                            return { name: part.slice(0, separator), value: part.slice(separator + 1), url: origin };
                        }).filter(Boolean);
                        if (cookies.length) await page.setCookie(...cookies);
                    }

                    if (verbose) {
                        page.on('console', msg => dbg(`page console [${msg.type()}]: ${msg.text()}`));
                        page.on('pageerror', err => dbg(`page error: ${err}`));
                    }

                    let payloadResolve, payloadReject;
                    const payloadPromise = new Promise((res, rej) => {
                        payloadResolve = res;
                        payloadReject  = rej;
                    });
                    disconnectedHandler = () => {
                        if (settled) return;
                        settled = true;
                        payloadReject(actionError(
                            'browser_disconnected',
                            'Chromium disconnected during browser payload capture',
                            { url: name }
                        ));
                    };
                    connectedBrowser.once('disconnected', disconnectedHandler);
                    page.once('error', error => {
                        if (settled) return;
                        settled = true;
                        payloadReject(actionError(
                            'page_crashed',
                            `Browser page crashed: ${String(error && error.message || error)}`,
                            { url: name }
                        ));
                    });

                    const armTimer = () => {
                        if (timer) clearTimeout(timer);
                        timer = setTimeout(() => {
                            if (settled) return;
                            settled = true;
                            payloadReject(actionError('capture_timeout', `Payload not captured within ${timeoutMs}ms from: ${name}`, { url: name }));
                        }, timeoutMs);
                    };
                    armTimer();

                    const clearChallenge = () => {
                        if (challengeTimer) clearTimeout(challengeTimer);
                        challengeTimer = null;
                    };
                    const scheduleChallenge = (url, status = null) => {
                        if (settled || challengeTimer) return;
                        challengeTimer = setTimeout(() => {
                            if (settled) return;
                            settled = true;
                            challengesPending++;
                            payloadReject(actionError(
                                'browser_challenge',
                                'Cloudflare managed challenge did not resolve automatically',
                                { url, status }
                            ));
                        }, challengeGraceMs);
                    };

                    await page.exposeFunction('passPayload', (data) => {
                        if (settled) return;
                        settled = true;
                        dbg(`passPayload called (${String(data).length} chars)`);
                        if (timer) clearTimeout(timer);
                        timer = null;
                        clearChallenge();
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

                    page.on('request', req => {
                        const type = req.resourceType();
                        dbg(`request [${type}]: ${req.url().slice(0, 120)}`);
                    });
                    page.on('response', response => {
                        const request = response.request();
                        if (!request.isNavigationRequest() || response.frame() !== page.mainFrame()) return;
                        const status = response.status();
                        const url = response.url();
                        if ([403, 429, 503].includes(status) || /\/cdn-cgi\/challenge-platform/i.test(url)) {
                            scheduleChallenge(url, status);
                        } else if (status >= 200 && status < 400 && !/challenges\.cloudflare\.com/i.test(url)) {
                            clearChallenge();
                        }
                    });
                    page.on('framenavigated', frame => {
                        const url = frame.url();
                        if (/challenges\.cloudflare\.com|\/cdn-cgi\/challenge-platform/i.test(url)) {
                            scheduleChallenge(url, null);
                        }
                    });
                    page.on('domcontentloaded', async () => {
                        try {
                            const challenge = await page.evaluate(() => {
                                const title = document.title || '';
                                return /just a moment|attention required/i.test(title)
                                    || !!document.querySelector('#challenge-running, #challenge-stage, iframe[src*="challenges.cloudflare.com"]');
                            });
                            if (challenge) scheduleChallenge(page.url(), null);
                        } catch (_) {}
                    });

                    dbg('starting navigation');
                    page.goto(name, { timeout: timeoutMs + 5000 }).catch(e => {
                        dbg(`goto error: ${e}`);
                        if (settled) return;
                        settled = true;
                        payloadReject(actionError(
                            'navigation_error',
                            `Browser navigation failed: ${String(e && e.message || e)}`,
                            { url: name }
                        ));
                    });

                    if (autoScroll) scrollInterval = setInterval(async () => {
                        try {
                            await page.evaluate(() => {
                                window.scrollTo(0, document.body.scrollHeight);
                                window.dispatchEvent(new Event('scroll', { bubbles: true }));
                            });
                        } catch (_) {}
                    }, 1500);

                    capturedPayload = await payloadPromise;
                } finally {
                    if (scrollInterval) clearInterval(scrollInterval);
                    if (challengeTimer) clearTimeout(challengeTimer);
                    if (timer) clearTimeout(timer);
                    if (disconnectedHandler) connectedBrowser.off('disconnected', disconnectedHandler);
                    cleanupStatus = await closePage(entry, page);
                }
                if (cleanupStatus !== 'closed') {
                    throw actionError(
                        cleanupStatus === 'timeout' ? 'page_cleanup_timeout' : 'page_cleanup_error',
                        cleanupStatus === 'timeout'
                            ? 'Browser page cleanup exceeded one second; Chromium was retired'
                            : 'Browser page cleanup failed; Chromium was retired',
                        { url: name }
                    );
                }
                dbg(`payload captured (${capturedPayload.length} chars)`);
                respond(id, true, capturedPayload, null);

            } else {
                throw new Error(`Unknown action: ${action}`);
            }
        } catch (e) {
            respond(id, false, null, e && e.kaniActionError ? e.kaniActionError : {
                code: 'action_error',
                message: String(e),
            });
        }
    });
});

rl.on('close', () => {
    enqueue(async () => {
        await closeAllBrowsers('stdin-closed');
    });
});

process.stdout.write(JSON.stringify({ ready: true }) + '\n');
