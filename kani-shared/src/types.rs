//! Shared data types for manga information.

use crate::bindings::kani::extension::types as wit_types;

// ── Host-only imports ───────────────────────────────────────────────────────

#[cfg(feature = "host")]
use serde::{Deserialize, Deserializer, Serialize};
#[cfg(feature = "host")]
use serde::de::{self, SeqAccess, Visitor};
#[cfg(feature = "host")]
use std::fmt;

// ── Always-available types (WASM-safe, no serde/ts-rs dependency) ──────────

/// Manga publication status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "host", derive(Serialize, Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "host", ts(export, export_to = "bindings/"))]
#[cfg_attr(feature = "host", serde(rename_all = "lowercase"))]
pub enum MangaStatus {
    Ongoing,
    Completed,
    Hiatus,
    Cancelled,
    Unknown,
}

impl std::fmt::Display for MangaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MangaStatus::Ongoing => write!(f, "Ongoing"),
            MangaStatus::Completed => write!(f, "Completed"),
            MangaStatus::Hiatus => write!(f, "Hiatus"),
            MangaStatus::Cancelled => write!(f, "Cancelled"),
            MangaStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for MangaStatus {
    fn default() -> Self {
        MangaStatus::Unknown
    }
}

impl From<i64> for MangaStatus {
    fn from(value: i64) -> Self {
        match value {
            0 => MangaStatus::Ongoing,
            1 => MangaStatus::Completed,
            2 => MangaStatus::Hiatus,
            3 => MangaStatus::Cancelled,
            _ => MangaStatus::Unknown,
        }
    }
}

impl From<MangaStatus> for i64 {
    fn from(status: MangaStatus) -> Self {
        match status {
            MangaStatus::Ongoing => 0,
            MangaStatus::Completed => 1,
            MangaStatus::Hiatus => 2,
            MangaStatus::Cancelled => 3,
            MangaStatus::Unknown => 4,
        }
    }
}

/// Filter state used by extensions to read active filter values.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "host", derive(Serialize, Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "host", ts(export, export_to = "bindings/"))]
#[cfg_attr(feature = "host", serde(tag = "kind", content = "data"))]
pub enum FilterState {
    Selection { name: String, value: String },
    Checkbox(bool),
    TextInput(String),
    Multiselect(Vec<String>),
}

impl From<FilterState> for wit_types::FilterState {
    fn from(s: FilterState) -> Self {
        match s {
            FilterState::Selection { name, value } => {
                wit_types::FilterState::Selection(wit_types::OptionState { name, value })
            }
            FilterState::Checkbox(b) => wit_types::FilterState::Checkbox(b),
            FilterState::TextInput(s) => wit_types::FilterState::TextInput(s),
            FilterState::Multiselect(values) => wit_types::FilterState::Multiselect(values),
        }
    }
}

impl From<wit_types::FilterState> for FilterState {
    fn from(s: wit_types::FilterState) -> Self {
        match s {
            wit_types::FilterState::Selection(opt) => {
                FilterState::Selection { name: opt.name, value: opt.value }
            }
            wit_types::FilterState::Checkbox(b) => FilterState::Checkbox(b),
            wit_types::FilterState::TextInput(s) => FilterState::TextInput(s),
            wit_types::FilterState::Multiselect(values) => FilterState::Multiselect(values),
        }
    }
}

/// Active filter value passed to extension search/popular methods.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "host", derive(Serialize, Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "host", ts(export, export_to = "bindings/"))]
pub struct ActiveFilter {
    pub filter_name: String,
    pub state: FilterState,
}

/// Convert a WIT-generated `ActiveFilter` list (guest side) to the shared `ActiveFilter` type.
pub fn to_shared_filters(filters: Vec<wit_types::ActiveFilter>) -> Vec<ActiveFilter> {
    filters
        .into_iter()
        .map(|f| ActiveFilter { filter_name: f.filter_name, state: f.state.into() })
        .collect()
}

// ── filter_list! macro ──────────────────────────────────────────────────────
// Produces `$crate::wit_types::FilterList` directly (no intermediate type).

