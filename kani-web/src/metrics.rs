use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::IntoResponse;
use axum::routing::get;
use axum_prometheus::metrics_exporter_prometheus::PrometheusHandle;
use axum_prometheus::{EndpointLabel, PrometheusMetricLayer, PrometheusMetricLayerBuilder};
use std::sync::OnceLock;

/// Collapses paths that axum could not match to a route. Without this every
/// content-hashed asset filename becomes its own Prometheus time series, and
/// every rebuild mints a fresh set — an unbounded cardinality leak. Real routes
/// are unaffected: they carry a MatchedPath and never reach this.
fn collapse_unmatched_path(path: &str) -> String {
    for prefix in ["/js/", "/css/", "/fonts/", "/icons/", "/locales/"] {
        if path.starts_with(prefix) {
            return format!("{prefix}*");
        }
    }
    // Everything else is the SPA fallback or a 404 — one server-side operation,
    // and an open door for cardinality if a crawler walks random URLs.
    "/other".to_string()
}

pub fn prometheus() -> &'static (PrometheusMetricLayer<'static>, PrometheusHandle) {
    static PROM: OnceLock<(PrometheusMetricLayer<'static>, PrometheusHandle)> = OnceLock::new();
    PROM.get_or_init(|| {
        PrometheusMetricLayerBuilder::new()
            .with_endpoint_label_type(EndpointLabel::MatchedPathWithFallbackFn(
                collapse_unmatched_path,
            ))
            .with_default_metrics()
            .build_pair()
    })
}

pub fn describe() {
    let _ = prometheus();

    metrics::describe_counter!(
        "kani_log_errors_total",
        "Total ERROR-level log events emitted"
    );
    metrics::describe_gauge!("kani_sse_clients", "Currently connected SSE clients");
    metrics::describe_gauge!("kani_jobs_running", "Background jobs currently running");
    metrics::describe_counter!(
        "kani_jobs_failed_total",
        "Background jobs that terminated in failure"
    );
    metrics::describe_gauge!(
        "kani_downloads_active",
        "Chapter downloads currently in flight"
    );
    metrics::describe_counter!(
        "kani_wasm_calls_total",
        "Extension backend calls, by extension and method"
    );
    metrics::describe_histogram!(
        "kani_wasm_call_duration_seconds",
        "Extension backend call latency, by extension and method"
    );
    metrics::describe_counter!(
        "kani_wasm_call_errors_total",
        "Extension backend calls that returned an error"
    );
    metrics::describe_gauge!(
        "kani_circuit_open",
        "1 when the per-host circuit breaker is open, 0 otherwise"
    );
    metrics::describe_counter!("kani_v8_calls_total", "Total calls into the V8 subprocess");
    metrics::describe_counter!(
        "kani_v8_process_restarts_total",
        "Times the V8 subprocess has been restarted"
    );

    metrics::counter!("kani_log_errors_total").increment(0);
    metrics::gauge!("kani_sse_clients").set(0.0);
    metrics::gauge!("kani_jobs_running").set(0.0);
    metrics::gauge!("kani_downloads_active").set(0.0);
    sync_runtime_counters();
}

pub fn router(state: crate::state::AppState) -> Router {
    Router::new()
        .route("/metrics", get(render))
        .with_state(MetricsState {
            handle: prometheus().1.clone(),
            app: state,
        })
}

#[derive(Clone)]
pub struct MetricsState {
    handle: PrometheusHandle,
    app: crate::state::AppState,
}

fn token_matches(supplied: &str, expected: &str) -> bool {
    if supplied.len() != expected.len() {
        return false;
    }
    supplied
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
}

/// Unlike /health, this endpoint discloses extension names, upstream host names
/// via the circuit gauge, version and error counts — so an unset shared token
/// denies rather than allows.
fn authorized_with(headers: &HeaderMap, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    bearer(headers).is_some_and(|t| token_matches(t, expected))
}

/// A `metrics:read`-scoped API token is the preferred credential: it is
/// revocable per scraper and auditable via `last_used_at`, neither of which the
/// shared env-var token offers. `KANI_METRICS_TOKEN` stays supported because a
/// token belongs to a user account, and monitoring should not go dark because
/// that account was disabled.
async fn authorized_by_scoped_token(app: &crate::state::AppState, headers: &HeaderMap) -> bool {
    let Some(raw) = bearer(headers) else {
        return false;
    };
    match app.service.authenticate_api_token(raw).await {
        Ok(Some(auth)) => {
            auth.kind == kani_app::service::api_tokens::TokenKind::Api
                && auth
                    .scopes
                    .contains(&kani_app::permissions::Permission::Metrics(
                        kani_app::permissions::Metrics::Read,
                    ))
        }
        _ => false,
    }
}

pub fn sync_runtime_counters() {
    let stats = kani_core::v8_process::browser_stats();
    metrics::counter!("kani_v8_calls_total").absolute(stats.calls_total);
    metrics::counter!("kani_v8_process_restarts_total").absolute(stats.restarts);
}

async fn render(State(state): State<MetricsState>, headers: HeaderMap) -> impl IntoResponse {
    let expected = std::env::var("KANI_METRICS_TOKEN").unwrap_or_default();
    if !authorized_with(&headers, &expected)
        && !authorized_by_scoped_token(&state.app, &headers).await
    {
        let body = if expected.is_empty() {
            "metrics require a credential: mint an API token scoped to \
             `metrics:read`, or set KANI_METRICS_TOKEN, and scrape with \
             `Authorization: Bearer <token>`"
        } else {
            "unauthorized"
        };
        return (StatusCode::UNAUTHORIZED, body).into_response();
    }
    sync_runtime_counters();
    (StatusCode::OK, state.handle.render()).into_response()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn unmatched_static_paths_collapse_to_one_label_each() {
        assert_eq!(
            collapse_unmatched_path("/js/dist/chunk-GAAGDHTX.js"),
            "/js/*"
        );
        assert_eq!(
            collapse_unmatched_path("/js/dist/settings-4Y7Q3MMA.js"),
            "/js/*"
        );
        assert_eq!(collapse_unmatched_path("/css/main.css"), "/css/*");
        assert_eq!(collapse_unmatched_path("/fonts/x.woff2"), "/fonts/*");
    }

    #[test]
    fn arbitrary_paths_share_a_single_bucket() {
        assert_eq!(collapse_unmatched_path("/library"), "/other");
        assert_eq!(collapse_unmatched_path("/settings"), "/other");
        assert_eq!(collapse_unmatched_path("/wp-admin.php"), "/other");
    }

    #[test]
    fn token_matches_accepts_identical_tokens() {
        assert!(token_matches("s3cret", "s3cret"));
    }

    #[test]
    fn token_matches_rejects_wrong_or_truncated_tokens() {
        assert!(!token_matches("s3cret", "s3creT"));
        assert!(!token_matches("s3cre", "s3cret"));
        assert!(!token_matches("", "s3cret"));
        assert!(!token_matches("s3cretlonger", "s3cret"));
    }

    fn bearer(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, value.parse().unwrap());
        h
    }

    #[test]
    fn an_unset_token_denies_rather_than_exposing_metrics() {
        assert!(!authorized_with(&HeaderMap::new(), ""));
        assert!(
            !authorized_with(&bearer("Bearer anything"), ""),
            "no configured token means no scraping, whatever the caller sends"
        );
    }

    #[test]
    fn configured_token_requires_matching_bearer() {
        assert!(authorized_with(&bearer("Bearer s3cret"), "s3cret"));
    }

    #[test]
    fn configured_token_rejects_missing_or_wrong_credentials() {
        assert!(!authorized_with(&HeaderMap::new(), "s3cret"));
        assert!(!authorized_with(&bearer("Bearer nope"), "s3cret"));
        assert!(!authorized_with(&bearer("s3cret"), "s3cret"));
        assert!(!authorized_with(&bearer("Basic s3cret"), "s3cret"));
    }
}
