# DSL Grammar

!!! note "TODO"
    This page is a stub. Full grammar reference coming soon. See `kani-shared/src/ast.rs` for the authoritative `Expr` enum.

## Overview

The Kani DSL is an expression language used inside YAML field definitions and by Rust extensions
to build `Blueprint` structures. Expressions are serialised as `postcard` and evaluated natively
on the host.

## Core variants

| Variant | Description |
|---------|-------------|
| `Expr::Select(selector)` | CSS selector query, returns element list |
| `Expr::Text` | Text content of an element |
| `Expr::Attr(name)` | Attribute value |
| `Expr::Href` | Shorthand for `Attr("href")` |
| `Expr::Src` | Shorthand for `Attr("src")` |
| `Expr::Trim` | Strip leading/trailing whitespace |
| `Expr::Replace(from, to)` | String replacement |
| `Expr::Regex(pattern)` | Regex extract (first capture group) |
| `Expr::Pref(key)` | Inject user preference value |
| `Expr::JsonPath(path)` | JSONPath navigation |

## Preferences

Preference values are injected as `$pref:key`:

```rust
Expr::pref("nsfw")   // resolves to the "nsfw" preference value
```

## Builder API (Rust)

```rust
use kani_shared::ast::{Blueprint, BlueprintBuilder, Expr};

let blueprint = BlueprintBuilder::new(".item")
    .field("title", Expr::Select("h3").then(Expr::Text).then(Expr::Trim))
    .field("url", Expr::Select("a").then(Expr::Href))
    .build();
```

## See also

- [YAML schema](yaml-schema.md) — use the DSL without writing Rust.
