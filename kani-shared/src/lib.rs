//! Kani Shared - Shared types and traits for the Kani manga downloader.
//!
//! This crate contains:
//! - Shared data types (MangaInfo, Chapter, etc.) used across the host and WASM extensions
//! - The MangaExtension trait that all extensions must implement
//! - Host ABI definitions for WASM-to-host communication
//! - Error types for extension operations

pub mod types;
pub use types::*;

pub mod bindings {
    // When the `host` feature is enabled (kani-app, kani-core) the generated WIT
    // types get serde derives so the service layer can serialize/deserialize them
    // for the JSON cache.  WASM extension crates never enable `host`, so they
    // compile without any serde dependency.
    #[cfg(feature = "host")]
    wit_bindgen::generate!({
        path: "../kani-core/wit/kani.wit",
        world: "kani-extension",
        additional_derives: [serde::Serialize, serde::Deserialize],
        pub_export_macro: true,
        default_bindings_module: "kani_shared::bindings",
        with: {
            "kani:extension/types/manga-status": crate::types::MangaStatus
        }
    });

    #[cfg(not(feature = "host"))]
    wit_bindgen::generate!({
        path: "../kani-core/wit/kani.wit",
        world: "kani-extension",
        pub_export_macro: true,
        default_bindings_module: "kani_shared::bindings",
        with: {
            "kani:extension/types/manga-status": crate::types::MangaStatus
        }
    });
}

pub use bindings::kani::extension::types as wit_types;
pub use bindings::kani::extension::{html, http, scripting, utility};

pub mod extension;
pub use extension::*;

pub mod host_abi;

pub mod encoding;
pub use encoding::{decode_manga_id, encode_manga_id};

pub mod ast;
pub use ast::{OffsetType, PaginationConfig};

pub mod filters;
pub use filters::{ApplyFilters, ArrayFormat, FilterGroups};

#[cfg(target_family = "wasm")]
pub use talc::TalckWasm as __TalckWasm;

#[macro_export]
macro_rules! guest_alloc {
    () => {
        #[cfg(target_family = "wasm")]
        #[global_allocator]
        static ALLOCATOR: $crate::__TalckWasm = unsafe { $crate::__TalckWasm::new_global() };
    };
}
