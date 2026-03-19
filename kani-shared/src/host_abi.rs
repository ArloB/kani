//! Host ABI wrapper for WASM extensions.
//!
//! This module provides a high-level API for extensions to interact with the host,
//! wrapping the low-level `wit-bindgen` generated code.

use crate::bindings::kani::extension::{html, http, json, utility};

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
}

impl HttpRequest {
    /// Create a new HTTP request builder
    pub fn new() -> Self {
        Self {
            method: HttpMethod::Get,
            url: None,
            headers: Vec::new(),
            body: None,
        }
    }

    /// Create a new GET request (convenience method)
    pub fn get(url: &str) -> Self {
        Self::new().method(HttpMethod::Get).url(url.to_string())
    }

    /// Create a new POST request (convenience method)
    pub fn post(url: &str) -> Self {
        Self::new().method(HttpMethod::Post).url(url.to_string())
    }

    /// Set the HTTP method
    pub fn method(mut self, method: HttpMethod) -> Self {
        self.method = method;
        self
    }

    /// Set the URL
    pub fn url(mut self, url: String) -> Self {
        self.url = Some(url);
        self
    }

    /// Add a header to the request
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    /// Set the request body
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    /// Send the request and get the response body as a string.
    pub fn send(self) -> Result<http::Response, crate::ExtensionError> {
        let url = match self.url {
            Some(u) => u,
            None => return Err(crate::ExtensionError::Other("URL is not set".to_string())),
        };

        let req = http::Request {
            method: self.method,
            url,
            headers: self.headers,
            body: self.body,
        };

        match http::send(&req) {
            Ok(res) => Ok(res),
            Err(e) => Err(crate::ExtensionError::NetworkError(e)),
        }
    }

    pub fn send_json_handle(self) -> Result<JsonHandle, crate::ExtensionError> {
        let res = self.send()?;

        if res.body.is_empty() {
            return Err(crate::ExtensionError::NetworkError(
                "Empty response".to_string(),
            ));
        }

        JsonHandle::parse(&res.body)
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
    pub fn new(html: &str) -> Result<Self, crate::ExtensionError> {
        let handle = html::parse(html).map_err(crate::ExtensionError::Other)?;
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

// ============================================================
// JSON API
// ============================================================

/// RAII wrapper around a JSON handle. Automatically drops the host-side
/// Value when this goes out of scope.
pub struct JsonHandle {
    handle: json::JsonHandle,
}

impl JsonHandle {
    /// Parse raw response bytes into a host-side JSON value.
    pub fn parse(data: &[u8]) -> Result<Self, crate::ExtensionError> {
        let handle = json::parse(data).map_err(crate::ExtensionError::ParseError)?;
        Ok(Self { handle })
    }

    /// Get a string at the given JSON Pointer path.
    pub fn get_str(&self, ptr: &str) -> Option<String> {
        json::get_str(self.handle, ptr).ok().flatten()
    }

    /// Get a required string, returning ParseError if absent.
    pub fn require_str(&self, ptr: &str) -> Result<String, crate::ExtensionError> {
        self.get_str(ptr).ok_or_else(|| {
            crate::ExtensionError::ParseError(format!("Missing required field: {}", ptr))
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
    pub fn array_get(&self, ptr: &str, index: i32) -> Result<JsonHandle, crate::ExtensionError> {
        let child =
            json::array_get(self.handle, ptr, index).map_err(crate::ExtensionError::ParseError)?;
        Ok(JsonHandle { handle: child })
    }

    /// Iterate over all elements of an array at the given path.
    pub fn array_iter(&self, ptr: &str) -> impl Iterator<Item = JsonHandle> + '_ {
        let len = self.array_len(ptr).unwrap_or(0);
        let ptr = ptr.to_string();
        (0..len).filter_map(move |i| self.array_get(&ptr, i).ok())
    }

    pub fn object_keys(&self, ptr: &str) -> Vec<String> {
        json::object_keys(self.handle, ptr)
            .ok()
            .unwrap_or_default()
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
    pub fn object_iter<'a>(&'a self, ptr: &'a str) -> impl Iterator<Item = (String, JsonHandle)> + 'a {
        self.object_keys(ptr)
            .into_iter()
            .filter_map(move |k| {
                // By using the same 'a, we guarantee ptr lives as long as the iterator
                self.object_get(ptr, &k).map(|v| (k, v))
            })
    }
}

impl Drop for JsonHandle {
    fn drop(&mut self) {
        json::drop_json(self.handle);
    }
}

// ============================================================
// Prefs API
// ============================================================

pub fn get_preference_value(key: &str) -> Option<String> {
    crate::bindings::kani::extension::prefs::get_value(key)
}

pub fn get_preference_list(key: &str) -> Vec<String> {
    get_preference_value(key)
        .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .unwrap_or_default()
}