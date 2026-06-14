# Kani Declarative Extraction: Specification

This document specifies the three core formalisms of the declarative extraction system: the Extraction DSL, the JSON Intermediate Model (IM), and the YAML Extension Format.

---

## 1. Extraction DSL

The Extraction DSL is a small, functional, pipeline-oriented language embedded as strings within YAML field definitions. It describes how to extract a single value from a document element.

### 1.1 Lexical Structure

**Identifiers:** `[a-zA-Z_][a-zA-Z0-9_]*`

**Variable names:** `$` followed by an identifier: `$base`, `$cover_path`

**String literals:** Double-quoted with standard escapes: `"hello"`, `"/manga"`, `"Chapter\s+"`. Single quotes are not supported to avoid YAML quoting conflicts.

**Numeric literals:** Integer or float: `0`, `2`, `3.14`, `-1`

**Keywords:** `let`, `self`, `dom`, `json`, `index`, `null`, `true`, `false`, `if`, `then`, `else`, `pref`, `merge`, `format`

**Operators:** `.` (method chain), `=` (binding), `;` (statement separator), `,` (argument separator), `{` `}` (map literal), `(` `)` (grouping/call), `+` `-` `*` `/` (arithmetic), `==` `!=` `<` `>` `<=` `>=` (comparison), `&&` `||` (logical)

**Whitespace:** Ignored between tokens. Newlines are significant only in that they can substitute for `;` between `let` statements.

**Comments:** `/* */` block comments. Nesting is not supported.

### 1.2 Grammar

```ebnf
program        = let_expr | expr ;
let_expr       = "let" variable "=" expr ( ";" | NEWLINE ) ( let_expr | expr ) ;

(* Binary operators, in ascending precedence order *)
expr           = or_expr ;
or_expr        = and_expr { "||" and_expr } ;
and_expr       = cmp_expr { "&&" cmp_expr } ;
cmp_expr       = add_expr { ( "==" | "!=" | "<" | ">" | "<=" | ">=" ) add_expr } ;
add_expr       = mul_expr { ( "+" | "-" ) mul_expr } ;
mul_expr       = chain { ( "*" | "/" ) chain } ;
chain          = atom { "." method_call } ;

atom           = "self"
               | "dom" "(" string ")"
               | "json" "(" string ")"
               | "pref" "(" string ")"
               | "index" "(" ")"
               | "merge" "(" "[" [ expr { "," expr } ] "]" ")"
               | "format" "(" string [ "," expr { "," expr } ] ")"
               | variable
               | string
               | number
               | "null"
               | "true"
               | "false"
               | "[" [ expr { "," expr } ] "]"
               | "if" expr "then" expr "else" expr
               | "(" expr ")" ;
method_call    = IDENT "(" [ arg_list ] ")" ;
arg_list       = expr { "," expr } ;
variable       = "$" IDENT ;
string         = '"' { CHAR } '"' ;
number         = [ "-" ] DIGIT+ [ "." DIGIT+ ] ;
integer        = [ "-" ] DIGIT+ ;
(* map_literal is only valid as the sole argument to .lookup() — it is not a general atom *)
map_literal    = "{" [ map_entry { "," map_entry } ] "}" ;
map_entry      = string ":" string ;
```

### 1.3 Semantics

Every expression evaluates to a **Value**, which is one of:

| Type | Description |
|------|-------------|
| `String` | UTF-8 text |
| `Number` | 64-bit float |
| `Int` | 64-bit integer |
| `Bool` | Boolean |
| `Null` | Absent/missing value |
| `List` | Ordered sequence of values (used as iterator input/output) |
| `Element` | Reference to an HTML element (opaque, cannot be serialized) |
| `Json` | Reference to a JSON sub-tree (opaque until extracted) |

**Type coercion rules:**
- DOM and string operations on a `Null` input propagate `Null` (null-safe chaining). For example, `self.first(".title").text()` returns `Null` if `.first()` finds no match rather than erroring.
- DOM operations (`attr`, `text`, `inner_html`) return `String` or `Null`. `text()` returns an empty string (not Null) if the element exists but has no text content.
- `parse_float()` and `parse_int()` on a non-numeric string return `Null` (soft failure, not an error).
- `date_parse()` and `date_parse_rfc3339()` on a malformed string return `Null` (soft failure).
- `fallback(default)` replaces `Null` or an empty string with the default value. It does **not** catch runtime errors — only null/empty values.
- Final field values must be `String`, `Number`, `Int`, or `Null` (for optional fields). `Element` and `Json` cannot be output directly.

### 1.4 Root Expressions

These are the starting points for extraction chains:

| Expression | Description |
|------------|-------------|
| `self` | The current element in the container iteration. Only valid inside a field extraction (not in top-level bindings). |
| `dom("selector")` | Select the **first** matching element from the document root. Returns `Element` or `Null` if no match. Use `self.select("selector")` to get all matching elements as a `List`. |
| `json("pointer")` | Navigate to a JSON value using a JSON Pointer (RFC 6901). Returns a `Json` value. |
| `index()` | The 0-based iteration index of the current container element. Returns `Int`. |
| `pref("key")` | Read an extension preference value by key. Returns `String` or `Null` if the preference is unset. |
| `scalar("name")` | Read a named scalar value computed at the document level (from the `scalars` section of the blueprint). Returns whatever type the scalar expression produces. Useful for feeding document-level computation into per-element expressions without re-evaluating it per row. |
| `$variable` | Reference a previously bound variable. |

### 1.5 Method Reference

#### DOM Methods

| Method | Input Type | Return Type | Description |
|--------|-----------|-------------|-------------|
| `.attr("name")` | Element | String/Null | Get the value of an HTML attribute. Returns `Null` if the attribute doesn't exist. |
| `.text()` | Element | String | Get the combined text content of the element and all descendants. Returns empty string if no text. |
| `.inner_html()` | Element | String | Get the inner HTML of the element as a string. |
| `.select("selector")` | Element | List&lt;Element&gt; | Select all matching descendant elements. Returns an empty `List` if none match. |
| `.first("selector")` | Element | Element/Null | Select the first matching descendant element, or `Null` if none match. |
| `.inner_html()` | Element | String | Get the inner HTML of the element as a raw HTML string. |
| `.has_class("name")` | Element | Bool | Test if the element has a given CSS class. |
| `.children()` | Element | List&lt;Element&gt; | Return the direct child elements as a `List`. |

#### String Methods

