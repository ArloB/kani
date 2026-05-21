use crate::models::ReadingStats;
use dashmap::DashMap;
use moka::future::Cache;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct RequestCache {
    manga_details: Cache<(i64, String), String>,
    popular_manga: Cache<(i64, i32, i32, String), String>,
    chapter_list: Cache<(i64, String, i32, i32, String), String>,
    pages: Cache<(i64, String, String), String>,
    search_results: Cache<(i64, String, i32, i32, String), String>,
    pub preference_schema: DashMap<i64, Vec<kani_core::PreferenceSpec>>,
    /// Reading stats keyed by (user_id, period_days). 10-minute TTL.
    pub stats: Cache<(i64, i32), Arc<ReadingStats>>,
}

impl RequestCache {
    pub fn new() -> Self {
        Self {
            manga_details: Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(15 * 60))
                .support_invalidation_closures()
                .build(),
            popular_manga: Cache::builder()
                .max_capacity(200)
                .time_to_live(Duration::from_secs(5 * 60))
                .support_invalidation_closures()
                .build(),
            chapter_list: Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(10 * 60))
                .support_invalidation_closures()
                .build(),
            pages: Cache::builder()
                .max_capacity(50)
                .time_to_live(Duration::from_secs(10 * 60))
                .support_invalidation_closures()
                .build(),
            search_results: Cache::builder()
                .max_capacity(300)
                .time_to_live(Duration::from_secs(90))
                .support_invalidation_closures()
                .build(),
            preference_schema: DashMap::new(),
            stats: Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(10 * 60))
                .support_invalidation_closures()
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
        page_size: i32,
        filters_key: String,
        init: F,
    ) -> Result<String, Arc<E>>
    where
        F: Future<Output = Result<String, E>>,
        E: Send + Sync + 'static,
    {
        self.popular_manga
            .try_get_with((source_id, page, page_size, filters_key), init)
            .await
    }

    pub async fn get_or_fetch_chapter_list<F, E>(
        &self,
        source_id: i64,
        manga_id: &str,
        page: i32,
        page_size: i32,
        sort: &str,
        init: F,
    ) -> Result<String, Arc<E>>
    where
        F: Future<Output = Result<String, E>>,
        E: Send + Sync + 'static,
    {
        self.chapter_list
            .try_get_with((source_id, manga_id.to_string(), page, page_size, sort.to_string()), init)
            .await
    }

    pub async fn get_or_fetch_search_results<F, E>(
        &self,
        source_id: i64,
        query: &str,
        page: i32,
        page_size: i32,
        filters_key: String,
        init: F,
    ) -> Result<String, Arc<E>>
    where
        F: Future<Output = Result<String, E>>,
        E: Send + Sync + 'static,
    {
        self.search_results
            .try_get_with((source_id, query.to_string(), page, page_size, filters_key), init)
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

    pub fn get_preference_schema(
        &self,
        source_id: i64,
    ) -> Option<Vec<kani_core::PreferenceSpec>> {
        self.preference_schema
            .get(&source_id)
            .map(|r| r.value().clone())
    }

    pub fn insert_preference_schema(
        &self,
        source_id: i64,
        schema: Vec<kani_core::PreferenceSpec>,
    ) {
        self.preference_schema.insert(source_id, schema);
    }

    pub async fn invalidate_chapter_list_for_manga(&self, source_id: i64, manga_id: &str) {
        let owned = manga_id.to_string();
        let _ = self
            .chapter_list
            .invalidate_entries_if(move |(sid, mid, _page, _page_size, _sort), _| {
                *sid == source_id && *mid == owned
            });
    }

    pub fn invalidate_stats(&self, user_id: i64) {
        let _ = self
            .stats
            .invalidate_entries_if(move |(id, _period), _| *id == user_id);
    }

    pub fn clear_all(&self) {
        self.manga_details.invalidate_all();
        self.popular_manga.invalidate_all();
        self.chapter_list.invalidate_all();
        self.pages.invalidate_all();
        self.search_results.invalidate_all();
        self.stats.invalidate_all();
    }

    pub fn invalidate_source(&self, source_id: i64) {
        let sid = source_id;
        let _ = self
            .manga_details
            .invalidate_entries_if(move |(id, _), _| *id == sid);
        let _ = self
            .popular_manga
            .invalidate_entries_if(move |(id, ..), _| *id == sid);
        let _ = self
            .chapter_list
            .invalidate_entries_if(move |(id, ..), _| *id == sid);
        let _ = self
            .pages
            .invalidate_entries_if(move |(id, ..), _| *id == sid);
        let _ = self
            .search_results
            .invalidate_entries_if(move |(id, ..), _| *id == sid);
        self.preference_schema.remove(&source_id);
    }
}

