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
pub fn register_host_functions(linker: &mut Linker<HostState>) -> Result<()> {
    register_http_functions(linker)?;
    register_html_functions(linker)?;
    register_cleanup_functions(linker)?;
    register_utility_functions(linker)?;
    linker.func_wrap(
        "env",
        "sys_get_last_error",
        |caller: Caller<'_, HostState>| -> i32 { caller.data().last_error.unwrap_or(0) },
    )?;

    Ok(())
}

/// Register HTTP-related host functions.
fn register_http_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // request_create(method: i32, url_ptr: i32, url_len: i32) -> RequestHandle
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
                    caller.data_mut().last_error = Some(1);
                    return -1;
                }
            };

            let http_method = match method {
                0 => rquest::Method::GET,
                1 => rquest::Method::POST,
                2 => rquest::Method::PUT,
                3 => rquest::Method::DELETE,
                _ => {
                    tracing::error!("Invalid HTTP method: {}", method);
                    caller.data_mut().last_error = Some(2);
                    return -2;
                }
            };

            tracing::debug!("Creating request for URL: {}, method: {}", url, http_method);

            let parsed_url = match url.parse::<rquest::Url>() {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("Failed to parse URL '{}': {}", url, e);
                    caller.data_mut().last_error = Some(3);
                    return -3;
                }
            };

            let state = caller.data_mut();
            let handle = state.next_request_handle;
            state.next_request_handle += 1;
            state.last_error = None;

            let request_builder = state.http_client.inner().request(http_method, parsed_url);
            state.requests.insert(handle, request_builder);

            handle
        },
    )?;

    // request_set_header(handle: RequestHandle, key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32)
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
                    caller.data_mut().last_error = Some(1);
                    return;
                }
            };

            let val = match memory::read_string_from_guest(&caller, &memory, val_ptr, val_len) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to read value from guest memory: {}", e);
                    caller.data_mut().last_error = Some(1);
                    return;
                }
            };

            let state = caller.data_mut();
            let request_builder = match state.requests.remove(&handle) {
                Some(request) => request,
                None => {
                    tracing::error!("Request handle {} not found", handle);
                    state.last_error = Some(4); // Error: Invalid handle
                    return;
                }
            };

            tracing::debug!(
                "request_set_header called for handle {}, key: {}, val: {}",
                handle,
                key,
                val
            );

            let updated_request = request_builder.header(&key, &val);
            state.requests.insert(handle, updated_request);
            state.last_error = None;
        },
    )?;

    // request_send(handle: RequestHandle) -> ResponseHandle
    linker.func_wrap_async(
        "env",
        "request_send",
        |mut caller: Caller<'_, HostState>, (handle,): (RequestHandle,)| {
            tracing::debug!("request_send called for handle {}", handle);
            // Extract request_builder before async block
            let request_builder = caller.data_mut().requests.remove(&handle);

            if request_builder.is_none() {
                caller.data_mut().last_error = Some(4); // Error: Invalid handle
            } else {
                caller.data_mut().last_error = None;
            }

            Box::new(async move {
                let request_builder = match request_builder {
                    Some(request) => request,
                    None => {
                        tracing::error!("Request handle {} not found", handle);
                        return (-1,);
                    }
                };

                let request = match request_builder.build() {
                    Ok(req) => req,
                    Err(e) => {
                        tracing::error!("Failed to build request: {}", e);
                        return (-2,);
                    }
                };

                // Use http_client (SmartClient) to send, which handles FlareSolverr if needed
                let response = match caller.data().http_client.send_request(request).await {
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
    linker.func_wrap(
        "env",
        "response_get_body_len",
        |caller: Caller<'_, HostState>, handle: ResponseHandle| -> i32 {
            tracing::debug!("response_get_body_len called for handle {}", handle);
            match caller.data().responses.get(&handle) {
                Some(response) => response.body.len() as i32,
                None => {
                    tracing::error!("Response handle {} not found", handle);
                    -1
                }
            }
        },
    )?;

    // response_get_body(handle: ResponseHandle, buf_ptr: i32, buf_len: i32) -> i32
    linker.func_wrap(
        "env",
        "response_get_body",
        |mut caller: Caller<'_, HostState>,
         handle: ResponseHandle,
         buf_ptr: i32,
         buf_len: i32|
         -> i32 {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let body = match caller.data().responses.get(&handle) {
                Some(response) => response.body.clone(),
                None => {
                    tracing::error!("Response handle {} not found", handle);
                    caller.data_mut().last_error = Some(4);
                    return -1;
                }
            };

            let bytes = body.as_bytes();
            let to_write = if buf_len >= 0 {
                bytes.len().min(buf_len as usize)
            } else {
                bytes.len()
            };

            let bytes_written = match memory::write_bytes_to_guest(
                &mut caller,
                &memory,
                buf_ptr,
                &bytes[..to_write],
            ) {
                Ok(bytes_written) => bytes_written,
                Err(e) => {
                    tracing::error!("Failed to write response body to guest memory: {}", e);
                    return -2;
                }
            };

            tracing::debug!(
                "response_get_body called for handle {}, buf_len {}, bytes_written {}, body: {}",
                handle,
                buf_len,
                bytes_written,
                body
            );

            bytes_written.try_into().unwrap()
        },
    )?;

    // response_get_status(handle: ResponseHandle) -> i32
    linker.func_wrap(
        "env",
        "response_get_status",
        |caller: Caller<'_, HostState>, handle: ResponseHandle| -> i32 {
            tracing::debug!("response_get_status called for handle {}", handle);
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
    // html_parse(body_ptr: i32, body_len: i32) -> DocumentHandle
    linker.func_wrap(
        "env",
        "html_parse",
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

            tracing::debug!("html_parse called on handle {}", handle);

            state.html_docs.insert(handle, html);

            handle
        },
    )?;

    // html_select(handle: DocumentHandle, sel_ptr: i32, sel_len: i32) -> ListHandle
    linker.func_wrap(
        "env",
        "html_select",
        |mut caller: Caller<'_, HostState>,
         handle: DocumentHandle,
         sel_ptr: i32,
         sel_len: i32|
         -> i32 {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let selector_str =
                match memory::read_string_from_guest(&caller, &memory, sel_ptr, sel_len) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("Failed to read selector from guest memory: {}", e);
                        return -1;
                    }
                };

            let state = caller.data_mut();
            let doc_str = match state.html_docs.get(&handle) {
                Some(s) => s,
                None => {
                    tracing::error!("Document handle {} not found", handle);
                    return -1;
                }
            };

            let selector = match scraper::Selector::parse(&selector_str) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Invalid CSS selector '{}': {:?}", selector_str, e);
                    return -2;
                }
            };

            let doc = scraper::Html::parse_fragment(doc_str);
            let matches: Vec<String> = doc
                .root_element()
                .select(&selector)
                .map(|e| e.html())
                .collect();

            let list_handle = state.next_doc_handle;

            tracing::debug!(
                "html_select called, selector: {}, matches: {:?}, list_handle: {}",
                selector_str,
                matches,
                list_handle
            );

            state.next_doc_handle += 1;

            state.html_lists.insert(list_handle, matches);

            list_handle
        },
    )?;

    // html_list_len(handle: ListHandle) -> i32
    linker.func_wrap(
        "env",
        "html_list_len",
        |caller: Caller<'_, HostState>, handle: i32| -> i32 {
            tracing::debug!("html_list_len called for handle {}", handle);
            match caller.data().html_lists.get(&handle) {
                Some(list) => list.len() as i32,
                None => {
                    tracing::error!("List handle {} not found", handle);
                    -1
                }
            }
        },
    )?;

    // html_list_get(handle: ListHandle, index: i32) -> DocumentHandle
    linker.func_wrap(
        "env",
        "html_list_get",
        |mut caller: Caller<'_, HostState>, handle: i32, index: i32| -> DocumentHandle {
            let state = caller.data_mut();
            let html_item = match state.html_lists.get(&handle) {
                Some(list) => {
                    if index < 0 || index >= list.len() as i32 {
                        tracing::error!(
                            "Index {} out of bounds for list {} (len {})",
                            index,
                            handle,
                            list.len()
                        );
                        return -1;
                    }
                    list[index as usize].clone()
                }
                None => {
                    tracing::error!("List handle {} not found", handle);
                    return -1;
                }
            };

            tracing::debug!(
                "html_list_get called for handle {}, index {}, html_item: {}",
                handle,
                index,
                html_item
            );

            let doc_handle = state.next_doc_handle;
            state.next_doc_handle += 1;

            state.html_docs.insert(doc_handle, html_item);
            doc_handle
        },
    )?;

    linker.func_wrap(
        "env",
        "html_attr",
        |mut caller: Caller<'_, HostState>,
         handle: DocumentHandle,
         sel_ptr: i32,
         sel_len: i32,
         attr_ptr: i32,
         attr_len: i32,
         buf_ptr: i32,
         buf_len: i32|
         -> i32 {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let selector_str =
                match memory::read_string_from_guest(&caller, &memory, sel_ptr, sel_len) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

            let attr_name =
                match memory::read_string_from_guest(&caller, &memory, attr_ptr, attr_len) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

            let doc_str = match caller.data().html_docs.get(&handle) {
                Some(s) => s,
                None => return -1,
            };

            let selector = match scraper::Selector::parse(&selector_str) {
                Ok(s) => s,
                Err(_) => return -2,
            };

            let doc = scraper::Html::parse_fragment(doc_str);
            let result_str = match doc.root_element().select(&selector).next() {
                Some(el) => el.value().attr(&attr_name).unwrap_or("").to_string(),
                None => String::new(),
            };

            let bytes = result_str.as_bytes();
            let to_write = if buf_len >= 0 {
                bytes.len().min(buf_len as usize)
            } else {
                bytes.len()
            };

            tracing::debug!(
                "html_attr called for handle {}, selector: {}, attr: {}, result: {}",
                handle,
                selector_str,
                attr_name,
                result_str
            );

            match memory::write_bytes_to_guest(&mut caller, &memory, buf_ptr, &bytes[..to_write]) {
                Ok(n) => n as i32,
                Err(_) => -1,
            }
        },
    )?;

    // html_text(handle: DocumentHandle, sel_ptr: i32, sel_len: i32, buf_ptr: i32, buf_len: i32) -> i32
    linker.func_wrap(
        "env",
        "html_text",
        |mut caller: Caller<'_, HostState>,
         handle: DocumentHandle,
         sel_ptr: i32,
         sel_len: i32,
         buf_ptr: i32,
         buf_len: i32|
         -> i32 {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let selector_str =
                match memory::read_string_from_guest(&caller, &memory, sel_ptr, sel_len) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

            let doc_str = match caller.data().html_docs.get(&handle) {
                Some(s) => s,
                None => return -1,
            };

            let selector = match scraper::Selector::parse(&selector_str) {
                Ok(s) => s,
                Err(_) => return -2,
            };

            let doc = scraper::Html::parse_fragment(doc_str);
            let result_str = match doc.root_element().select(&selector).next() {
                Some(el) => el.text().collect::<Vec<_>>().join(""),
                None => String::new(),
            };

            let bytes = result_str.as_bytes();
            let to_write = if buf_len >= 0 {
                bytes.len().min(buf_len as usize)
            } else {
                bytes.len()
            };

            tracing::debug!(
                "html_text called for handle {}, selector: {}, result: {}",
                handle,
                selector_str,
                result_str
            );

            match memory::write_bytes_to_guest(&mut caller, &memory, buf_ptr, &bytes[..to_write]) {
                Ok(n) => n as i32,
                Err(_) => -1,
            }
        },
    )?;

    // html_inner_html(handle: DocumentHandle, buf_ptr: i32, buf_len: i32) -> i32
    linker.func_wrap(
        "env",
        "html_inner_html",
        |mut caller: Caller<'_, HostState>,
         handle: DocumentHandle,
         buf_ptr: i32,
         buf_len: i32|
         -> i32 {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let doc_str = match caller.data().html_docs.get(&handle) {
                Some(s) => s.clone(),
                None => {
                    tracing::error!("Document handle {} not found", handle);
                    return -1;
                }
            };

            let fragment = scraper::Html::parse_fragment(&doc_str);
            let result_str = if let Some(root) = fragment.root_element().child_elements().next() {
                root.inner_html()
            } else {
                doc_str
            };

            let bytes = result_str.as_bytes();
            let to_write = if buf_len >= 0 {
                bytes.len().min(buf_len as usize)
            } else {
                bytes.len()
            };

            tracing::debug!(
                "html_inner_html called for handle {}, result: {}",
                handle,
                result_str
            );

            match memory::write_bytes_to_guest(&mut caller, &memory, buf_ptr, &bytes[..to_write]) {
                Ok(n) => n as i32,
                Err(_) => -1,
            }
        },
    )?;

    // html_first(handle: DocumentHandle, sel_ptr: i32, sel_len: i32) -> DocumentHandle
    linker.func_wrap(
        "env",
        "html_first",
        |mut caller: Caller<'_, HostState>,
         handle: DocumentHandle,
         sel_ptr: i32,
         sel_len: i32|
         -> DocumentHandle {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let selector_str =
                match memory::read_string_from_guest(&caller, &memory, sel_ptr, sel_len) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

            let doc_str = match caller.data().html_docs.get(&handle) {
                Some(s) => s.clone(),
                None => return -1,
            };

            let selector = match scraper::Selector::parse(&selector_str) {
                Ok(s) => s,
                Err(_) => return -2,
            };

            let doc = scraper::Html::parse_fragment(&doc_str);
            let first_html = match doc.root_element().select(&selector).next() {
                Some(el) => el.html(),
                None => return -3,
            };

            let state = caller.data_mut();
            let doc_handle = state.next_doc_handle;

            tracing::debug!(
                "html_first called for handle {}, selector: {}, result: {}",
                handle,
                selector_str,
                first_html
            );

            state.next_doc_handle += 1;
            state.html_docs.insert(doc_handle, first_html);

            doc_handle
        },
    )?;

    // html_children(handle: DocumentHandle) -> ListHandle
    linker.func_wrap(
        "env",
        "html_children",
        |mut caller: Caller<'_, HostState>, handle: DocumentHandle| -> i32 {
            let doc_str = match caller.data().html_docs.get(&handle) {
                Some(s) => s.clone(),
                None => {
                    tracing::error!("Document handle {} not found", handle);
                    return -1;
                }
            };

            let fragment = scraper::Html::parse_fragment(&doc_str);
            let children: Vec<String> = fragment
                .root_element()
                .child_elements()
                .flat_map(|root| root.child_elements())
                .map(|el| el.html())
                .collect();

            let state = caller.data_mut();
            let list_handle = state.next_doc_handle;

            tracing::debug!(
                "html_children called for handle {}, children: {:?}",
                handle,
                children
            );

            state.next_doc_handle += 1;
            state.html_lists.insert(list_handle, children);

            list_handle
        },
    )?;

    // html_outer_html(handle: DocumentHandle, buf_ptr: i32, buf_len: i32) -> i32
    linker.func_wrap(
        "env",
        "html_outer_html",
        |mut caller: Caller<'_, HostState>,
         handle: DocumentHandle,
         buf_ptr: i32,
         buf_len: i32|
         -> i32 {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let doc_str = match caller.data().html_docs.get(&handle) {
                Some(s) => s.clone(),
                None => return -1,
            };

            let bytes = doc_str.as_bytes();
            let to_write = if buf_len >= 0 {
                bytes.len().min(buf_len as usize)
            } else {
                bytes.len()
            };

            tracing::debug!(
                "html_outer_html called for handle {}, result: {}",
                handle,
                doc_str
            );

            match memory::write_bytes_to_guest(&mut caller, &memory, buf_ptr, &bytes[..to_write]) {
                Ok(n) => n as i32,
                Err(_) => -1,
            }
        },
    )?;

    Ok(())
}

