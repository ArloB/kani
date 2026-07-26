use super::super::*;

// Per-manga download-rule preview and chapter filtering.

impl AppService {
    /// Returns `(matching_chapters, total_chapters)` for a hypothetical rule set,
    /// without modifying any state.
    ///
    /// `matching` is the count that would *actually* be downloaded: the same
    /// pipeline auto-download runs — the rule predicate **and** the per-manga
    /// scanlator whitelist/block + priority dedup. Applying only the rule
    /// predicate here (as this once did) overstated the number, since the UI
    /// promises "{matching} of {total} would be downloaded with these rules".
    pub async fn preview_download_rules(
        &self,
        manga_id: MangaId,
        kinds: Vec<DownloadRuleKind>,
    ) -> Result<(usize, usize)> {
        let rows = sqlx::query_as::<_, ChapterFilterRow>(
            "SELECT id, scanlator, language, name, chapter_number, uploaded_at
             FROM chapters WHERE manga_id = ?",
        )
        .bind(manga_id)
        .fetch_all(&self.db_read)
        .await?;

        let total = rows.len();

        let ids_after_rules: Vec<i64> = if kinds.is_empty() {
            rows.iter().map(|r| r.id).collect()
        } else {
            let predicate = self.build_chapter_predicate(kinds);
            rows.iter().filter(|r| predicate(r)).map(|r| r.id).collect()
        };

        let survivors = self
            .select_by_scanlator_prefs(manga_id, ids_after_rules)
            .await;
        Ok((survivors.len(), total))
    }

    fn build_chapter_predicate(
        &self,
        rules: Vec<DownloadRuleKind>,
    ) -> impl Fn(&ChapterFilterRow) -> bool {
        move |chapter| {
            // Axes 0 (Language) and 1 (Title) use include/exclude semantics:
            // if any include rule exists on the axis, at least one must match;
            // all exclude rules on the axis must pass.
            for axis in 0u8..2 {
                let axis_rules: Vec<_> = rules.iter().filter(|r| r.axis() == axis).collect();
                if axis_rules.is_empty() {
                    continue;
                }
                let includes: Vec<_> = axis_rules.iter().filter(|r| r.is_include()).collect();
                let excludes: Vec<_> = axis_rules.iter().filter(|r| !r.is_include()).collect();
                if !includes.is_empty() && !includes.iter().any(|r| r.passes(chapter)) {
                    return false;
                }
                if !excludes.iter().all(|r| r.passes(chapter)) {
                    return false;
                }
            }
            // All remaining axes (2=range, 3=fractional, 4=time) must all pass.
            for rule in rules.iter().filter(|r| r.axis() >= 2) {
                if !rule.passes(chapter) {
                    return false;
                }
            }
            true
        }
    }

    pub async fn filter_chapters_by_rules(
        &self,
        manga_id: MangaId,
        chapter_ids: Vec<crate::ids::ChapterId>,
    ) -> Vec<crate::ids::ChapterId> {
        let candidate_ids: Vec<i64> = chapter_ids.into_iter().map(|c| c.0).collect();
        let raw_rules: Vec<DownloadRuleRow> = sqlx::query_as!(
            DownloadRuleRow,
            "SELECT id, manga_id, rule_type, value
                 FROM download_rules
                 WHERE manga_id = ?",
            manga_id
        )
        .fetch_all(&self.db_read)
        .await
        .unwrap_or_default();

        let ids_after_rules = if raw_rules.is_empty() {
            candidate_ids
        } else {
            if candidate_ids.is_empty() {
                return vec![];
            }

            let rules: Vec<DownloadRule> = raw_rules
                .into_iter()
                .filter_map(|row| DownloadRule::try_from(row).ok())
                .collect();

            let predicate =
                self.build_chapter_predicate(rules.into_iter().map(|dr| dr.kind).collect());

            let chapter_map: HashMap<i64, ChapterFilterRow> = {
                let mut qb = sqlx::QueryBuilder::new(
                    "SELECT id, scanlator, language, name, chapter_number, uploaded_at FROM chapters WHERE id IN (",
                );
                let mut sep = qb.separated(", ");
                for id in &candidate_ids {
                    sep.push_bind(id);
                }
                qb.push(")");
                qb.build_query_as::<ChapterFilterRow>()
                    .fetch_all(&self.db_read)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|row| (row.id, row))
                    .collect()
            };

            candidate_ids
                .iter()
                .copied()
                .filter(|id| chapter_map.get(id).map(&predicate).unwrap_or(false))
                .collect()
        };

