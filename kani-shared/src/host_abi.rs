//! Host ABI definitions for WASM extensions.
//!
//! This module defines the interface between WASM extensions and the host.
//! Extensions can call these functions to interact with the host environment
//! (make HTTP requests, parse HTML, etc.).
//!
//! When compiled to WASM, these will be imported from the host.
//! When compiled to native (for testing), they will panic.

/// HTTP method constants
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get = 0,
    Post = 1,
    Put = 2,
    Delete = 3,
}

/// Handle type for handles
pub type RequestHandle = i32;
pub type ResponseHandle = i32;
pub type DocumentHandle = i32;
pub type ListHandle = i32;

// ============================================================
// External declarations (imported from host when running in WASM)
// ============================================================

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    // --- HTTP Functions ---
    pub fn request_create(method: i32, url_ptr: *const u8, url_len: i32) -> RequestHandle;
    pub fn request_set_header(
        handle: RequestHandle,
        key_ptr: *const u8,
        key_len: i32,
        val_ptr: *const u8,
        val_len: i32,
    );
    pub fn request_send(handle: RequestHandle) -> ResponseHandle;
    pub fn request_drop(handle: RequestHandle);

    pub fn response_get_body(handle: ResponseHandle, buf_ptr: *mut u8, buf_len: i32) -> i32;
    pub fn response_get_body_len(handle: ResponseHandle) -> i32;
    pub fn response_get_status(handle: ResponseHandle) -> i32;
    pub fn response_drop(handle: ResponseHandle);

    // --- HTML Parsing Functions ---

    /// Parse HTML string into a document
    pub fn html_parse(body_ptr: *const u8, body_len: i32) -> DocumentHandle;

    /// Select elements from document, returns list handle
    pub fn html_select(handle: DocumentHandle, sel_ptr: *const u8, sel_len: i32) -> ListHandle;

    /// Get first element matching selector as new document
    pub fn html_first(handle: DocumentHandle, sel_ptr: *const u8, sel_len: i32) -> DocumentHandle;

    /// Get list length
    pub fn html_list_len(handle: ListHandle) -> i32;

    /// Get item from list as new document
    pub fn html_list_get(handle: ListHandle, index: i32) -> DocumentHandle;

    /// Get child elements as a list
    pub fn html_children(handle: DocumentHandle) -> ListHandle;

    /// Get attribute value from first matching element
    pub fn html_attr(
        handle: DocumentHandle,
        sel_ptr: *const u8,
        sel_len: i32,
        attr_ptr: *const u8,
        attr_len: i32,
        buf_ptr: *mut u8,
        buf_len: i32,
    ) -> i32;

    /// Get text content from first matching element
    pub fn html_text(
        handle: DocumentHandle,
        sel_ptr: *const u8,
        sel_len: i32,
        buf_ptr: *mut u8,
        buf_len: i32,
    ) -> i32;

    /// Get inner HTML (content without the element tag)
    pub fn html_inner_html(handle: DocumentHandle, buf_ptr: *mut u8, buf_len: i32) -> i32;

    /// Get outer HTML (full element including tag)
    pub fn html_outer_html(handle: DocumentHandle, buf_ptr: *mut u8, buf_len: i32) -> i32;

    pub fn document_drop(handle: DocumentHandle);
    pub fn list_drop(handle: ListHandle);

    pub fn sys_get_last_error() -> i32;

    // --- Utility Functions ---

    /// Parse a date string with the given format and return epoch seconds
    pub fn date_parse(date_ptr: *const u8, date_len: i32, fmt_ptr: *const u8, fmt_len: i32) -> i64;
}

// ============================================================
// Safe wrapper functions
// ============================================================

// --- HTTP Wrappers ---

