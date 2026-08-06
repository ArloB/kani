//! Kind-parameterized content transforms. Extensions declare a per-page
//! `transform` hint; the registry resolves it — while upstream headers are live —
//! to a [`ResolvedTransform`] that carries the parsed parameters plus the output
//! format, so a caller can decide buffer-vs-stream and extension/content-type
//! before reading the body. Resolution is lenient: an unknown or unparseable hint
//! returns `None` for passthrough.

use rquest::header::HeaderMap;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

pub mod image;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransformKind {
    Image,
    Text,
    Audio,
    Table,
    Json,
}

#[derive(Debug)]
pub enum TransformError {
    KindMismatch {
        expected: TransformKind,
        got: TransformKind,
    },
    Apply(String),
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransformError::KindMismatch { expected, got } => {
                write!(
                    f,
                    "transform kind mismatch: expected {expected:?}, got {got:?}"
                )
            }
            TransformError::Apply(msg) => write!(f, "transform failed: {msg}"),
        }
    }
}

impl std::error::Error for TransformError {}

#[derive(Debug, Clone, Copy)]
pub struct TransformOutput {
    pub file_extension: &'static str,
    pub content_type: &'static str,
}

type ApplyFn = Box<dyn Fn(&[u8]) -> Result<Vec<u8>, TransformError> + Send + Sync>;

/// A transform resolved for one page: the output format plus a closure that
/// applies it to the buffered body.
pub struct ResolvedTransform {
    output: TransformOutput,
    apply: ApplyFn,
}

impl ResolvedTransform {
    pub fn new(
        output: TransformOutput,
        apply: impl Fn(&[u8]) -> Result<Vec<u8>, TransformError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            output,
            apply: Box::new(apply),
        }
    }

    pub fn output(&self) -> TransformOutput {
        self.output
    }

    pub fn apply(&self, data: &[u8]) -> Result<Vec<u8>, TransformError> {
        (self.apply)(data)
    }
}

pub trait Transform: Send + Sync {
    fn names(&self) -> &'static [&'static str];
    fn kind(&self) -> TransformKind;
    fn description(&self) -> &'static str;
    fn resolve(&self, hint: &str, headers: &HeaderMap) -> Option<ResolvedTransform>;
}

pub struct TransformRegistry {
    transforms: HashMap<&'static str, Arc<dyn Transform>>,
}

impl Default for TransformRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TransformRegistry {
    pub fn new() -> Self {
        Self {
            transforms: HashMap::new(),
        }
    }

    pub fn register(&mut self, t: impl Transform + 'static) {
        let arc: Arc<dyn Transform> = Arc::new(t);
        for name in arc.names() {
            self.transforms.insert(name, Arc::clone(&arc));
        }
    }

    /// Resolve `hint` for the expected `kind`. The lookup key is the part before
    /// an inline-parameter `:` (`lcg-tile-5x5:12345` → `lcg-tile-5x5`). A name
    /// match with a different kind is a passthrough (warn + `None`), consistent
    /// with the lenient semantics.
    pub fn resolve(
        &self,
        hint: &str,
        kind: TransformKind,
        headers: &HeaderMap,
    ) -> Option<ResolvedTransform> {
        let key = hint.split(':').next().unwrap_or(hint);
        let t = self.transforms.get(key)?;
        if t.kind() != kind {
            tracing::warn!(
                "transform '{key}' is {:?}, not the expected {:?} — skipping",
                t.kind(),
                kind
            );
            return None;
        }
        t.resolve(hint, headers)
    }

    /// Each registered transform once (a transform indexed under several names is
    /// yielded a single time).
    pub fn iter(&self) -> impl Iterator<Item = &dyn Transform> {
        let mut seen = std::collections::HashSet::new();
        self.transforms.values().filter_map(move |arc| {
            let id = Arc::as_ptr(arc) as *const ();
            seen.insert(id).then(|| arc.as_ref())
        })
    }

    fn builtin() -> Self {
        let mut r = Self::new();
        r.register(image::LcgTileDescramble);
        r
    }
}

/// The process-wide, compiled-in transform registry.
pub fn registry() -> &'static TransformRegistry {
    static REGISTRY: LazyLock<TransformRegistry> = LazyLock::new(TransformRegistry::builtin);
    &REGISTRY
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    struct Identity;
    impl Transform for Identity {
        fn names(&self) -> &'static [&'static str] {
            &["identity"]
        }
        fn kind(&self) -> TransformKind {
            TransformKind::Image
        }
        fn description(&self) -> &'static str {
            "returns the bytes unchanged"
        }
        fn resolve(&self, _hint: &str, _headers: &HeaderMap) -> Option<ResolvedTransform> {
            Some(ResolvedTransform::new(
                TransformOutput {
                    file_extension: "bin",
                    content_type: "application/octet-stream",
                },
                |data| Ok(data.to_vec()),
            ))
        }
    }

    fn reg() -> TransformRegistry {
        let mut r = TransformRegistry::new();
        r.register(Identity);
        r
    }

    #[test]
    fn resolve_applies_matching_transform() {
        let r = reg();
        let resolved = r
            .resolve("identity", TransformKind::Image, &HeaderMap::new())
            .expect("identity resolves");
        assert_eq!(resolved.apply(b"hello").unwrap(), b"hello");
    }

    #[test]
    fn resolve_unknown_hint_returns_none() {
        assert!(
            reg()
                .resolve("nope", TransformKind::Image, &HeaderMap::new())
                .is_none()
        );
    }

    #[test]
    fn resolve_kind_mismatch_is_passthrough() {
        assert!(
            reg()
                .resolve("identity", TransformKind::Text, &HeaderMap::new())
                .is_none(),
            "a name match with the wrong kind is a passthrough, not a match"
        );
    }

    #[test]
    fn builtin_registers_the_image_descrambler() {
        assert!(
            registry()
                .resolve(
                    "lcg-tile-5x5:12345",
                    TransformKind::Image,
                    &HeaderMap::new()
                )
                .is_some()
        );
    }
}