#[macro_export]
macro_rules! filter_list {
    // 1. Base Case
    (@munch [ $($output:tt)* ]) => {
        $crate::wit_types::FilterList { filters: vec![ $($output)* ] }
    };

    // -------------------------------------------------------------------------
    // INTERNAL SUB-MUNCHER: Parses mixed arrays item-by-item
    // -------------------------------------------------------------------------
    (@opts $id:expr, [ $($accum:tt)* ] ) => { vec![ $($accum)* ] };

    (@opts $id:expr, [ $($accum:tt)* ] ( $n:expr, $v:expr ) $(, $($rest:tt)*)? ) => {
        filter_list!(@opts $id, [
            $($accum)*
            $crate::wit_types::FilterOption {
                filter_name: $id.to_string(),
                name: $n.to_string(),
                value: $v.to_string(),
            },
        ] $($($rest)*)? )
    };

    (@opts $id:expr, [ $($accum:tt)* ] $n:expr $(, $($rest:tt)*)? ) => {
        filter_list!(@opts $id, [
            $($accum)*
            $crate::wit_types::FilterOption {
                filter_name: $id.to_string(),
                name: $n.to_string(),
                value: $n.to_string(),
            },
        ] $($($rest)*)? )
    };

    // -------------------------------------------------------------------------
    // INTERNAL BUILDER
    // -------------------------------------------------------------------------
    (@build [ $($output:tt)* ] $id:expr, $display:expr, $kind:ident, $opts:expr, $def:expr, $sem:expr $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [
            $($output)*
            $crate::wit_types::FilterDef {
                id: $id.to_string(),
                name: $display.to_string(),
                tag: $crate::wit_types::FilterTypeTag::$kind,
                options: $opts,
                default_value: $def,
                semantic: $sem,
            },
        ] $($($rest)*)?)
    };

    // ==========================================
    // SELECT & SORT
    // ==========================================
    // 2-arg, Tuple Default
    (@munch [ $($output:tt)* ] $id:expr, $display:expr, $kind:ident, [ $($opts:tt)* ], default: ( $def_n:expr, $def_v:expr ) $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@build [ $($output)* ] $id, $display, $kind,
            filter_list!(@opts $id, [] $($opts)*),
            Some($crate::wit_types::FilterState::Selection($crate::wit_types::OptionState {
                name: $def_n.to_string(),
                value: $def_v.to_string(),
            })),
            { #[allow(unused_mut)] let mut _s: Option<$crate::wit_types::FilterSemantic> = None; $( _s = Some($sem); )? _s }
            $(; $($rest)*)?
        )
    };
    // 2-arg, Flat Default (Translates to Tuple)
    (@munch [ $($output:tt)* ] $id:expr, $display:expr, $kind:ident, [ $($opts:tt)* ], default: $def:expr $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $id, $display, $kind, [ $($opts)* ], default: ($def, $def) $(, semantic: $sem)? $(; $($rest)*)? )
    };
    // 2-arg, No Default
    (@munch [ $($output:tt)* ] $id:expr, $display:expr, $kind:ident, [ $($opts:tt)* ] $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@build [ $($output)* ] $id, $display, $kind,
            filter_list!(@opts $id, [] $($opts)*),
            None,
            { #[allow(unused_mut)] let mut _s: Option<$crate::wit_types::FilterSemantic> = None; $( _s = Some($sem); )? _s }
            $(; $($rest)*)?
        )
    };

    // 1-arg, Tuple Default
    (@munch [ $($output:tt)* ] $display:expr, $kind:ident, [ $($opts:tt)* ], default: ( $def_n:expr, $def_v:expr ) $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $display, $display, $kind, [ $($opts)* ], default: ($def_n, $def_v) $(, semantic: $sem)? $(; $($rest)*)? )
    };
    // 1-arg, Flat Default
    (@munch [ $($output:tt)* ] $display:expr, $kind:ident, [ $($opts:tt)* ], default: $def:expr $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $display, $display, $kind, [ $($opts)* ], default: ($def, $def) $(, semantic: $sem)? $(; $($rest)*)? )
    };
    // 1-arg, No Default
    (@munch [ $($output:tt)* ] $display:expr, $kind:ident, [ $($opts:tt)* ] $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $display, $display, $kind, [ $($opts)* ] $(, semantic: $sem)? $(; $($rest)*)? )
    };

    // ==========================================
    // CHECKBOX
    // ==========================================
    // 2-arg, With Default
    (@munch [ $($output:tt)* ] $id:expr, $display:expr, Checkbox, default: $def:expr $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@build [ $($output)* ] $id, $display, Checkbox,
            vec![],
            Some($crate::wit_types::FilterState::Checkbox($def)),
            { #[allow(unused_mut)] let mut _s: Option<$crate::wit_types::FilterSemantic> = None; $( _s = Some($sem); )? _s }
            $(; $($rest)*)?
        )
    };
    // 2-arg, No Default
    (@munch [ $($output:tt)* ] $id:expr, $display:expr, Checkbox $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@build [ $($output)* ] $id, $display, Checkbox,
            vec![],
            None,
            { #[allow(unused_mut)] let mut _s: Option<$crate::wit_types::FilterSemantic> = None; $( _s = Some($sem); )? _s }
            $(; $($rest)*)?
        )
    };
    // 1-arg, With Default
    (@munch [ $($output:tt)* ] $display:expr, Checkbox, default: $def:expr $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $display, $display, Checkbox, default: $def $(, semantic: $sem)? $(; $($rest)*)? )
    };
    // 1-arg, No Default
    (@munch [ $($output:tt)* ] $display:expr, Checkbox $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $display, $display, Checkbox $(, semantic: $sem)? $(; $($rest)*)? )
    };

    // ==========================================
    // TEXT INPUT
    // ==========================================
    // 2-arg, With Default
    (@munch [ $($output:tt)* ] $id:expr, $display:expr, TextInput, default: $def:expr $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@build [ $($output)* ] $id, $display, TextInput,
            vec![],
            Some($crate::wit_types::FilterState::TextInput($def.to_string())),
            { #[allow(unused_mut)] let mut _s: Option<$crate::wit_types::FilterSemantic> = None; $( _s = Some($sem); )? _s }
            $(; $($rest)*)?
        )
    };
    // 2-arg, No Default
    (@munch [ $($output:tt)* ] $id:expr, $display:expr, TextInput $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@build [ $($output)* ] $id, $display, TextInput,
            vec![],
            None,
            { #[allow(unused_mut)] let mut _s: Option<$crate::wit_types::FilterSemantic> = None; $( _s = Some($sem); )? _s }
            $(; $($rest)*)?
        )
    };
    // 1-arg, With Default
    (@munch [ $($output:tt)* ] $display:expr, TextInput, default: $def:expr $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $display, $display, TextInput, default: $def $(, semantic: $sem)? $(; $($rest)*)? )
    };
    // 1-arg, No Default
    (@munch [ $($output:tt)* ] $display:expr, TextInput $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $display, $display, TextInput $(, semantic: $sem)? $(; $($rest)*)? )
    };

    // ==========================================
    // MULTISELECT
    // ==========================================
    // 2-arg, No Default
    (@munch [ $($output:tt)* ] $id:expr, $display:expr, Multiselect, [ $($opts:tt)* ] $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@build [ $($output)* ] $id, $display, Multiselect,
            filter_list!(@opts $id, [] $($opts)*),
            None,
            { #[allow(unused_mut)] let mut _s: Option<$crate::wit_types::FilterSemantic> = None; $( _s = Some($sem); )? _s }
            $(; $($rest)*)?
        )
    };
    // 1-arg, No Default
    (@munch [ $($output:tt)* ] $display:expr, Multiselect, [ $($opts:tt)* ] $(, semantic: $sem:expr)? $(; $($rest:tt)*)? ) => {
        filter_list!(@munch [ $($output)* ] $display, $display, Multiselect, [ $($opts)* ] $(, semantic: $sem)? $(; $($rest)*)? )
    };

    // ==========================================
    // ERROR CATCHER & ENTRY POINTS
    // ==========================================
    (@munch $($invalid:tt)*) => {
        compile_error!(concat!("Invalid filter syntax. Stuck on: ", stringify!($($invalid)*)))
    };
    () => { $crate::wit_types::FilterList { filters: vec![] } };
    ($first:expr, $($rest:tt)*) => { filter_list!(@munch [] $first, $($rest)*) };
}