/// Register resource cleanup functions to prevent memory leaks.
fn register_cleanup_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // request_drop(handle: RequestHandle)
    linker.func_wrap(
        "env",
        "request_drop",
        |mut caller: Caller<'_, HostState>, handle: RequestHandle| {
            tracing::debug!("request_drop called for handle {}", handle);
            caller.data_mut().requests.remove(&handle);
        },
    )?;

    // response_drop(handle: ResponseHandle)
    linker.func_wrap(
        "env",
        "response_drop",
        |mut caller: Caller<'_, HostState>, handle: ResponseHandle| {
            tracing::debug!("response_drop called for handle {}", handle);
            caller.data_mut().responses.remove(&handle);
        },
    )?;

    // document_drop(handle: DocumentHandle)
    linker.func_wrap(
        "env",
        "document_drop",
        |mut caller: Caller<'_, HostState>, handle: DocumentHandle| {
            tracing::debug!("document_drop called for handle {}", handle);
            caller.data_mut().html_docs.remove(&handle);
        },
    )?;

    // list_drop(handle: ListHandle)
    linker.func_wrap(
        "env",
        "list_drop",
        |mut caller: Caller<'_, HostState>, handle: i32| {
            tracing::debug!("list_drop called for handle {}", handle);
            caller.data_mut().html_lists.remove(&handle);
        },
    )?;

    Ok(())
}

