//! Guest-safe row access for unpacking blueprint extraction results.
//!
//! Extraction returns `{ "rows": [...], "scalars": {...} }`. The interpreted
//! engine holds it as a `serde_json::Value`; the guest holds a `JsonHandle` (an
//! opaque handle to the same tree living host-side, never materialised guest-side).
//! [`JsonRows`] is the common surface over both, so the `unpack_*` functions can be
//! written once and run in either engine — the interpreter over a `Value`, the
//! generated guest code over a `JsonHandle`.

use crate::extension::ExtensionError;

/// Read access to a blueprint extraction result and its rows, by JSON Pointer.
///
/// The same type answers both roles: the whole result (`rows_len`, `rows_get`,
/// `get_scalar_*`) and a single row (`get_str`, `require_str`, …), mirroring how a
/// `JsonHandle` returned from `rows_get` is itself queried by field pointer.
pub trait JsonRows: Sized {
    fn get_str(&self, ptr: &str) -> Option<String>;
    fn get_i64(&self, ptr: &str) -> Option<i64>;
    fn get_f64(&self, ptr: &str) -> Option<f64>;
    fn get_bool(&self, ptr: &str) -> Option<bool>;
    fn get_array_of_strings(&self, ptr: &str) -> Vec<String>;

    /// A required string field; absence is a spec mismatch.
    fn require_str(&self, ptr: &str) -> Result<String, ExtensionError> {
        self.get_str(ptr)
            .ok_or_else(|| ExtensionError::parse(format!("Missing required field: {ptr}")))
    }

    fn rows_len(&self) -> i32;
    fn rows_get(&self, index: i32) -> Result<Self, ExtensionError>;

    fn get_scalar_str(&self, name: &str) -> Option<String>;
    fn get_scalar_bool(&self, name: &str) -> bool;
    fn get_scalar_i64(&self, name: &str) -> Option<i64>;
}

// ── Guest: JsonHandle (host-side tree behind an opaque handle) ────────────────

impl JsonRows for crate::host_abi::JsonHandle {
    // Inherent methods win name resolution, so these forward without recursion.
    fn get_str(&self, ptr: &str) -> Option<String> {
        crate::host_abi::JsonHandle::get_str(self, ptr)
    }
    fn get_i64(&self, ptr: &str) -> Option<i64> {
        crate::host_abi::JsonHandle::get_i64(self, ptr)
    }
    fn get_f64(&self, ptr: &str) -> Option<f64> {
        crate::host_abi::JsonHandle::get_f64(self, ptr)
    }
    fn get_bool(&self, ptr: &str) -> Option<bool> {
        crate::host_abi::JsonHandle::get_bool(self, ptr)
    }
    fn get_array_of_strings(&self, ptr: &str) -> Vec<String> {
        crate::host_abi::JsonHandle::get_array_of_strings(self, ptr)
    }
    fn require_str(&self, ptr: &str) -> Result<String, ExtensionError> {
        crate::host_abi::JsonHandle::require_str(self, ptr)
    }
    fn rows_len(&self) -> i32 {
        crate::host_abi::JsonHandle::rows_len(self)
    }
    fn rows_get(&self, index: i32) -> Result<Self, ExtensionError> {
        crate::host_abi::JsonHandle::rows_get(self, index)
    }
    fn get_scalar_str(&self, name: &str) -> Option<String> {
        crate::host_abi::JsonHandle::get_scalar_str(self, name)
    }
    fn get_scalar_bool(&self, name: &str) -> bool {
        crate::host_abi::JsonHandle::get_scalar_bool(self, name)
    }
    fn get_scalar_i64(&self, name: &str) -> Option<i64> {
        crate::host_abi::JsonHandle::get_scalar_i64(self, name)
    }
}

// ── Host: serde_json::Value (the interpreted engine's representation) ──────────

#[cfg(feature = "host")]
impl JsonRows for serde_json::Value {
    fn get_str(&self, ptr: &str) -> Option<String> {
        self.pointer(ptr)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }
    fn get_i64(&self, ptr: &str) -> Option<i64> {
        self.pointer(ptr).and_then(serde_json::Value::as_i64)
    }
    fn get_f64(&self, ptr: &str) -> Option<f64> {
        self.pointer(ptr).and_then(serde_json::Value::as_f64)
    }
    fn get_bool(&self, ptr: &str) -> Option<bool> {
        self.pointer(ptr).and_then(serde_json::Value::as_bool)
    }
    fn get_array_of_strings(&self, ptr: &str) -> Vec<String> {
        self.pointer(ptr)
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn rows_len(&self) -> i32 {
        self.pointer("/rows")
            .and_then(serde_json::Value::as_array)
            .map(|a| a.len() as i32)
            .unwrap_or(0)
    }
    fn rows_get(&self, index: i32) -> Result<Self, ExtensionError> {
        self.pointer(&format!("/rows/{index}"))
            .cloned()
            .ok_or_else(|| ExtensionError::parse(format!("row {index} out of bounds")))
    }
    fn get_scalar_str(&self, name: &str) -> Option<String> {
        self.get_str(&format!("/scalars/{name}"))
    }
    fn get_scalar_bool(&self, name: &str) -> bool {
        self.get_bool(&format!("/scalars/{name}")).unwrap_or(false)
    }
    fn get_scalar_i64(&self, name: &str) -> Option<i64> {
        self.get_i64(&format!("/scalars/{name}"))
    }
}

#[cfg(all(test, feature = "host"))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn value_rows_and_fields() {
        let result = json!({
            "rows": [{"id": "a", "n": 3, "tags": ["x", "y"]}, {"id": "b"}],
            "scalars": {"has_next_page": true, "total_pages": 5}
        });
        assert_eq!(result.rows_len(), 2);
        let row0 = result.rows_get(0).unwrap();
        assert_eq!(row0.require_str("/id").unwrap(), "a");
        assert_eq!(row0.get_i64("/n"), Some(3));
        assert_eq!(row0.get_array_of_strings("/tags"), vec!["x", "y"]);
        let row1 = result.rows_get(1).unwrap();
        assert!(row1.require_str("/id").is_ok());
        assert_eq!(row1.get_str("/id").as_deref(), Some("b"));
        assert!(result.get_scalar_bool("has_next_page"));
        assert_eq!(result.get_scalar_i64("total_pages"), Some(5));
    }

    #[test]
    fn value_missing_required_is_error() {
        let row = json!({"title": "t"});
        assert!(row.require_str("/id").is_err());
    }

    #[test]
    fn value_out_of_bounds_row_is_error() {
        let result = json!({"rows": []});
        assert!(result.rows_get(0).is_err());
    }
}
