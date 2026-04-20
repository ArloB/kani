//! Host ABI wrapper for WASM extensions.
//!
//! This module provides a high-level API for extensions to interact with the host,
//! wrapping the low-level `wit-bindgen` generated code.

use crate::{ExtensionError, bindings::kani::extension::{html, http, json, utility}};

// Re-export common types
pub use http::Method as HttpMethod;
pub type DocumentHandle = html::DocHandle;
pub type ListHandle = html::ListHandle;

// ============================================================
// HTTP API
// ============================================================

/// High-level API for making HTTP requests from extensions.
pub struct HttpRequest {
    method: HttpMethod,
    url: Option<String>,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    queries: Vec<(String, String)>,
}

impl HttpRequest {
    /// Create a new HTTP request builder
    pub fn new() -> Self {
        Self {
            method: HttpMethod::Get,
            url: None,
            headers: Vec::new(),
            body: None,
            queries: Vec::new(),
        }
    }
    
    fn new_method<S: Into<String>>(url: S, method: HttpMethod) -> Self {
        Self {
            url: Some(url.into()),
            method,
            headers: Vec::new(),
            body: None,
            queries: Vec::new(), // Assuming you added this from the previous step
        }
    }

    /// Create a new GET request (convenience method)
    pub fn get<S: Into<String>>(url: S) -> Self { Self::new_method(url, HttpMethod::Get) }

    /// Create a new POST request (convenience method)
    pub fn post<S: Into<String>>(url: S) -> Self { Self::new_method(url, HttpMethod::Post) }
    
    /// Create a new PUT request (convenience method)
    pub fn put<S: Into<String>>(url: S) -> Self { Self::new_method(url, HttpMethod::Put) }
    
    /// Create a new DELETE request (convenience method)
    pub fn delete<S: Into<String>>(url: S) -> Self { Self::new_method(url, HttpMethod::Delete) }

    /// Set the HTTP method
    pub fn method(mut self, method: HttpMethod) -> Self {
        self.method = method;
        self
    }

    /// Set the URL
    pub fn url<S: Into<String>>(mut self, url: S) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Add a header to the request
    pub fn header<K: Into<String>, V: std::fmt::Display>(mut self, key: K, value: V) -> Self {
        self.headers.push((key.into(), value.to_string()));
        self
    }

    /// Set the request body
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }
    
    #[cfg(feature = "host")]
    pub fn json<T: serde::Serialize>(mut self, payload: &T) -> Result<Self, ExtensionError> {
        let body = serde_json::to_vec(payload)
            .map_err(|e| ExtensionError::ParseError(e.to_string()))?;

        self.body = Some(body);
        Ok(self.header("Content-Type", "application/json"))
    }
    
    pub fn form<K: std::fmt::Display, V: std::fmt::Display>(mut self, data: &[(K, V)]) -> Self {
        let string_data: Vec<(String, String)> = data.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        
        let form_string = crate::utility::encode_form(&string_data);
            
        self.body = Some(form_string.into_bytes());
        self.header("Content-Type", "application/x-www-form-urlencoded")
    }
    
    /// Appends a URL-encoded query parameter to the request.
    pub fn query<K: Into<String>, V: std::fmt::Display>(mut self, key: K, value: V) -> Self {
        self.queries.push((key.into(), value.to_string()));
        self
    }
    
    /// Build the final URL with query parameters.
    fn build_final_url(&self) -> Result<String, ExtensionError> {
        let url = match &self.url {
            Some(u) => u,
            None => return Err(ExtensionError::Other("URL is not set".to_string())),
        };
        
        if self.queries.is_empty() {
            return Ok(url.clone());
        }

        crate::utility::build_url(url, &self.queries)
            .map_err(crate::ExtensionError::Other)
    }

    /// Send the request and get the response body as a string.
    pub fn send(self) -> Result<http::Response, ExtensionError> {
        let url = self.build_final_url()?;

        let req = http::Request {
            method: self.method,
            url,
            headers: self.headers,
            body: self.body,
        };

        match http::send(&req) {
            Ok(res) => Ok(res),
            Err(e) => Err(ExtensionError::NetworkError(e)),
        }
    }
    
    pub fn send_html(self) -> Result<HtmlDocument, ExtensionError> {
        let resp = self.send()?;
        let html_string = String::from_utf8(resp.body)
            .map_err(|_| ExtensionError::ParseError("Invalid UTF-8 in HTML".into()))?;
        HtmlDocument::new(&html_string)
    }

    pub fn send_json_handle(self) -> Result<JsonHandle, ExtensionError> {
        let res = self.send()?;

        if res.body.is_empty() {
            return Err(ExtensionError::NetworkError(
                "Empty response".to_string(),
            ));
        }

        JsonHandle::parse(&res.body)
    }
    
    #[cfg(feature = "host")]
    pub fn send_json<T: serde::de::DeserializeOwned>(self) -> Result<T, ExtensionError> {
        let resp = self.send()?;
        serde_json::from_slice(&resp.body)
            .map_err(|e| ExtensionError::ParseError(e.to_string()))
    }
}

