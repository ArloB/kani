#![allow(clippy::unwrap_used)]

use kani_core::evaluator::{html_eval::extract_html, json_eval::extract_json};
use kani_core::wasm::{AllowedHost, HostState};
use kani_shared::ast::{BlueprintBuilder, Expr, RequestDef};
use std::sync::{Arc, Mutex};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_state(allowed: AllowedHost) -> HostState {
    let client = kani_core::http::SmartClient::new(None).unwrap();
    HostState::new(
        client,
        allowed,
        Arc::new(kani_core::cache::InMemoryCache::new()),
        String::new(),
        Arc::new(Mutex::new(None)),
    )
    .unwrap()
}

// ── JSON Fetch: list → detail ─────────────────────────────────────────────────

#[tokio::test]
async fn json_fetch_list_then_detail() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(wiremock::matchers::path("/list"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                r#"[{"id":1,"detail_url":"/detail/1"},{"id":2,"detail_url":"/detail/2"},{"id":3,"detail_url":"/detail/3"}]"#,
            ),
        )
        .mount(&server)
        .await;

    for i in 1..=3 {
        let body = format!(r#"{{"id":{i},"title":"Title {i}"}}"#);
        Mock::given(method("GET"))
            .and(wiremock::matchers::path(format!("/detail/{i}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
    }

    let detail_bp = BlueprintBuilder::new("")
        .field("title", Expr::self_ref().ptr("/title").str_val())
        .build();

    let list_bp = BlueprintBuilder::new("")
        .with_request(RequestDef {
            url: format!("{}/list", server.uri()),
            method: "GET".into(),
            headers: vec![],
            queries: vec![],
        })
        .field("id", Expr::self_ref().ptr("/id").int_val())
        .field(
            "detail",
            Expr::fetch_json(
                Expr::format(
                    "{}{}",
                    vec![
                        Expr::lit(server.uri()),
                        Expr::self_ref().ptr("/detail_url").str_val(),
                    ],
                ),
                detail_bp,
            ),
        )
        .build();

    let base_url = server.uri();
    let mut state = make_state(AllowedHost::Restricted(base_url.to_string()));
    let result = extract_json(&mut state, None, &list_bp).await.unwrap();

    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "expected 3 rows");
    assert_eq!(rows[0]["id"], 1);
    assert_eq!(rows[0]["detail"]["title"], "Title 1");
    assert_eq!(rows[1]["detail"]["title"], "Title 2");
    assert_eq!(rows[2]["detail"]["title"], "Title 3");
    assert_eq!(state.io_count, 4, "1 list fetch + 3 detail fetches = 4");
}

// ── HTML Fetch: single sub-fetch ──────────────────────────────────────────────

#[tokio::test]
async fn html_fetch_sub_blueprint() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(wiremock::matchers::path("/list"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"<ul><li><a href="/item/1">A</a></li><li><a href="/item/2">B</a></li></ul>"#,
        ))
        .mount(&server)
        .await;

    for (i, name) in [(1, "Detail A"), (2, "Detail B")] {
        let body = format!(r#"<html><body><h1>{name}</h1></body></html>"#);
        Mock::given(method("GET"))
            .and(wiremock::matchers::path(format!("/item/{i}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
    }

    let detail_bp = BlueprintBuilder::new(":root")
        .field("heading", Expr::dom("h1").text())
        .build();

    let list_bp = BlueprintBuilder::new("li")
        .with_request(RequestDef {
            url: format!("{}/list", server.uri()),
            method: "GET".into(),
            headers: vec![],
            queries: vec![],
        })
        .field("href", Expr::self_ref().first("a").attr("href"))
        .field(
            "detail",
            Expr::fetch_html(
                Expr::format(
                    "{}{}",
                    vec![
                        Expr::lit(server.uri()),
                        Expr::self_ref().first("a").attr("href"),
                    ],
                ),
                detail_bp,
            ),
        )
        .build();

    let base_url = server.uri();
    let mut state = make_state(AllowedHost::Restricted(base_url.to_string()));
    let result = extract_html(&mut state, None, &list_bp).await.unwrap();

    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["detail"]["heading"], "Detail A");
    assert_eq!(rows[1]["detail"]["heading"], "Detail B");
    assert_eq!(state.io_count, 3, "1 list + 2 detail fetches = 3");
}

// ── Disallowed host rejected ──────────────────────────────────────────────────

#[tokio::test]
async fn fetch_disallowed_host_is_rejected() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(wiremock::matchers::path("/list"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"[{"url":"https://evil.example.com/page"}]"#),
        )
        .mount(&server)
        .await;

    let detail_bp = BlueprintBuilder::new("").build();
    let list_bp = BlueprintBuilder::new("")
        .with_request(RequestDef {
            url: format!("{}/list", server.uri()),
            method: "GET".into(),
            headers: vec![],
            queries: vec![],
        })
        .field(
            "data",
            Expr::fetch_json(Expr::self_ref().ptr("/url").str_val(), detail_bp),
        )
        .build();

    let base_url = server.uri();
    let mut state = make_state(AllowedHost::Restricted(base_url.to_string()));
    let err = extract_json(&mut state, None, &list_bp).await.unwrap_err();
    assert!(
        err.contains("blocked") || err.contains("only contact"),
        "expected host restriction error, got: {err}"
    );
}

// ── Nested Fetch rejected ─────────────────────────────────────────────────────

#[tokio::test]
async fn nested_fetch_is_rejected() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(wiremock::matchers::path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"[{"inner_url":"http://example.com/inner"}]"#),
        )
        .mount(&server)
        .await;

    let innermost_bp = BlueprintBuilder::new("").build();
    let inner_bp = BlueprintBuilder::new("")
        .field(
            "nested",
            Expr::fetch_json(Expr::self_ref().ptr("/inner_url").str_val(), innermost_bp),
        )
        .build();

    let outer_bp = BlueprintBuilder::new("")
        .with_request(RequestDef {
            url: format!("{}/", server.uri()),
            method: "GET".into(),
            headers: vec![],
            queries: vec![],
        })
        .field(
            "data",
            Expr::fetch_json(Expr::self_ref().ptr("/inner_url").str_val(), inner_bp),
        )
        .build();

    let mut state = make_state(AllowedHost::Unrestricted);
    let err = extract_json(&mut state, None, &outer_bp).await.unwrap_err();
    assert!(
        err.contains("Nested") || err.contains("not allowed"),
        "expected nested Fetch error, got: {err}"
    );
}

// ── I/O budget: 33rd sub-fetch trips the limit ───────────────────────────────

#[tokio::test]
async fn fetch_budget_exceeded_after_32_requests() {
    let server = MockServer::start().await;

    let list_items: String = (0..32)
        .map(|i| format!(r#"{{"url":"/item/{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let list_body = format!("[{list_items}]");

    Mock::given(method("GET"))
        .and(wiremock::matchers::path("/list"))
        .respond_with(ResponseTemplate::new(200).set_body_string(list_body))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"x":1}"#))
        .mount(&server)
        .await;

    let detail_bp = BlueprintBuilder::new("").build();
    let list_bp = BlueprintBuilder::new("")
        .with_request(RequestDef {
            url: format!("{}/list", server.uri()),
            method: "GET".into(),
            headers: vec![],
            queries: vec![],
        })
        .field(
            "data",
            Expr::fetch_json(
                Expr::format(
                    "{}{}",
                    vec![
                        Expr::lit(server.uri()),
                        Expr::self_ref().ptr("/url").str_val(),
                    ],
                ),
                detail_bp,
            ),
        )
        .build();

    let mut state = make_state(AllowedHost::Unrestricted);
    let err = extract_json(&mut state, None, &list_bp).await.unwrap_err();
    assert!(
        err.contains("maximum") || err.contains("exceeded"),
        "expected budget exceeded error, got: {err}"
    );
}
