use axum::{
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

fn compute_etag(body: &[u8]) -> String {
    format!("\"{}\"", xxhash_rust::xxh3::xxh3_64(body))
}

fn matches_if_none_match(request_headers: &HeaderMap, etag: &str) -> bool {
    request_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag)
}

pub fn etag_json_response<T: Serialize>(
    request_headers: &HeaderMap,
    data: &T,
) -> Result<Response, serde_json::Error> {
    let body = serde_json::to_vec(data)?;
    Ok(build_etag_response(request_headers, body))
}

pub fn etag_bytes_response(request_headers: &HeaderMap, body: impl AsRef<[u8]>) -> Response {
    build_etag_response(request_headers, body.as_ref().to_vec())
}

fn build_etag_response(request_headers: &HeaderMap, body: Vec<u8>) -> Response {
    let etag = compute_etag(&body);

    if matches_if_none_match(request_headers, &etag) {
        return StatusCode::NOT_MODIFIED.into_response();
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&etag).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );

    (StatusCode::OK, headers, body).into_response()
}