| Method | Input Type | Return Type | Description |
|--------|-----------|-------------|-------------|
| `.split("delim")` | String | List&lt;String&gt; | Split the string on `delim`. Returns a `List` of all segments. Use `.at(n)` to extract a specific segment. |
| `.split_n("delim", n)` | String | List&lt;String&gt; | Split into at most `n` parts. The last part contains the remainder of the string (unsplit). Useful when a delimiter appears multiple times but only the first few splits matter. |
| `.replace("from", "to")` | String | String | Replace all occurrences of `from` with `to`. |
| `.trim()` | String | String | Remove leading and trailing whitespace. |
| `.lower()` | String | String | Convert to lowercase. |
| `.prepend(expr)` | String | String | Prepend the result of `expr` to this string. |
| `.append(expr)` | String | String | Append the result of `expr` to this string. |
| `.matches("regex")` | String | Bool | Test if the string matches the regex pattern. |
| `.capture("regex")` | String | List&lt;String/Null&gt; | Capture all groups from the first match. Index `0` is the full match; `1`+ are capture groups. Returns an empty `List` if there is no match. Use `.at(n)` to extract a specific group. |
| `.starts_with("prefix")` | String | Bool | Test if the string starts with the given prefix. |
| `.ends_with("suffix")` | String | Bool | Test if the string ends with the given suffix. |
| `.slice(start, end)` | String | String | Substring by character index (0-based, exclusive end). Negative values count from the end. `end` is optional; omitting it slices to the end of the string. |
| `.to_string()` | Int/Number/Bool | String | Convert a numeric or boolean value to its string representation. `Null` propagates as `Null`. |
| `.string_len()` | String | Int | Number of Unicode characters (not bytes) in the string. |
| `.url_encode()` | String | String | Percent-encode a string for use as a URL query parameter value (e.g. `"hello world"` → `"hello%20world"`). |
| `.url_decode()` | String | String | Decode a percent-encoded string. Invalid `%`-sequences are passed through unchanged. |
| `.format_padded(width, fill, align)` | String | String | Pad or align a string to at least `width` Unicode characters using `fill` (a single character) and `align` (`"left"`, `"right"`, or `"center"`). If the string is already at least `width` characters, it is returned unchanged. |
| `format("template {}", arg1, arg2, ...)` | — | String | Interpolate `{}` placeholders in the template string with the evaluated arguments in order. Each `{}` is replaced by the corresponding argument's string value. Returns `String`. |

#### List Methods

| Method | Input Type | Return Type | Description |
|--------|-----------|-------------|-------------|
| `.at(n)` | List | Any | Get the element at index `n`. Negative indices count from the end: `-1` is the last element. Returns an error if out of bounds (`.fallback()` does not catch errors — guard with `if`/`.matches()` when the index may be absent). Works on any `List`, including results from `.split()`, `.select()`, and `.children()`. |
| `.join("delim")` | List&lt;String&gt; | String | Join a list of strings into a single string using `delim` as the separator. `Null` elements are skipped. |
| `.take(n)` | List | List | Return the first `n` elements. Returns the whole list if `n` exceeds its length. |
| `.skip(n)` | List | List | Drop the first `n` elements and return the rest. Returns an empty list if `n` exceeds the length. |
| `.reverse()` | List | List | Reverse the list in place. |
| `.sort_by(key_expr)` | List | List | Sort the list by a key expression evaluated per element with `$item` in scope. Numeric types (`Int`, `Number`) sort numerically; `String` sorts lexicographically; `Bool` sorts false&lt;true; `Null`, `Element`, `Json`, and nested `List` values sort to the end (stable, equal among themselves). |
| `.unique()` | List | List | Remove duplicate elements. First occurrence is kept; subsequent duplicates are dropped. Order is preserved. |
| `merge([list1, list2, ...])` | — | List | Concatenate multiple lists into a single flat list. Each argument must evaluate to a `List`. Unlike `.flat_map()`, the lists are given directly rather than derived from a parent collection. Useful for merging results from disjoint selectors. |

#### Type Coercion Methods

| Method | Input Type | Return Type | Description |
|--------|-----------|-------------|-------------|
| `.parse_float()` | String | Number/Null | Parse as a 64-bit float. Returns Null on parse failure. |
| `.parse_int()` | String | Int/Null | Parse as a 64-bit integer. Returns Null on parse failure. |

#### Control Flow Methods

| Method | Input Type | Return Type | Description |
|--------|-----------|-------------|-------------|
| `.fallback(expr)` | Any | Any | If the target is `Null` or an empty string, evaluate and return `expr` instead. |
| `.lookup({"k1": "v1", ...})` | String | String/Null | Look up the target string in the provided map literal. Returns the mapped value, or `Null` if not found. Chain `.fallback("default")` to supply a default. |
| `.map(body)` | List | List | Iterate over the list. For each element, evaluate `body` with `$item` bound to the current element and `$index` bound to its 0-based position. `Null` results are dropped. Returns a new `List`. |
| `.flat_map(body)` | List | List | Like `.map(body)`, but each `body` evaluation must return a `List`; all result lists are concatenated into a single flat `List`. Useful when each element expands into multiple values. |
| `.fold(base, body)` | List | Any | Left fold over the list. Evaluates `base` as the initial accumulator, then for each element evaluates `body` with `$acc` bound to the running accumulator, `$item` to the current element, and `$index` to its 0-based position. The result of each `body` evaluation becomes the new `$acc`. Returns the final accumulator value. |
| `.filter(predicate)` | List | List | Keep only elements for which `predicate` evaluates to `true`. `predicate` is evaluated with `$item` and `$index` in scope. Elements where the predicate returns `false` or `Null` are dropped. Produces a `List` of the same element type. |
| `if cond then a else b` | — | Any | If `cond` is `true`, evaluates and returns `a`; if `false` or `Null`, evaluates and returns `b`. Short-circuits: only the selected branch is evaluated. `cond` must be `Bool` (or `Null`, which is treated as `false`). |
| `.not()` | Bool/Null | Bool | Boolean negation. `Null` is treated as `false`, so `.not()` on `Null` returns `true`. |

#### Binary Operators

Binary operators are written infix: `lhs op rhs`. Both operands are expressions and can be arbitrarily complex chains. Operator precedence follows standard rules: `*`/`/` bind tighter than `+`/`-`, which bind tighter than comparisons, which bind tighter than `&&`, which binds tighter than `||`. Use parentheses to override.

| Operator | Operand Types | Return Type | Description |
|----------|--------------|-------------|-------------|
| `+` | Number, Number | Number | Addition. Also valid for Int + Int → Int, or Int + Number → Number. |
| `-` | Number, Number | Number | Subtraction. |
| `*` | Number, Number | Number | Multiplication. |
| `/` | Number, Number | Number | Division. |
| `==` | Any, Any | Bool | Equality. Compares `String`, `Number`, `Int`, `Bool`, and `Null`. |
| `!=` | Any, Any | Bool | Inequality. |
| `<` | Number/Int, Number/Int | Bool | Less than. |
| `>` | Number/Int, Number/Int | Bool | Greater than. |
| `<=` | Number/Int, Number/Int | Bool | Less than or equal. |
| `>=` | Number/Int, Number/Int | Bool | Greater than or equal. |
| `&&` | Bool, Bool | Bool | Logical and. Short-circuits: right side not evaluated if left is `false`. |
| `\|\|` | Bool, Bool | Bool | Logical or. Short-circuits: right side not evaluated if left is `true`. |

