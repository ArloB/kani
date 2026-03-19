use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use moka::future::Cache;

#[derive(Clone)]
pub struct RequestCache {
    manga_details: Cache<(i64, String), String>,
    popular_manga: Cache<(i64, i32), String>,
    chapter_list:  Cache<(i64, String, i32), String>,
    pages:         Cache<(i64, String, String), String>,
    preference_schema: Cache<i64, Vec<crate::types::PreferenceDescriptor>>,
}

impl RequestCache {
    pub fn new() -> Self {
        Self {
            manga_details: Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(15 * 60))
                .build(),
            popular_manga: Cache::builder()
                .max_capacity(200)
                .time_to_live(Duration::from_secs(5 * 60))
                .build(),
            chapter_list: Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(10 * 60))
                .build(),
            pages: Cache::builder()
                .max_capacity(50)
                .time_to_live(Duration::from_secs(10 * 60))
                .build(),
            preference_schema: Cache::builder()
                .max_capacity(500)
                .build(),
        }
    }

    pub async fn get_or_fetch_manga_details<F, E>(
        &self,
        source_id: i64,
        manga_id: &str,
        init: F,
    ) -> Result<String, Arc<E>>
    where
        F: Future<Output = Result<String, E>>,
        E: Send + Sync + 'static,
    {
        self.manga_details
            .try_get_with((source_id, manga_id.to_string()), init)
            .await
    }

    pub async fn get_or_fetch_popular_manga<F, E>(
        &self,
        source_id: i64,
        page: i32,
        init: F,
    ) -> Result<String, Arc<E>>
    where
        F: Future<Output = Result<String, E>>,
        E: Send + Sync + 'static,
    {
        self.popular_manga
            .try_get_with((source_id, page), init)
            .await
    }

    pub async fn get_or_fetch_chapter_list<F, E>(
        &self,
        source_id: i64,
        manga_id: &str,
        page: i32,
        init: F,
    ) -> Result<String, Arc<E>>
    where
        F: Future<Output = Result<String, E>>,
        E: Send + Sync + 'static,
    {
        self.chapter_list
            .try_get_with((source_id, manga_id.to_string(), page), init)
            .await
    }

    pub async fn get_or_fetch_pages<F, E>(
        &self,
        source_id: i64,
        manga_id: &str,
        chapter_id: &str,
        init: F,
    ) -> Result<String, Arc<E>>
    where
        F: Future<Output = Result<String, E>>,
        E: Send + Sync + 'static,
    {
        self.pages
            .try_get_with(
                (source_id, manga_id.to_string(), chapter_id.to_string()),
                init,
            )
            .await
    }

    pub async fn get_or_fetch_preference_schema<F, E>(
        &self,
        source_id: i64,
        init: F,
    ) -> Result<Vec<crate::types::PreferenceDescriptor>, Arc<E>>
    where
        F: Future<Output = Result<Vec<crate::types::PreferenceDescriptor>, E>>,
        E: Send + Sync + 'static,
    {
        self.preference_schema
            .try_get_with(source_id, init)
            .await
    }

    pub async fn invalidate_chapter_list_for_manga(&self, source_id: i64, manga_id: &str) {
        let owned = manga_id.to_string();
        let _ = self.chapter_list.invalidate_entries_if(
            move |(sid, mid, _page), _| *sid == source_id && *mid == owned,
        );
    }

    pub async fn invalidate_source(&self, source_id: i64) {
        let sid = source_id;
        let _ = self.manga_details.invalidate_entries_if(move |(id, _), _| *id == sid);
        let _ = self.popular_manga.invalidate_entries_if(move |(id, _), _| *id == sid);
        let _ = self.chapter_list.invalidate_entries_if(move |(id, ..), _| *id == sid);
        let _ = self.pages.invalidate_entries_if(move |(id, ..), _| *id == sid);
        self.preference_schema.invalidate(&source_id).await;
    }
}

impl Default for RequestCache {
    fn default() -> Self {
        Self::new()
    }
}