/// Register utility host functions.
fn register_utility_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // date_parse(date_ptr: *const u8, date_len: i32, fmt_ptr: *const u8, fmt_len: i32) -> i64
    linker.func_wrap(
        "env",
        "date_parse",
        |mut caller: Caller<'_, HostState>,
         date_ptr: i32,
         date_len: i32,
         fmt_ptr: i32,
         fmt_len: i32|
         -> i64 {
            let memory = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .unwrap();

            let date_str =
                match memory::read_string_from_guest(&caller, &memory, date_ptr, date_len) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("Failed to read date string from guest memory: {}", e);
                        caller.data_mut().last_error = Some(1);
                        return -1;
                    }
                };

            let fmt_str = match memory::read_string_from_guest(&caller, &memory, fmt_ptr, fmt_len) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to read format string from guest memory: {}", e);
                    caller.data_mut().last_error = Some(1);
                    return -1;
                }
            };

            match chrono::NaiveDateTime::parse_from_str(&date_str, &fmt_str) {
                Ok(dt) => {
                    let epoch = dt.and_utc().timestamp();
                    tracing::debug!(
                        "date_parse called, date: {}, fmt: {}, epoch: {}",
                        date_str,
                        fmt_str,
                        epoch
                    );
                    caller.data_mut().last_error = None;
                    epoch
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to parse date '{}' with format '{}': {}",
                        date_str,
                        fmt_str,
                        e
                    );
                    caller.data_mut().last_error = Some(5);
                    -1
                }
            }
        },
    )?;

    Ok(())
}
