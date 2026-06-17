//! Integration tests that load `kani-test-abi.wasm` through the full WasmRuntime
//! stack and call each WIT export to verify host ABI correctness end-to-end.
//!
//! These tests are **skipped** (with an explanatory message) when the WASM binary
//! has not been built yet. Build it first:
//!
//!   cargo run -p kani-cli -- build kani-test-abi
//!
//! Then run the tests:
//!
//!   cargo test -p kani-core --test wasm_abi

#![allow(clippy::unwrap_used)]
#![allow(clippy::pedantic)]

use kani_core::wasm::WasmRuntime;

// ── helpers ───────────────────────────────────────────────────────────────────

fn workspace_root() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().to_path_buf()
}

fn load_wasm(name: &str) -> Option<Vec<u8>> {
    let path = workspace_root()
        .join("wasm_sources")
        .join(format!("{}.wasm", name));
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!(
                "\n[SKIP] wasm_sources/{name}.wasm not found.\n\
                 Build it with: cargo run -p kani-cli -- build {name}\n"
            );
            None
        }
    }
}

/// A generous tick budget so the epoch watchdog never fires in tests
/// (no background thread calls engine.increment_epoch()).
const EPOCH_TICKS: u64 = 50_000;

macro_rules! skip_if_missing {
    ($name:expr) => {{
        let Some(bytes) = load_wasm($name) else {
            return;
        };
        bytes
    }};
}

async fn make_instance(
    bytes: &[u8],
) -> (
    WasmRuntime,
    wasmtime::Store<kani_core::wasm::HostState>,
    kani_core::wasm::KaniExtension,
) {
    let rt = WasmRuntime::new(1).unwrap();
    let component = rt.compile_component(bytes).unwrap();
    let mut store = rt.create_store();
    store.set_epoch_deadline(EPOCH_TICKS);
    let instance = rt.instantiate(&mut store, &component).await.unwrap();
    (rt, store, instance)
}

// ── test_abi: html host imports ───────────────────────────────────────────────

#[tokio::test]
async fn abi_html_imports_return_extracted_attr_and_text() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_popular_manga(&mut store, 1, 20, &[])
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert_eq!(result.manga.len(), 1, "html test should return 1 item");
    let m = &result.manga[0];
    // id = data-id attribute ("42"), title = text of .title child ("Hello World")
    assert_eq!(m.id, "42", "html attr extraction");
    assert_eq!(m.title, "Hello World", "html text extraction");
    assert_eq!(m.cover_url.as_deref(), Some("html-ok"), "html sentinel");
}

// ── test_abi: json host imports ───────────────────────────────────────────────

#[tokio::test]
async fn abi_json_imports_return_extracted_fields() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_popular_manga(&mut store, 2, 20, &[])
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert_eq!(result.manga.len(), 1, "json test should return 1 item");
    let m = &result.manga[0];
    // id = get_str("/name"), title = get_i64("/age").to_string()
    assert_eq!(m.id, "Alice", "json get_str");
    assert_eq!(m.title, "30", "json get_i64");
    assert_eq!(
        m.cover_url.as_deref(),
        Some("json-ok"),
        "json sentinel: active/score/tags/keys all verified"
    );
}

// ── test_abi: utility host imports ───────────────────────────────────────────

#[tokio::test]
async fn abi_utility_imports_return_decoded_string_and_query_param() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_popular_manga(&mut store, 3, 20, &[])
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert_eq!(result.manga.len(), 1, "utility test should return 1 item");
    let m = &result.manga[0];
    assert_eq!(m.id, "util-ok", "utility sentinel");
    // title = url_decode("hello%20world") = "hello world"
    assert_eq!(m.title, "hello world", "url_encode+decode round-trip");
    // cover_url = get_query_param(built_url, "pg") = "5"
    assert_eq!(
        m.cover_url.as_deref(),
        Some("5"),
        "get_query_param from build_url result"
    );
}

// ── test_abi: prefs host imports ──────────────────────────────────────────────

#[tokio::test]
async fn abi_prefs_get_value_returns_injected_preferences() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    {
        let data = store.data_mut();
        data.preferences
            .insert("test_str".into(), "injected_value".into());
        data.preferences.insert("test_bool".into(), "true".into());
        data.preferences.insert("test_i64".into(), "42".into());
        data.preferences.insert("test_f64".into(), "3.14".into());
    }

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_search_manga(&mut store, "prefs", 1, 20, &[])
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert_eq!(result.manga.len(), 1, "prefs test should return 1 item");
    let m = &result.manga[0];
    // id = prefs::get_str("test_str") = "injected_value"
    assert_eq!(m.id, "injected_value", "prefs get_str");
    // title = prefs::get_bool("test_bool").to_string() = "true"
    assert_eq!(m.title, "true", "prefs get_bool");
    assert_eq!(
        m.cover_url.as_deref(),
        Some("prefs-ok"),
        "prefs sentinel: raw/missing checks passed"
    );
}

