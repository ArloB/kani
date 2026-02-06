//! Host ABI function implementations for WASM extensions.
//!
//! This module implements the host-side functions that WASM extensions can call.
//! The ABI contract is defined in `kani-shared/src/host_abi.rs`.

use wasmtime::{Caller, Linker};

use super::HostState;
use crate::error::Result;
use crate::wasm::{ResponseData, memory};

// Re-export type aliases from shared ABI for consistency
type RequestHandle = i32;
type ResponseHandle = i32;
type DocumentHandle = i32;

/// Registers all host ABI functions with the wasmtime linker.
///
/// These functions are imported by WASM extensions via `extern "C"` declarations
/// in `kani-shared/src/host_abi.rs`.
pub fn register_host_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // HTTP Request Functions
    register_http_functions(linker)?;

    // HTML Parsing Functions
    register_html_functions(linker)?;

    // Resource Cleanup Functions
    register_cleanup_functions(linker)?;

    Ok(())
}

/// Register HTTP-related host functions.
fn register_http_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // request_create(method: i32, url_ptr: *const u8, url_len: i32) -> RequestHandle
    linker.func_wrap(
        "env",
        "request_create",
        |mut caller: Caller<'_, HostState>,
         method: i32,
         url_ptr: i32,
         url_len: i32|
         -> RequestHandle {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let url = match memory::read_string_from_guest(&caller, &memory, url_ptr, url_len) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to read URL from guest memory: {}", e);
                    return -1;
                }
            };

            tracing::debug!("Creating request for URL: {}", url);

            // Convert i32 method to Method enum
            let http_method = match method {
                0 => rquest::Method::GET,
                1 => rquest::Method::POST,
                2 => rquest::Method::PUT,
                3 => rquest::Method::DELETE,
                _ => {
                    tracing::error!("Invalid HTTP method: {}", method);
                    return -2;
                }
            };

            // Parse URL string into Url type
            let parsed_url = match url.parse::<rquest::Url>() {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("Failed to parse URL '{}': {}", url, e);
                    return -3;
                }
            };

            let state = caller.data_mut();
            let handle = state.next_request_handle;
            state.next_request_handle += 1;

            let request_builder = state.http_client.request(http_method, parsed_url);
            state.requests.insert(handle, request_builder);

            handle
        },
    )?;

    // request_set_header(handle: RequestHandle, key_ptr: *const u8, key_len: i32, val_ptr: *const u8, val_len: i32)
    linker.func_wrap(
        "env",
        "request_set_header",
        |mut caller: Caller<'_, HostState>,
         handle: RequestHandle,
         key_ptr: i32,
         key_len: i32,
         val_ptr: i32,
         val_len: i32| {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let key = match memory::read_string_from_guest(&caller, &memory, key_ptr, key_len) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to read key from guest memory: {}", e);
                    return;
                }
            };

            let val = match memory::read_string_from_guest(&caller, &memory, val_ptr, val_len) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to read value from guest memory: {}", e);
                    return;
                }
            };

            let state = caller.data_mut();
            let request_builder = match state.requests.remove(&handle) {
                Some(request) => request,
                None => {
                    tracing::error!("Request handle {} not found", handle);
                    return;
                }
            };

            let updated_request = request_builder.header(&key, &val);
            state.requests.insert(handle, updated_request);
        },
    )?;

    // request_send(handle: RequestHandle) -> ResponseHandle
    linker.func_wrap_async(
        "env",
        "request_send",
        |mut caller: Caller<'_, HostState>, (handle,): (RequestHandle,)| {
            // Extract request_builder before async block
            let request_builder = caller.data_mut().requests.remove(&handle);

            Box::new(async move {
                let request_builder = match request_builder {
                    Some(request) => request,
                    None => {
                        tracing::error!("Request handle {} not found", handle);
                        return (-1,);
                    }
                };

                let response = match request_builder.send().await {
                    Ok(response) => response,
                    Err(e) => {
                        tracing::error!("Failed to send request: {}", e);
                        return (-2,);
                    }
                };

                let status = response.status();

                let res = match response.text().await {
                    Ok(res) => res,
                    Err(e) => {
                        tracing::error!("Failed to get response body: {}", e);
                        return (-3,);
                    }
                };

                let state = caller.data_mut();
                let response_handle = state.next_response_handle;
                state.next_response_handle += 1;

                state.responses.insert(
                    response_handle,
                    ResponseData {
                        body: res,
                        status: status.into(),
                    },
                );

                (response_handle,)
            })
        },
    )?;

    // response_get_body_len(handle: ResponseHandle) -> i32
    // Returns the length of the response body, or -1 if not found
    linker.func_wrap(
        "env",
        "response_get_body_len",
        |caller: Caller<'_, HostState>, handle: ResponseHandle| -> i32 {
            match caller.data().responses.get(&handle) {
                Some(response) => response.body.len() as i32,
                None => {
                    tracing::error!("Response handle {} not found", handle);
                    -1
                }
            }
        },
    )?;

    // response_get_body(handle: ResponseHandle, buf_ptr: *mut u8, buf_len: i32) -> i32
    linker.func_wrap(
        "env",
        "response_get_body",
        |mut caller: Caller<'_, HostState>,
         handle: ResponseHandle,
         buf_ptr: i32,
         _buf_len: i32|
         -> i32 {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let body = match caller.data().responses.get(&handle) {
                Some(response) => response.body.clone(),
                None => {
                    tracing::error!("Response handle {} not found", handle);
                    return -1;
                }
            };

            let bytes_written = match memory::write_bytes_to_guest(
                &mut caller,
                &memory,
                buf_ptr,
                body.as_bytes(),
            ) {
                Ok(bytes_written) => bytes_written,
                Err(e) => {
                    tracing::error!("Failed to write response body to guest memory: {}", e);
                    return -2;
                }
            };

            bytes_written.try_into().unwrap()
        },
    )?;

    // response_get_status(handle: ResponseHandle) -> i32
    linker.func_wrap(
        "env",
        "response_get_status",
        |caller: Caller<'_, HostState>, handle: ResponseHandle| -> i32 {
            match caller.data().responses.get(&handle) {
                Some(response) => response.status.into(),
                None => {
                    tracing::error!("Response handle {} not found", handle);
                    -1
                }
            }
        },
    )?;

    Ok(())
}

