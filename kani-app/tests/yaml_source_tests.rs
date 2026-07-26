#![allow(clippy::unwrap_used)]

mod common;
use common::test_service;

use std::collections::HashMap;
use std::sync::Arc;

use kani_app::source::{SourceBackend, SourceRegistry, YamlSource};
use kani_shared::ast::Expr;
use kani_yaml::yaml::model::{
    FieldSource, ValidatedEndpoint, ValidatedExtension, ValidatedField, ValidatedHnp,
    ValidatedPopular, ValidatedTotalPages,
};
use kani_yaml::yaml::schema::{
    FilterEntry, FilterKind, FilterOption as SchemaFilterOption, FilterSemantic, PreferenceEntry,
    PreferenceKind, ResponseType,
};

async fn start_html_server(html: &'static str) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html.len(),
                    html
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    port
}

fn text_field(name: &str, selector: &str) -> ValidatedField {
    ValidatedField {
        name: name.to_string(),
        source: FieldSource::Blueprint(Expr::Text {
            target: Box::new(Expr::First {
                target: Box::new(Expr::SelfRef),
                selector: selector.to_string(),
            }),
        }),
        optional: false,
    }
}

fn self_attr_field(name: &str, attr: &str) -> ValidatedField {
    ValidatedField {
        name: name.to_string(),
        source: FieldSource::Blueprint(Expr::Attr {
            target: Box::new(Expr::SelfRef),
            name: attr.to_string(),
        }),
        optional: false,
    }
}

fn list_endpoint(route: &str, container: &str) -> ValidatedEndpoint {
    ValidatedEndpoint {
        route: route.to_string(),
        method: "GET".into(),
        headers: vec![],
        queries: vec![],
        filter_mapping: vec![],
        filter_format: None,
        response_type: ResponseType::Html,
        container: container.to_string(),
        bindings: vec![],
        fields: vec![
            self_attr_field("id", "data-id"),
            text_field("title", ".title"),
        ],
        scalars: vec![],
        has_next_page: ValidatedHnp::Static(false),
        total_pages: ValidatedTotalPages::None,
        pagination: None,
        composite_id_decodes: vec![],
        then_steps: vec![],
        for_each_steps: vec![],
        via: None,
        page_url: None,
        script_name: None,
        timeout_ms: 10_000,
    }
}

fn yaml_source(_base_url: &str, ext: ValidatedExtension) -> YamlSource {
    yaml_source_with_browser(ext, true)
}

fn yaml_source_with_browser(ext: ValidatedExtension, browser_enabled: bool) -> YamlSource {
    let cache = Arc::new(kani_core::cache::InMemoryCache::new());
    let http = kani_core::http::SmartClient::new(None).unwrap();
    YamlSource::new(
        Arc::new(ext),
        http,
        cache,
        "test:".into(),
        HashMap::new(),
        browser_enabled,
    )
}

#[tokio::test]
async fn popular_manga_extracts_items_from_html() {
    let html: &'static str = r#"<html><body>
        <div class="item" data-id="manga-1"><span class="title">My Manga</span></div>
        <div class="item" data-id="manga-2"><span class="title">Another Manga</span></div>
    </body></html>"#;

    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            popular: Some(ValidatedPopular::Full(Box::new(list_endpoint(
                "/popular", ".item",
            )))),
            ..Default::default()
        },
    );

    let result = src.get_popular_manga(1, 20, &[]).await.unwrap();

    assert_eq!(result.manga.len(), 2);
    assert_eq!(result.manga[0].id, "manga-1");
    assert_eq!(result.manga[0].title, "My Manga");
    assert_eq!(result.manga[1].id, "manga-2");
    assert_eq!(result.manga[1].title, "Another Manga");
    assert!(!result.has_next_page);
}

#[tokio::test]
async fn search_manga_extracts_items_from_html() {
    let html: &'static str = r#"<html><body>
        <div class="item" data-id="s-1"><span class="title">Search Hit</span></div>
    </body></html>"#;

    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            search: Some(list_endpoint("/search", ".item")),
            ..Default::default()
        },
    );

    let result = src.search_manga("Search Hit", 1, 20, &[]).await.unwrap();

    assert_eq!(result.manga.len(), 1);
    assert_eq!(result.manga[0].id, "s-1");
    assert_eq!(result.manga[0].title, "Search Hit");
}

#[tokio::test]
async fn manga_details_extracts_title_and_id() {
    let html: &'static str = r#"<html><body>
        <div class="manga" data-id="manga-42"><h1>My Title</h1></div>
    </body></html>"#;

    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let details_ep = ValidatedEndpoint {
        route: "/manga/$manga_id$".into(),
        fields: vec![self_attr_field("id", "data-id"), text_field("title", "h1")],
        container: ".manga".into(),
        ..list_endpoint("/manga/$manga_id$", ".manga")
    };

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            manga_details: Some(details_ep),
            ..Default::default()
        },
    );

    let result = src.get_manga_details("manga-42").await.unwrap();

    assert_eq!(result.id, "manga-42");
    assert_eq!(result.title, "My Title");
}