// ── test_abi: extraction::extract_html ───────────────────────────────────────

#[tokio::test]
async fn abi_extract_html_returns_rows_from_blueprint() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_search_manga(&mut store, "extract-html", 1, 20, &[])
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert_eq!(result.manga.len(), 2, "extract_html should return 2 rows");
    assert_eq!(result.manga[0].id, "1");
    assert_eq!(result.manga[0].title, "Alpha");
    assert_eq!(result.manga[1].id, "2");
    assert_eq!(result.manga[1].title, "Beta");
}

// ── test_abi: extraction::extract_json ───────────────────────────────────────

#[tokio::test]
async fn abi_extract_json_returns_rows_from_blueprint() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_search_manga(&mut store, "extract-json", 1, 20, &[])
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert_eq!(result.manga.len(), 2, "extract_json should return 2 rows");
    assert_eq!(result.manga[0].id, "j1");
    assert_eq!(result.manga[0].title, "JsonAlpha");
    assert_eq!(result.manga[1].id, "j2");
    assert_eq!(result.manga[1].title, "JsonBeta");
}

// ── test_abi: error paths ─────────────────────────────────────────────────────

#[tokio::test]
async fn abi_invalid_handles_return_errors() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_manga_details(&mut store, "error-paths")
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert_eq!(
        result.description.as_deref(),
        Some("error-paths-ok"),
        "all invalid-handle ABI calls must return Err"
    );
}

// ── test_abi: get_metadata ────────────────────────────────────────────────────

#[tokio::test]
async fn abi_get_metadata_returns_correct_extension_info() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let raw_meta = provider
        .call_get_metadata(&mut store)
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");
    let meta: kani_shared::ExtensionMetadata =
        serde_json::from_str(&raw_meta).expect("metadata is valid JSON");

    assert_eq!(meta.id, "test-abi");
    assert_eq!(meta.name, "TestAbi");
    assert!(!meta.nsfw);
    assert!(!meta.unrestricted_http);
    assert_eq!(meta.language, "en");
}

// ── test_abi: empty returns (other pages/queries) ─────────────────────────────

#[tokio::test]
async fn abi_unknown_page_returns_empty_list() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_popular_manga(&mut store, 99, 20, &[])
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert!(result.manga.is_empty());
}

#[tokio::test]
async fn abi_unknown_query_returns_empty_list() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_search_manga(&mut store, "unknown", 1, 20, &[])
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert!(result.manga.is_empty());
}

#[tokio::test]
async fn abi_get_chapter_list_returns_empty() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_chapter_list(&mut store, "any", 1, None, None)
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert!(result.chapters.is_empty());
    assert!(!result.has_next_page);
}

#[tokio::test]
async fn abi_get_filter_list_returns_empty() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_filter_list(&mut store)
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert!(result.filters.is_empty());
}

#[tokio::test]
async fn abi_get_preferences_returns_empty() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_preferences(&mut store)
        .await
        .expect("WASM call failed")
        .expect("extension returned Err");

    assert!(result.is_empty());
}

// ── test_abi: handle cleanup (no leaks between calls) ────────────────────────

#[tokio::test]
async fn abi_handles_are_cleaned_up_after_each_call() {
    let bytes = skip_if_missing!("kani-test-abi");
    let (_rt, mut store, instance) = make_instance(&bytes).await;

    let provider = instance.kani_extension_manga_provider();

    // Run the HTML test (allocates several handles internally)
    provider
        .call_get_popular_manga(&mut store, 1, 20, &[])
        .await
        .unwrap()
        .unwrap();

    // After the call, no handles should remain (the extension drops them all)
    let state = store.data();
    assert!(
        state.html_docs.is_empty(),
        "html_docs should be empty after HTML test: {:?}",
        state.html_docs.keys().collect::<Vec<_>>()
    );
    assert!(
        state.html_lists.is_empty(),
        "html_lists should be empty after HTML test"
    );

    provider
        .call_get_popular_manga(&mut store, 2, 20, &[])
        .await
        .unwrap()
        .unwrap();

    assert!(
        store.data().json_docs.is_empty(),
        "json_docs should be empty after JSON test"
    );
}
