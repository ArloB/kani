use super::*;
use crate::models::{DailyActivity, GenreCount, MangaReadCount, PaceEntry, ReadingStats};
use std::sync::Arc;
use time::macros::format_description;

static DATE_FMT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

impl AppService {
    pub async fn get_reading_stats(
        &self,
        user_id: i64,
        period_days: i32,
    ) -> Result<Arc<ReadingStats>> {
        let key = (user_id, period_days);
        if let Some(cached) = self.cache.stats.get(&key).await {
            return Ok(cached);
        }
        let stats = Arc::new(self.compute_reading_stats(user_id, period_days).await?);
        self.cache.stats.insert(key, stats.clone()).await;
        Ok(stats)
    }

    async fn compute_reading_stats(&self, user_id: i64, period_days: i32) -> Result<ReadingStats> {
        // Compute cutoff date in Rust to avoid SQLite string concat in datemod arg
        let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::days(period_days as i64))
            .date()
            .format(DATE_FMT)
            .unwrap_or_default();

        let (totals, completed_manga, daily_rows, top_rows, genre_rows, pace_rows) = tokio::try_join!(
            sqlx::query!(
                r#"
                SELECT
                    COUNT(*) as "chapters_read!: i64",
                    COUNT(DISTINCT c.manga_id) as "manga_read!: i64"
                FROM user_chapter_tracking uct
                JOIN chapters c ON c.id = uct.chapter_id
                WHERE uct.is_read = TRUE AND uct.user_id = ?
                "#,
                user_id
            )
            .fetch_one(&self.db),
            sqlx::query_scalar!(
                r#"SELECT COUNT(*) as "count: i64" FROM user_manga_tracking WHERE user_id = ? AND status = 4"#,
                user_id
            )
            .fetch_one(&self.db),
            sqlx::query!(
                r#"
                SELECT
                    DATE(last_read_at) as "date!: String",
                    COUNT(*) as "chapters_read!: i64"
                FROM user_chapter_tracking
                WHERE is_read = TRUE AND user_id = ? AND last_read_at >= ?
                GROUP BY DATE(last_read_at)
                ORDER BY 1 ASC
                "#,
                user_id,
                cutoff
            )
            .fetch_all(&self.db),
            sqlx::query!(
                r#"
                SELECT
                    m.id as "manga_id!: i64",
                    COALESCE(m.local_name, m.name) as "manga_name!: String",
                    COUNT(uct.chapter_id) as "chapters_read!: i64"
                FROM user_chapter_tracking uct
                JOIN chapters c ON c.id = uct.chapter_id
                JOIN manga m ON m.id = c.manga_id
                WHERE uct.is_read = TRUE AND uct.user_id = ?
                GROUP BY m.id
                ORDER BY 3 DESC
                LIMIT 10
                "#,
                user_id
            )
            .fetch_all(&self.db),
            sqlx::query!(
                r#"
                SELECT
                    t.name as "genre!: String",
                    COUNT(uct.chapter_id) as "chapters_read!: i64"
                FROM user_chapter_tracking uct
                JOIN chapters c ON c.id = uct.chapter_id
                JOIN manga_tags mt ON mt.manga_id = c.manga_id
                JOIN tags t ON t.id = mt.tag_id
                WHERE uct.is_read = TRUE AND uct.user_id = ?
                GROUP BY t.name
                ORDER BY 2 DESC
                LIMIT 15
                "#,
                user_id
            )
            .fetch_all(&self.db),
            sqlx::query!(
                r#"
                SELECT
                    DATE(uct.last_read_at)         AS "date!: String",
                    COALESCE(SUM(c.page_count), 0)  AS "pages!: i64"
                FROM user_chapter_tracking uct
                JOIN chapters c ON c.id = uct.chapter_id
                WHERE uct.is_read = TRUE AND uct.user_id = ? AND uct.last_read_at >= ?
                  AND c.page_count IS NOT NULL
                GROUP BY DATE(uct.last_read_at)
                ORDER BY 1 ASC
                "#,
                user_id,
                cutoff
            )
            .fetch_all(&self.db),
        )?;

        let daily_activity: Vec<DailyActivity> = daily_rows
            .into_iter()
            .map(|r| DailyActivity {
                date: r.date,
                chapters_read: r.chapters_read,
            })
            .collect();
        let top_manga: Vec<MangaReadCount> = top_rows
            .into_iter()
            .map(|r| MangaReadCount {
                manga_id: r.manga_id,
                manga_name: r.manga_name,
                chapters_read: r.chapters_read,
            })
            .collect();
        let genre_breakdown: Vec<GenreCount> = genre_rows
            .into_iter()
            .map(|r| GenreCount {
                genre: r.genre,
                chapters_read: r.chapters_read,
            })
            .collect();

        let reading_pace: Vec<PaceEntry> = pace_rows
            .into_iter()
            .map(|r| PaceEntry {
                date: r.date,
                pages: r.pages,
            })
            .collect();