        self.select_by_scanlator_prefs(manga_id, ids_after_rules)
            .await
            .into_iter()
            .map(crate::ids::ChapterId)
            .collect()
    }

    /// The second half of the auto-download selection: given the chapter ids that
    /// already passed the download rules, apply the per-manga scanlator
    /// whitelist/block mode and then keep only the highest-priority scanlator's
    /// chapter for each chapter number. Order of the input is preserved.
    ///
    /// Shared by [`Self::filter_chapters_by_rules`] and
    /// [`Self::preview_download_rules`] so the preview count matches what
    /// auto-download actually grabs.
    async fn select_by_scanlator_prefs(
        &self,
        manga_id: MangaId,
        ids_after_rules: Vec<i64>,
    ) -> Vec<i64> {
        let scanlator_mode = self
            .get_scanlator_mode(manga_id)
            .await
            .unwrap_or_else(|_| "priority".into());

        struct PrefEntry {
            priority: i64,
            blocked: bool,
        }

        // Effective, not per-manga: a library-wide default that auto-download
        // ignored would be a preference in name only.
        let prefs: HashMap<String, PrefEntry> = self
            .effective_scanlator_prefs(manga_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                (
                    r.scanlator,
                    PrefEntry {
                        priority: r.priority,
                        blocked: r.blocked,
                    },
                )
            })
            .collect();

        if prefs.is_empty() {
            return ids_after_rules;
        }

        if ids_after_rules.is_empty() {
            return vec![];
        }

        struct ChapRow {
            id: i64,
            chapter_number: f64,
            scanlator: Option<String>,
        }

        let rows: Vec<ChapRow> = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT id, chapter_number, scanlator FROM chapters WHERE id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in &ids_after_rules {
                sep.push_bind(id);
            }
            qb.push(")");
            qb.build_query_as::<(i64, f64, Option<String>)>()
                .fetch_all(&self.db_read)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(id, chapter_number, scanlator)| ChapRow {
                    id,
                    chapter_number,
                    scanlator,
                })
                .collect()
        };

        // In whitelist mode: only chapters whose scanlator appears in prefs.
        // In priority mode: exclude chapters whose scanlator is explicitly blocked.
        let rows: Vec<ChapRow> = rows
            .into_iter()
            .filter(|row| {
                let scanlator = row.scanlator.as_deref().unwrap_or("");
                match scanlator_mode.as_str() {
                    "whitelist" => prefs.contains_key(scanlator),
                    _ => !prefs.get(scanlator).is_some_and(|e| e.blocked),
                }
            })
            .collect();

        let mut best: HashMap<OrderedFloat<f64>, (i64, i64)> = HashMap::new();

        for row in &rows {
            let prio = row
                .scanlator
                .as_deref()
                .and_then(|s| prefs.get(s).map(|e| e.priority))
                .unwrap_or(-1);
            let key = OrderedFloat(row.chapter_number);
            best.entry(key)
                .and_modify(|(existing_id, existing_prio)| {
                    if prio > *existing_prio {
                        *existing_id = row.id;
                        *existing_prio = prio;
                    }
                })
                .or_insert((row.id, prio));
        }

        let winner_ids: std::collections::HashSet<i64> = best.values().map(|(id, _)| *id).collect();
        ids_after_rules
            .into_iter()
            .filter(|id| winner_ids.contains(id))
            .collect()
    }
}