#[tokio::test]
async fn delegated_popular_does_not_deadlock() {
    let html: &'static str = r#"<html><body>
        <div class="item" data-id="x-1"><span class="title">Delegated</span></div>
    </body></html>"#;

    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            popular: Some(ValidatedPopular::Delegated {
                delegate_to: "search".into(),
                empty_without_filters: false,
            }),
            search: Some(list_endpoint("/search", ".item")),
            ..Default::default()
        },
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        src.get_popular_manga(1, 20, &[]),
    )
    .await
    .expect("timed out — semaphore double-acquire deadlock")
    .unwrap();

    assert_eq!(result.manga.len(), 1);
    assert_eq!(result.manga[0].id, "x-1");
}

#[tokio::test]
async fn empty_without_filters_returns_empty_list_without_http() {
    let src = yaml_source(
        "http://127.0.0.1:1",
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: "http://127.0.0.1:1".into(),
            language: "en".into(),
            unrestricted_http: true,
            popular: Some(ValidatedPopular::Delegated {
                delegate_to: "search".into(),
                empty_without_filters: true,
            }),
            ..Default::default()
        },
    );

    let result = src.get_popular_manga(1, 20, &[]).await.unwrap();
    assert!(result.manga.is_empty());
    assert!(!result.has_next_page);
}

#[test]
fn capability_mismatch_produces_load_error() {
    let result = kani_app::install_gating::check_required_capabilities(&[
        "nonexistent_capability".to_string()
    ]);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("nonexistent_capability"));
}

#[test]
fn known_capabilities_are_all_accepted() {
    let caps: Vec<String> = [
        "unrestricted_http",
        "browser_payload",
        "rhai_scripting",
        "scoped_cache",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert!(kani_app::install_gating::check_required_capabilities(&caps).is_ok());
}

#[test]
fn capability_unrestricted_http_is_supported() {
    let result =
        kani_app::install_gating::check_required_capabilities(&["unrestricted_http".to_string()]);
    assert!(result.is_ok());
}

#[tokio::test]
async fn metadata_serialises_from_config() {
    let cache = Arc::new(kani_core::cache::InMemoryCache::new());
    let http = kani_core::http::SmartClient::new(None).unwrap();
    let src = YamlSource::new(
        Arc::new(ValidatedExtension {
            id: "test-id".into(),
            name: "Test Source".into(),
            ..Default::default()
        }),
        http,
        cache,
        String::new(),
        HashMap::new(),
        true,
    );
    let meta_json = src.get_metadata().await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&meta_json).unwrap();
    assert_eq!(parsed["id"].as_str().unwrap(), "test-id");
    assert_eq!(parsed["name"].as_str().unwrap(), "Test Source");
}

#[tokio::test]
async fn chapter_list_extracts_chapters_from_html() {
    let html: &'static str = r#"<html><body>
        <div class="ch" data-id="ch-1"><span class="title">Chapter 1</span></div>
        <div class="ch" data-id="ch-2"><span class="title">Chapter 2</span></div>
    </body></html>"#;

    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let chapter_ep = ValidatedEndpoint {
        route: "/manga/$manga_id$/chapters".into(),
        fields: vec![
            self_attr_field("id", "data-id"),
            text_field("title", ".title"),
        ],
        container: ".ch".into(),
        ..list_endpoint("/manga/$manga_id$/chapters", ".ch")
    };

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            chapter_list: Some(chapter_ep),
            ..Default::default()
        },
    );

    let result = src
        .get_chapter_list("manga-1", 1, None, None)
        .await
        .unwrap();

    assert_eq!(result.chapters.len(), 2);
    assert_eq!(result.chapters[0].id, "ch-1");
    assert_eq!(result.chapters[1].id, "ch-2");
    assert!(!result.has_next_page);
}

#[tokio::test]
async fn get_pages_extracts_page_urls_from_html() {
    let html: &'static str = r#"<html><body>
        <div class="page" data-url="https://cdn.example.com/p1.jpg"></div>
        <div class="page" data-url="https://cdn.example.com/p2.jpg"></div>
    </body></html>"#;

    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let pages_ep = ValidatedEndpoint {
        route: "/manga/$manga_id$/chapter/$chapter_id$".into(),
        fields: vec![ValidatedField {
            name: "url".to_string(),
            source: FieldSource::Blueprint(Expr::Attr {
                target: Box::new(Expr::SelfRef),
                name: "data-url".to_string(),
            }),
            optional: false,
        }],
        container: ".page".into(),
        ..list_endpoint("/manga/$manga_id$/chapter/$chapter_id$", ".page")
    };

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            pages: Some(pages_ep),
            ..Default::default()
        },
    );

    let result = src.get_pages("manga-1", "ch-1").await.unwrap();

    assert_eq!(result.pages.len(), 2);
    assert_eq!(result.pages[0].url, "https://cdn.example.com/p1.jpg");
    assert_eq!(result.pages[0].index, 0);
    assert_eq!(result.pages[1].url, "https://cdn.example.com/p2.jpg");
    assert_eq!(result.pages[1].index, 1);
}