        let (current_streak, longest_streak) = calculate_streaks(&daily_activity);

        Ok(ReadingStats {
            total_chapters_read: totals.chapters_read,
            total_manga_read: totals.manga_read,
            completed_manga,
            current_streak,
            longest_streak,
            daily_activity,
            top_manga,
            genre_breakdown,
            reading_pace,
        })
    }

    // ── Reading-pace history (#34) ────────────────────────────────────────────
    /// Returns one row per day: date + pages read that day, for the last `period_days`.
    /// Derived entirely from `user_chapter_tracking` — no new table required.
    pub async fn get_reading_pace(&self, user_id: i64, period_days: i32) -> Result<Vec<PaceEntry>> {
        let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::days(period_days as i64))
            .date()
            .format(DATE_FMT)
            .unwrap_or_default();

        let rows = sqlx::query!(
            r#"
            SELECT
                DATE(uct.last_read_at)           AS "date!: String",
                COALESCE(SUM(c.page_count), 0)   AS "pages!: i64"
            FROM user_chapter_tracking uct
            JOIN chapters c ON c.id = uct.chapter_id
            WHERE uct.is_read = TRUE
              AND uct.user_id = ?
              AND uct.last_read_at >= ?
              AND c.page_count IS NOT NULL
            GROUP BY DATE(uct.last_read_at)
            ORDER BY 1 ASC
            "#,
            user_id,
            cutoff,
        )
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| PaceEntry {
                date: r.date,
                pages: r.pages,
            })
            .collect())
    }
}

fn calculate_streaks(activity: &[DailyActivity]) -> (i64, i64) {
    if activity.is_empty() {
        return (0, 0);
    }

    let mut dates: Vec<&str> = activity.iter().map(|d| d.date.as_str()).collect();
    dates.sort_unstable();
    dates.dedup();

    if dates.is_empty() {
        return (0, 0);
    }

    // Longest consecutive streak
    let mut longest: i64 = 1;
    let mut run: i64 = 1;
    for i in 1..dates.len() {
        if is_next_day(dates[i - 1], dates[i]) {
            run += 1;
            if run > longest {
                longest = run;
            }
        } else {
            run = 1;
        }
    }

    // Current streak: must end today or yesterday to be "live"
    let now = time::OffsetDateTime::now_utc();
    let today = now.date().format(DATE_FMT).unwrap_or_default();
    let yesterday = (now - time::Duration::days(1))
        .date()
        .format(DATE_FMT)
        .unwrap_or_default();

    let last = *dates.last().expect("non-empty");
    if last != today && last != yesterday {
        return (0, longest);
    }

    let mut current: i64 = 1;
    let mut i = dates.len();
    while i > 1 {
        if is_next_day(dates[i - 2], dates[i - 1]) {
            current += 1;
            i -= 1;
        } else {
            break;
        }
    }

    (current, longest)
}

fn is_next_day(a: &str, b: &str) -> bool {
    let Some((ay, am, ad)) = parse_ymd(a) else {
        return false;
    };
    let Some((by_, bm, bd)) = parse_ymd(b) else {
        return false;
    };

    let dim = |y: u32, m: u32| -> u32 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400) {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    };

    let (ny, nm, nd) = if ad < dim(ay, am) {
        (ay, am, ad + 1)
    } else if am < 12 {
        (ay, am + 1, 1)
    } else {
        (ay + 1, 1, 1)
    };

    by_ == ny && bm == nm && bd == nd
}

fn parse_ymd(s: &str) -> Option<(u32, u32, u32)> {
    if s.len() < 10 {
        return None;
    }
    Some((
        s[0..4].parse().ok()?,
        s[5..7].parse().ok()?,
        s[8..10].parse().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acts(dates: &[&str]) -> Vec<DailyActivity> {
        dates
            .iter()
            .map(|d| DailyActivity {
                date: d.to_string(),
                chapters_read: 1,
            })
            .collect()
    }

    #[test]
    fn streak_empty() {
        assert_eq!(calculate_streaks(&[]), (0, 0));
    }

    #[test]
    fn streak_single_old_day() {
        let (cur, best) = calculate_streaks(&acts(&["2020-01-01"]));
        assert_eq!(cur, 0);
        assert_eq!(best, 1);
    }

    #[test]
    fn streak_gap_resets_longest() {
        let (_, best) = calculate_streaks(&acts(&["2026-01-01", "2026-01-02", "2026-01-04"]));
        assert_eq!(best, 2);
    }

    #[test]
    fn is_next_day_month_boundary() {
        assert!(is_next_day("2026-01-31", "2026-02-01"));
    }

    #[test]
    fn is_next_day_year_boundary() {
        assert!(is_next_day("2025-12-31", "2026-01-01"));
    }

    #[test]
    fn is_next_day_leap_year() {
        assert!(is_next_day("2024-02-28", "2024-02-29"));
        assert!(!is_next_day("2025-02-28", "2025-02-29"));
        assert!(is_next_day("2025-02-28", "2025-03-01"));
    }
}
