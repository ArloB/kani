//! End-to-end integration test that loads `kani-example.wasm` through the full
//! WasmRuntime stack, verifying the compile → instantiate → call lifecycle.
//!
//! `kani-example` is excluded from `--all` builds, so you must build it explicitly:
//!
//!   cargo run -p kani-cli -- build kani-example
//!
//! Then run:
//!
//!   cargo test -p kani-core --test wasm_example

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

const EPOCH_TICKS: u64 = 50_000;

macro_rules! skip_if_missing {
    ($name:expr) => {{
        let Some(bytes) = load_wasm($name) else {
            return;
        };
        bytes
    }};
}

async fn example_instance() -> (
    WasmRuntime,
    wasmtime::Store<kani_core::wasm::HostState>,
    kani_core::wasm::KaniExtension,
) {
    let bytes = load_wasm("kani-example").expect("kani-example.wasm must exist for this helper");
    let rt = WasmRuntime::new(1).unwrap();
    let component = rt.compile_component(&bytes).unwrap();
    let mut store = rt.create_store();
    store.set_epoch_deadline(EPOCH_TICKS);
    let instance = rt.instantiate(&mut store, &component).await.unwrap();
    (rt, store, instance)
}

// ── lifecycle: compile + instantiate ─────────────────────────────────────────

#[tokio::test]
async fn example_wasm_compiles_and_instantiates() {
    let bytes = skip_if_missing!("kani-example");
    let rt = WasmRuntime::new(1).unwrap();
    let component = rt.compile_component(&bytes).unwrap();
    let mut store = rt.create_store();
    store.set_epoch_deadline(EPOCH_TICKS);
    // Just instantiate — no call needed; this exercises the full runtime init path.
    let _instance = rt.instantiate(&mut store, &component).await.unwrap();
}

// ── get_metadata ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_metadata_returns_correct_fields() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let meta = provider
        .call_get_metadata(&mut store)
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert_eq!(meta.id, "example");
    assert_eq!(meta.name, "Example");
    assert_eq!(meta.language, "multi");
    assert!(!meta.nsfw);
    assert!(!meta.unrestricted_http);
    assert_eq!(meta.base_url, "https://example.com");
}

// ── get_popular_manga ─────────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_popular_manga_returns_empty_list() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_popular_manga(&mut store, 1, 20, &[])
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert!(result.manga.is_empty());
    assert!(!result.has_next_page);
    assert!(result.total_pages.is_none());
}

// ── search_manga ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn example_search_manga_returns_empty_list() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_search_manga(&mut store, "anything", 1, 20, &[])
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert!(result.manga.is_empty());
    assert!(!result.has_next_page);
}

// ── get_manga_details ─────────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_manga_details_returns_hardcoded_info() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let info = provider
        .call_get_manga_details(&mut store, "any-id")
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    // kani-example returns a fixed MangaInfo regardless of id
    assert_eq!(info.id, "any-id");
    assert_eq!(info.title, "Example");
    assert_eq!(info.description.as_deref(), Some("Example"));
    assert!(matches!(
        info.status,
        kani_core::wasm::kani::extension::types::MangaStatus::Ongoing
    ));
    assert!(info.authors.is_empty());
    assert!(info.artists.is_empty());
    assert!(info.tags.is_empty());
}

// ── get_chapter_list ──────────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_chapter_list_returns_empty() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_chapter_list(&mut store, "any-manga", 1, None, None)
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert!(result.chapters.is_empty());
    assert!(!result.has_next_page);
}

// ── get_pages ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_pages_returns_empty_chapter() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let chapter = provider
        .call_get_pages(&mut store, "manga", "chapter")
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert!(chapter.pages.is_empty());
}

// ── get_filter_list ───────────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_filter_list_returns_empty() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_filter_list(&mut store)
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert!(result.filters.is_empty());
}

// ── get_preferences ───────────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_preferences_returns_empty() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_preferences(&mut store)
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert!(result.is_empty());
}

// ── get_url ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_url_returns_default_not_implemented_error() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_url(&mut store, "manga-id")
        .await
        .expect("WASM call trapped");

    // kani-example inherits the default MangaExtension::get_url which returns Err
    assert!(
        result.is_err(),
        "kani-example should return Err for get_url"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("not implemented") || err.contains("get_url"),
        "error should mention not-implemented: {err}"
    );
}

// ── get_chapter_sort_list ─────────────────────────────────────────────────────

#[tokio::test]
async fn example_get_chapter_sort_list_returns_empty() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;

    let provider = instance.kani_extension_manga_provider();
    let result = provider
        .call_get_chapter_sort_list(&mut store)
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert!(result.is_empty());
}

// ── instantiate_pre (pooled-instance path) ────────────────────────────────────

#[tokio::test]
async fn example_instantiate_pre_and_call() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let bytes = load_wasm("kani-example").unwrap();
    let rt = WasmRuntime::new(2).unwrap();
    let component = rt.compile_component(&bytes).unwrap();
    let pre = rt.instantiate_pre(&component).unwrap();

    let mut store = rt.create_store();
    store.set_epoch_deadline(EPOCH_TICKS);

    // Use the pre-linked instance to create a live instance
    let instance = pre
        .instantiate_async(&mut store)
        .await
        .expect("pre-instantiation failed");

    let provider = instance.kani_extension_manga_provider();
    let meta = provider
        .call_get_metadata(&mut store)
        .await
        .expect("WASM call trapped")
        .expect("extension returned Err");

    assert_eq!(meta.id, "example");
}

// ── no handle leaks after calls ───────────────────────────────────────────────

#[tokio::test]
async fn example_no_handles_allocated_for_any_call() {
    if load_wasm("kani-example").is_none() {
        return;
    }
    let (_rt, mut store, instance) = example_instance().await;
    let provider = instance.kani_extension_manga_provider();

    // All kani-example calls use no host imports and allocate no handles
    provider
        .call_get_popular_manga(&mut store, 1, 20, &[])
        .await
        .unwrap()
        .unwrap();
    provider
        .call_search_manga(&mut store, "q", 1, 20, &[])
        .await
        .unwrap()
        .unwrap();
    provider
        .call_get_manga_details(&mut store, "id")
        .await
        .unwrap()
        .unwrap();
    provider
        .call_get_chapter_list(&mut store, "id", 1, None, None)
        .await
        .unwrap()
        .unwrap();
    provider
        .call_get_pages(&mut store, "m", "c")
        .await
        .unwrap()
        .unwrap();

    let state = store.data();
    assert!(state.html_docs.is_empty(), "html_docs should be empty");
    assert!(state.html_lists.is_empty(), "html_lists should be empty");
    assert!(state.json_docs.is_empty(), "json_docs should be empty");
}