// ── chapter_sort_list! macro ────────────────────────────────────────────────
// Produces `Vec<$crate::wit_types::ChapterSortOption>`.
//
// Syntax (entries separated by `;`):
//   "field_id", "Display Name"          — auto-generates a descending and
//                                         ascending ChapterSortOption pair
//   raw: <expr>                          — inserts a single ChapterSortOption
//                                         as-is, for edge-case singlets
//
// The auto-generated IDs follow the `{field_id}_desc` / `{field_id}_asc`
// convention.  Extensions that need to recover the field and direction from
// the sort string should split on the *last* underscore (via `rsplit_once`)
// so that field IDs containing underscores (e.g. `created_at`) are handled
// correctly.

#[macro_export]
macro_rules! chapter_sort_list {
    // Base case: nothing left to process
    (@munch [$($output:tt)*]) => {
        vec![$($output)*]
    };

    // Pair entry: "field_id", "Display Name" → desc + asc options
    (@munch [$($output:tt)*] $id:literal, $name:literal $(; $($rest:tt)*)?) => {
        chapter_sort_list!(@munch [
            $($output)*
            $crate::wit_types::ChapterSortOption {
                id: concat!($id, "_desc").to_string(),
                name: concat!($name, " (descending)").to_string(),
            },
            $crate::wit_types::ChapterSortOption {
                id: concat!($id, "_asc").to_string(),
                name: concat!($name, " (ascending)").to_string(),
            },
        ] $($($rest)*)?)
    };

    // Raw entry: raw: <expr> → single option inserted verbatim
    (@munch [$($output:tt)*] raw: $expr:expr $(; $($rest:tt)*)?) => {
        chapter_sort_list!(@munch [
            $($output)*
            $expr,
        ] $($($rest)*)?)
    };

    // Error catcher & entry points
    (@munch $($invalid:tt)*) => {
        compile_error!(concat!("Invalid chapter_sort_list syntax. Stuck on: ", stringify!($($invalid)*)))
    };
    () => { vec![] };
    ($($rest:tt)+) => { chapter_sort_list!(@munch [] $($rest)+) };
}

