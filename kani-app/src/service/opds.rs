use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use std::io::Cursor;
use time::OffsetDateTime;

use super::AppService;
use crate::error::Result;
use kani_shared::types::{ChapterSortOrder, MangaSortOrder};

impl AppService {
    /// Root OPDS navigation feed — links to catalogue and search.
    pub fn opds_root_feed(&self, base_url: &str) -> String {
        let updated = now_rfc3339();
        let mut w = Utf8Writer::new();
        w.decl();
        w.open(
            "feed",
            &[
                ("xmlns", "http://www.w3.org/2005/Atom"),
                ("xmlns:opds", "http://opds-spec.org/2010/catalog"),
            ],
        );
        w.leaf("id", &[], "urn:kani:root");
        w.leaf("title", &[], "Kani Manga Reader");
        w.leaf("updated", &[], &updated);
        w.self_close(
            "link",
            &[
                ("rel", "self"),
                ("href", &format!("{base_url}/opds")),
                ("type", ATOM_PROFILE),
            ],
        );
        w.self_close(
            "link",
            &[
                ("rel", "start"),
                ("href", &format!("{base_url}/opds")),
                ("type", ATOM_PROFILE),
            ],
        );
        w.self_close(
            "link",
            &[
                ("rel", "search"),
                ("href", &format!("{base_url}/opds/search")),
                ("type", "application/opensearchdescription+xml"),
            ],
        );
        nav_entry(
            &mut w,
            "urn:kani:catalogue",
            "Library",
            "Browse the full manga library",
            &updated,
            &format!("{base_url}/opds/catalogue"),
        );
        w.close("feed");
        w.finish()
    }

    /// Paginated acquisition feed of the library. `page` is 1-based.
    pub async fn opds_catalogue_feed(
        &self,
        page: i32,
        page_size: i32,
        search: Option<String>,
        base_url: &str,
    ) -> Result<String> {
        let (manga_list, has_next, _) = self
            .get_library_filtered(
                0,
                page,
                page_size,
                search.clone(),
                None,
                None,
                None,
                None,
                None,
                None,
                false,
                false,
                None,
                MangaSortOrder::default(),
            )
            .await?;

        let updated = now_rfc3339();
        let mut w = Utf8Writer::new();
        w.decl();
        w.open(
            "feed",
            &[
                ("xmlns", "http://www.w3.org/2005/Atom"),
                ("xmlns:opds", "http://opds-spec.org/2010/catalog"),
            ],
        );
        w.leaf("id", &[], "urn:kani:catalogue");
        w.leaf("title", &[], "Kani Library");
        w.leaf("updated", &[], &updated);
        w.self_close(
            "link",
            &[
                ("rel", "self"),
                (
                    "href",
                    &catalogue_href(base_url, page, page_size, search.as_deref()),
                ),
                ("type", ATOM_PROFILE),
            ],
        );
        w.self_close(
            "link",
            &[
                ("rel", "start"),
                ("href", &format!("{base_url}/opds")),
                ("type", ATOM_PROFILE),
            ],
        );
        if page > 1 {
            w.self_close(
                "link",
                &[
                    ("rel", "previous"),
                    (
                        "href",
                        &catalogue_href(base_url, page - 1, page_size, search.as_deref()),
                    ),
                    ("type", ATOM_PROFILE),
                ],
            );
        }
        if has_next {
            w.self_close(
                "link",
                &[
                    ("rel", "next"),
                    (
                        "href",
                        &catalogue_href(base_url, page + 1, page_size, search.as_deref()),
                    ),
                    ("type", ATOM_PROFILE),
                ],
            );
        }

        for m in &manga_list {
            w.open("entry", &[]);
            w.leaf("id", &[], &format!("urn:kani:manga:{}", m.id));
            w.leaf("title", &[], &m.name);
            w.leaf("updated", &[], &updated);
            w.self_close(
                "link",
                &[
                    ("rel", "http://opds-spec.org/image"),
                    ("href", &format!("{base_url}/rest/manga/{}/cover", m.id)),
                    ("type", "image/jpeg"),
                ],
            );
            w.self_close(
                "link",
                &[
                    ("rel", "subsection"),
                    ("href", &format!("{base_url}/opds/manga/{}", m.id)),
                    ("type", ATOM_PROFILE),
                ],
            );
            w.close("entry");
        }

        w.close("feed");
        Ok(w.finish())
    }