Applying an operator to incompatible types (e.g., `String + Number`) is a runtime error.

#### Date Methods

| Method | Input Type | Return Type | Description |
|--------|-----------|-------------|-------------|
| `.date_parse("format")` | String | Int/Null | Parse a date string using the given format pattern (Rust `time` crate syntax). Returns a Unix timestamp (`Int`) or `Null` on parse failure. `Null` input propagates as `Null`. |
| `.date_parse_rfc3339()` | String | Int/Null | Parse an RFC 3339 / ISO 8601 date string. Returns a Unix timestamp (`Int`) or `Null` on parse failure. `Null` input propagates as `Null`. |

#### URL Methods

| Method | Input Type | Return Type | Description |
|--------|-----------|-------------|-------------|
| `.resolve_url(base_expr)` | String | String | Resolve a relative URL against a base URL. `base_expr` can be a string literal or any expression evaluating to a `String`. |

#### JSON Methods

When operating on a `Json` value (from `json()` root or within a JSON-mode blueprint):

| Method | Input Type | Return Type | Description |
|--------|-----------|-------------|-------------|
| `.ptr("pointer")` | Json | Json/Null | Navigate deeper using a JSON Pointer (RFC 6901). Returns `Null` if the path does not exist. |
| `.str()` | Json | String/Null | Extract the JSON value as a string. Returns `Null` if not a string. |
| `.int()` | Json | Int/Null | Extract the JSON value as a 64-bit integer. Returns `Null` if not a number. |
| `.float()` | Json | Number/Null | Extract the JSON value as a 64-bit float. Returns `Null` if not a number. |
| `.bool()` | Json | Bool/Null | Extract the JSON value as a boolean. Returns `Null` if not a boolean. |
| `.array_len()` | Json | Int | Get the length of a JSON array. Returns `0` if the value is not an array. |
| `.keys()` | Json | List&lt;String&gt; | Get the keys of a JSON object as a `List` of strings. Returns an empty `List` if not an object. |
| `.get(key_expr)` | Json | Json/Null | Access an object field by a dynamically-evaluated key expression. Unlike `.ptr()`, the key can be a variable or any expression. Returns `Null` if the field is absent or the target is not an object. |
| `.find(key_expr, value_expr)` | Json | Json/Null | Search a JSON array for the first element (object) where `element[key] == value`. Both `key` and `value` are expressions evaluated to strings. Returns `Null` if no match is found or the target is not an array. |
| `.json_fold()` | Json | Json | Reduce all elements of a JSON array into a single merged value. Objects are merged by key (later keys win); arrays are concatenated. E.g. `[{"en":"A"},{"ja":"B"}]` → `{"en":"A","ja":"B"}`. Returns the target unchanged if it is not an array. |
| `.coalesce_keys([key1, key2, ...])` | Json | String/Null | Try each key expression in order via `.get(key).str()`, returning the first non-null string value. Equivalent to chained `.get(k1).str().fallback(.get(k2).str()).fallback(...)`. Keys can be literals, `pref()` calls, variables, or any expression returning a string. **Rust builder convenience only** — in the text DSL, write the equivalent `.get().str().fallback()` chain directly. |

### 1.6 Examples

**Simple attribute extraction:**
```
self.attr("href").split("/").at(2)
```

**Multi-step with variable binding:**
```
let $base = dom("meta[property='og:url']").attr("content").split("/manga").at(0);
self.select("img.cover").attr("src").prepend($base)
```

**Status mapping:**
```
dom("div.status").text().trim().lower().lookup({
  "publishing": "ongoing",
  "finished": "completed",
  "on hiatus": "hiatus",
  "discontinued": "cancelled"
}).fallback("unknown")
```

**Chapter number parsing with fallback:**
```
self.text().trim().split(" ").at(-1).parse_float().fallback(0.0)
```

**Date parsing:**
```
self.attr("datetime").date_parse_rfc3339()
```

**Regex capture (group 1):**
```
self.text().capture("Chapter\s+(\d+(?:\.\d+)?)").at(1).parse_float().fallback(0.0)
```

**Collecting text from child elements (iteration):**
```
self.children().map($item.text().trim())
```

**Flattening nested lists (flat iteration):**
```
self.children().flat_map($item.children().map($item.attr("href")))
```

**Summing parsed numbers (fold):**
```
self.select("td.price").map($item.text().trim().parse_float()).fold(0.0, $acc + $item)
```

**Checking all items satisfy a condition (fold):**
```
self.select("input.required").map($item.attr("value").matches(".+")).fold(true, $acc && $item)
```

**Conditional based on page state:**
```
if dom("span.status").text().trim().lower().matches("adult") then "nsfw" else "safe"
```

**Collecting tags as a comma-joined string:**
```
self.select("a.tag").map($item.text().trim()).filter($item.matches("[^\s]")).join(", ")
```

**List literal for static values:**
```
["Ongoing", "Completed", "Hiatus"].at(0)
```

**Number to string for URL construction:**
```
let $n = dom("span.count").text().trim().parse_int().fallback(0);
"/api/items?count=".append($n.to_string())
```

**Dynamic JSON field access (language preference):**
```
json("/data/attributes/title").get(pref(language)).str().fallback(json("/data/attributes/title/en").str())
```

**Find first matching element in a JSON array:**
```
json("/data/relationships").find("type", "cover_art").ptr("/attributes/fileName").str()
```

**Merging tags from multiple selectors into one list:**
```
merge([
  self.select("ul li:first-child a").map($item.text().trim()),
  self.select("ul li:nth-child(2) a").map($item.text().trim()),
  self.select("ul li:nth-child(3) span").map($item.text().trim())
])
```

**URL construction with format:**
```
format("https://cdn.example.com/covers/{}/{}.jpg", json("/manga_id").str(), json("/filename").str())
```

**Reading a preference value:**
```
json("/data/attributes/title").get(pref("language")).str().fallback(json("/data/attributes/title/en").str())
```

**Coalescing over a localised JSON object (try multiple keys in order):**
```
json("/data/attributes/title").get(pref("language")).str()
  .fallback(json("/data/attributes/title").get("en").str())
  .fallback(json("/data/attributes/title").get("ja-ro").str())
  .fallback(json("/data/attributes/title").get("ja").str())
```

In the Rust builder this pattern is available as `.coalesce_keys([Expr::pref("language"), Expr::lit("en"), Expr::lit("ja-ro"), Expr::lit("ja")])` on any `Json`-typed expression.

