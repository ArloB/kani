use kani_shared::wit_types::ChapterInfo;

pub struct ChapterChunk {
    pub chapters: Vec<ChapterInfo>,
    pub cursor: Option<String>,
}

#[async_trait::async_trait]
pub trait StreamingChapterSource: Send + Sync {
    async fn init(&mut self, manga_id: &str) -> crate::error::Result<String>;
    async fn next(&mut self, cursor: String) -> crate::error::Result<ChapterChunk>;
}

pub fn stream_buffer_size() -> usize {
    std::env::var("KANI_CHAPTER_STREAM_BUFFER_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
}

#[cfg(feature = "streaming-chapters")]
mod enabled {
    use super::{ChapterChunk, StreamingChapterSource, stream_buffer_size};
    use crate::error::{Result, ServiceError};
    use crate::events::AppEvent;
    use crate::ids::MangaId;
    use crate::utils::decode_manga_id;

    impl super::super::AppService {
        pub async fn stream_chapter_list(
            &self,
            source_id: i64,
            manga_db_id: MangaId,
            manga_source_id: &str,
            source: &mut dyn StreamingChapterSource,
        ) -> Result<()> {
            let streaming_ok: bool = sqlx::query_scalar::<_, i64>(
                "SELECT streaming_chapters FROM sources WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(source_id)
            .fetch_optional(&self.db)
            .await
            .map_err(ServiceError::Db)?
            .map(|v| v != 0)
            .unwrap_or(false);

            if !streaming_ok {
                return Err(ServiceError::NotFound(
                    "Source does not support streaming chapter lists".into(),
                ));
            }

            let cursor = source.init(manga_source_id).await.map_err(|e| {
                let _ = self.refresh_tx.send(AppEvent::ChapterListError {
                    manga_id: manga_db_id,
                    error: e.to_string(),
                });
                e
            })?;

            let buf = stream_buffer_size();
            let mut total = 0usize;
            let mut current_cursor = cursor;

            loop {
                let chunk = match source.next(current_cursor.clone()).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = self.refresh_tx.send(AppEvent::ChapterListError {
                            manga_id: manga_db_id,
                            error: e.to_string(),
                        });
                        return Err(e);
                    }
                };

                if !chunk.chapters.is_empty() {
                    let mut tx = self.db.begin().await.map_err(ServiceError::Db)?;
                    let mut qb = sqlx::QueryBuilder::new(
                        "INSERT OR IGNORE INTO chapters \
                        (manga_id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at, discovered_at) ",
                    );
                    qb.push_values(&chunk.chapters, |mut b, ch| {
                        b.push_bind(manga_db_id.0)
                            .push_bind(decode_manga_id(&ch.id))
                            .push_bind(ch.title.clone())
                            .push_bind(ch.number)
                            .push_bind(ch.language.clone())
                            .push_bind(ch.volume)
                            .push_bind(ch.scanlator.clone())
                            .push_bind(ch.date_uploaded);
                        b.push("CURRENT_TIMESTAMP");
                    });
                    qb.build()
                        .execute(&mut *tx)
                        .await
                        .map_err(ServiceError::Db)?;
                    tx.commit().await.map_err(ServiceError::Db)?;

                    total += chunk.chapters.len();

                    let boundary = (total / buf) > ((total - chunk.chapters.len()) / buf);
                    if boundary || chunk.cursor.is_none() {
                        let _ = self.refresh_tx.send(AppEvent::ChapterListPartial {
                            manga_id: manga_db_id,
                            received: total,
                        });
                    }
                }

                match chunk.cursor {
                    Some(next) => current_cursor = next,
                    None => {
                        let _ = self.refresh_tx.send(AppEvent::ChapterListComplete {
                            manga_id: manga_db_id,
                            total,
                        });
                        break;
                    }
                }
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use kani_shared::wit_types::ChapterInfo;

    fn make_chapter(id: &str, number: f64) -> ChapterInfo {
        ChapterInfo {
            id: id.to_string(),
            number,
            title: None,
            volume: None,
            scanlator: None,
            date_uploaded: None,
            language: "en".to_string(),
        }
    }

    struct StubSource {
        chunks: Vec<Vec<ChapterInfo>>,
        pos: usize,
    }

    impl StubSource {
        fn new(chunks: Vec<Vec<ChapterInfo>>) -> Self {
            Self { chunks, pos: 0 }
        }
    }

    #[async_trait::async_trait]
    impl StreamingChapterSource for StubSource {
        async fn init(&mut self, _manga_id: &str) -> crate::error::Result<String> {
            Ok("cursor-0".to_string())
        }

        async fn next(&mut self, _cursor: String) -> crate::error::Result<ChapterChunk> {
            if self.pos < self.chunks.len() {
                let chapters = self.chunks[self.pos].clone();
                self.pos += 1;
                let cursor = if self.pos < self.chunks.len() {
                    Some(format!("cursor-{}", self.pos))
                } else {
                    None
                };
                Ok(ChapterChunk { chapters, cursor })
            } else {
                Ok(ChapterChunk {
                    chapters: vec![],
                    cursor: None,
                })
            }
        }
    }

    #[test]
    fn stub_source_yields_all_chunks() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut src = StubSource::new(vec![
                vec![
                    make_chapter("c1", 1.0),
                    make_chapter("c2", 2.0),
                    make_chapter("c3", 3.0),
                    make_chapter("c4", 4.0),
                ],
                vec![
                    make_chapter("c5", 5.0),
                    make_chapter("c6", 6.0),
                    make_chapter("c7", 7.0),
                    make_chapter("c8", 8.0),
                ],
                vec![
                    make_chapter("c9", 9.0),
                    make_chapter("c10", 10.0),
                    make_chapter("c11", 11.0),
                    make_chapter("c12", 12.0),
                ],
            ]);

            let cursor = src.init("manga-1").await.unwrap();
            let mut total = 0;
            let mut current = cursor;
            loop {
                let chunk = src.next(current.clone()).await.unwrap();
                total += chunk.chapters.len();
                match chunk.cursor {
                    Some(next) => current = next,
                    None => break,
                }
            }
            assert_eq!(total, 12);
        });
    }

    #[test]
    fn stream_buffer_size_default() {
        assert_eq!(stream_buffer_size(), 50);
    }
}
