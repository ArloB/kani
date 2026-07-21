use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::IntoResponse;
use axum::routing::get;
use axum_prometheus::PrometheusMetricLayer;
use axum_prometheus::metrics_exporter_prometheus::PrometheusHandle;
use std::sync::OnceLock;

pub fn prometheus() -> &'static (PrometheusMetricLayer<'static>, PrometheusHandle) {
    static PROM: OnceLock<(PrometheusMetricLayer<'static>, PrometheusHandle)> = OnceLock::new();
    PROM.get_or_init(PrometheusMetricLayer::pair)
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

pub fn router() -> Router {
    Router::new()
        .route("/metrics", get(render))
        .with_state(prometheus().1.clone())
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

fn authorized_with(headers: &HeaderMap, expected: &str) -> bool {
    if expected.is_empty() {
        return true;
    }
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| token_matches(t, expected))
}

pub fn sync_runtime_counters() {
    let stats = kani_core::v8_process::browser_stats();
    metrics::counter!("kani_v8_calls_total").absolute(stats.calls_total);
    metrics::counter!("kani_v8_process_restarts_total").absolute(stats.restarts);
}

async fn render(State(handle): State<PrometheusHandle>, headers: HeaderMap) -> impl IntoResponse {
    let expected = std::env::var("KANI_METRICS_TOKEN").unwrap_or_default();
    if !authorized_with(&headers, &expected) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    sync_runtime_counters();
    (StatusCode::OK, handle.render()).into_response()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

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
    fn unset_token_leaves_metrics_unauthenticated() {
        assert!(authorized_with(&HeaderMap::new(), ""));
        assert!(authorized_with(&bearer("Bearer anything"), ""));
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