#[tokio::test]
async fn get_filter_list_maps_all_kinds_and_options() {
    let src = yaml_source(
        "http://127.0.0.1:1",
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: "http://127.0.0.1:1".into(),
            language: "en".into(),
            filters: vec![
                FilterEntry {
                    id: "genre".into(),
                    name: "Genre".into(),
                    kind: FilterKind::Select,
                    options: vec![
                        SchemaFilterOption {
                            name: "Action".into(),
                            value: "action".into(),
                            nsfw: false,
                        },
                        SchemaFilterOption {
                            name: "Romance".into(),
                            value: "romance".into(),
                            nsfw: false,
                        },
                    ],
                    default: None,
                    semantic: None,
                    name_i18n: None,
                    options_ref: None,
                    min: None,
                    max: None,
                    step: None,
                },
                FilterEntry {
                    id: "author".into(),
                    name: "Author".into(),
                    kind: FilterKind::TextInput,
                    options: vec![],
                    default: None,
                    semantic: Some(FilterSemantic::Author),
                    name_i18n: None,
                    options_ref: None,
                    min: None,
                    max: None,
                    step: None,
                },
                FilterEntry {
                    id: "completed".into(),
                    name: "Completed".into(),
                    kind: FilterKind::Checkbox,
                    options: vec![],
                    default: Some(kani_yaml::yaml::schema::FilterDefault::Bool(false)),
                    semantic: None,
                    name_i18n: None,
                    options_ref: None,
                    min: None,
                    max: None,
                    step: None,
                },
                FilterEntry {
                    id: "year_range".into(),
                    name: "Year".into(),
                    kind: FilterKind::IntRange,
                    options: vec![],
                    default: None,
                    semantic: None,
                    name_i18n: None,
                    options_ref: None,
                    min: Some(2000.0),
                    max: Some(2025.0),
                    step: None,
                },
            ],
            ..Default::default()
        },
    );

    let filter_list = src.get_filter_list().await.unwrap();

    assert_eq!(filter_list.filters.len(), 4);

    let genre = &filter_list.filters[0];
    assert_eq!(genre.id, "genre");
    assert_eq!(genre.name, "Genre");
    assert!(matches!(
        genre.tag,
        kani_core::wasm::kani::extension::types::FilterTypeTag::Select
    ));
    assert_eq!(genre.options.len(), 2);
    assert_eq!(genre.options[0].filter_name, "genre");
    assert_eq!(genre.options[0].name, "Action");
    assert_eq!(genre.options[0].value, "action");

    let author = &filter_list.filters[1];
    assert_eq!(author.id, "author");
    assert!(matches!(
        author.tag,
        kani_core::wasm::kani::extension::types::FilterTypeTag::TextInput
    ));
    assert!(matches!(
        author.semantic,
        Some(kani_core::wasm::kani::extension::types::FilterSemantic::Author)
    ));

    let completed = &filter_list.filters[2];
    assert_eq!(completed.id, "completed");
    assert!(matches!(
        completed.tag,
        kani_core::wasm::kani::extension::types::FilterTypeTag::Checkbox
    ));
    assert!(matches!(
        completed.default_value,
        Some(kani_core::wasm::kani::extension::types::FilterState::Checkbox(false))
    ));

    let year = &filter_list.filters[3];
    assert_eq!(year.id, "year_range");
    assert!(matches!(
        year.tag,
        kani_core::wasm::kani::extension::types::FilterTypeTag::TextInput
    ));
}