    /// Acquisition feed of downloaded chapters for one manga.
    pub async fn opds_manga_feed(&self, manga_id: i64, base_url: &str) -> Result<String> {
        let manga = self.get_manga_by_id(manga_id).await?;

        let (chapters, _, _) = self
            .get_local_chapters(
                manga_id,
                1,
                500,
                ChapterSortOrder::default(),
                0,
                Some(true),
                None,
                None,
            )
            .await?;

        let updated = now_rfc3339();
        let mut w = Utf8Writer::new();
        w.decl();
        w.open(
            "feed",
            &[
                ("xmlns", "http://www.w3.org/2005/Atom"),
                ("xmlns:opds", "http://opds-spec.org/2010/catalog"),
            ],
        );
        w.leaf("id", &[], &format!("urn:kani:manga:{manga_id}"));
        w.leaf("title", &[], &manga.name);
        w.leaf("updated", &[], &updated);
        w.self_close(
            "link",
            &[
                ("rel", "self"),
                ("href", &format!("{base_url}/opds/manga/{manga_id}")),
                ("type", ATOM_PROFILE),
            ],
        );
        w.self_close(
            "link",
            &[
                ("rel", "start"),
                ("href", &format!("{base_url}/opds")),
                ("type", ATOM_PROFILE),
            ],
        );
        w.self_close(
            "link",
            &[
                ("rel", "http://opds-spec.org/image"),
                ("href", &format!("{base_url}/rest/manga/{manga_id}/cover")),
                ("type", "image/jpeg"),
            ],
        );

        for ch in &chapters {
            let title = chapter_display_title(ch);
            w.open("entry", &[]);
            w.leaf("id", &[], &format!("urn:kani:chapter:{}", ch.id));
            w.leaf("title", &[], &title);
            w.leaf("updated", &[], &updated);
            w.self_close(
                "link",
                &[
                    ("rel", "http://opds-spec.org/acquisition"),
                    ("href", &format!("{base_url}/rest/chapters/{}/cbz", ch.id)),
                    ("type", "application/x-cbz"),
                    ("title", &title),
                ],
            );
            w.close("entry");
        }

        w.close("feed");
        Ok(w.finish())
    }

    /// Search results feed.
    pub async fn opds_search_feed(&self, query: &str, page: i32, base_url: &str) -> Result<String> {
        self.opds_catalogue_feed(page, 20, Some(query.to_owned()), base_url)
            .await
    }

    /// OpenSearch description document.
    pub fn opds_opensearch_description(&self, base_url: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <ShortName>Kani</ShortName>
  <Description>Search the Kani manga library</Description>
  <Url type="application/atom+xml;profile=opds-catalog"
       template="{base_url}/opds/search?q={{searchTerms}}&amp;page={{startPage}}"/>
</OpenSearchDescription>"#
        )
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

const ATOM_PROFILE: &str = "application/atom+xml;profile=opds-catalog";

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn catalogue_href(base_url: &str, page: i32, page_size: i32, search: Option<&str>) -> String {
    let mut href = format!("{base_url}/opds/catalogue?page={page}&page_size={page_size}");
    if let Some(q) = search {
        href.push_str(&format!("&q={}", urlencoding::encode(q)));
    }
    href
}

fn chapter_display_title(ch: &kani_shared::types::Chapter) -> String {
    let mut s = String::new();
    if let Some(vol) = ch.volume {
        s.push_str(&format!("Vol. {vol} "));
    }
    if ch.number.fract().abs() < f64::EPSILON {
        s.push_str(&format!("Ch. {}", ch.number as i64));
    } else {
        s.push_str(&format!("Ch. {:.1}", ch.number));
    }
    if let Some(ref title) = ch.title
        && !title.is_empty()
    {
        s.push_str(&format!(" - {title}"));
    }
    s
}

fn nav_entry(w: &mut Utf8Writer, id: &str, title: &str, content: &str, updated: &str, href: &str) {
    w.open("entry", &[]);
    w.leaf("id", &[], id);
    w.leaf("title", &[], title);
    w.leaf("updated", &[], updated);
    w.leaf("content", &[("type", "text")], content);
    w.self_close(
        "link",
        &[
            ("rel", "subsection"),
            ("href", href),
            ("type", ATOM_PROFILE),
        ],
    );
    w.close("entry");
}

// ─── Minimal XML writer (quick-xml writer wrapper) ────────────────────────────

struct Utf8Writer {
    writer: Writer<Cursor<Vec<u8>>>,
}

impl Utf8Writer {
    fn new() -> Self {
        Self {
            writer: Writer::new(Cursor::new(Vec::new())),
        }
    }

    fn finish(self) -> String {
        String::from_utf8(self.writer.into_inner().into_inner()).unwrap_or_default()
    }

    fn decl(&mut self) {
        let decl = BytesDecl::new("1.0", Some("UTF-8"), None);
        self.writer.write_event(Event::Decl(decl)).ok();
    }

    fn open(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        let mut start = BytesStart::new(tag);
        for (k, v) in attrs {
            start.push_attribute((*k, *v));
        }
        self.writer.write_event(Event::Start(start)).ok();
    }

    fn close(&mut self, tag: &str) {
        self.writer.write_event(Event::End(BytesEnd::new(tag))).ok();
    }

    fn self_close(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        let mut start = BytesStart::new(tag);
        for (k, v) in attrs {
            start.push_attribute((*k, *v));
        }
        self.writer.write_event(Event::Empty(start)).ok();
    }

    fn leaf(&mut self, tag: &str, attrs: &[(&str, &str)], text: &str) {
        self.open(tag, attrs);
        self.writer
            .write_event(Event::Text(BytesText::new(text)))
            .ok();
        self.close(tag);
    }
}
