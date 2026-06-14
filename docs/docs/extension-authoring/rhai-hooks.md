# Rhai Hooks

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Overview

Rhai hooks are an escape hatch for extension logic that the YAML DSL cannot express. A Rhai script
can be attached to any YAML extension step to post-process results or handle unusual page
structures.

## When to use Rhai

Use Rhai hooks when:

- The target site uses JavaScript-rendered content that requires a V8 subprocess call.
- Field extraction involves conditional logic not expressible in the DSL.
- You need to call a secondary URL to resolve page images.

For most sites the YAML DSL is sufficient — prefer it over Rhai for simpler maintenance.

## Script location

Place hook scripts in the same directory as your YAML file:

```text
my-source.yaml
my-source/
  page_list.rhai
```

Reference in YAML:

```yaml
page_list:
  hook: my-source/page_list.rhai
```

## Script environment

<!-- TODO: document the globals and functions available inside a Rhai hook -->

## See also

- [YAML schema](yaml-schema.md)
- [DSL grammar](dsl-grammar.md)
