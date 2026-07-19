use super::super::*;
use crate::ids::{MangaId, UserId};

impl AppService {
    /// Returns the continue-reading shelf: manga the user has started that still have
    /// unread chapters, ordered by most-recently-read first.
    pub async fn get_continue_reading_shelf(
        &self,
        user_id: UserId,
        limit: i64,
    ) -> Result<Vec<crate::models::ContinueReadingItem>> {
        let mangas = sqlx::query!(
            r#"
            SELECT m.id, COALESCE(m.local_name, m.name) as "name!: String", m.cover_url, m.local_cover_path, s.base_url
            FROM manga m
            JOIN sources s ON s.id = m.source_id
            JOIN chapters c ON c.manga_id = m.id
            JOIN user_chapter_tracking uct ON uct.chapter_id = c.id AND uct.user_id = ?
            WHERE uct.is_read = true
              AND EXISTS (
                  SELECT 1 FROM chapters c2
                  WHERE c2.manga_id = m.id
                    AND NOT EXISTS (
                        SELECT 1 FROM chapters c3
                        JOIN user_chapter_tracking uct2 ON uct2.chapter_id = c3.id
                        WHERE c3.manga_id = m.id
                          AND c3.chapter_number = c2.chapter_number
                          AND uct2.user_id = ?
                          AND uct2.is_read = true
                    )
              )
            GROUP BY m.id
            ORDER BY MAX(uct.last_read_at) DESC
            LIMIT ?
            "#,
            user_id,
            user_id,
            limit,
        )
        .fetch_all(&self.db_read)
        .await?;

        let mut items = Vec::new();
        for row in mangas {
            let Ok(Some(next)) = self
                .get_continue_reading_chapter(user_id, MangaId(row.id))
                .await
            else {
                continue;
            };
            items.push(crate::models::ContinueReadingItem {
                manga_id: MangaId(row.id),
                manga_name: row.name,
                cover_url: row.cover_url,
                local_cover_path: row.local_cover_path,
                base_url: row.base_url,
                chapter_id: crate::ids::ChapterId(next.chapter_id),
                chapter_number: next.chapter_number,
                last_page: next.last_page,
                page_count: next.page_count,
            });
        }
        Ok(items)
    }
}