**Merging title objects from multiple sources (Rust builder only):**
```rust
Expr::json_array(vec![
    Expr::self_ref().ptr("/attributes/altTitles").json_fold(), // [{en:"A"}, {ja:"B"}] → {en:"A",ja:"B"}
    Expr::self_ref().ptr("/attributes/title"),                  // {en:"Primary Title"}
])
.filter(Expr::var("$item").ne(Expr::null()))
.json_fold()                                                    // merge all objects into one
.coalesce_keys(["en", "ja-ro", "ja"].map(Expr::lit))           // pick best available key
.fallback_str("Unknown Title")
```
`json_array([...])` constructs a `Json` array of `Json` values (unlike `[...]` which produces a `List`). This allows `.json_fold()` to merge the objects and `.coalesce_keys()` to pick the first non-null string across preferred keys.

**Boolean negation:**
```
if dom("span.is-completed").text().trim().matches("Completed").not() then "ongoing" else "completed"
```

**String length check:**
```
if dom("p.description").text().string_len() > 0 then dom("p.description").text() else null
```

**Split into at most 2 parts (everything after the first `/`):**
```
self.attr("href").split_n("/", 2).at(1)
```

**First 5 chapters of a list:**
```
self.select("a.chapter").map($item.text().trim()).take(5)
```

**Drop the pinned first entry and sort the rest by chapter number:**
```
self.select("a.chapter").map($item.text().trim().parse_float()).skip(1).sort_by($item)
```

**Deduplicate tags extracted from multiple elements:**
```
merge([
  self.select("a.genre").map($item.text().trim()),
  self.select("a.category").map($item.text().trim())
]).unique()
```

**URL-encode a search query for manual URL construction:**
```
let $q = dom("input#search").attr("value").url_encode();
format("https://example.com/search?q={}", $q)
```

**Right-pad a chapter number display string to 6 characters:**
```
self.text().trim().format_padded(6, " ", "right")
```

**Read a document-level scalar inside a per-element expression:**
```
scalar("base_url").append(self.attr("href"))
```

---

## 2. JSON Intermediate Model (IM)

The JSON IM is the serialized representation of the `Expr` AST. It is what gets embedded in compiled WASM extensions and transmitted across the FFI boundary as part of a blueprint. It uses a tagged-object format for clarity and debuggability.

### 2.1 Encoding Rules

Each `Expr` node is encoded as a JSON object with a `"op"` field identifying the node type, plus fields specific to that type. Nested expressions are encoded recursively.

### 2.2 Node Encodings

#### Leaf Nodes

```json
{ "op": "self" }
```

```json
{ "op": "dom", "selector": ".title" }
```

```json
{ "op": "json", "pointer": "/data/title" }
```

```json
{ "op": "var", "name": "$base" }
```

```json
{ "op": "lit", "value": "https://example.com" }
```

```json
{ "op": "num", "value": 3.14 }
```

```json
{ "op": "null" }
```

```json
{ "op": "bool", "value": true }
```

```json
{ "op": "index" }
```

#### DOM Operations

```json
{ "op": "attr", "target": { "op": "self" }, "name": "href" }
```

```json
{ "op": "text", "target": { "op": "self" } }
```

```json
{ "op": "inner_html", "target": { "op": "self" } }
```

```json
{ "op": "select", "target": { "op": "self" }, "selector": "img.cover" }
```

```json
{ "op": "first", "target": { "op": "self" }, "selector": "img.cover" }
```

```json
{ "op": "has_class", "target": { "op": "self" }, "class": "active" }
```

```json
{ "op": "children", "target": { "op": "self" } }
```

#### List Operations

```json
{ "op": "at", "target": { ... }, "index": 2 }
```

`"index"` may be negative: `-1` is the last element. Returns an error if out of bounds (not null).

#### String Operations

```json
{ "op": "split", "target": { ... }, "delimiter": "/" }
```

Always returns a `List<String>`. Use `.at(n)` to extract a specific segment.

```json
{ "op": "replace", "target": { ... }, "from": "Chapter ", "to": "" }
```

```json
{ "op": "trim", "target": { ... } }
```

```json
{ "op": "lower", "target": { ... } }
```

```json
{ "op": "prepend", "target": { ... }, "prefix": { "op": "var", "name": "$base" } }
```

```json
{ "op": "append", "target": { ... }, "suffix": { "op": "lit", "value": ".jpg" } }
```

```json
{ "op": "matches", "target": { ... }, "pattern": "^Chapter" }
```

```json
{ "op": "capture", "target": { ... }, "pattern": "Chapter\\s+(\\d+)" }
```

```json
{ "op": "slice", "target": { ... }, "start": 0, "end": 5 }
```

`"end"` is optional; omit to slice to the end of the string. Negative values count from the end.

```json
{ "op": "starts_with", "target": { ... }, "prefix": "Chapter" }
```

```json
{ "op": "ends_with", "target": { ... }, "suffix": ".jpg" }
```

#### Coercion

```json
{ "op": "parse_float", "target": { ... } }
```

```json
{ "op": "parse_int", "target": { ... } }
```

```json
{ "op": "to_string", "target": { ... } }
```

#### Control Flow

```json
{
  "op": "let",
  "name": "$base",
  "value": { "op": "attr", "target": { "op": "dom", "selector": "meta" }, "name": "content" },
  "body": { "op": "prepend", "target": { "op": "attr", "target": { "op": "self" }, "name": "src" }, "prefix": { "op": "var", "name": "$base" } }
}
```

```json
{ "op": "fallback", "target": { ... }, "default": { "op": "lit", "value": "unknown" } }
```

```json
{
  "op": "lookup",
  "target": { ... },
  "entries": [
    ["publishing", "ongoing"],
    ["finished", "completed"],
    ["on hiatus", "hiatus"]
  ]
}
```

```json
{ "op": "list", "items": [{ ... }, { ... }, { ... }] }
```

```json
{ "op": "concat", "parts": [{ ... }, { ... }, { ... }] }
```

```json
{ "op": "join", "target": { ... }, "delimiter": ", " }
```

```json
{ "op": "if", "condition": { ... }, "then": { ... }, "else": { ... } }
```

```json
{
  "op": "map",
  "target": { "op": "children", "target": { "op": "self" } },
  "body": { "op": "text", "target": { "op": "var", "name": "$item" } }
}
```

`"target"` must evaluate to a `List`. The `"body"` expression is evaluated once per element with `$item` bound to the current element and `$index` bound to its 0-based position. `Null` results are dropped. Returns a `List`.

```json
{
  "op": "flat_map",
  "target": { "op": "children", "target": { "op": "self" } },
  "body": { "op": "children", "target": { "op": "var", "name": "$item" } }
}
```

Like `"map"`, but each body evaluation must return a `List`; all result lists are concatenated into a single flat `List`.

```json
{
  "op": "filter",
  "target": { "op": "dom", "selector": "a.chapter-link" },
  "predicate": { "op": "binop", "kind": "==", "lhs": { "op": "has_class", "target": { "op": "var", "name": "$item" }, "class": "active" }, "rhs": { "op": "lit", "value": "true" } }
}
```

