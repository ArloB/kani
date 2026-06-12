use super::{get_access_token, get_mapping};
use crate::error::Result;
use crate::ids::{MangaId, UserId};
use crate::service::AppService;

impl AppService {
    /// Sync a single manga's tracking state with all linked external trackers.
    pub async fn sync_manga_trackers(&self, user_id: UserId, manga_id: MangaId) -> Result<()> {
        let local = self.get_manga_tracking(user_id, manga_id).await?;

        if !local.tracking_enabled {
            return Ok(());
        }

        // Collect tracker IDs and names to process; avoids holding the read lock across awaits
        // on DB calls that could be slow, and prevents any theoretical deadlock.
        let tracker_ids: Vec<i64> = {
            let registry = self.tracker_registry.read().await;
            registry.trackers.keys().copied().collect()
        };

        for tracker_id in tracker_ids {
            let Some(tracker_manga_id) =
                get_mapping(&self.db, user_id, tracker_id, manga_id).await?
            else {
                continue;
            };

            let (access_token, remote) = {
                let registry = self.tracker_registry.read().await;
                let Some(tracker) = registry.get(tracker_id) else {
                    continue;
                };

                let token = match get_access_token(
                    &self.db,
                    tracker_id,
                    user_id,
                    tracker,
                    self.encryption.as_deref(),
                )
                .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(
                            "Skipping sync for tracker {} ({}): {e}",
                            tracker_id,
                            tracker.name()
                        );
                        continue;
                    }
                };

                let remote = match tracker.get_status(&token, &tracker_manga_id).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to get status from {} for manga {}: {e}",
                            tracker.name(),
                            tracker_manga_id
                        );
                        continue;
                    }
                };

                (token, remote)
            };

            // Conflict resolution: progress only moves forward.
            let chapters_read = local.chapters_read.max(remote.chapters_read);
            let status = local.status.or(remote.status);
            let score = local.score.or(remote.score);

            // Push merged state to the remote tracker (lock-free — we dropped the read lock).
            if let Some(s) = status {
                let registry = self.tracker_registry.read().await;
                if let Some(tracker) = registry.get(tracker_id)
                    && let Err(e) = tracker
                        .update_status(&access_token, &tracker_manga_id, s, score, chapters_read)
                        .await
                {
                    tracing::warn!("Failed to push to {}: {e}", tracker.name());
                }
            }

            if remote.status.is_some()
                && local.status.is_none()
                && let Some(s) = remote.status
            {
                self.set_manga_status(user_id, manga_id, s).await?;
            }

            if remote.score.is_some()
                && local.score.is_none()
                && let Some(s) = remote.score
            {
                self.set_manga_score(user_id, manga_id, s).await?;
            }

            if remote.chapters_read > local.chapters_read {
                tracing::info!(
                    "Remote tracker {} has more progress ({} vs {})",
                    tracker_id,
                    remote.chapters_read,
                    local.chapters_read
                );
            }
        }

        Ok(())
    }

    /// Sync all mapped manga for a user across all linked trackers.
    pub async fn sync_all_trackers(&self, user_id: UserId) -> Result<()> {
        let manga_ids: Vec<i64> = sqlx::query_scalar!(
            "SELECT DISTINCT manga_id FROM tracker_manga_mappings WHERE user_id = ?",
            user_id,
        )
        .fetch_all(&self.db)
        .await?;

        for manga_id in manga_ids {
            if let Err(e) = self.sync_manga_trackers(user_id, MangaId(manga_id)).await {
                tracing::warn!("Sync failed for manga {manga_id}: {e}");
            }
        }

        Ok(())
    }
}