impl Default for HttpRequest {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// HTML Parsing API
// ============================================================

pub struct HtmlDocument {
    handle: DocumentHandle,
}

impl HtmlDocument {
    /// Parse an HTML string into a document
    pub fn new(html: &str) -> Result<Self, ExtensionError> {
        let handle = html::parse(html).map_err(ExtensionError::Other)?;
        Ok(Self { handle })
    }

    /// Get the raw handle (for advanced usage)
    pub fn handle(&self) -> DocumentHandle {
        self.handle
    }

    /// Select all elements matching a CSS selector
    pub fn select(&self, selector: &str) -> Vec<HtmlDocument> {
        let list_handle = match html::select(self.handle, selector) {
            Ok(h) => h,
            Err(_) => return Vec::new(),
        };

        let len = html::list_len(list_handle).unwrap_or(0);
        let mut docs = Vec::with_capacity(len as usize);

        for i in 0..len {
            if let Ok(doc_h) = html::list_get(list_handle, i) {
                docs.push(HtmlDocument { handle: doc_h });
            }
        }

        html::drop_list(list_handle);
        docs
    }

    /// Get the first element matching a CSS selector, or None if not found
    pub fn first(&self, selector: &str) -> Option<HtmlDocument> {
        html::first(self.handle, selector)
            .ok()
            .flatten()
            .map(|h| HtmlDocument { handle: h })
    }

    /// Get all direct child elements
    pub fn children(&self) -> Vec<HtmlDocument> {
        let list_handle = match html::children(self.handle) {
            Ok(h) => h,
            Err(_) => return Vec::new(),
        };

        let len = html::list_len(list_handle).unwrap_or(0);
        let mut docs = Vec::with_capacity(len as usize);

        for i in 0..len {
            if let Ok(doc_h) = html::list_get(list_handle, i) {
                docs.push(HtmlDocument { handle: doc_h });
            }
        }

        html::drop_list(list_handle);
        docs
    }