#[tokio::test]
async fn get_fetched_option_sets_lists_fetch_configured_filters() {
    use kani_yaml::yaml::schema::{FetchedOptionsDef, OptionSetDef, ResponseType};
    use std::collections::BTreeMap;

    let mut option_sets = BTreeMap::new();
    option_sets.insert(
        "genres".to_string(),
        OptionSetDef::Fetched {
            options_fetched_by: FetchedOptionsDef {
                route: "/genres".into(),
                response_type: ResponseType::Html,
                container: Some(".genre".into()),
                fields: BTreeMap::from([
                    ("name".to_string(), ".name".to_string()),
                    ("value".to_string(), "data-id".to_string()),
                ]),
                nsfw_field: None,
                cache: None,
            },
        },
    );

    let src = yaml_source(
        "http://127.0.0.1:1",
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: "http://127.0.0.1:1".into(),
            language: "en".into(),
            filters: vec![
                FilterEntry {
                    id: "genre".into(),
                    name: "Genre".into(),
                    kind: FilterKind::Select,
                    options: vec![],
                    default: None,
                    semantic: None,
                    name_i18n: None,
                    options_ref: Some("genres".into()),
                    min: None,
                    max: None,
                    step: None,
                },
                FilterEntry {
                    id: "author".into(),
                    name: "Author".into(),
                    kind: FilterKind::TextInput,
                    options: vec![],
                    default: None,
                    semantic: Some(FilterSemantic::Author),
                    name_i18n: None,
                    options_ref: None,
                    min: None,
                    max: None,
                    step: None,
                },
            ],
            option_sets,
            ..Default::default()
        },
    );

    let raw = src.get_fetched_option_sets().await.unwrap();
    let parsed: Vec<kani_shared::filter_fetch::FilterFetchDef> =
        serde_json::from_str(&raw).unwrap();

    assert_eq!(
        parsed.len(),
        1,
        "only the filter with a Fetched options_ref should be listed"
    );
    let entry = &parsed[0];
    assert_eq!(entry.filter_id, "genre");
    assert_eq!(entry.option_set_name, "genres");
    assert_eq!(entry.route, "/genres");
    assert_eq!(entry.response_type, "html");
    assert_eq!(entry.container.as_deref(), Some(".genre"));
    assert_eq!(entry.cache_ttl, 300, "default TTL when no cache block set");
}

#[tokio::test]
async fn get_preferences_maps_all_kinds() {
    let src = yaml_source(
        "http://127.0.0.1:1",
        ValidatedExtension {
            id: "fixture-source".into(),
            name: "Fixture Source".into(),
            version: "1.0.0".into(),
            base_url: "http://127.0.0.1:1".into(),
            language: "en".into(),
            preferences: vec![
                PreferenceEntry {
                    key: "enable_nsfw".into(),
                    label: "Show NSFW".into(),
                    kind: PreferenceKind::Toggle,
                    options: vec![],
                    default: "false".into(),
                    description: Some("Enable NSFW content".into()),
                    secret: false,
                    options_ref: None,
                },
                PreferenceEntry {
                    key: "quality".into(),
                    label: "Image Quality".into(),
                    kind: PreferenceKind::Select,
                    options: vec![
                        kani_yaml::yaml::schema::PrefOption {
                            name: "High".into(),
                            value: "high".into(),
                        },
                        kani_yaml::yaml::schema::PrefOption {
                            name: "Low".into(),
                            value: "low".into(),
                        },
                    ],
                    default: "high".into(),
                    description: None,
                    secret: false,
                    options_ref: None,
                },
            ],
            ..Default::default()
        },
    );

    let prefs = src.get_preferences().await.unwrap();

    assert_eq!(prefs.len(), 2);

    let toggle = &prefs[0];
    assert_eq!(toggle.key, "enable_nsfw");
    assert_eq!(toggle.label, "Show NSFW");
    assert!(matches!(
        toggle.kind,
        kani_core::wasm::kani::extension::types::PrefKind::Toggle
    ));
    assert_eq!(toggle.default, "false");
    assert_eq!(toggle.description, Some("Enable NSFW content".into()));

    let select = &prefs[1];
    assert_eq!(select.key, "quality");
    assert!(matches!(
        select.kind,
        kani_core::wasm::kani::extension::types::PrefKind::Select
    ));
    assert_eq!(select.options.len(), 2);
    assert_eq!(select.options[0], ("High".to_string(), "high".to_string()));
}

#[tokio::test]
async fn scan_yaml_registers_in_db_and_loads_into_registry() {
    let html: &'static str = r#"<html><body>
        <div class="item" data-id="m-1"><span class="title">Scan Test</span></div>
    </body></html>"#;
    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let dir = tempfile::tempdir().unwrap();
    let yaml_content = format!(
        r#"id: scan-test-source
name: scan-test-source
version: "1.0.0"
base_url: "{base_url}"
language: en
requires_capabilities:
  - unrestricted_http
endpoints:
  popular:
    route: /popular
    container: ".item"
    fields:
      id: 'self.attr("data-id")'
      title: 'self.first(".title").text()'
"#
    );
    std::fs::write(dir.path().join("scan-test-source.yaml"), &yaml_content).unwrap();

    let svc = test_service().await;
    svc.scan_and_load_yaml_dir_for_test(dir.path())
        .await
        .unwrap();

    use sqlx::Row as _;
    let row =
        sqlx::query("SELECT enabled, load_error FROM sources WHERE name = 'scan-test-source'")
            .fetch_one(&svc.db)
            .await
            .unwrap();
    let enabled: i64 = row.try_get("enabled").unwrap();
    let load_error: Option<String> = row.try_get("load_error").unwrap();
    assert_eq!(enabled, 1, "source should be enabled after successful scan");
    assert!(load_error.is_none(), "load_error should be NULL on success");

    let source_id: i64 =
        sqlx::query_scalar("SELECT id FROM sources WHERE name = 'scan-test-source'")
            .fetch_one(&svc.db)
            .await
            .unwrap();
    assert!(
        svc.sources.contains_key(source_id),
        "source should be in registry after scan+load"
    );

    let backend = svc.sources.get_backend(source_id).unwrap();
    let result = backend.get_popular_manga(1, 20, &[]).await.unwrap();
    assert_eq!(result.manga.len(), 1);
    assert_eq!(result.manga[0].id, "m-1");
    assert_eq!(result.manga[0].title, "Scan Test");
}

