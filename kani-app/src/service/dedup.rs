use crate::error::{Result, ServiceError};
use sqlx::SqlitePool;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SimilarMangaHit {
    pub id: i64,
    pub name: String,
    pub source_id: i64,
    pub similarity: f64,
    pub author_match: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct DuplicatePair {
    pub manga_a: MangaSummary,
    pub manga_b: MangaSummary,
    pub similarity: f64,
    pub author_match: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct MangaSummary {
    pub id: i64,
    pub name: String,
    pub source_id: i64,
    pub cover_url: Option<String>,
    pub local_cover_path: Option<String>,
}

pub fn normalise_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let stripped = lower
        .strip_prefix("the ")
        .or_else(|| lower.strip_prefix("a "))
        .or_else(|| lower.strip_prefix("an "))
        .unwrap_or(&lower);

    // Remove vol/chapter suffixes
    let no_suffix = {
        let patterns = [", vol.", " vol.", ", volume ", " volume ", ", ch.", " ch."];
        let mut s = stripped.to_string();
        for p in patterns {
            if let Some(idx) = s.find(p) {
                s.truncate(idx);
                break;
            }
        }
        s
    };

    // Keep only alphanumeric and spaces, collapse whitespace
    no_suffix
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns manga in the library that are similar to `title`/`authors`.
/// `exclude_manga_id` skips the manga itself (pass when checking existing manga for
/// the duplicates scan; pass `None` when checking at import time).
pub async fn find_similar_manga(
    pool: &SqlitePool,
    title: &str,
    authors: &[String],
    exclude_manga_id: Option<i64>,
) -> Result<Vec<SimilarMangaHit>> {
    let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM manga WHERE is_orphaned = FALSE")
        .fetch_one(pool)
        .await?;

    if total > 5_000 {
        tracing::warn!("Library has {total} manga — skipping duplicate check (threshold: 5000)");
        return Ok(vec![]);
    }

    let norm = normalise_title(title);
    let first_word = norm.split_whitespace().next().unwrap_or(&norm).to_string();

    struct CandidateRow {
        id: i64,
        name: String,
        source_id: i64,
    }

    let candidates = if let Some(excl) = exclude_manga_id {
        sqlx::query_as!(
            CandidateRow,
            "SELECT id, name, source_id FROM manga \
             WHERE is_orphaned = FALSE AND id != ? AND name LIKE '%' || ? || '%'",
            excl,
            first_word
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as!(
            CandidateRow,
            "SELECT id, name, source_id FROM manga \
             WHERE is_orphaned = FALSE AND name LIKE '%' || ? || '%'",
            first_word
        )
        .fetch_all(pool)
        .await?
    };

    let mut hits = Vec::new();

    for c in candidates {
        let sim = strsim::jaro_winkler(&norm, &normalise_title(&c.name));
        if sim >= 0.85 {
            let author_match = if !authors.is_empty() {
                let db_authors: Vec<String> = sqlx::query_scalar!(
                    "SELECT p.name FROM manga_people mp \
                     JOIN people p ON mp.person_id = p.id \
                     WHERE mp.manga_id = ?",
                    c.id
                )
                .fetch_all(pool)
                .await
                .unwrap_or_default();

                authors.iter().any(|a| {
                    db_authors.iter().any(|db_a| {
                        strsim::jaro_winkler(&a.to_lowercase(), &db_a.to_lowercase()) >= 0.80
                    })
                })
            } else {
                false
            };

            hits.push(SimilarMangaHit {
                id: c.id,
                name: c.name,
                source_id: c.source_id,
                similarity: sim,
                author_match,
            });
        }
    }

    hits.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(hits)
}

/// Called after every successful manga insert. Finds similar manga and persists
/// the pairs to `duplicate_pairs`. Existing rows (including dismissed ones) are
/// left untouched via `INSERT OR IGNORE`.
pub async fn record_duplicates_for_manga(pool: &SqlitePool, new_manga_id: i64) -> Result<()> {
    struct TitleRow {
        name: String,
    }
    let row = sqlx::query_as!(
        TitleRow,
        "SELECT name FROM manga WHERE id = ?",
        new_manga_id
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else { return Ok(()) };

    let authors: Vec<String> = sqlx::query_scalar!(
        "SELECT p.name FROM manga_people mp \
         JOIN people p ON mp.person_id = p.id WHERE mp.manga_id = ?",
        new_manga_id
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let hits = find_similar_manga(pool, &row.name, &authors, Some(new_manga_id)).await?;

    for hit in hits {
        let (a, b) = if new_manga_id < hit.id {
            (new_manga_id, hit.id)
        } else {
            (hit.id, new_manga_id)
        };
        let author_match = hit.author_match;
        let sim = hit.similarity;
        sqlx::query!(
            "INSERT OR IGNORE INTO duplicate_pairs \
             (manga_a_id, manga_b_id, similarity, author_match) VALUES (?, ?, ?, ?)",
            a,
            b,
            sim,
            author_match
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Read persisted (non-dismissed) duplicate pairs from the DB, joining manga details.
pub async fn list_duplicate_pairs(pool: &SqlitePool) -> Result<Vec<DuplicatePair>> {
    struct PairRow {
        manga_a_id: i64,
        name_a: String,
        source_id_a: i64,
        cover_url_a: Option<String>,
        local_cover_path_a: Option<String>,
        manga_b_id: i64,
        name_b: String,
        source_id_b: i64,
        cover_url_b: Option<String>,
        local_cover_path_b: Option<String>,
        similarity: f64,
        author_match: bool,
    }

    let rows: Vec<PairRow> = sqlx::query_as!(
        PairRow,
        r#"SELECT
            dp.manga_a_id,
            ma.name AS name_a,
            ma.source_id AS source_id_a,
            ma.cover_url AS cover_url_a,
            ma.local_cover_path AS local_cover_path_a,
            dp.manga_b_id,
            mb.name AS name_b,
            mb.source_id AS source_id_b,
            mb.cover_url AS cover_url_b,
            mb.local_cover_path AS local_cover_path_b,
            dp.similarity AS "similarity: f64",
            dp.author_match AS "author_match: bool"
           FROM duplicate_pairs dp
           JOIN manga ma ON ma.id = dp.manga_a_id
           JOIN manga mb ON mb.id = dp.manga_b_id
           WHERE dp.dismissed = FALSE
             AND ma.is_orphaned = FALSE
             AND mb.is_orphaned = FALSE
           ORDER BY dp.similarity DESC"#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| DuplicatePair {
            manga_a: MangaSummary {
                id: r.manga_a_id,
                name: r.name_a,
                source_id: r.source_id_a,
                cover_url: r.cover_url_a,
                local_cover_path: r.local_cover_path_a,
            },
            manga_b: MangaSummary {
                id: r.manga_b_id,
                name: r.name_b,
                source_id: r.source_id_b,
                cover_url: r.cover_url_b,
                local_cover_path: r.local_cover_path_b,
            },
            similarity: r.similarity,
            author_match: r.author_match,
        })
        .collect())
}

/// Mark a pair as dismissed. Canonical ordering is enforced so callers can pass
/// the IDs in either order.
pub async fn dismiss_duplicate_pair(
    pool: &SqlitePool,
    manga_id_x: i64,
    manga_id_y: i64,
) -> Result<()> {
    let (a, b) = if manga_id_x < manga_id_y {
        (manga_id_x, manga_id_y)
    } else {
        (manga_id_y, manga_id_x)
    };
    sqlx::query!(
        "UPDATE duplicate_pairs SET dismissed = TRUE \
         WHERE manga_a_id = ? AND manga_b_id = ?",
        a,
        b
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Full-library rescan: computes all pairs O(n²) and persists new ones.
/// Existing rows (including dismissed ones) are left untouched via INSERT OR IGNORE.
/// Returns the number of new pairs recorded.
pub async fn scan_and_persist_duplicates(pool: &SqlitePool) -> Result<u32> {
    let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM manga WHERE is_orphaned = FALSE")
        .fetch_one(pool)
        .await?;

    if total > 5_000 {
        tracing::warn!("Library has {total} manga — duplicate scan skipped (threshold: 5000)");
        return Ok(0);
    }

    struct Row {
        id: i64,
        name: String,
    }

    let all = sqlx::query_as!(Row, "SELECT id, name FROM manga WHERE is_orphaned = FALSE")
        .fetch_all(pool)
        .await?;

    let mut new_pairs: u32 = 0;
    let mut seen: std::collections::HashSet<(i64, i64)> = Default::default();

    for a in &all {
        let norm_a = normalise_title(&a.name);
        let first_word = norm_a
            .split_whitespace()
            .next()
            .unwrap_or(&norm_a)
            .to_string();

        for b in &all {
            if b.id <= a.id {
                continue;
            }
            if !b.name.to_lowercase().contains(&first_word) {
                continue;
            }
            let key = (a.id, b.id); // a.id < b.id guaranteed by the guard above
            if seen.contains(&key) {
                continue;
            }
            let sim = strsim::jaro_winkler(&norm_a, &normalise_title(&b.name));
            if sim < 0.85 {
                continue;
            }
            seen.insert(key);

            let authors_a: Vec<String> = sqlx::query_scalar!(
                "SELECT p.name FROM manga_people mp \
                 JOIN people p ON mp.person_id = p.id WHERE mp.manga_id = ?",
                a.id
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            let authors_b: Vec<String> = sqlx::query_scalar!(
                "SELECT p.name FROM manga_people mp \
                 JOIN people p ON mp.person_id = p.id WHERE mp.manga_id = ?",
                b.id
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            let author_match = authors_a.iter().any(|aa| {
                authors_b
                    .iter()
                    .any(|ab| strsim::jaro_winkler(&aa.to_lowercase(), &ab.to_lowercase()) >= 0.80)
            });

            let affected = sqlx::query!(
                "INSERT OR IGNORE INTO duplicate_pairs \
                 (manga_a_id, manga_b_id, similarity, author_match) VALUES (?, ?, ?, ?)",
                a.id,
                b.id,
                sim,
                author_match
            )
            .execute(pool)
            .await?
            .rows_affected();

            if affected == 1 {
                new_pairs += 1;
            }
        }
    }

    Ok(new_pairs)
}

/// Merge two manga: keep `keep_id`, migrate tracking from `discard_id`, then delete discard.
pub async fn merge_manga(pool: &SqlitePool, keep_id: i64, discard_id: i64) -> Result<()> {
    if keep_id == discard_id {
        return Err(ServiceError::Validation(
            "keep_id and discard_id must be different".into(),
        ));
    }

    let keep_exists = sqlx::query_scalar!("SELECT id FROM manga WHERE id = ?", keep_id)
        .fetch_optional(pool)
        .await?
        .is_some();
    let discard_exists = sqlx::query_scalar!("SELECT id FROM manga WHERE id = ?", discard_id)
        .fetch_optional(pool)
        .await?
        .is_some();
    if !keep_exists || !discard_exists {
        return Err(ServiceError::NotFound("One or both manga not found".into()));
    }

    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"INSERT OR IGNORE INTO user_chapter_tracking (user_id, chapter_id, is_read, last_page_read, last_read_at)
           SELECT uct.user_id,
                  (SELECT c_keep.id FROM chapters c_keep
                   WHERE c_keep.manga_id = ? AND c_keep.chapter_number = c_disc.chapter_number
                   LIMIT 1) AS target_chapter_id,
                  uct.is_read,
                  uct.last_page_read,
                  uct.last_read_at
           FROM user_chapter_tracking uct
           JOIN chapters c_disc ON c_disc.id = uct.chapter_id
           WHERE c_disc.manga_id = ?
             AND (SELECT c_keep.id FROM chapters c_keep
                  WHERE c_keep.manga_id = ? AND c_keep.chapter_number = c_disc.chapter_number
                  LIMIT 1) IS NOT NULL"#,
        keep_id,
        discard_id,
        keep_id
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"INSERT INTO user_manga_tracking (user_id, manga_id, status, score)
           SELECT umt.user_id, ?, umt.status, umt.score
           FROM user_manga_tracking umt
           WHERE umt.manga_id = ?
           ON CONFLICT(user_id, manga_id) DO UPDATE
             SET status = MAX(excluded.status, status),
                 score  = COALESCE(score, excluded.score)"#,
        keep_id,
        discard_id
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT OR IGNORE INTO manga_categories (manga_id, category_id) \
         SELECT ?, category_id FROM manga_categories WHERE manga_id = ?",
        keep_id,
        discard_id
    )
    .execute(&mut *tx)
    .await?;

    // Delete the discard manga — cascades chapters, tracking, duplicate_pairs rows, etc.
    sqlx::query!("DELETE FROM manga WHERE id = ?", discard_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::normalise_title;

    #[test]
    fn strips_leading_the() {
        assert_eq!(normalise_title("The Title"), normalise_title("Title"));
    }

    #[test]
    fn strips_leading_a() {
        assert_eq!(normalise_title("A Story"), normalise_title("Story"));
    }

    #[test]
    fn strips_leading_an() {
        assert_eq!(
            normalise_title("An Adventure"),
            normalise_title("Adventure")
        );
    }

    #[test]
    fn lowercases_output() {
        assert_eq!(normalise_title("My Manga"), normalise_title("my manga"));
    }

    #[test]
    fn strips_punctuation() {
        assert_eq!(normalise_title("My: Manga!"), normalise_title("My Manga"));
    }

    #[test]
    fn strips_vol_suffix() {
        assert_eq!(
            normalise_title("My Manga, Vol. 3"),
            normalise_title("My Manga")
        );
    }

    #[test]
    fn strips_volume_suffix() {
        assert_eq!(
            normalise_title("My Manga, Volume 1"),
            normalise_title("My Manga")
        );
    }

    #[test]
    fn strips_ch_suffix() {
        assert_eq!(
            normalise_title("My Manga Ch. 5"),
            normalise_title("My Manga")
        );
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(normalise_title("foo  bar   baz"), "foo bar baz");
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(normalise_title(""), "");
    }
}