#[cfg(target_arch = "wasm32")]
pub fn http_get(url: &str) -> RequestHandle {
    unsafe { request_create(HttpMethod::Get as i32, url.as_ptr(), url.len() as i32) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn http_get(_url: &str) -> RequestHandle {
    panic!("http_get can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn http_post(url: &str) -> RequestHandle {
    unsafe { request_create(HttpMethod::Post as i32, url.as_ptr(), url.len() as i32) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn http_post(_url: &str) -> RequestHandle {
    panic!("http_post can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn set_header(handle: RequestHandle, key: &str, value: &str) {
    unsafe {
        request_set_header(
            handle,
            key.as_ptr(),
            key.len() as i32,
            value.as_ptr(),
            value.len() as i32,
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set_header(_handle: RequestHandle, _key: &str, _value: &str) {
    panic!("set_header can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn send_request(handle: RequestHandle) -> ResponseHandle {
    unsafe { request_send(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn send_request(_handle: RequestHandle) -> ResponseHandle {
    panic!("send_request can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn get_response_body(handle: ResponseHandle) -> String {
    let len = unsafe { response_get_body_len(handle) };
    if len < 0 {
        return String::new();
    }
    let mut buf = vec![0u8; len as usize];
    unsafe { response_get_body(handle, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf8_lossy(&buf).to_string()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get_response_body(_handle: ResponseHandle) -> String {
    panic!("get_response_body can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn get_response_status(handle: ResponseHandle) -> i32 {
    unsafe { response_get_status(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get_response_status(_handle: ResponseHandle) -> i32 {
    panic!("get_response_status can only be called from WASM context");
}

// --- HTML Parsing Wrappers ---

#[cfg(target_arch = "wasm32")]
pub fn parsed_html(html: &str) -> DocumentHandle {
    unsafe { html_parse(html.as_ptr(), html.len() as i32) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn parsed_html(_html: &str) -> DocumentHandle {
    panic!("parsed_html can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn select(handle: DocumentHandle, selector: &str) -> ListHandle {
    unsafe { html_select(handle, selector.as_ptr(), selector.len() as i32) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn select(_handle: DocumentHandle, _selector: &str) -> ListHandle {
    panic!("select can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn list_len(handle: ListHandle) -> i32 {
    unsafe { html_list_len(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_len(_handle: ListHandle) -> i32 {
    panic!("list_len can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn list_get(handle: ListHandle, index: i32) -> DocumentHandle {
    unsafe { html_list_get(handle, index) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_get(_handle: ListHandle, _index: i32) -> DocumentHandle {
    panic!("list_get can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn attr(handle: DocumentHandle, selector: &str, attr: &str) -> String {
    let mut buf = vec![0u8; 4096]; // Fixed size buffer for simple attr extraction
    let len = unsafe {
        html_attr(
            handle,
            selector.as_ptr(),
            selector.len() as i32,
            attr.as_ptr(),
            attr.len() as i32,
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    if len < 0 {
        return String::new();
    }
    buf.truncate(len as usize);
    String::from_utf8_lossy(&buf).to_string()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn attr(_handle: DocumentHandle, _selector: &str, _attr: &str) -> String {
    panic!("attr can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn text(handle: DocumentHandle, selector: &str) -> String {
    let mut buf = vec![0u8; 16384]; // 16KB guess
    let len = unsafe {
        html_text(
            handle,
            selector.as_ptr(),
            selector.len() as i32,
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    if len < 0 {
        return String::new();
    }
    buf.truncate(len as usize);
    String::from_utf8_lossy(&buf).to_string()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn text(_handle: DocumentHandle, _selector: &str) -> String {
    panic!("text can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn first(handle: DocumentHandle, selector: &str) -> Option<DocumentHandle> {
    let h = unsafe { html_first(handle, selector.as_ptr(), selector.len() as i32) };
    if h >= 0 { Some(h) } else { None }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn first(_handle: DocumentHandle, _selector: &str) -> Option<DocumentHandle> {
    panic!("first can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn children(handle: DocumentHandle) -> ListHandle {
    unsafe { html_children(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn children(_handle: DocumentHandle) -> ListHandle {
    panic!("children can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn inner_html(handle: DocumentHandle) -> String {
    let mut buf = vec![0u8; 32768]; // 32KB
    let len = unsafe { html_inner_html(handle, buf.as_mut_ptr(), buf.len() as i32) };
    if len < 0 {
        return String::new();
    }
    buf.truncate(len as usize);
    String::from_utf8_lossy(&buf).to_string()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn inner_html(_handle: DocumentHandle) -> String {
    panic!("inner_html can only be called from WASM context");
}

#[cfg(target_arch = "wasm32")]
pub fn outer_html(handle: DocumentHandle) -> String {
    let mut buf = vec![0u8; 32768]; // 32KB
    let len = unsafe { html_outer_html(handle, buf.as_mut_ptr(), buf.len() as i32) };
    if len < 0 {
        return String::new();
    }
    buf.truncate(len as usize);
    String::from_utf8_lossy(&buf).to_string()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn outer_html(_handle: DocumentHandle) -> String {
    panic!("outer_html can only be called from WASM context");
}

// --- Utility Wrappers ---

/// Parse a date string with the given chrono format into epoch seconds.
/// Returns `None` if parsing fails.
///
/// Format specifiers follow chrono's `strftime` syntax, e.g.:
/// - `"%Y-%m-%d %H:%M:%S"` for `"2025-01-15 12:30:00"`
/// - `"%b %d, %Y"` for `"Jan 15, 2025"` (time defaults to 00:00:00)
#[cfg(target_arch = "wasm32")]
pub fn parse_date(date: &str, fmt: &str) -> Option<i64> {
    let result = unsafe {
        date_parse(
            date.as_ptr(),
            date.len() as i32,
            fmt.as_ptr(),
            fmt.len() as i32,
        )
    };
    if result < 0 { None } else { Some(result) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn parse_date(_date: &str, _fmt: &str) -> Option<i64> {
    panic!("parse_date can only be called from WASM context");
}

// --- Cleanup Wrappers ---

#[cfg(target_arch = "wasm32")]
pub fn drop_request(handle: RequestHandle) {
    unsafe { request_drop(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn drop_request(_handle: RequestHandle) {}

#[cfg(target_arch = "wasm32")]
pub fn drop_response(handle: ResponseHandle) {
    unsafe { response_drop(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn drop_response(_handle: ResponseHandle) {}

#[cfg(target_arch = "wasm32")]
pub fn drop_document(handle: DocumentHandle) {
    unsafe { document_drop(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn drop_document(_handle: DocumentHandle) {}

#[cfg(target_arch = "wasm32")]
pub fn drop_list(handle: ListHandle) {
    unsafe { list_drop(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn drop_list(_handle: ListHandle) {}

#[cfg(target_arch = "wasm32")]
pub fn get_last_error() -> Option<i32> {
    let err = unsafe { sys_get_last_error() };
    if err == 0 { None } else { Some(err) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get_last_error() -> Option<i32> {
    None
}

// ============================================================
// Convenience API
// ============================================================

/// High-level API for making HTTP requests from extensions.
#[cfg(target_arch = "wasm32")]
pub struct HttpRequest {
    handle: Option<RequestHandle>,
    method: Option<HttpMethod>,
    url: Option<String>,
}

#[cfg(target_arch = "wasm32")]
impl HttpRequest {
    /// Create a new HTTP request builder
    pub fn new() -> Self {
        Self {
            handle: None,
            method: None,
            url: None,
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
        self.method = Some(method);
        self
    }

    /// Set the URL
    pub fn url(mut self, url: String) -> Self {
        self.url = Some(url);
        self
    }

    /// Add a header to the request
    pub fn header(mut self, key: &str, value: &str) -> Self {
        // Create the request handle if not already created
        if self.handle.is_none() {
            let method = self.method.unwrap_or(HttpMethod::Get);
            let url = self
                .url
                .as_ref()
                .expect("URL must be set before adding headers");

            let h = unsafe { request_create(method as i32, url.as_ptr(), url.len() as i32) };
            if h < 0 {
                // Should probably log this or handle error state in the builder
                // For now, we just don't set the handle, subsequent calls will fail safe
                return self;
            }
            self.handle = Some(h);
        }

        if let Some(h) = self.handle {
            if h >= 0 {
                set_header(h, key, value);
            }
        }
        self
    }

    /// Send the request and get the response body as a string
    pub fn send(mut self) -> String {
        // Create the request handle if not already created
        if self.handle.is_none() {
            let method = self.method.unwrap_or(HttpMethod::Get);
            let url = self.url.as_ref().expect("URL must be set before sending");
            let h = unsafe { request_create(method as i32, url.as_ptr(), url.len() as i32) };
            if h < 0 {
                return String::new();
            }
            self.handle = Some(h);
        }

        if let Some(h) = self.handle {
            if h < 0 {
                return String::new();
            }
            let response_handle = send_request(h);
            if response_handle < 0 {
                drop_request(h); // Clean up request if send failed
                return String::new();
            }

            let body = get_response_body(response_handle);
            drop_response(response_handle);
            drop_request(h);
            body
        } else {
            String::new()
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for HttpRequest {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Non-WASM stub implementations
// ============================================================
// These stubs allow extensions to compile and get IDE support on non-WASM
// targets, even though they will only actually run in WASM context.

#[cfg(not(target_arch = "wasm32"))]
pub struct HttpRequest;

#[cfg(not(target_arch = "wasm32"))]
impl HttpRequest {
    pub fn new() -> Self {
        Self
    }

    pub fn get(_url: &str) -> Self {
        Self
    }

    pub fn post(_url: &str) -> Self {
        Self
    }

    pub fn method(self, _method: HttpMethod) -> Self {
        self
    }

    pub fn url(self, _url: String) -> Self {
        self
    }

    pub fn header(self, _key: &str, _value: &str) -> Self {
        self
    }

    pub fn send(self) -> String {
        panic!("HttpRequest::send can only be used in WASM context");
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for HttpRequest {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct HtmlDocument;

#[cfg(not(target_arch = "wasm32"))]
impl HtmlDocument {
    pub fn new(_html: &str) -> Self {
        Self
    }

    pub fn handle(&self) -> DocumentHandle {
        0
    }

    pub fn select(&self, _selector: &str) -> Vec<HtmlDocument> {
        vec![]
    }

    pub fn first(&self, _selector: &str) -> Option<HtmlDocument> {
        None
    }

    pub fn children(&self) -> Vec<HtmlDocument> {
        vec![]
    }

    pub fn attr(&self, _selector: &str, _attribute: &str) -> String {
        String::new()
    }

    pub fn get_attr(&self, _attribute: &str) -> String {
        String::new()
    }

    pub fn text(&self, _selector: &str) -> String {
        String::new()
    }

    pub fn get_text(&self) -> String {
        String::new()
    }

    pub fn inner_html(&self) -> String {
        String::new()
    }

    pub fn outer_html(&self) -> String {
        String::new()
    }

    pub fn has_class(&self, _class_name: &str) -> bool {
        false
    }

    pub fn tag_name(&self) -> String {
        String::new()
    }
}

// ============================================================
// Helper struct for HTML Document (WASM implementation)
// ============================================================

#[cfg(target_arch = "wasm32")]
pub struct HtmlDocument {
    handle: DocumentHandle,
}

#[cfg(target_arch = "wasm32")]
impl HtmlDocument {
    /// Parse an HTML string into a document
    pub fn new(html: &str) -> Self {
        Self {
            handle: parsed_html(html),
        }
    }

    /// Get the raw handle (for advanced usage)
    pub fn handle(&self) -> DocumentHandle {
        self.handle
    }

    /// Select all elements matching a CSS selector
    pub fn select(&self, selector: &str) -> Vec<HtmlDocument> {
        let list = select(self.handle, selector);
        if list < 0 {
            return Vec::new();
        }
        let len = list_len(list);
        if len < 0 {
            drop_list(list);
            return Vec::new();
        }
        let mut docs = Vec::with_capacity(len as usize);
        for i in 0..len {
            docs.push(HtmlDocument {
                handle: list_get(list, i),
            });
        }
        drop_list(list);
        docs
    }

    /// Get the first element matching a CSS selector, or None if not found
    pub fn first(&self, selector: &str) -> Option<HtmlDocument> {
        first(self.handle, selector).map(|h| HtmlDocument { handle: h })
    }

    /// Get all direct child elements
    pub fn children(&self) -> Vec<HtmlDocument> {
        let list = children(self.handle);
        if list < 0 {
            return Vec::new();
        }
        let len = list_len(list);
        if len < 0 {
            drop_list(list);
            return Vec::new();
        }
        let mut docs = Vec::with_capacity(len as usize);
        for i in 0..len {
            docs.push(HtmlDocument {
                handle: list_get(list, i),
            });
        }
        drop_list(list);
        docs
    }

    /// Get an attribute from the first element matching the selector
    pub fn attr(&self, selector: &str, attribute: &str) -> String {
        attr(self.handle, selector, attribute)
    }

    /// Get an attribute from this element directly
    pub fn get_attr(&self, attribute: &str) -> String {
        attr(self.handle, "*", attribute)
    }

    /// Get text content from the first element matching the selector
    pub fn text(&self, selector: &str) -> String {
        text(self.handle, selector)
    }

    /// Get text content from this element directly
    pub fn get_text(&self) -> String {
        text(self.handle, "*")
    }

    /// Get the inner HTML (content without the outer tag)
    pub fn inner_html(&self) -> String {
        inner_html(self.handle)
    }

    /// Get the outer HTML (full element including tag)
    pub fn outer_html(&self) -> String {
        outer_html(self.handle)
    }

    /// Check if this element has a specific class
    pub fn has_class(&self, class_name: &str) -> bool {
        let classes = self.get_attr("class");
        classes.split_whitespace().any(|c| c == class_name)
    }

    /// Get the tag name of this element
    pub fn tag_name(&self) -> String {
        // Extract tag name from outer_html
        let html = self.outer_html();
        if html.starts_with('<') {
            html[1..]
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for HtmlDocument {
    fn drop(&mut self) {
        drop_document(self.handle);
    }
}
