/// Map a Tachiyomi source ID to the corresponding Kani source name.
/// Source IDs from tachiyomi-extensions and mihon-extensions.
pub fn tachiyomi_source_to_kani_name(source_id: i64) -> Option<&'static str> {
    match source_id {
        2499283573021220255 => Some("MangaDex"),
        2131019126180322627 => Some("WeebCentral"),
        8448310129093543312 => Some("MangaPill"),
        6338219619148105941 | 1470847599087460255 | 2013845246758512290 => Some("Cubari"),
        _ => None,
    }
}

/// Map a Tachiyomi tracker syncId to the Kani tracker name.
/// Only AniList and MyAnimeList are supported by Kani.
pub fn tachiyomi_sync_id_to_tracker_name(sync_id: i32) -> Option<&'static str> {
    match sync_id {
        1 => Some("MyAnimeList"),
        2 => Some("AniList"),
        _ => None, // 3=Kitsu, 6=MangaUpdates, 8=Kitsu, 14=Shikimori — not in Kani
    }
}