`"predicate"` is evaluated for each element with `$item` and `$index` in scope. Elements where the predicate returns `false` or `Null` are dropped. Must return `Bool`.

```json
{
  "op": "fold",
  "target": { "op": "children", "target": { "op": "self" } },
  "base": { "op": "num", "value": 0 },
  "body": { "op": "binop", "kind": "+", "lhs": { "op": "var", "name": "$acc" }, "rhs": { "op": "var", "name": "$item" } }
}
```

Left fold. `"base"` is evaluated once to produce the initial accumulator. For each element, `"body"` is evaluated with `$acc` bound to the running accumulator, `$item` to the current element, and `$index` to its 0-based position. The result becomes the new `$acc`.

#### Binary Operators

```json
{ "op": "binop", "kind": "+",  "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "-",  "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "*",  "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "/",  "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "==", "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "!=", "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "<",  "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": ">",  "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "<=", "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": ">=", "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "&&", "lhs": { ... }, "rhs": { ... } }
{ "op": "binop", "kind": "||", "lhs": { ... }, "rhs": { ... } }
```

`&&` and `||` short-circuit: `rhs` is not evaluated if `lhs` determines the result.

#### Date Operations

```json
{ "op": "date_parse", "target": { ... }, "format": "[year]-[month]-[day]" }
```

```json
{ "op": "date_parse_rfc3339", "target": { ... } }
```

#### JSON Operations

```json
{ "op": "json_ptr", "target": { ... }, "pointer": "/attributes/title" }
```

```json
{ "op": "json_str", "target": { ... } }
```

```json
{ "op": "json_int", "target": { ... } }
```

```json
{ "op": "json_float", "target": { ... } }
```

```json
{ "op": "json_bool", "target": { ... } }
```

```json
{ "op": "array_len", "target": { ... } }
```

```json
{ "op": "keys", "target": { ... } }
```

Returns the keys of a JSON object as a `List<String>`. Returns an empty `List` if the target is not an object.

```json
{ "op": "json_get", "target": { ... }, "key": { "op": "var", "name": "$lang" } }
```

```json
{ "op": "json_find", "target": { ... }, "key": { "op": "lit", "value": "type" }, "value": { "op": "lit", "value": "cover_art" } }
```

```json
{ "op": "json_fold", "target": { ... } }
```

```json
{ "op": "json_array", "items": [{ ... }, { ... }] }
```

Constructs a `Json` array from N evaluated expressions. Unlike `{ "op": "list" }` (which produces a `List` value), `json_array` produces a `Json` value that supports `.json_fold()`, `.filter()`, and other JSON-native operations. **Rust builder only** — not directly parseable from the text DSL.

#### Boolean Operations

```json
{ "op": "not", "target": { ... } }
```

`"target"` must evaluate to `Bool` or `Null`. `Null` is treated as `false`.

#### String Utilities

```json
{ "op": "string_len", "target": { ... } }
```

Returns the Unicode character count of the string as an `Int`.

```json
{ "op": "format", "template": "Hello {}, you have {} items", "args": [{ ... }, { ... }] }
```

Interpolates `{}` placeholders with the evaluated string arguments in order.

#### Preference Access

```json
{ "op": "pref", "key": "cover_size" }
```

Reads the extension preference named `key`. Returns `String` or `Null` if unset.

#### List Merging

```json
{
  "op": "merge",
  "lists": [
    { "op": "select", "target": { "op": "self" }, "selector": "ul:first-child a" },
    { "op": "select", "target": { "op": "self" }, "selector": "ul:nth-child(2) a" }
  ]
}
```

Concatenates multiple lists. Each element of `"lists"` must evaluate to a `List`.

#### URL Operations

```json
{ "op": "resolve_url", "target": { ... }, "base": { "op": "lit", "value": "https://example.com" } }
```

#### DSL v2 List Operations

```json
{ "op": "split_n", "target": { ... }, "delimiter": "/", "n": 2 }
```

```json
{ "op": "take", "target": { ... }, "n": 5 }
```

```json
{ "op": "skip", "target": { ... }, "n": 1 }
```

```json
{ "op": "reverse", "target": { ... } }
```

```json
{ "op": "sort_by", "target": { ... }, "key": { "op": "var", "name": "$item" } }
```

`"key"` is evaluated per element with `$item` bound to the current element. Numeric types sort numerically; `String` lexicographically; `Bool` false&lt;true; non-comparable types (`Null`, `Element`, `Json`, `List`) sort to the end, stable.

```json
{ "op": "unique", "target": { ... } }
```

#### DSL v2 String Operations

```json
{ "op": "url_encode", "target": { ... } }
```

```json
{ "op": "url_decode", "target": { ... } }
```

```json
{ "op": "format_padded", "target": { ... }, "width": 6, "fill": " ", "align": "right" }
```

`"align"` is one of `"left"`, `"right"`, `"center"`.

#### DSL v2 Scalar Access

```json
{ "op": "scalar", "name": "base_url" }
```

Reads the named value from the document-level `scalars` map, computed before per-element iteration begins. The scalar must be declared in the `scalars` section of the blueprint.

#### Sub-blueprint Fetch (Rust builder only)

```json
{
  "op": "fetch",
  "url_expr": { ... },
  "blueprint": { ... },
  "method": "Get",
  "headers": [],
  "kind": "Json"
}
```

`"method"` is one of `"Get"`, `"Post"`, `"Put"`, `"Delete"`. `"kind"` is `"Html"` or `"Json"` and determines how the fetched response is parsed before the sub-blueprint is applied. The sub-blueprint is a full blueprint object (§2.3). The host evaluates `url_expr`, fetches the URL (subject to the SSRF `AllowedHost` gate and the per-extension I/O budget), and returns the first row of the sub-extraction as a `Json` value, or `Null` if the result is empty. Nesting `fetch` inside another fetch's sub-blueprint is rejected at evaluation time. Available in the Rust builder via `Expr::fetch_html(url_expr, blueprint)` and `Expr::fetch_json(url_expr, blueprint)`.

### 2.3 Blueprint Encoding

A complete blueprint is a JSON object with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `request` | object/null | HTTP request definition — `url`, `method`, `headers`, `queries`. Omit when an existing document handle is passed instead. |
| `container` | string | CSS selector (HTML) or JSON Pointer (JSON) for the repeating container. Use `":root"` (HTML) for a single-element HTML container. For JSON: use `""` to target the document root, or a JSON Pointer like `"/data"` to target a nested value. If the resolved JSON value is an array it is iterated element-by-element; any other JSON type (object, string, etc.) is treated as a single-item container — useful for detail endpoints where the document itself is the row. |
| `fields` | array | Field definitions extracted per container element. Each has `name`, `expr`, `optional`. |
| `bindings` | array | Document-level variable bindings evaluated once before iteration. Each has `name` and `expr`. |
| `scalars` | array | Document-level output values evaluated once (not per-element). Same shape as `fields` — each entry has `name`, `expr`, and `optional`. When `optional: true`, a `Null` result is included in the output as JSON `null` rather than causing an error. Returned in the `scalars` map of the output alongside `rows`. |
| `pagination` | object/null | When set, enables `paginated-extract-html` mode. See Pagination Config below. |