// ── preference_list! macro ──────────────────────────────────────────────────
// Produces `Vec<$crate::wit_types::PreferenceSpec>`.
//
// Syntax (each entry separated by `;`):
//   "key", "Label", Toggle, default: <bool>                    // explicit key
//   "key", "Label", Select, [("Label1","v1"), ...], default: "v"
//   "key", "Label", Text, default: "value"
//   "key", "Label", MultiValueList                             // default is always []
//   "Label", Toggle, default: <bool>                           // key = label
//   "Label", Select, [("Label1","v1"), ...], default: "v"
//   "Label", Text, default: "value"
//   "Label", MultiValueList
// Optional modifiers (order matters for Text):
//   , description: "..."
//   , secret: true                      (Text only; false by default)
//   , description: "...", secret: true
// MultiValueList-specific modifier (before description):
//   , placeholder: "..."

#[macro_export]
macro_rules! preference_list {
    // Base case: no more entries
    (@munch [ $($output:tt)* ]) => { vec![ $($output)* ] };

    // ── Option accumulator for Select ───────────────────────────────────────
    (@opts [ $($accum:tt)* ]) => { vec![ $($accum)* ] };
    (@opts [ $($accum:tt)* ] ( $l:expr, $v:expr ) $(, $($rest:tt)*)? ) => {
        preference_list!(@opts [
            $($accum)*
            ($l.to_string(), $v.to_string()),
        ] $($($rest)*)? )
    };

    // ── Internal builder ────────────────────────────────────────────────────
    (@build [ $($output:tt)* ] $key:expr, $label:expr, $kind:ident, $opts:expr, $def:expr, $desc:expr, $secret:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@munch [
            $($output)*
            $crate::wit_types::PreferenceSpec {
                key: $key.to_string(),
                label: $label.to_string(),
                kind: $crate::wit_types::PrefKind::$kind,
                options: $opts,
                default: $def.to_string(),
                description: $desc,
                secret: $secret,
            },
        ] $($($rest)*)?)
    };

    // ── Toggle ──────────────────────────────────────────────────────────────
    // 2-arg, with description
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, Toggle, default: $def:expr, description: $desc:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, Toggle,
            vec![], if $def { "true" } else { "false" }, Some($desc.to_string()), false
            $(; $($rest)*)?
        )
    };
    // 2-arg, no description
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, Toggle, default: $def:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, Toggle,
            vec![], if $def { "true" } else { "false" }, None, false
            $(; $($rest)*)?
        )
    };
    // 1-arg (forward to 2-arg)
    (@munch [ $($output:tt)* ] $label:expr, Toggle, default: $def:expr $(, description: $desc:expr)? $(; $($rest:tt)*)? ) => {
        preference_list!(@munch [ $($output)* ] $label, $label, Toggle, default: $def $(, description: $desc)? $(; $($rest)*)? )
    };

    // ── Select ──────────────────────────────────────────────────────────────
    // 2-arg, with description
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, Select, [ $($opts:tt)* ], default: $def:expr, description: $desc:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, Select,
            preference_list!(@opts [] $($opts)*), $def, Some($desc.to_string()), false
            $(; $($rest)*)?
        )
    };
    // 2-arg, no description
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, Select, [ $($opts:tt)* ], default: $def:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, Select,
            preference_list!(@opts [] $($opts)*), $def, None, false
            $(; $($rest)*)?
        )
    };
    // 1-arg (forward to 2-arg)
    (@munch [ $($output:tt)* ] $label:expr, Select, [ $($opts:tt)* ], default: $def:expr $(, description: $desc:expr)? $(; $($rest:tt)*)? ) => {
        preference_list!(@munch [ $($output)* ] $label, $label, Select, [ $($opts)* ], default: $def $(, description: $desc)? $(; $($rest)*)? )
    };

    // ── Text ────────────────────────────────────────────────────────────────
    // 2-arg, description + secret: true
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, Text, default: $def:expr, description: $desc:expr, secret: true $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, Text, vec![], $def, Some($desc.to_string()), true $(; $($rest)*)?)
    };
    // 2-arg, description only
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, Text, default: $def:expr, description: $desc:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, Text, vec![], $def, Some($desc.to_string()), false $(; $($rest)*)?)
    };
    // 2-arg, secret: true only
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, Text, default: $def:expr, secret: true $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, Text, vec![], $def, None, true $(; $($rest)*)?)
    };
    // 2-arg, no modifiers
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, Text, default: $def:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, Text, vec![], $def, None, false $(; $($rest)*)?)
    };
    // 1-arg (forward to 2-arg)
    (@munch [ $($output:tt)* ] $label:expr, Text, default: $def:expr $(, $($mods:tt)*)? $(; $($rest:tt)*)? ) => {
        preference_list!(@munch [ $($output)* ] $label, $label, Text, default: $def $(, $($mods)*)? $(; $($rest)*)? )
    };

    // ── MultiValueList ───────────────────────────────────────────────────────
    // Default is always "[]" (empty JSON array); placeholder stored in options.
    // 2-arg: with placeholder, with description
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, MultiValueList, placeholder: $ph:expr, description: $desc:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, MultiValueList,
            vec![("placeholder".to_string(), $ph.to_string())], "[]", Some($desc.to_string()), false
            $(; $($rest)*)?
        )
    };
    // 2-arg: with placeholder, no description
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, MultiValueList, placeholder: $ph:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, MultiValueList,
            vec![("placeholder".to_string(), $ph.to_string())], "[]", None, false
            $(; $($rest)*)?
        )
    };
    // 2-arg: no placeholder, with description
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, MultiValueList, description: $desc:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, MultiValueList,
            vec![], "[]", Some($desc.to_string()), false
            $(; $($rest)*)?
        )
    };
    // 2-arg: no placeholder, no description
    (@munch [ $($output:tt)* ] $key:expr, $label:expr, MultiValueList $(; $($rest:tt)*)? ) => {
        preference_list!(@build [ $($output)* ] $key, $label, MultiValueList,
            vec![], "[]", None, false
            $(; $($rest)*)?
        )
    };
    // 1-arg: with placeholder, with description
    (@munch [ $($output:tt)* ] $label:expr, MultiValueList, placeholder: $ph:expr, description: $desc:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@munch [ $($output)* ] $label, $label, MultiValueList, placeholder: $ph, description: $desc $(; $($rest)*)? )
    };
    // 1-arg: with placeholder, no description
    (@munch [ $($output:tt)* ] $label:expr, MultiValueList, placeholder: $ph:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@munch [ $($output)* ] $label, $label, MultiValueList, placeholder: $ph $(; $($rest)*)? )
    };
    // 1-arg: no placeholder, with description
    (@munch [ $($output:tt)* ] $label:expr, MultiValueList, description: $desc:expr $(; $($rest:tt)*)? ) => {
        preference_list!(@munch [ $($output)* ] $label, $label, MultiValueList, description: $desc $(; $($rest)*)? )
    };
    // 1-arg: no placeholder, no description
    (@munch [ $($output:tt)* ] $label:expr, MultiValueList $(; $($rest:tt)*)? ) => {
        preference_list!(@munch [ $($output)* ] $label, $label, MultiValueList $(; $($rest)*)? )
    };

    // ── Error catcher & entry points ─────────────────────────────────────────
    (@munch $($invalid:tt)*) => {
        compile_error!(concat!("Invalid preference syntax. Stuck on: ", stringify!($($invalid)*)))
    };
    () => { vec![] };
    ($first:expr, $($rest:tt)*) => { preference_list!(@munch [] $first, $($rest)*) };
}

