// Rust code generation from a ValidatedExtension.

pub mod blueprint;
pub mod crate_layout;
pub mod endpoints;
pub mod expr;
pub mod format;
pub mod macros;
pub mod request;
pub mod templates;

use crate::yaml::model::ValidatedExtension;
use crate_layout::{emit_cargo_toml, emit_guest_impl, emit_lib_header};
use endpoints::{emit_chapter_list, emit_manga_details, emit_pages, emit_popular, emit_search};
use macros::{emit_filter_list, emit_preference_list};
use templates::MANGA_EXT_STUB_SORT;

pub struct GeneratedCrate {
    pub id: String,
    pub cargo_toml: String,
    pub lib_rs: String,
}

pub fn generate(ext: &ValidatedExtension, embedded_bytes: bool) -> GeneratedCrate {
    let cargo_toml = emit_cargo_toml(ext, embedded_bytes);
    let lib_rs = emit_lib_rs(ext, embedded_bytes);
    GeneratedCrate {
        id: ext.id.clone(),
        cargo_toml,
        lib_rs,
    }
}

fn emit_lib_rs(ext: &ValidatedExtension, embedded_bytes: bool) -> String {
    let mut parts: Vec<String> = Vec::new();

    parts.push(emit_lib_header(ext, embedded_bytes));

    // MangaExtension impl
    parts.push(format!(
        "impl MangaExtension for {} {{",
        crate_layout::to_pascal_case(&ext.id)
    ));
    parts.push(format!("fn name(&self) -> &str {{ \"{}\" }}", ext.name));

    if let Some(popular) = &ext.popular {
        parts.push(emit_popular(popular, embedded_bytes));
    } else {
        parts.push(
            "fn get_popular_manga(&self, _page: i32, _page_size: i32, _filters: &[ActiveFilter]) -> ExtensionResult<MangaList> {\n\
             Ok(MangaList { manga: vec![], has_next_page: false, total_pages: None })\n\
             }".into()
        );
    }

    if let Some(search) = &ext.search {
        parts.push(emit_search(search, embedded_bytes));
    } else {
        parts.push(
            "fn search_manga(&self, _query: &str, _page: i32, _page_size: i32, _filters: &[ActiveFilter]) -> ExtensionResult<MangaList> {\n\
             Ok(MangaList { manga: vec![], has_next_page: false, total_pages: None })\n\
             }".into()
        );
    }

    if let Some(details) = &ext.manga_details {
        parts.push(emit_manga_details(details, embedded_bytes));
    } else {
        parts.push(
            "fn get_manga_details(&self, _manga_id: &str) -> ExtensionResult<MangaInfo> {\n\
             Err(kani_shared::ExtensionError::NotImplemented)\n\
             }"
            .into(),
        );
    }

    if let Some(chapters) = &ext.chapter_list {
        parts.push(emit_chapter_list(chapters, embedded_bytes));
    } else {
        parts.push(
            "fn get_chapter_list(&self, _manga_id: &str, _page: i32, _page_size: Option<i32>, _sort: Option<String>) -> ExtensionResult<ChapterList> {\n\
             Ok(ChapterList { chapters: vec![], has_next_page: false, total_pages: None })\n\
             }".into()
        );
    }

    if let Some(pages) = &ext.pages {
        parts.push(emit_pages(pages, embedded_bytes));
    } else {
        parts.push(
            "fn get_pages(&self, _manga_id: &str, _chapter_id: &str) -> ExtensionResult<Chapter> {\n\
             Err(kani_shared::ExtensionError::NotImplemented)\n\
             }".into()
        );
    }

    if let Some(url_template) = &ext.get_url {
        let rust_template = url_template.replace("$manga_id$", "{manga_id}");
        parts.push(format!(
            "fn get_url(&self, manga_id: &str) -> ExtensionResult<String> {{\n\
             Ok(format!(\"{rust_template}\"))\n\
             }}"
        ));
    }

    parts.push(MANGA_EXT_STUB_SORT.into());

    // filter_list and preference_list
    parts.push(format!(
        "fn get_filter_list(&self) -> ExtensionResult<wit_types::FilterList> {{\n{}\n}}",
        emit_filter_list(&ext.filters)
    ));

    parts.push(format!(
        "fn get_preferences(&self) -> ExtensionResult<Vec<PreferenceSpec>> {{\n{}\n}}",
        emit_preference_list(&ext.preferences)
    ));

    parts.push("}".into()); // end MangaExtension impl

    parts.push(emit_guest_impl(ext));

    let raw = parts.join("\n\n");
    format::try_rustfmt(&raw)
}