/// Register HTML parsing host functions.
fn register_html_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // html_add(body_ptr: *const u8, body_len: i32) -> DocumentHandle
    linker.func_wrap(
        "env",
        "html_add",
        |mut caller: Caller<'_, HostState>, body_ptr: i32, body_len: i32| -> DocumentHandle {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let html = match memory::read_string_from_guest(&caller, &memory, body_ptr, body_len) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to read HTML from guest memory: {}", e);
                    return -1;
                }
            };

            let state = caller.data_mut();
            let handle = state.next_doc_handle;
            state.next_doc_handle += 1;

            state.html_docs.insert(handle, html);

            handle
        },
    )?;

    Ok(())
}

/// Register resource cleanup functions to prevent memory leaks.
fn register_cleanup_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // request_drop(handle: RequestHandle)
    // Drops a request builder that was created but never sent
    linker.func_wrap(
        "env",
        "request_drop",
        |mut caller: Caller<'_, HostState>, handle: RequestHandle| {
            if caller.data_mut().requests.remove(&handle).is_none() {
                tracing::warn!("request_drop: handle {} not found", handle);
            }
        },
    )?;

    // response_drop(handle: ResponseHandle)
    // Drops a response when the extension is done with it
    linker.func_wrap(
        "env",
        "response_drop",
        |mut caller: Caller<'_, HostState>, handle: ResponseHandle| {
            if caller.data_mut().responses.remove(&handle).is_none() {
                tracing::warn!("response_drop: handle {} not found", handle);
            }
        },
    )?;

    // document_drop(handle: DocumentHandle)
    // Drops an HTML document when the extension is done with it
    linker.func_wrap(
        "env",
        "document_drop",
        |mut caller: Caller<'_, HostState>, handle: DocumentHandle| {
            if caller.data_mut().html_docs.remove(&handle).is_none() {
                tracing::warn!("document_drop: handle {} not found", handle);
            }
        },
    )?;

    Ok(())
}
