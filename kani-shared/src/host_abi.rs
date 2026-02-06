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

/// Handle type for HTTP requests/responses
pub type RequestHandle = i32;
pub type ResponseHandle = i32;
pub type DocumentHandle = i32;

// ============================================================
// External declarations (imported from host when running in WASM)
// ============================================================

#[cfg(target_arch = "wasm32")]
extern "C" {
    /// Create a new HTTP request.
    ///
    /// # Arguments
    /// * `method` - HTTP method (0=GET, 1=POST, etc.)
    /// * `url_ptr` - Pointer to URL string in WASM memory
    /// * `url_len` - Length of URL string
    ///
    /// # Returns
    /// Request handle for subsequent operations
    pub fn request_create(method: i32, url_ptr: *const u8, url_len: i32) -> RequestHandle;

    /// Set a header on an HTTP request.
    ///
    /// # Arguments
    /// * `handle` - Request handle from request_create
    /// * `key_ptr` - Pointer to header name
    /// * `key_len` - Length of header name
    /// * `val_ptr` - Pointer to header value
    /// * `val_len` - Length of header value
    pub fn request_set_header(
        handle: RequestHandle,
        key_ptr: *const u8,
        key_len: i32,
        val_ptr: *const u8,
        val_len: i32,
    );

    /// Send an HTTP request.
    ///
    /// # Arguments
    /// * `handle` - Request handle
    ///
    /// # Returns
    /// Response handle
    pub fn request_send(handle: RequestHandle) -> ResponseHandle;

    /// Get the response body.
    ///
    /// # Arguments
    /// * `handle` - Response handle
    /// * `buf_ptr` - Pointer to buffer to write body into
    /// * `buf_len` - Size of buffer
    ///
    /// # Returns
    /// Number of bytes written
    pub fn response_get_body(handle: ResponseHandle, buf_ptr: *mut u8, buf_len: i32) -> i32;

    /// Get the response status.
    ///
    /// # Arguments
    /// * `handle` - Response handle
    ///
    /// # Returns
    /// Response status
    pub fn response_get_status(handle: ResponseHandle) -> i32;

    /// Parse HTML and create a document handle.
    ///
    /// # Arguments
    /// * `body_handle` - Response handle containing HTML
    /// * `selector_ptr` - Pointer to CSS selector string
    /// * `selector_len` - Length of selector string
    ///
    /// # Returns
    /// Document handle for parsed HTML
    pub fn html_parse(
        body_handle: ResponseHandle,
        selector_ptr: *const u8,
        selector_len: i32,
    ) -> DocumentHandle;
}

// ============================================================
// Safe wrapper functions
// ============================================================

/// Create a new HTTP GET request.
///
/// # Safety
/// This function is safe to call from WASM extensions.
/// In native context, it will panic.
#[cfg(target_arch = "wasm32")]
pub fn http_get(url: &str) -> RequestHandle {
    unsafe { request_create(HttpMethod::Get as i32, url.as_ptr(), url.len() as i32) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn http_get(_url: &str) -> RequestHandle {
    panic!("http_get can only be called from WASM context");
}

/// Create a new HTTP POST request.
#[cfg(target_arch = "wasm32")]
pub fn http_post(url: &str) -> RequestHandle {
    unsafe { request_create(HttpMethod::Post as i32, url.as_ptr(), url.len() as i32) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn http_post(_url: &str) -> RequestHandle {
    panic!("http_post can only be called from WASM context");
}

/// Set a request header.
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

/// Send an HTTP request and get the response.
#[cfg(target_arch = "wasm32")]
pub fn send_request(handle: RequestHandle) -> ResponseHandle {
    unsafe { request_send(handle) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn send_request(_handle: RequestHandle) -> ResponseHandle {
    panic!("send_request can only be called from WASM context");
}

/// Read the response body as a string.
#[cfg(target_arch = "wasm32")]
pub fn get_response_body(handle: ResponseHandle) -> String {
    // Allocate a buffer (16KB should be enough for most responses)
    let mut buf = vec![0u8; 16384];
    let len = unsafe { response_get_body(handle, buf.as_mut_ptr(), buf.len() as i32) };
    if len < 0 {
        return String::new();
    }
    buf.truncate(len as usize);
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

/// Parse HTML from a response.
#[cfg(target_arch = "wasm32")]
pub fn parse_html(body_handle: ResponseHandle, selector: &str) -> DocumentHandle {
    unsafe { html_parse(body_handle, selector.as_ptr(), selector.len() as i32) }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn parse_html(_body_handle: ResponseHandle, _selector: &str) -> DocumentHandle {
    panic!("parse_html can only be called from WASM context");
}

// ============================================================
// Convenience API
// ============================================================

/// High-level API for making HTTP requests from extensions.
#[cfg(target_arch = "wasm32")]
pub struct HttpRequest {
    handle: RequestHandle,
}

#[cfg(target_arch = "wasm32")]
impl HttpRequest {
    /// Create a new GET request.
    pub fn get(url: &str) -> Self {
        Self {
            handle: http_get(url),
        }
    }

    /// Create a new POST request.
    pub fn post(url: &str) -> Self {
        Self {
            handle: http_post(url),
        }
    }

    /// Add a header to the request.
    pub fn header(self, key: &str, value: &str) -> Self {
        set_header(self.handle, key, value);
        self
    }

    /// Send the request and get the response body as a string.
    pub fn send(self) -> String {
        let response_handle = send_request(self.handle);
        get_response_body(response_handle)
    }
}
