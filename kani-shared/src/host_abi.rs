//! Host ABI wrapper for WASM extensions.
//!
//! This module provides a high-level API for extensions to interact with the host,
//! wrapping the low-level `wit-bindgen` generated code.

use crate::bindings::kani::extension::{html, http, utility};

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
        html::attr(self.handle, "*", attribute)
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
        html::text(self.handle, "*")
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
