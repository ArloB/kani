//! Memory access utilities for WASM guest memory.

use wasmtime::{Caller, Memory};

use super::HostState;
use crate::error::Error;

/// Reads a UTF-8 string from guest linear memory.
pub fn read_string_from_guest(
    caller: &Caller<'_, HostState>,
    memory: &Memory,
    ptr: i32,
    len: i32,
) -> Result<String, Error> {
    let ptr = ptr as u32 as usize;
    let len = len as u32 as usize;

    let data = memory.data(caller);
    if ptr + len > data.len() {
        return Err(Error::WasmMemoryAccess(
            "string read out of bounds".to_string(),
        ));
    }

    let bytes = &data[ptr..ptr + len];
    String::from_utf8(bytes.to_vec()).map_err(|e| Error::WasmMemoryAccess(e.to_string()))
}

/// Writes bytes to guest linear memory.
/// Returns the number of bytes written (may be less than data.len() if buffer is smaller).
pub fn write_bytes_to_guest(
    caller: &mut Caller<'_, HostState>,
    memory: &Memory,
    ptr: i32,
    data: &[u8],
) -> Result<usize, Error> {
    let ptr = ptr as u32 as usize;

    let mem_data = memory.data_mut(caller);
    let available = mem_data.len().saturating_sub(ptr);
    let to_write = data.len().min(available);

    if to_write == 0 && !data.is_empty() {
        return Err(Error::WasmMemoryAccess("write out of bounds".to_string()));
    }

    mem_data[ptr..ptr + to_write].copy_from_slice(&data[..to_write]);
    Ok(to_write)
}