    /// Get an attribute from the first element matching the selector
    pub fn attr(&self, selector: &str, attribute: &str) -> String {
        html::attr(self.handle, selector, attribute)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Get an attribute from this element directly
    pub fn get_attr(&self, attribute: &str) -> String {
        html::attr(self.handle, "", attribute)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Get text content from the first element matching the selector
    pub fn text(&self, selector: &str) -> String {
        html::text(self.handle, selector)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Get text content from this element directly
    pub fn get_text(&self) -> String {
        html::text(self.handle, "")
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Get the inner HTML (content without the outer tag)
    pub fn inner_html(&self) -> String {
        html::inner_html(self.handle)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Get the outer HTML (full element including tag)
    pub fn outer_html(&self) -> String {
        html::outer_html(self.handle)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Check if this element has a specific class
    pub fn has_class(&self, class_name: &str) -> bool {
        let classes = self.get_attr("class");
        classes.split_whitespace().any(|c| c == class_name)
    }

    /// Get the tag name of this element
    pub fn tag_name(&self) -> String {
        let html = self.outer_html();
        if html.starts_with('<')
            && let Some(stripped) = html.strip_prefix('<')
        {
            return stripped
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or("")
                .to_string();
        }
        String::new()
    }
}

impl std::fmt::Debug for HtmlDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let content = self.outer_html(); 
        let preview = if content.len() > 100 {
            format!("{}...", &content[..100])
        } else {
            content
        };
        write!(f, "HtmlDocument(handle: {}, html: {:?})", self.handle, preview)
    }
}

impl Drop for HtmlDocument {
    fn drop(&mut self) {
        html::drop_doc(self.handle);
    }
}

// ============================================================
// Utility API
// ============================================================

/// Parse a date string with the given chrono format into epoch seconds.
/// Returns `None` if parsing fails.
pub fn parse_date(date: &str, fmt: &str) -> Option<i64> {
    let result = utility::date_parse(date, fmt);
    match result {
        Ok(t) if t >= 0 => Some(t),
        _ => None,
    }
}

pub fn parse_date_rfc3339(date: &str) -> Option<i64> {
    let result = utility::date_parse_rfc3339(date);
    match result {
        Ok(t) if t >= 0 => Some(t),
        _ => None,
    }
}

/// Joins a base URL and a relative path, handling slashes.
pub fn resolve<B: Into<String>, P: Into<String>>(base: B, path: P) -> Result<String, String> {
    crate::utility::resolve_url(&base.into(), &path.into())
}

/// URL-encode a raw string
pub fn encode<S: Into<String>>(input: S) -> String {
    crate::utility::url_encode(&input.into())
}

/// URL-decode a percent-encoded string.
pub fn decode<S: Into<String>>(input: S) -> Result<String, String> {
    crate::utility::url_decode(&input.into())
}

/// Extract the value of a specific query parameter from a full URL.
pub fn get_query_param<U: Into<String>, K: Into<String>>(url: U, key: K) -> Option<String> {
    crate::utility::get_query_param(&url.into(), &key.into())
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);
        $crate::utility::log(1, line!(), &msg);
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);
        $crate::utility::log(3, line!(), &msg);
    };
}

#[macro_export]
macro_rules! dbg {
    () => {
        $crate::utility::log(0, line!(), "");
    };
    ($val:expr $(,)?) => {
        match $val {
            tmp => {
                let msg = format!("{} = {:#?}", stringify!($val), &tmp);
                $crate::utility::log(0, line!(), &msg);
                tmp
            }
        }
    };
}

// ============================================================
// JSON API
// ============================================================

/// RAII wrapper around a JSON handle. Automatically drops the host-side
/// Value when this goes out of scope.
pub struct JsonHandle {
    handle: json::JsonHandle,
}

impl JsonHandle {
    /// Wrap a raw handle already registered on the host side (e.g. returned by extract_html).
    pub fn from_raw(handle: json::JsonHandle) -> Self {
        Self { handle }
    }

    /// Parse raw response bytes into a host-side JSON value.
    pub fn parse(data: &[u8]) -> Result<Self, ExtensionError> {
        let handle = json::parse(data).map_err(ExtensionError::ParseError)?;
        Ok(Self { handle })
    }

    /// Get a string at the given JSON Pointer path.
    pub fn get_str(&self, ptr: &str) -> Option<String> {
        json::get_str(self.handle, ptr).ok().flatten()
    }

    /// Get a required string, returning ParseError if absent.
    pub fn require_str(&self, ptr: &str) -> Result<String, ExtensionError> {
        self.get_str(ptr).ok_or_else(|| {
            ExtensionError::ParseError(format!("Missing required field: {}", ptr))
        })
    }

    pub fn get_i64(&self, ptr: &str) -> Option<i64> {
        json::get_i64(self.handle, ptr).ok().flatten()
    }

    pub fn get_f64(&self, ptr: &str) -> Option<f64> {
        json::get_f64(self.handle, ptr).ok().flatten()
    }

    pub fn get_bool(&self, ptr: &str) -> Option<bool> {
        json::get_bool(self.handle, ptr).ok().flatten()
    }

    /// Returns the length of an array at the given path.
    pub fn array_len(&self, ptr: &str) -> Option<i32> {
        json::array_len(self.handle, ptr).ok().flatten()
    }

