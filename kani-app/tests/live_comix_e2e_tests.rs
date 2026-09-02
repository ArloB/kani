#![allow(clippy::unwrap_used)]

//! End-to-end functional parity for the Comix source over the solver path.
//!
//! This drives the real `comix.yaml` through Kani's own pipeline — endpoint
//! dispatch, browser-script lookup, `capture_page_payload_resilient`, the
//! solver, then blueprint evaluation and unpacking into Kani's typed models.
//! Asserting on parsed output rather than raw payload bytes is the point: a
//! payload can be well-formed JSON and still fail to unpack into a `MangaList`.
//!
//! Ignored by default — needs network and a Kani-compatible solver. Run with:
//!
//! ```text
//! KANI_LIVE_SOLVER_URL=http://127.0.0.1:8191/v1 \
//! KANI_COMIX_YAML=/home/arlob/code/kani-extensions/comix.yaml \
//! cargo test -p kani-app --test live_comix_e2e_tests -- --ignored --nocapture
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use kani_app::source::YamlSource;

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

fn comix_source() -> YamlSource {
    let path = env_or(
        "KANI_COMIX_YAML",
        "/home/arlob/code/kani-extensions/comix.yaml",
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let ext = kani_yaml::parse_and_validate(&text, std::path::Path::new(&path)).unwrap_or_else(
        |errors| {
            panic!(
                "comix.yaml must parse: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        },
    );

    let solver = std::env::var("KANI_LIVE_SOLVER_URL")
        .expect("KANI_LIVE_SOLVER_URL must point at a Kani-compatible solver");
    let http = kani_core::http::SmartClient::new(Some(solver)).unwrap();
    let cache = Arc::new(kani_core::cache::InMemoryCache::new());

    YamlSource::new(
        Arc::new(ext),
        http,
        cache,
        "comix:".into(),
        HashMap::new(),
        true,
    )
}

#[tokio::test]
#[ignore = "live e2e: needs network and a Kani-compatible solver"]
async fn all_four_comix_endpoints_parse_into_kani_models() {
    let source = comix_source();

    let popular = source
        .get_popular_manga(1, 28, &[])
        .await
        .expect("popular must resolve through the solver");
    println!("popular: {} items", popular.manga.len());
    assert!(
        popular.manga.len() >= 20,
        "a browse page should yield a full rail, got {}",
        popular.manga.len()
    );
    for item in popular.manga.iter().take(5) {
        assert!(!item.id.is_empty(), "every item needs an encoded id");
        assert!(!item.title.is_empty(), "every item needs a title");
    }

    let first = popular.manga.first().unwrap().clone();
    println!("details target: {} ({})", first.title, first.id);

    let details = source
        .get_manga_details(&first.id)
        .await
        .expect("details must resolve for an id taken from popular");
    println!("details: {}", details.title);
    assert!(!details.title.is_empty(), "details must carry a title");

    let chapters = source
        .get_chapter_list(&first.id, 1, None, None)
        .await
        .expect("chapter list must resolve");
    println!("chapters: {}", chapters.chapters.len());
    assert!(
        !chapters.chapters.is_empty(),
        "a chapter list should not be empty"
    );
    let chapter = chapters.chapters.first().unwrap().clone();
    assert!(
        !chapter.language.is_empty(),
        "chapters must carry a language"
    );

    let pages = source
        .get_pages(&first.id, &chapter.id)
        .await
        .expect("page list must resolve");
    println!("pages: {}", pages.pages.len());
    assert!(
        pages.pages.len() > 5,
        "a chapter should have more than a handful of pages, got {}",
        pages.pages.len()
    );
    for page in pages.pages.iter().take(3) {
        assert!(
            page.url.starts_with("http"),
            "page urls must be absolute, got {:?}",
            page.url
        );
    }
}