**Output format**: `{ "rows": [{...}, ...], "scalars": {"key": value, ...} }`

**Example:**

```json
{
  "request": {
    "url": "https://example.com/search/data",
    "method": "GET",
    "headers": [],
    "queries": [["display_mode", "Full Display"], ["sort", "Popularity"]]
  },
  "container": "body > article",
  "fields": [
    {
      "name": "id",
      "expr": { "op": "at", "target": { "op": "split", "target": { "op": "attr", "target": { "op": "first", "target": { "op": "self" }, "selector": "a.line-clamp-1" }, "name": "href" }, "delimiter": "/" }, "index": -2 },
      "optional": false
    },
    {
      "name": "title",
      "expr": { "op": "text", "target": { "op": "first", "target": { "op": "self" }, "selector": "a.line-clamp-1" } },
      "optional": false
    },
    {
      "name": "cover_url",
      "expr": { "op": "attr", "target": { "op": "first", "target": { "op": "self" }, "selector": "img" }, "name": "src" },
      "optional": true
    }
  ],
  "bindings": [],
  "scalars": [
    {
      "name": "has_next_page",
      "expr": { "op": "matches", "target": { "op": "text", "target": { "op": "dom", "selector": ".col-span-2" } }, "pattern": ".+" },
      "optional": false
    }
  ],
  "pagination": {
    "native_page_size": 32,
    "offset_param": "offset",
    "offset_type": "ItemOffset"
  }
}
```

**Pagination Config** (`pagination` field):