#[tokio::test]
async fn scan_yaml_with_bad_capability_sets_load_error_and_disables() {
    let dir = tempfile::tempdir().unwrap();
    let yaml_content = r#"id: bad-cap-source
name: bad-cap-source
version: "1.0.0"
base_url: "https://example.com"
language: en
requires_capabilities:
  - nonexistent_capability
endpoints: {}
"#;
    std::fs::write(dir.path().join("bad-cap-source.yaml"), yaml_content).unwrap();

    let svc = test_service().await;
    svc.scan_and_load_yaml_dir_for_test(dir.path())
        .await
        .unwrap();

    use sqlx::Row as _;
    let row = sqlx::query("SELECT enabled, load_error FROM sources WHERE name = 'bad-cap-source'")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    let enabled: i64 = row.try_get("enabled").unwrap();
    let load_error: Option<String> = row.try_get("load_error").unwrap();
    assert_eq!(enabled, 0, "source with bad capability should be disabled");
    assert!(
        load_error.is_some(),
        "load_error should be set on capability mismatch"
    );
    assert!(
        load_error.unwrap().contains("nonexistent_capability"),
        "error message should name the missing capability"
    );

    let source_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM sources WHERE name = 'bad-cap-source'")
            .fetch_optional(&svc.db)
            .await
            .unwrap();
    if let Some(id) = source_id {
        assert!(
            !svc.sources.contains_key(id),
            "source with bad capability must not be in registry"
        );
    }
}

#[tokio::test]
async fn yaml_supersedes_wasm_when_both_files_present_in_dir() {
    let html: &'static str = r#"<html><body>
        <div class="item" data-id="y-1"><span class="title">Yaml Wins</span></div>
    </body></html>"#;
    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let dir = tempfile::tempdir().unwrap();
    let yaml_content = format!(
        r#"id: supersede-src
name: supersede-src
version: "1.0.0"
base_url: "{base_url}"
language: en
requires_capabilities:
  - unrestricted_http
endpoints:
  popular:
    route: /popular
    container: ".item"
    fields:
      id: 'self.attr("data-id")'
      title: 'self.first(".title").text()'
"#
    );
    std::fs::write(dir.path().join("supersede-src.yaml"), &yaml_content).unwrap();

    let svc = test_service().await;
    svc.scan_and_load_yaml_dir_for_test(dir.path())
        .await
        .unwrap();

    let source_id: i64 = sqlx::query_scalar("SELECT id FROM sources WHERE name = 'supersede-src'")
        .fetch_one(&svc.db)
        .await
        .unwrap();

    std::fs::write(dir.path().join("supersede-src.wasm"), b"not-a-real-wasm").unwrap();

    svc.load_yaml_sources_from_dir_for_test(dir.path())
        .await
        .unwrap();

    let backend = svc.sources.get_backend(source_id).unwrap();
    assert!(
        backend.is_yaml(),
        "YAML backend must be selected when both .yaml and .wasm exist for the same source"
    );

    let result = backend.get_popular_manga(1, 20, &[]).await.unwrap();
    assert_eq!(result.manga[0].title, "Yaml Wins");
}

#[tokio::test]
async fn browser_payload_endpoint_returns_clear_error() {
    use kani_yaml::yaml::schema::EndpointVia;

    let mut ep = list_endpoint("/popular", ".item");
    ep.via = Some(EndpointVia::BrowserPayload);

    let src = yaml_source(
        "http://127.0.0.1:1",
        ValidatedExtension {
            id: "bp-test".into(),
            name: "bp-test".into(),
            version: "1.0.0".into(),
            base_url: "http://127.0.0.1:1".into(),
            language: "en".into(),
            unrestricted_http: true,
            popular: Some(ValidatedPopular::Full(Box::new(ep))),
            ..Default::default()
        },
    );

    let result = src.get_popular_manga(1, 20, &[]).await;
    assert!(result.is_err(), "browser_payload must fail with error");
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("browser_payload") || err_str.contains("browser runtime"),
        "error should mention browser_payload: {err_str}"
    );
}