    /// Returns a child handle for the element at ptr[index].
    pub fn array_get(&self, ptr: &str, index: i32) -> Result<JsonHandle, ExtensionError> {
        let child =
            json::array_get(self.handle, ptr, index).map_err(ExtensionError::ParseError)?;
        Ok(JsonHandle { handle: child })
    }

    /// Iterate over all elements of an array at the given path.
    pub fn array_iter(&self, ptr: &str) -> impl Iterator<Item = JsonHandle> + '_ {
        let len = self.array_len(ptr).unwrap_or(0);
        let ptr = ptr.to_string();
        (0..len).filter_map(move |i| self.array_get(&ptr, i).ok())
    }

    pub fn object_keys(&self, ptr: &str) -> Vec<String> {
        json::object_keys(self.handle, ptr).ok().unwrap_or_default()
    }

    /// Returns a child handle for the value at object[key] under `ptr`.
    /// Returns `None` if the key doesn't exist or `ptr` is not an object.
    pub fn object_get(&self, ptr: &str, key: &str) -> Option<JsonHandle> {
        json::object_get(self.handle, ptr, key)
            .ok()
            .flatten()
            .map(|h| JsonHandle { handle: h })
    }

    /// Iterate over all (key, value) pairs of a JSON object at `ptr`.
    pub fn object_iter<'a>(
        &'a self,
        ptr: &'a str,
    ) -> impl Iterator<Item = (String, JsonHandle)> + 'a {
        self.object_keys(ptr).into_iter().filter_map(move |k| {
            self.object_get(ptr, &k).map(|v| (k, v))
        })
    }

    // ── Blueprint result accessors ───────────────────────────────────────────
    // Blueprint results are returned as { "rows": [...], "scalars": {...} }.

    /// Number of rows returned by a blueprint extraction.
    pub fn rows_len(&self) -> i32 {
        self.array_len("/rows").unwrap_or(0)
    }

    /// Get the row at `index` from a blueprint extraction result.
    pub fn rows_get(&self, index: i32) -> Result<JsonHandle, crate::ExtensionError> {
        self.array_get("/rows", index)
    }

    /// Iterate over all rows from a blueprint extraction result.
    pub fn rows_iter(&self) -> impl Iterator<Item = JsonHandle> + '_ {
        let len = self.rows_len();
        (0..len).filter_map(|i| self.rows_get(i).ok())
    }

    /// Get a scalar value (document-level) from a blueprint extraction result.
    pub fn get_scalar_str(&self, name: &str) -> Option<String> {
        self.get_str(&format!("/scalars/{}", name))
    }

    /// Get a scalar boolean from a blueprint extraction result. Returns `false` if absent.
    pub fn get_scalar_bool(&self, name: &str) -> bool {
        self.get_bool(&format!("/scalars/{}", name)).unwrap_or(false)
    }

    /// Get a scalar integer from a blueprint extraction result.
    pub fn get_scalar_i64(&self, name: &str) -> Option<i64> {
        self.get_i64(&format!("/scalars/{}", name))
    }

    // ── Array-of-strings helper ──────────────────────────────────────────────

    /// Collect all string elements of a JSON array at `ptr` into a `Vec<String>`.
    /// Non-string elements and missing values are silently skipped.
    pub fn get_array_of_strings(&self, ptr: &str) -> Vec<String> {
        self.array_iter(ptr).filter_map(|h| h.get_str("")).collect()
    }
}

impl std::fmt::Debug for JsonHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let json_str = json::to_string(self.handle).unwrap_or_else(|_| "null".to_string());
        write!(f, "JsonHandle(handle: {}, value: {})", self.handle, json_str)
    }
}

impl Drop for JsonHandle {
    fn drop(&mut self) {
        json::drop_json(self.handle);
    }
}

// ============================================================
// BlueprintBuilder extension — request() using HttpRequest
// ============================================================

#[cfg(feature = "builder")]
impl crate::ast::BlueprintBuilder {
    /// Attach an HTTP request that the host will fetch before running extraction.
    /// Converts the `HttpRequest` builder into a `RequestDef` (body is not forwarded —
    /// blueprint requests are used for fetching pages, not posting data).
    pub fn request(self, req: HttpRequest) -> Self {
        self.with_request(http_request_to_def(req))
    }
}