// ── Host-only types ─────────────────────────────────────────────────────────

/// Deserializes `Vec<NamedItem>` from either an array of objects or plain strings.
#[cfg(feature = "host")]
fn deserialize_named_item_vec<'de, D>(deserializer: D) -> Result<Vec<NamedItem>, D::Error>
where
    D: Deserializer<'de>,
{
    struct NamedItemVecVisitor;

    impl<'de> Visitor<'de> for NamedItemVecVisitor {
        type Value = Vec<NamedItem>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an array of NamedItem objects or strings")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut items = Vec::new();
            while let Some(value) = seq.next_element::<serde_json::Value>()? {
                let item = match value {
                    serde_json::Value::String(s) => NamedItem { id: 0, name: s },
                    obj => serde_json::from_value(obj).map_err(de::Error::custom)?,
                };
                items.push(item);
            }
            Ok(items)
        }
    }

    deserializer.deserialize_seq(NamedItemVecVisitor)
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct NamedItem {
    pub id: i64,
    pub name: String,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MangaListItem {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MangaList {
    pub manga: Vec<MangaListItem>,
    pub has_next_page: bool,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MangaInfo {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(deserialize_with = "deserialize_named_item_vec")]
    pub authors: Vec<NamedItem>,
    #[serde(deserialize_with = "deserialize_named_item_vec")]
    pub artists: Vec<NamedItem>,
    pub status: MangaStatus,
    #[serde(deserialize_with = "deserialize_named_item_vec")]
    pub tags: Vec<NamedItem>,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct Chapter {
    pub id: String,
    pub title: Option<String>,
    pub number: f64,
    pub volume: Option<i64>,
    pub language: String,
    pub scanlator: Option<String>,
    pub date_uploaded: Option<i64>,
    #[serde(default)]
    pub download_status: i64,
    #[serde(default)]
    pub is_orphaned: bool,
    pub page_count: Option<i64>,
    #[serde(default)]
    pub is_read: bool,
    #[serde(default)]
    pub last_page_read: Option<i64>,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct ChapterList {
    pub chapters: Vec<Chapter>,
    pub has_next_page: bool,
}

/// A sort option declared by a source extension for its chapter list.
#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct ChapterSortOption {
    pub id: String,
    pub name: String,
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[serde(rename_all = "snake_case")]
pub enum ChapterSortOrder {
    #[default]
    ChapterDesc,
    ChapterAsc,
    UploadedDesc,
    UploadedAsc,
    VolumeDesc,
    VolumeAsc,
    LanguageAsc,
    LanguageDesc,
    ScanlatorAsc,
    ScanlatorDesc,
}

#[cfg(feature = "host")]
impl ChapterSortOrder {
    pub fn to_sql_order(&self) -> &'static str {
        match self {
            Self::ChapterDesc => "c.chapter_number DESC, c.id DESC",
            Self::ChapterAsc => "c.chapter_number ASC, c.id ASC",
            Self::UploadedDesc => "c.uploaded_at DESC, c.chapter_number DESC",
            Self::UploadedAsc => "c.uploaded_at ASC, c.chapter_number ASC",
            Self::VolumeDesc => "c.volume DESC NULLS LAST, c.chapter_number DESC",
            Self::VolumeAsc => "c.volume ASC NULLS FIRST, c.chapter_number ASC",
            Self::LanguageAsc => "c.language ASC NULLS LAST, c.chapter_number DESC",
            Self::LanguageDesc => "c.language DESC NULLS LAST, c.chapter_number DESC",
            Self::ScanlatorAsc => "c.scanlator ASC NULLS LAST, c.chapter_number DESC",
            Self::ScanlatorDesc => "c.scanlator DESC NULLS LAST, c.chapter_number DESC",
        }
    }

    pub fn to_select_value(&self) -> &'static str {
        match self {
            Self::ChapterDesc => "chapter_desc",
            Self::ChapterAsc => "chapter_asc",
            Self::UploadedDesc => "uploaded_desc",
            Self::UploadedAsc => "uploaded_asc",
            Self::VolumeDesc => "volume_desc",
            Self::VolumeAsc => "volume_asc",
            Self::LanguageAsc => "language_asc",
            Self::LanguageDesc => "language_desc",
            Self::ScanlatorAsc => "scanlator_asc",
            Self::ScanlatorDesc => "scanlator_desc",
        }
    }

    pub fn from_select_value(s: &str) -> Self {
        match s {
            "chapter_asc" => Self::ChapterAsc,
            "uploaded_desc" => Self::UploadedDesc,
            "uploaded_asc" => Self::UploadedAsc,
            "volume_desc" => Self::VolumeDesc,
            "volume_asc" => Self::VolumeAsc,
            "language_asc" => Self::LanguageAsc,
            "language_desc" => Self::LanguageDesc,
            "scanlator_asc" => Self::ScanlatorAsc,
            "scanlator_desc" => Self::ScanlatorDesc,
            _ => Self::ChapterDesc,
        }
    }
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[serde(rename_all = "snake_case")]
pub enum MangaSortOrder {
    #[default]
    NameDesc,
    NameAsc,
    UpdatedDesc,
    UpdatedAsc,
    AddedDesc,
    AddedAsc,
    ScoreDesc,
    ScoreAsc,
    LastReadDesc,
}

#[cfg(feature = "host")]
impl MangaSortOrder {
    pub fn to_sql_order(&self) -> &'static str {
        match self {
            Self::NameDesc => "m.name DESC, m.id DESC",
            Self::NameAsc => "m.name ASC, m.id ASC",
            Self::UpdatedDesc => "m.updated_at DESC, m.name ASC",
            Self::UpdatedAsc => "m.updated_at ASC, m.name ASC",
            Self::AddedDesc => "m.created_at DESC, m.name ASC",
            Self::AddedAsc => "m.created_at ASC, m.name ASC",
            Self::ScoreDesc => "umt.score DESC NULLS LAST, m.name ASC",
            Self::ScoreAsc => "umt.score ASC NULLS LAST, m.name ASC",
            Self::LastReadDesc => "max_last_read DESC NULLS LAST, m.name ASC",
        }
    }

    pub fn to_select_value(&self) -> &'static str {
        match self {
            Self::NameDesc => "name_desc",
            Self::NameAsc => "name_asc",
            Self::UpdatedDesc => "updated_desc",
            Self::UpdatedAsc => "updated_asc",
            Self::AddedDesc => "added_desc",
            Self::AddedAsc => "added_asc",
            Self::ScoreDesc => "score_desc",
            Self::ScoreAsc => "score_asc",
            Self::LastReadDesc => "last_read_desc",
        }
    }

    pub fn from_select_value(s: &str) -> Self {
        match s {
            "name_desc" => Self::NameDesc,
            "name_asc" => Self::NameAsc,
            "updated_desc" => Self::UpdatedDesc,
            "updated_asc" => Self::UpdatedAsc,
            "added_desc" => Self::AddedDesc,
            "added_asc" => Self::AddedAsc,
            "score_desc" => Self::ScoreDesc,
            "score_asc" => Self::ScoreAsc,
            "last_read_desc" => Self::LastReadDesc,
            _ => Self::NameAsc,
        }
    }

    /// Returns true if the sort requires the `max_last_read` subquery column in the SELECT.
    pub fn needs_last_read_column(&self) -> bool {
        matches!(self, Self::LastReadDesc)
    }

    /// Returns true if the sort requires joining `user_manga_tracking`.
    pub fn needs_tracking_join(&self) -> bool {
        matches!(self, Self::ScoreDesc | Self::ScoreAsc | Self::LastReadDesc)
    }
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadProgressEvent {
    ChapterStarted {
        chapter_id: i64,
        chapter_name: String,
        manga_id: i64,
        manga_title: String,
        total_pages: usize,
    },
    PageCompleted {
        chapter_id: i64,
        chapter_name: String,
        page_index: i32,
    },
    ChapterCompleted {
        chapter_id: i64,
        chapter_name: String,
        manga_id: i64,
        manga_title: String,
        successful_pages: usize,
    },
    ChapterFailed {
        chapter_id: i64,
        chapter_name: String,
        error: String,
    },
    ChapterCancelled {
        chapter_id: i64,
        chapter_name: String,
    },
    ChapterDeferred {
        chapter_id: i64,
        chapter_name: String,
        reason: String,
    },
}

#[cfg(feature = "host")]
#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct LibraryPage {
    pub items: Vec<MangaListItem>,
    pub has_next_page: bool,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub base_url: String,
    pub enabled: bool,
    pub favourited: bool,
    pub unrestricted_http: bool,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct GlobalSearchResult {
    pub source_id: i64,
    pub source_name: String,
    pub has_next_page: bool,
    pub manga: Vec<MangaListItem>,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub enum SearchScope {
    FavouritedOnly,
    AllEnabled,
    Sources(Vec<i64>),
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct ChapterFilterRow {
    pub id: i64,
    pub scanlator: Option<String>,
    pub language: String,
    pub name: Option<String>,
    pub chapter_number: f64,
    /// Seconds since Unix epoch; `None` if the source didn't provide an upload date.
    pub uploaded_at: Option<i64>,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub enum DownloadRuleKind {
    LanguageInclude(String),
    LanguageExclude(String),
    TitleContains(String),
    TitleExcludes(String),
    ChapterNumberMin(f64),
    ChapterNumberMax(f64),
    ExcludeFractional,
    MaxAgeDays(i32),
    PublishedAfter(i64),
}

#[cfg(feature = "host")]
impl DownloadRuleKind {
    pub fn axis(&self) -> u8 {
        match self {
            Self::LanguageInclude(_) | Self::LanguageExclude(_) => 0,
            Self::TitleContains(_) | Self::TitleExcludes(_) => 1,
            Self::ChapterNumberMin(_) | Self::ChapterNumberMax(_) => 2,
            Self::ExcludeFractional => 3,
            Self::MaxAgeDays(_) | Self::PublishedAfter(_) => 4,
        }
    }

    pub fn is_include(&self) -> bool {
        matches!(self, Self::LanguageInclude(_) | Self::TitleContains(_))
    }

    pub fn passes(&self, chapter: &ChapterFilterRow) -> bool {
        match self {
            Self::LanguageInclude(v) => chapter.language.eq_ignore_ascii_case(v),
            Self::LanguageExclude(v) => !chapter.language.eq_ignore_ascii_case(v),
            Self::TitleContains(v) => chapter
                .name
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&v.to_lowercase()),
            Self::TitleExcludes(v) => !chapter
                .name
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&v.to_lowercase()),
            Self::ChapterNumberMin(min) => chapter.chapter_number >= *min,
            Self::ChapterNumberMax(max) => chapter.chapter_number <= *max,
            Self::ExcludeFractional => {
                (chapter.chapter_number - chapter.chapter_number.floor()).abs() < f64::EPSILON
            }
            Self::MaxAgeDays(days) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let cutoff = now - (*days as i64) * 86_400;
                chapter.uploaded_at.is_none_or(|t| t >= cutoff)
            }
            Self::PublishedAfter(epoch) => {
                chapter.uploaded_at.is_none_or(|t| t >= *epoch)
            }
        }
    }
}

#[cfg(feature = "host")]
impl std::fmt::Display for DownloadRuleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadRuleKind::LanguageInclude(v) => write!(f, "Language includes {v}"),
            DownloadRuleKind::LanguageExclude(v) => write!(f, "Language excludes {v}"),
            DownloadRuleKind::TitleContains(v) => write!(f, "Title contains {v}"),
            DownloadRuleKind::TitleExcludes(v) => write!(f, "Title excludes {v}"),
            DownloadRuleKind::ChapterNumberMin(n) => write!(f, "Chapter number ≥ {n}"),
            DownloadRuleKind::ChapterNumberMax(n) => write!(f, "Chapter number ≤ {n}"),
            DownloadRuleKind::ExcludeFractional => write!(f, "Exclude fractional chapters"),
            DownloadRuleKind::MaxAgeDays(n) => write!(f, "Published within last {n} days"),
            DownloadRuleKind::PublishedAfter(ts) => write!(f, "Published after epoch {ts}"),
        }
    }
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct DownloadRule {
    pub id: i64,
    pub manga_id: i64,
    pub kind: DownloadRuleKind,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct ScanlatorPreference {
    pub id: i64,
    pub manga_id: i64,
    pub scanlator: String,
    pub priority: i64,
    /// In `priority` mode: if `true` this scanlator is completely blocked.
    pub blocked: bool,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct AppSettings {
    pub flaresolverr_url: String,
    pub library_path: String,
    pub concurrent_page_downloads: i64,
    pub concurrent_manga_downloads: i64,
    pub chapter_queue_size: i64,
    pub max_retries: i64,
    pub initial_retry_delay_ms: i64,
    pub max_wasm_instances: i64,
    pub auto_scan: bool,
    pub scan_interval_minutes: i64,
    pub default_tracking_enabled: bool,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct RecentUpdate {
    pub recent_updates: Vec<RecentUpdateItem>,
    pub has_next_page: bool,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct RecentUpdateItem {
    pub manga_id: i64,
    pub manga_name: String,
    pub cover_url: Option<String>,
    #[serde(skip)]
    pub local_cover_path: Option<String>,
    pub base_url: String,
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub chapter_name: Option<String>,
    #[ts(type = "string")]
    #[serde(with = "time::serde::rfc3339::option")]
    pub discovered_at: std::option::Option<time::OffsetDateTime>,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct ChapterContents {
    pub pages: Vec<Page>,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct Page {
    pub index: i64,
    pub url: String,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MigrationResult {
    pub chapters_matched: usize,
    pub chapters_orphaned: usize,
    pub chapters_new: usize,
    pub chapters_kept: usize,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MigrationPreview {
    pub target_title: String,
    pub target_cover_url: Option<String>,
    pub chapters_matched: usize,
    pub chapters_orphaned: usize,
    pub chapters_new: usize,
    pub downloaded_chapters_at_risk: usize,
}

#[cfg(feature = "host")]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct AuthenticatedUser {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub roles: Vec<String>,
}

#[cfg(feature = "host")]
impl AuthenticatedUser {
    pub fn has_role(&self, slug: &str) -> bool {
        self.roles.iter().any(|r| r == slug)
    }
    pub fn is_admin(&self) -> bool {
        self.has_role("admin")
    }
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct DownloadSettings {
    pub concurrent_page_downloads: i64,
    pub concurrent_manga_downloads: i64,
    pub chapter_queue_size: i64,
    pub max_retries: i64,
    pub initial_retry_delay_ms: i64,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct ScanSettings {
    pub auto_scan: bool,
    pub scan_interval_minutes: i64,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct AdvancedSettings {
    pub flaresolverr_url: String,
    pub library_path: String,
    pub max_wasm_instances: i64,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct TrackingSettings {
    pub default_tracking_enabled: bool,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub enum SettingsUpdate {
    Download(DownloadSettings),
    Scan(ScanSettings),
    Advanced(AdvancedSettings),
    Tracking(TrackingSettings),
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[serde(rename_all = "snake_case")]
#[repr(i64)]
pub enum MangaTrackingStatus {
    Reading = 0,
    OnHold = 1,
    Dropped = 2,
    PlanToRead = 3,
    Completed = 4,
    Rereading = 5,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MangaTracking {
    pub status: Option<MangaTrackingStatus>,
    pub score: Option<f64>,
    pub chapters_read: i64,
    pub total_chapters: i64,
    pub tracking_enabled: bool,
}

#[cfg(feature = "host")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct ContinueReadingChapter {
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub last_page: i64,
}