#[tokio::test]
async fn browser_payload_endpoint_missing_script_returns_clear_error() {
    use kani_yaml::yaml::schema::EndpointVia;

    let mut ep = list_endpoint("/popular", ".item");
    ep.via = Some(EndpointVia::BrowserPayload);
    ep.page_url = Some("https://example.com/manga/$manga_id$".into());
    ep.script_name = Some("undeclared_script".into());

    let src = yaml_source(
        "http://127.0.0.1:1",
        ValidatedExtension {
            id: "bp-test".into(),
            name: "bp-test".into(),
            version: "1.0.0".into(),
            base_url: "http://127.0.0.1:1".into(),
            language: "en".into(),
            unrestricted_http: true,
            manga_details: Some(ep),
            ..Default::default()
        },
    );

    let result = src.get_manga_details("manga-1").await;
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("undeclared_script"),
        "error should name the missing script: {err_str}"
    );
}

#[tokio::test]
async fn browser_payload_endpoint_reaches_capture_page_payload() {
    use kani_yaml::yaml::schema::EndpointVia;

    // Deterministic, environment-independent short-circuit inside
    // v8_process::capture_page_payload — proves arg resolution and script
    // lookup succeeded and the call reached the browser-runtime boundary,
    // without needing a real headless browser available in the test env.
    unsafe {
        std::env::set_var("KANI_BROWSER_ENABLED", "false");
    }

    let mut ep = list_endpoint("/popular", ".item");
    ep.via = Some(EndpointVia::BrowserPayload);
    ep.page_url = Some("https://example.com/manga/$manga_id$".into());
    ep.script_name = Some("fetch_manga".into());

    let mut browser_scripts = std::collections::BTreeMap::new();
    browser_scripts.insert("fetch_manga".to_string(), "passPayload('{}');".to_string());

    let src = yaml_source(
        "http://127.0.0.1:1",
        ValidatedExtension {
            id: "bp-test".into(),
            name: "bp-test".into(),
            version: "1.0.0".into(),
            base_url: "http://127.0.0.1:1".into(),
            language: "en".into(),
            unrestricted_http: true,
            manga_details: Some(ep),
            browser_scripts,
            ..Default::default()
        },
    );

    let result = src.get_manga_details("manga-1").await;
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("KANI_BROWSER_ENABLED") || err_str.contains("disabled"),
        "expected the deterministic browser-disabled error, got: {err_str}"
    );
}

#[tokio::test]
async fn browser_payload_restricted_host_rejected_before_dispatch() {
    use kani_yaml::yaml::schema::EndpointVia;

    // A restricted source (unrestricted_http = false) must not be able to point
    // the browser at an arbitrary host. The AllowedHost check fires before any V8
    // dispatch, so the error is host-specific rather than a browser-runtime error.
    let mut ep = list_endpoint("/popular", ".item");
    ep.via = Some(EndpointVia::BrowserPayload);
    ep.page_url = Some("https://evil.example.com/manga/$manga_id$".into());
    ep.script_name = Some("fetch_manga".into());

    let mut browser_scripts = std::collections::BTreeMap::new();
    browser_scripts.insert("fetch_manga".to_string(), "passPayload('{}');".to_string());

    let src = yaml_source(
        "http://127.0.0.1:1",
        ValidatedExtension {
            id: "bp-test".into(),
            name: "bp-test".into(),
            version: "1.0.0".into(),
            base_url: "http://127.0.0.1:1".into(),
            language: "en".into(),
            unrestricted_http: false,
            manga_details: Some(ep),
            browser_scripts,
            ..Default::default()
        },
    );

    let result = src.get_manga_details("manga-1").await;
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("blocked")
            || err_str.contains("only contact")
            || err_str.contains("evil.example.com"),
        "restricted source should reject off-host browser target: {err_str}"
    );
}

#[tokio::test]
async fn browser_payload_rejected_when_browser_disabled_for_source() {
    use kani_yaml::yaml::schema::EndpointVia;

    let mut ep = list_endpoint("/popular", ".item");
    ep.via = Some(EndpointVia::BrowserPayload);
    ep.page_url = Some("https://example.com/manga/$manga_id$".into());
    ep.script_name = Some("fetch_manga".into());

    let mut browser_scripts = std::collections::BTreeMap::new();
    browser_scripts.insert("fetch_manga".to_string(), "passPayload('{}');".to_string());

    let ext = ValidatedExtension {
        id: "bp-test".into(),
        name: "bp-test".into(),
        version: "1.0.0".into(),
        base_url: "http://127.0.0.1:1".into(),
        language: "en".into(),
        unrestricted_http: true,
        manga_details: Some(ep),
        browser_scripts,
        ..Default::default()
    };
    let src = yaml_source_with_browser(ext, false);

    let result = src.get_manga_details("manga-1").await;
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("disabled"),
        "source with browser disabled should reject before dispatch: {err_str}"
    );
}