| Field | Type | Description |
|-------|------|-------------|
| `native_page_size` | integer | How many items the source returns per chunk (its real page size). |
| `offset_param` | string | Query parameter name the source uses for the offset/page (e.g. `"offset"`, `"page"`). |
| `offset_type` | string/object | `"ItemOffset"` (param = absolute item count: 0, 32, 64, …), `{"PageNumber": {"start": 1}}` (param = page number starting at `start`), or `{"CursorToken": {"next_cursor_field": "/next"}}` (JSON Pointer to the cursor field in each chunk's response). |

When `pagination` is set, the blueprint must be submitted via `paginated-extract-html` / `paginated-extract-json` rather than `extract-html` / `extract-json`. The host handles chunk-fetching, stitching, and `has_next_page` detection automatically.

**`CursorToken` mode:** The host reads the cursor value from `next_cursor_field` (a JSON Pointer into the chunk response) after each fetch, injects it as the `offset_param` query value on the next request, and stops when the field is absent or `null`. Use this for APIs that return a next-page token rather than a numeric offset (e.g. MangaDex's `offset`+`total` model can also be expressed this way, but opaque-token APIs require it).

### 2.4 Binary Encoding

Blueprints are serialized with **[`postcard`](https://docs.rs/postcard)** (a compact binary format) for the FFI call across the WASM boundary. The `Expr` enum's `serde` derives handle this transparently. Call `blueprint.to_bytes()` (from `BlueprintBuilder::build()`) to get the postcard bytes; the host deserializes via `postcard::from_bytes(&blueprint)`.

**DSL schema versioning.** The binary payload is prefixed with a `u32` schema version. The current version is **2** (introduced when the v2 `Expr` variants — `SplitN`, `Take`, `Skip`, `Reverse`, `SortBy`, `Unique`, `UrlEncode`, `UrlDecode`, `FormatPadded`, `ScalarOverride`, `Fetch` — were added). If a host receives a blueprint with a version it does not support, it returns a human-readable error (`"DSL schema version 2 requires Kani ≥ x.y.z"`) rather than an opaque decode failure. Version 1 blueprints (no prefix) remain decodable by v2 hosts for backward compatibility.

The JSON IM described in §2.2 reflects the logical structure of the AST and is useful for debugging; the wire format is binary, not JSON.

---

## 3. YAML Extension Format

The YAML format is the developer-facing representation of a kani extension. It is compiled to Rust source code by `kani-cli`.

### 3.1 Top-Level Structure

```yaml
# === Required metadata ===
id: string              # Unique extension identifier (lowercase, alphanumeric + hyphens)
name: string            # Human-readable display name
version: string         # Semantic version (e.g., "0.1.0")
base_url: string        # Base URL for the source website
language: string        # ISO 639-1 code or "multi" (default: "en")

# === Optional metadata ===
nsfw: bool              # Whether the source contains NSFW content (default: false)
unrestricted_http: bool # Whether the extension needs to contact external hosts (default: false)

# === Endpoint definitions ===
endpoints:
  popular: PopularEndpoint
  search: SearchEndpoint
  manga_details: DetailsEndpoint
  chapter_list: ChapterListEndpoint
  pages: PagesEndpoint

# === Optional sections ===
filters: FilterList
preferences: PreferenceList
```

### 3.2 Endpoint Types

Each endpoint corresponds to a method in the `manga-provider` WIT interface. The endpoint defines how to construct the HTTP request and how to extract data from the response.

#### Common Endpoint Fields

```yaml
endpoint_name:
  # --- Request construction ---
  route: string           # URL path appended to base_url. Supports {variable} templates.
  method: string          # HTTP method (default: "GET")
  headers:                # Additional headers (optional)
    Header-Name: value
  queries:                # Query parameters (optional)
    param: value          # Static value
    param: $variable$     # Dynamic value from function arguments
  type: string            # Response type: "html" (default) or "json"

  # --- Extraction ---
  container: string       # CSS selector (HTML) or JSON Pointer (JSON) for the list container
  bindings:               # Top-level variable bindings evaluated before iteration (optional)
    $var_name: "dsl expression"
  fields:                 # Field extractions from each container element
    field_name: "dsl expression"
    field_name:
      expr: "dsl expression"
      optional: true
  scalars:                # Document-level output values evaluated once before iteration (optional)
    scalar_name: "dsl expression"
    scalar_name:
      expr: "dsl expression"
      optional: true
```

#### Variable Interpolation

Inside `route`, `queries`, and `headers`, values wrapped in `$...$` are replaced with function arguments:

| Variable | Available In | Description |
|----------|-------------|-------------|
| `$query$` | search | The search query string |
| `$page$` | popular, search, chapter_list | The page number |
| `$page_size$` | popular, search, chapter_list | The requested page size |
| `$manga_id$` | manga_details, chapter_list, pages | The manga identifier |
| `$chapter_id$` | pages | The chapter identifier |
| `$pref:key$` | any | Value of a user preference |

#### PopularEndpoint

When the source has no distinct popular endpoint, use `delegate_to` to reuse another endpoint:

```yaml
popular:
  delegate_to: search
  empty_without_filters: true   # Optional: return empty list when no filters active
```

Otherwise define it as a full endpoint. JSON API example:

```yaml
popular:
  route: "/manga"
  queries:
    limit: $page_size$
    offset: "$page_size$ * ($page$ - 1)"
    includes[]: cover_art
    order[followedCount]: desc
  type: json
  container: "/data"
  scalars:
    has_next_page: 'json("/offset").int().fallback(0.0) + json("/limit").int().fallback(0.0) < json("/total").int().fallback(0.0)'
  fields:
    id: 'self.ptr("/id").str()'
    title: |
      self.ptr("/attributes/title").get(pref("language")).str()
        .fallback(self.ptr("/attributes/title/en").str())
        .fallback("Unknown Title")
    cover_url:
      expr: |
        let $filename = self.ptr("/relationships").find("type", "cover_art").ptr("/attributes/fileName").str()
        if $filename != null
          then format("https://cdn.example.com/covers/{}/{}{}", self.ptr("/id").str(), $filename, pref("cover_size").fallback(".512.jpg"))
          else null
      optional: true
```

#### SearchEndpoint

```yaml
search:
  route: "/search"
  queries:
    q: $query$
    page: $page$
  container: ".grid.gap-3 > div"
  fields:
    id: 'self.select("a").attr("href").split("/").at(2)'
    title: 'self.first(".line-clamp-2").text()'
    cover_url:
      expr: 'self.select("img").attr("data-src")'
      optional: true
```

#### DetailsEndpoint

The details endpoint does not iterate over a container. Instead, it extracts fields directly from the page. Set `container` to `":root"` (HTML) or `""` (JSON root) to select the document itself.

```yaml
manga_details:
  route: "/manga/$manga_id$"
  container: ":root"
  fields:
    id: '"$manga_id$"'   # Literal passthrough of the function argument
    title: 'dom("h1.font-bold").text()'
    description:
      expr: 'dom("p.text-sm").text()'
      optional: true
    status: |
      dom("div.grid:nth-child(3) > div:nth-child(2) > div:nth-child(2)").text().trim().lower().lookup({
        "publishing": "ongoing",
        "finished": "completed",
        "on hiatus": "hiatus",
        "discontinued": "cancelled",
        "not yet published": "hiatus"
      }).fallback("unknown")
    tags:
      expr: 'self.select("div.mb-3 a.text-sm").map($item.text().trim()).filter($item.matches("[^\s]")).join(", ")'
      optional: true
    cover_url:
      expr: 'dom("img").attr("data-src")'
      optional: true
```

#### ChapterListEndpoint

```yaml
chapter_list:
  route: "/manga/$manga_id$"
  container: "div.grid a.border"
  fields:
    id: 'self.attr("href").split("/").at(2)'
    number: 'self.text().trim().split(" ").at(-1).parse_float().fallback(0.0)'
    title:
      expr: "null"
      optional: true
    volume:
      expr: "null"
      optional: true
    scanlator:
      expr: "null"
      optional: true
    date_uploaded:
      expr: "null"
      optional: true
    language: '"en"'
  has_next_page: false    # Static value, or a DSL expression evaluated on the document
```

#### PagesEndpoint

```yaml
pages:
  route: "/chapters/$chapter_id$"
  container: "img.js-page"
  fields:
    index: "index()"
    url: 'self.attr("data-src")'
```

### 3.3 Pagination

Kani uses a `(page, page_size)` pagination model. Source websites may paginate differently — for example, a site might always return exactly 32 items per request regardless of the `limit` parameter. The `pagination` section (optional, per-endpoint) delegates the offset algebra to the framework:

```yaml
pagination:
  native_page_size: 32    # How many items the source returns per chunk
  offset_param: "offset"  # Query parameter name for the chunk offset
  offset_type: item        # "item" (0, 32, 64, …) or "page" with a start index
```

When `pagination` is set on an endpoint, the framework calls `paginated-extract-html` instead of `extract-html`. It automatically fetches as many chunks as needed to fulfil the client's `page_size`, injects the correct `offset_param` value per chunk, and determines `has_next_page`. Extensions do not need to implement pagination loops.

**`offset_type` values:**

| Value | Description |
|-------|-------------|
| `item` | Offset param = absolute item count: 0, 32, 64, … |
| `page` | Offset param = page number. Defaults to 1-based. Use `page_start: 0` for 0-based. |
| `cursor` | Cursor-token pagination. Set `cursor_field` to the JSON Pointer of the next-page token in the response (e.g. `cursor_field: "/next_cursor"`). The host injects the token as `offset_param` on each subsequent request and stops when the field is absent or null. |

**`has_next_page` detection:** If the blueprint includes a `scalars` entry named `has_next_page`, its value from the last fetched chunk is used. Otherwise the framework falls back to: last chunk was full (≥ `native_page_size` items) → more pages available.

For endpoints where the source supports arbitrary page sizes (the client's `page_size` is passed directly), omit `pagination` and use `$page$` / `$page_size$` in `queries` as before.

### 3.4 Filters Section

Filters map directly to the `filter_list!` macro output:

```yaml
filters:
  - id: "genre:Action"
    name: "Action"
    type: checkbox

  - id: "genre:Adventure"
    name: "Adventure"
    type: checkbox

  - id: type
    name: Type
    type: select
    options:
      - name: All
        value: ""
      - name: Manga
        value: manga
      - name: Manhua
        value: manhua
    default:
      name: All
      value: ""

  - id: status
    name: Status
    type: select
    options:
      - name: All
        value: ""
      - name: Ongoing
        value: publishing
    default:
      name: All
      value: ""
```

**Filter-to-query mapping:** When an endpoint receives filters, the codegen needs to know how to convert active filter values into query parameters. This is defined in the endpoint:

```yaml
search:
  route: "/search"
  queries:
    q: $query$
  filter_mapping:
    genre: genre          # Filter group "genre" maps to query param "genre"  
    type: type            # Filter "type" maps to query param "type"
    status: status        # Filter "status" maps to query param "status"
  container: "..."
  fields: { ... }
```

### 3.5 Preferences Section

```yaml
preferences:
  - key: cover_size
    label: "Cover Size"
    kind: select
    options:
      - name: Small
        value: ".256.jpg"
      - name: Medium
        value: ".512.jpg"
      - name: Full
        value: ""
    default: ".512.jpg"
    description: "Set the size of manga covers when browsing."

  - key: api_key
    label: "API Key"
    kind: text
    default: ""
    secret: true
    description: "Enter your API key for authenticated access."

  - key: show_nsfw
    label: "Show NSFW Content"
    kind: toggle
    default: "false"
```

### 3.6 Complete Schema Reference

```yaml
# Top-level fields
id: string                      # Required. Extension identifier.
name: string                    # Required. Display name.
version: string                 # Required. Semver.
base_url: string                # Required. Base URL.
language: string                # Optional. Default: "en".
nsfw: bool                      # Optional. Default: false.
unrestricted_http: bool         # Optional. Default: false.

# Endpoints (all optional but at least one should be defined)
endpoints:
  popular:                      # -> get_popular_manga
    delegate_to: string         # Optional: delegate to another endpoint (e.g. "search")
    empty_without_filters: bool # Optional: return empty list when no filters are active
    route: string
    method: string              # Default: "GET"
    headers: map<string, string>
    queries: map<string, string>
    filter_mapping: map<string, string>
    type: "html" | "json"       # Default: "html"
    container: string
    bindings: map<string, string>
    fields: map<string, FieldDef>
    scalars: map<string, FieldDef>
    has_next_page: bool | string # Default: true. Static or DSL expression.
    pagination: PaginationConfig

  search:                       # -> search_manga
    route: string
    method: string              # Default: "GET"
    headers: map<string, string>
    queries: map<string, string>
    filter_mapping: map<string, string>
    type: "html" | "json"       # Default: "html"
    container: string
    bindings: map<string, string>
    fields: map<string, FieldDef>
    scalars: map<string, FieldDef>
    has_next_page: bool | string # Default: true. Static or DSL expression.
    pagination: PaginationConfig

  manga_details:                # -> get_manga_details
    route: string
    method: string
    headers: map<string, string>
    queries: map<string, string>
    type: "html" | "json"
    container: string           # Usually ":root" or ""
    bindings: map<string, string>
    fields: map<string, FieldDef>
    scalars: map<string, FieldDef>

  chapter_list:                 # -> get_chapter_list
    route: string
    method: string
    headers: map<string, string>
    queries: map<string, string>
    type: "html" | "json"
    container: string
    bindings: map<string, string>
    fields: map<string, FieldDef>
    scalars: map<string, FieldDef>
    has_next_page: bool | string

  pages:                        # -> get_pages
    route: string
    method: string
    headers: map<string, string>
    queries: map<string, string>
    type: "html" | "json"
    container: string
    bindings: map<string, string>
    fields: map<string, FieldDef>
    scalars: map<string, FieldDef>

# FieldDef is either:
#   - A bare string (DSL expression, required field)
#   - A map with:
#       expr: string            # DSL expression
#       optional: bool          # Default: false

# Filters
filters:
  - id: string
    name: string
    type: "checkbox" | "select" | "text_input" | "sort"
    options:                    # For select/sort types
      - name: string
        value: string
    default:                    # Optional
      name: string
      value: string
    semantic: "author" | "artist" | "tag"  # Optional hint

# Preferences
preferences:
  - key: string
    label: string
    kind: "toggle" | "select" | "text" | "multi_value_list"
    options:                    # For select kind
      - name: string
        value: string
    default: string
    description: string         # Optional
    secret: bool                # Optional, for text kind. Default: false.

# Pagination (per-endpoint; omit for sources with flexible page sizes)
pagination:
  native_page_size: integer          # Source's fixed chunk size
  offset_param: string               # Query param name for offset/page/cursor
  offset_type: "item" | "page" | "cursor"
  page_start: integer                # For "page" type: starting page number (default: 1)
  cursor_field: string               # For "cursor" type: JSON Pointer to next-page token in response
```

### 3.7 YAML Validation Rules

The `kani-cli validate` command checks:

1. **Required fields present:** `id`, `name`, `version`, `base_url` must all be set.
2. **ID format:** Must match `[a-z][a-z0-9-]*` (lowercase, starts with letter).
3. **Version format:** Must be valid semver.
4. **Base URL format:** Must be a valid URL with scheme.
5. **DSL syntax:** All DSL strings must parse without errors.
6. **Field completeness:** For `manga_details`, the required fields are `id`, `title`, `status`. For `chapter_list`, the required fields are `id`, `number`, `language`. For `pages`, the required fields are `index`, `url`.
7. **Variable references:** All `$variable$` references in routes and queries must correspond to available function arguments or preference keys.
8. **Preference references:** All `$pref:key$` references must correspond to a declared preference.
9. **Filter mapping:** All filter mapping keys must correspond to declared filter group IDs.
10. **No unused bindings:** Warn if a top-level binding is declared but never referenced.

---

## 4. Extension Cache Interface

Extensions can store and retrieve values across invocations using the host-provided `cache` WIT interface. The cache is namespaced per extension and version, preventing cross-extension data leakage and stale reads after an upgrade.

### 4.1 Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `cache::get(key)` | `string → option<string>` | Retrieve a value by key. Returns `None` if absent or expired. |
| `cache::put(key, value, ttl_seconds)` | `string, string, u64 → ()` | Store a value with a TTL. Pass `0` for no expiry (evicted only by capacity limits). |
| `cache::delete(key)` | `string → ()` | Remove a specific key immediately. |
| `cache::clear_namespace()` | `→ ()` | Remove all keys for this extension's namespace. |

All values are serialized as strings at the boundary. Extensions are responsible for encoding/decoding structured values (e.g. JSON).

### 4.2 Scopes

The `scope` declared in the extension metadata controls the backend and key namespace:

| Scope | Backend | Lifetime | Key prefix |
|-------|---------|----------|------------|
| `session` | In-memory (per process) | Until server restart | `{ext_id}:{version}:session:` |
| `extension` | SQLite | Persistent across restarts | `{ext_id}:{version}:ext:` |
| `installation` | SQLite | Persistent, per installation | `{ext_id}:{version}:{install_id}:inst:` |

The version component means cache entries are automatically isolated between extension upgrades — a v2 extension will never read v1's cached data.

### 4.3 Capacity Limits

- **In-memory (`session`):** Global ceiling of `KANI_EXTENSION_CACHE_MAX_MB` (default 64 MB) across all extensions. LRU eviction within the global budget.
- **SQLite (`extension` / `installation`):** Per-namespace cap of 4 MB / 4096 rows (configurable). LRU eviction within the namespace when the cap is reached.

### 4.4 Usage in Rust Extensions

```rust
use kani_shared::host_abi::cache;

// Store the fetched cover CDN base URL for 10 minutes
cache::put("cdn_base", &cdn_url, 600)?;

// Retrieve on subsequent calls
if let Some(base) = cache::get("cdn_base")? {
    // use cached value
}

// Invalidate on auth refresh
cache::delete("auth_token")?;
```

### 4.5 TTL and Pruning

Expired entries are not returned by `cache::get` and are pruned by a background job running every 10 minutes (`spawn_cache_prune`). There is no guarantee of exact expiry timing — entries may persist slightly past their TTL until the next prune cycle, but will never be returned to callers after expiry.