impl Default for RequestCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn default_is_equivalent_to_new() {
        let _a = RequestCache::new();
        let _b = RequestCache::default();
    }

    #[tokio::test]
    async fn manga_details_miss_calls_init() {
        let cache = RequestCache::new();
        let result = cache
            .get_or_fetch_manga_details(1, "m1", async {
                Ok::<_, std::io::Error>("data".to_string())
            })
            .await;
        assert_eq!(result.unwrap().as_str(), "data");
    }

    #[tokio::test]
    async fn manga_details_hit_returns_cached_without_reinvoking_init() {
        let cache = RequestCache::new();
        let calls = Arc::new(AtomicU32::new(0));

        for _ in 0..3 {
            let c = calls.clone();
            let _ = cache
                .get_or_fetch_manga_details(1, "m1", async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, std::io::Error>("data".to_string())
                })
                .await
                .unwrap();
        }

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "init should only be called once"
        );
    }

    #[tokio::test]
    async fn manga_details_different_keys_are_independent() {
        let cache = RequestCache::new();
        let _ = cache
            .get_or_fetch_manga_details(1, "m1", async { Ok::<_, std::io::Error>("A".to_string()) })
            .await
            .unwrap();
        let v2 = cache
            .get_or_fetch_manga_details(1, "m2", async { Ok::<_, std::io::Error>("B".to_string()) })
            .await
            .unwrap();
        assert_eq!(v2.as_str(), "B");
    }

    #[tokio::test]
    async fn popular_manga_caches_per_page() {
        let cache = RequestCache::new();
        let _ = cache
            .get_or_fetch_popular_manga(1, 1, 20, "".to_string(), async {
                Ok::<_, std::io::Error>("p1".to_string())
            })
            .await
            .unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let _ = cache
            .get_or_fetch_popular_manga(1, 2, 20, "".to_string(), async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("p2".to_string())
            })
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn search_results_cached_for_same_query() {
        let cache = RequestCache::new();
        let calls = Arc::new(AtomicU32::new(0));

        for _ in 0..2 {
            let c = calls.clone();
            let _ = cache
                .get_or_fetch_search_results(1, "berserk", 1, 20, "".to_string(), async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, std::io::Error>("results".to_string())
                })
                .await
                .unwrap();
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn invalidate_chapter_list_removes_matching_entries() {
        let cache = RequestCache::new();
        let calls = Arc::new(AtomicU32::new(0));

        let c = calls.clone();
        let _ = cache
            .get_or_fetch_chapter_list(1, "m1", 1, 20, "", async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("chapters".to_string())
            })
            .await
            .unwrap();

        cache.invalidate_chapter_list_for_manga(1, "m1").await;
        cache.chapter_list.run_pending_tasks().await;
        std::thread::sleep(std::time::Duration::from_millis(200));
        cache.chapter_list.run_pending_tasks().await;

        let c = calls.clone();
        let _ = cache
            .get_or_fetch_chapter_list(1, "m1", 1, 20, "", async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("chapters".to_string())
            })
            .await
            .unwrap();

        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "init should be called again after invalidation"
        );
    }

    #[tokio::test]
    async fn invalidate_chapter_list_does_not_affect_other_manga() {
        let cache = RequestCache::new();
        let calls = Arc::new(AtomicU32::new(0));

        let _ = cache
            .get_or_fetch_chapter_list(1, "m1", 1, 20, "", async {
                Ok::<_, std::io::Error>("m1".to_string())
            })
            .await
            .unwrap();
        let c = calls.clone();
        let _ = cache
            .get_or_fetch_chapter_list(1, "m2", 1, 20, "", async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("m2".to_string())
            })
            .await
            .unwrap();

        cache.invalidate_chapter_list_for_manga(1, "m1").await;
        cache.chapter_list.run_pending_tasks().await;
        std::thread::sleep(std::time::Duration::from_millis(200));
        cache.chapter_list.run_pending_tasks().await;

        let c = calls.clone();
        let _ = cache
            .get_or_fetch_chapter_list(1, "m2", 1, 20, "", async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("m2".to_string())
            })
            .await
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1, "m2 should remain cached");
    }

    #[test]
    fn preference_schema_insert_and_get() {
        let cache = RequestCache::new();
        cache.insert_preference_schema(1, vec![]);
        assert!(cache.get_preference_schema(1).is_some());
    }

    #[test]
    fn preference_schema_missing_source_returns_none() {
        let cache = RequestCache::new();
        assert!(cache.get_preference_schema(99).is_none());
    }

    #[test]
    fn invalidate_source_removes_preference_schema() {
        let cache = RequestCache::new();
        cache.insert_preference_schema(5, vec![]);
        cache.invalidate_source(5);
        assert!(cache.get_preference_schema(5).is_none());
    }

    #[tokio::test]
    async fn concurrent_fetches_for_same_key_call_init_once() {
        let cache = Arc::new(RequestCache::new());
        let calls = Arc::new(AtomicU32::new(0));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let cache = cache.clone();
                let calls = calls.clone();
                tokio::spawn(async move {
                    let c = calls.clone();
                    cache
                        .get_or_fetch_manga_details(1, "concurrent", async move {
                            c.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                            Ok::<_, std::io::Error>("value".to_string())
                        })
                        .await
                        .unwrap()
                })
            })
            .collect();

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