#[tokio::test]
async fn refresh_auth_retries_and_succeeds() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let popular_hits = Arc::new(AtomicU32::new(0));
    let login_hits = Arc::new(AtomicU32::new(0));
    let popular_hits_srv = Arc::clone(&popular_hits);
    let login_hits_srv = Arc::clone(&login_hits);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let ph = Arc::clone(&popular_hits_srv);
            let lh = Arc::clone(&login_hits_srv);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req_str = std::str::from_utf8(&buf[..n]).unwrap_or("");
                let (status, body): (u16, &str) = if req_str.contains("GET /login") {
                    lh.fetch_add(1, Ordering::Relaxed);
                    (200, r#"<html><body></body></html>"#)
                } else {
                    let prev = ph.fetch_add(1, Ordering::Relaxed);
                    if prev == 0 {
                        (401, "")
                    } else {
                        (
                            200,
                            r#"<html><body><div class="item" data-id="r-1"><span class="title">Retried</span></div></body></html>"#,
                        )
                    }
                };
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    let base_url = format!("http://127.0.0.1:{port}");

    let mut on_status = std::collections::BTreeMap::new();
    on_status.insert("401".to_string(), r#"refresh_auth("search")"#.to_string());

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "auth-test".into(),
            name: "auth-test".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            popular: Some(ValidatedPopular::Full(Box::new(list_endpoint(
                "/popular", ".item",
            )))),
            search: Some(list_endpoint("/login", ".item")),
            on_status,
            ..Default::default()
        },
    );

    let result = src.get_popular_manga(1, 20, &[]).await.unwrap();
    assert_eq!(result.manga.len(), 1, "should succeed on retry");
    assert_eq!(result.manga[0].id, "r-1");
    assert_eq!(result.manga[0].title, "Retried");
    assert_eq!(
        popular_hits.load(Ordering::Relaxed),
        2,
        "/popular must be called exactly twice: once returning 401, once returning 200"
    );
    assert_eq!(
        login_hits.load(Ordering::Relaxed),
        1,
        "/login (auth endpoint) must be called exactly once by the refresh_auth dispatch"
    );
}

#[tokio::test]
async fn yaml_hot_swap_in_flight_call_completes_with_old_config() {
    let html_old: &'static str = r#"<html><body>
        <div class="item" data-id="old-1"><span class="title">Old Config</span></div>
    </body></html>"#;
    let html_new: &'static str = r#"<html><body>
        <div class="item" data-id="new-1"><span class="title">New Config</span></div>
    </body></html>"#;

    let port_old = start_html_server(html_old).await;
    let port_new = start_html_server(html_new).await;
    let base_old = format!("http://127.0.0.1:{port_old}");
    let base_new = format!("http://127.0.0.1:{port_new}");

    let registry = SourceRegistry::new();

    registry.insert(
        1,
        SourceBackend::Yaml(Box::new(yaml_source(
            &base_old,
            ValidatedExtension {
                id: "swap-src".into(),
                name: "swap-src".into(),
                version: "1.0.0".into(),
                base_url: base_old.clone(),
                language: "en".into(),
                unrestricted_http: true,
                popular: Some(ValidatedPopular::Full(Box::new(list_endpoint(
                    "/popular", ".item",
                )))),
                ..Default::default()
            },
        ))),
    );

    // Simulate a handler that already holds a reference to the old backend.
    let old_backend = registry.get_backend(1).unwrap();

    registry
        .hot_swap(
            1,
            SourceBackend::Yaml(Box::new(yaml_source(
                &base_new,
                ValidatedExtension {
                    id: "swap-src".into(),
                    name: "swap-src".into(),
                    version: "2.0.0".into(),
                    base_url: base_new.clone(),
                    language: "en".into(),
                    unrestricted_http: true,
                    popular: Some(ValidatedPopular::Full(Box::new(list_endpoint(
                        "/popular", ".item",
                    )))),
                    ..Default::default()
                },
            ))),
        )
        .await;

    // In-flight call completes with old config.
    let old_result = old_backend.get_popular_manga(1, 20, &[]).await.unwrap();
    assert_eq!(
        old_result.manga[0].id, "old-1",
        "in-flight call must use pre-swap config"
    );

    // New call through the registry resolves to the swapped config.
    let new_result = registry
        .get_backend(1)
        .unwrap()
        .get_popular_manga(1, 20, &[])
        .await
        .unwrap();
    assert_eq!(
        new_result.manga[0].id, "new-1",
        "post-swap call must use new config"
    );
}