#[cfg(feature = "builder")]
impl crate::ast::Blueprint {
    /// Return a clone of this blueprint with a different HTTP request attached.
    /// Useful for loop patterns where the URL/params change each iteration but
    /// the field definitions stay the same.
    pub fn with_request(&self, req: HttpRequest) -> crate::ast::Blueprint {
        self.with_request_def(http_request_to_def(req))
    }
}

#[cfg(feature = "builder")]
fn http_request_to_def(req: HttpRequest) -> crate::ast::RequestDef {
    crate::ast::RequestDef {
        url:     req.url.unwrap_or_default(),
        method:  match req.method {
            HttpMethod::Get    => "GET",
            HttpMethod::Post   => "POST",
            HttpMethod::Put    => "PUT",
            HttpMethod::Delete => "DELETE",
        }.to_string(),
        headers: req.headers,
        queries: req.queries,
    }
}

// ============================================================
// Extraction API
// ============================================================

#[cfg(feature = "builder")]
pub mod extract {
    use crate::ast::Blueprint;
    use crate::bindings::kani::extension::extraction;
    use crate::host_abi::JsonHandle;
    use crate::ExtensionError;

    /// Run a blueprint extraction over an HTML document handle.
    /// Returns a `JsonHandle` wrapping `[{field: value, ...}, ...]`.
    pub fn html(doc: Option<i32>, blueprint: &Blueprint) -> Result<JsonHandle, ExtensionError> {
        let bytes = blueprint.to_bytes();
        let handle = extraction::extract_html(doc, &bytes).map_err(ExtensionError::Other)?;
        Ok(JsonHandle::from_raw(handle))
    }

    /// Run a blueprint extraction over a JSON document handle.
    /// Returns a `JsonHandle` wrapping `[{field: value, ...}, ...]`.
    pub fn json(handle: Option<i32>, blueprint: &Blueprint) -> Result<JsonHandle, ExtensionError> {
        let bytes = blueprint.to_bytes();
        let result_handle = extraction::extract_json(handle, &bytes).map_err(ExtensionError::Other)?;
        Ok(JsonHandle::from_raw(result_handle))
    }

    /// Run a paginated HTML extraction.  The blueprint must include a `PaginationConfig`;
    /// the host handles multi-chunk fetching, stitching, and `has_next_page` detection.
    pub fn paginated_html(page: i32, page_size: i32, blueprint: &Blueprint) -> Result<JsonHandle, ExtensionError> {
        let bytes = blueprint.to_bytes();
        let handle = extraction::paginated_extract_html(page, page_size, &bytes).map_err(ExtensionError::Other)?;
        Ok(JsonHandle::from_raw(handle))
    }
}

// ============================================================
// Prefs API
// ============================================================

/// Typed helpers for reading extension preference values stored by the host.
///
/// Values are stored as strings by the host. Each helper converts to the
/// appropriate Rust type based on the preference kind declared in
/// `get_preferences()`.
pub mod prefs {
    fn raw(key: &str) -> Option<String> {
        crate::bindings::kani::extension::prefs::get_value(key)
    }

    /// Returns the stored string value, or an empty string if unset.
    pub fn get_str(key: &str) -> String {
        raw(key).unwrap_or_default()
    }

    /// Returns the stored string value, or `default` if unset or empty.
    pub fn get_str_or(key: &str, default: &str) -> String {
        raw(key).filter(|s| !s.is_empty()).unwrap_or_else(|| default.to_string())
    }

    /// Returns `true` if the stored value is exactly `"true"`.
    pub fn get_bool(key: &str) -> bool {
        raw(key).map(|v| v == "true").unwrap_or(false)
    }

    /// Parses the stored value as `i64`, returning `None` if unset or not parseable.
    pub fn get_i64(key: &str) -> Option<i64> {
        raw(key)?.parse().ok()
    }

    /// Parses the stored value as `f64`, returning `None` if unset or not parseable.
    pub fn get_f64(key: &str) -> Option<f64> {
        raw(key)?.parse().ok()
    }
}
