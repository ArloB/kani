# YAML Schema

Kani provides a declarative YAML format for writing simple extensions without raw Rust. The CLI
validates and compiles YAML definitions into WASM Components.

!!! note "TODO"
    Full schema reference coming soon. The authoritative schema is at `kani-cli/src/yaml/schema.rs`.

## Quick example

```yaml
name: my-source
base_url: https://example.com
version: "0.1.0"

search:
  path: /search?q={query}
  container: .manga-list .item
  fields:
    title: h3
    url: a[href]
    cover: img[src]

manga_detail:
  container: .manga-detail
  fields:
    title: h1
    description: .synopsis
    cover: .cover img[src]

chapter_list:
  container: .chapter-list li
  fields:
    title: a
    url: a[href]
    number: .chapter-num

page_list:
  container: .page-list img
  fields:
    url: "[src]"
```

## Scaffold a new extension

```bash
cargo run -p kani-cli -- new my-source
```

## Validate

```bash
cargo run -p kani-cli -- validate my-source.yaml
```

## Generate and build

```bash
cargo run -p kani-cli -- generate my-source.yaml   # produces Rust source
cargo run -p kani-cli -- build my-source            # compiles to WASM
```

## Preferences

Declare user-configurable preferences that appear in the source settings UI:

```yaml
preferences:
  - key: nsfw
    type: bool
    default: false
    label: "Show NSFW content"
```

Access in DSL: `$pref:nsfw`.

## See also

- [DSL grammar](dsl-grammar.md) — the expression language used in field definitions.
- [Rhai hooks](rhai-hooks.md) — escape hatch for logic that the YAML DSL can't express.