#[tokio::test]
async fn a_disabled_yaml_source_can_be_re_enabled() {
    let html: &'static str = r#"<html><body>
        <div class="item" data-id="m-1"><span class="title">Re-enable</span></div>
    </body></html>"#;
    let port = start_html_server(html).await;
    let base_url = format!("http://127.0.0.1:{port}");

    let dir = tempfile::tempdir().unwrap();
    let yaml_content = format!(
        r#"id: reenable-source
name: reenable-source
version: "1.0.0"
base_url: "{base_url}"
language: en
requires_capabilities:
  - unrestricted_http
endpoints:
  search:
    route: /search
    container: ".item"
    fields:
      id: 'self.attr("data-id")'
      title: 'self.first(".title").text()'
"#
    );
    std::fs::write(dir.path().join("reenable-source.yaml"), &yaml_content).unwrap();

    let svc = test_service().await;
    {
        let mut s = svc.settings.write().await;
        s.wasm_storage_path = dir.path().to_path_buf();
    }
    svc.scan_and_load_yaml_dir_for_test(dir.path())
        .await
        .unwrap();

    use sqlx::Row as _;
    let id: i64 = sqlx::query("SELECT id FROM sources WHERE name = 'reenable-source'")
        .fetch_one(&svc.db)
        .await
        .unwrap()
        .try_get("id")
        .unwrap();

    // Disable, then re-enable — the path that used to only ever read a .wasm.
    svc.toggle_source_enabled(id, false).await.unwrap();
    svc.toggle_source_enabled(id, true)
        .await
        .expect("re-enabling a YAML source must not fail reading a nonexistent .wasm");

    // The proof it is live again: a search reaches the source.
    let result = svc.search_manga(id, "anything", 1, 20, None).await;
    assert!(
        result.is_ok(),
        "a re-enabled YAML source must serve requests, got {result:?}"
    );
}

async fn start_status_server(status_line: &'static str, extra_headers: &'static str) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let _ = stream.read(&mut buf).await;
                let body = "rate limited";
                let response = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: text/html\r\n{extra_headers}\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len(),
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    port
}

fn extension_error(err: kani_core::error::Error) -> kani_shared::extension::ExtensionError {
    match err {
        kani_core::error::Error::Extension(e) => e,
        other => panic!("expected Error::Extension, got {other:?}"),
    }
}

#[tokio::test]
async fn yaml_source_429_classifies_as_rate_limited_with_retry_after() {
    use kani_shared::extension::ExtensionErrorKind;

    // Retry-After larger than the request's own timeout: before A16 the client
    // slept on it in-request (capped, ×MAX_RETRIES) and overran the 90s outer
    // timeout, so this surfaced as a misleading ParseError/timeout. Now the 429
    // is returned immediately and classifies as RateLimited carrying the hint.
    let port = start_status_server("429 Too Many Requests", "Retry-After: 120\r\n").await;
    let base_url = format!("http://127.0.0.1:{port}");

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "rl-source".into(),
            name: "RL Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            popular: Some(ValidatedPopular::Full(Box::new(list_endpoint(
                "/popular", ".item",
            )))),
            ..Default::default()
        },
    );

    let err = extension_error(src.get_popular_manga(1, 20, &[]).await.unwrap_err());
    assert_eq!(
        err.kind,
        ExtensionErrorKind::RateLimited,
        "a 429 must classify as RateLimited, not Parse"
    );
    assert_eq!(
        err.retry_after_secs,
        Some(120),
        "the server's Retry-After must survive to the typed error"
    );
}

#[tokio::test]
async fn yaml_source_503_classifies_as_retryable_network() {
    use kani_shared::extension::ExtensionErrorKind;

    let port = start_status_server("503 Service Unavailable", "").await;
    let base_url = format!("http://127.0.0.1:{port}");

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "svc-source".into(),
            name: "Svc Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            popular: Some(ValidatedPopular::Full(Box::new(list_endpoint(
                "/popular", ".item",
            )))),
            ..Default::default()
        },
    );

    let err = extension_error(src.get_popular_manga(1, 20, &[]).await.unwrap_err());
    assert_eq!(
        err.kind,
        ExtensionErrorKind::Network,
        "a 5xx must classify as retryable Network, not Parse"
    );
}

#[tokio::test]
async fn yaml_source_404_is_not_surfaced_as_a_typed_http_error() {
    let port = start_status_server("404 Not Found", "").await;
    let base_url = format!("http://127.0.0.1:{port}");

    let src = yaml_source(
        &base_url,
        ValidatedExtension {
            id: "nf-source".into(),
            name: "NF Source".into(),
            version: "1.0.0".into(),
            base_url: base_url.clone(),
            language: "en".into(),
            unrestricted_http: true,
            popular: Some(ValidatedPopular::Full(Box::new(list_endpoint(
                "/popular", ".item",
            )))),
            ..Default::default()
        },
    );

    // 404 is deliberately excluded from the typed-error guard: sources return it
    // to signal "no more pages", so the body is extracted (matching nothing here)
    // and the endpoint returns an empty list rather than a RateLimited/Network
    // error. That keeps the pagination loop's terminate-on-empty semantics.
    let result = src
        .get_popular_manga(1, 20, &[])
        .await
        .expect("a 404 must not surface as a typed HTTP error");
    assert!(result.manga.is_empty());
}
