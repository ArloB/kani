# Publishing & Distribution

This guide covers creating a signed extension repository and publishing extensions to it. Users add your
repository to their Kani instance and install extensions from it with one click. See
[Extension Repositories](../admin/extension-repositories.md) for the admin side of this process.

!!! note "Requires kani-cli from this repository"
    The `kani-cli keygen`, `publish`, and `repo` subcommands described in this guide
    are available in the current development version of `kani-cli`.

## Concepts

### Keypairs

Two types of keypairs are involved:

| Key | Who holds it | What it signs |
|---|---|---|
| **Maintainer key** | You, the repository owner | The repository `index.json` as a whole |
| **Author key** | Individual extension authors | The extension artifact itself |

These can be the same keypair if you are the sole author of all extensions in your repository. Use separate
author keys for multi-author repositories to allow per-author revocation.

### Trust chain

```text
Repo maintainer key  →  signs index.json
  Author key         →  signs extension artifact
    SHA-256          →  listed in index.json; verified against downloaded bytes
```

Kani verifies this chain on every install and update. A failure at any step is fatal — no bytes are written to disk.

## Setting up a repository

### 1. Generate keypairs

!!! note "Requires `kani-cli` ≥ 0.x"

```bash
# Generate your maintainer keypair
kani-cli keygen --out-dir ./keys

# If you have multiple authors, each generates their own key
kani-cli keygen --out-dir ./author-keys/alice
```

This produces two files per invocation:

| File | Contents | Keep secret? |
|---|---|---|
| `maintainer.key` | Private key (base64 seed, plaintext) | **Yes** |
| `maintainer.pub` | Public key (base64 Ed25519) | No — share this |

!!! warning "Key file security"
    Private key files are stored as plaintext base64. `kani-cli keygen` writes them
    `0600` on Unix; on Windows, restrict the file yourself. Never commit one to
    version control. There is no passphrase encryption — treat the file itself as
    the secret.

!!! warning "Back up your private key"
    Loss of the maintainer private key means you cannot update the repository index.
    Users with TOFU-pinned trust cannot be automatically migrated to a new key — they
    would need to remove and re-add the repository.

### 2. Initialise the repository

```bash
kani-cli repo init --maintainer-key ./keys/maintainer.pub --name "My Extensions"
```

This creates `index.json` and the `extensions/` directory:

!!! note
    `index.json.sig` is created by `publish --repo-sign-key` (step 3). `repo init` creates the skeleton only.

```text
my-repo/
├── index.json         ← signed catalog; this is the URL you give users
├── index.json.sig     ← maintainer signature over index.json
└── extensions/        ← artifact storage
```

**Share your public key fingerprint** alongside the repository URL whenever you announce the repo. Users need it
to complete the TOFU confirmation. You can print it at any time:

```bash
kani-cli repo show-fingerprint --key ./keys/maintainer.pub
# SHA256:AbCdEfGhIjKlMnOpQrStUvWxYz0123456789ABCDEF=
```

### 3. Publish extensions

#### From a YAML definition

```bash
kani-cli publish \
  --sign-key ./author-keys/alice/author.key \
  --repo-sign-key ./keys/maintainer.key \
  ./my-source.yaml
```

#### From a compiled WASM artifact

```bash
kani-cli publish \
  --sign-key ./author-keys/alice/author.key \
  --repo-sign-key ./keys/maintainer.key \
  ./my-source.wasm
```

`publish` performs the following steps:

1. Validates the extension (YAML schema check, or WASM component header check).
2. Computes a SHA-256 digest of the artifact bytes.
3. Signs the artifact with the author key, writing `<artifact>.sig` alongside it.
4. Copies the artifact pair to `extensions/<id>/<version>/`.
5. Upserts the index entry in `index.json`.
6. Re-signs `index.json` with the maintainer key, replacing `index.json.sig`.

#### Updating an extension version

Re-run `publish` with the same extension ID and a bumped version. The old version remains under
`extensions/<id>/<old-version>/`; Kani uses the highest semver-valid version listed in the index as the latest.

## Repository file format

### `index.json`

```json
{
  "name": "My Extensions",
  "maintainer_key": "<base64 Ed25519 public key>",
  "extensions": [
    {
      "id": "my-source",
      "name": "My Source",
      "version": "1.2.0",
      "format": "yaml",
      "description": "A source for Example Site",
      "language": "en",
      "nsfw": false,
      "min_kani_version": "0.1.0",
      "sha256": "<hex digest of the artifact>",
      "signature": "<base64 Ed25519 signature over the artifact bytes>",
      "author_key": "<base64 Ed25519 public key of the signing author>"
    }
  ]
}
```

`format` is `"yaml"` for interpreted YAML extensions and `"wasm"` for compiled WASM components.

### Artifact layout

```text
extensions/
└── my-source/
    └── 1.2.0/
        ├── extension.yaml       ← the artifact (or extension.wasm)
        └── extension.yaml.sig   ← Ed25519 signature (author key; raw bytes, base64-encoded)
```

The URL of an artifact is `<repo_base_url>/extensions/<id>/<version>/extension.<format>`. Kani constructs
this URL from the `index.json` entry; you do not configure it separately.

### `index.json.sig`

A raw Ed25519 signature over the UTF-8 bytes of `index.json`, base64-encoded, written to a separate file at
`<repo_base_url>/index.json.sig`. Kani fetches both files and verifies the signature before trusting any entry.

## Verifying repository integrity

Before publishing or in CI, verify that all signatures and digests in the repository are consistent:

```bash
kani-cli repo verify --repo-key ./keys/maintainer.pub
```

Exits 0 on success, non-zero if any signature fails or a digest does not match the artifact on disk. Run this
as a CI step to catch accidental file corruption or a missed re-sign after an edit.

## Hosting

A Kani repository is a directory of static files. Any static file host works:

- **GitHub Pages** — push the repo directory to a `gh-pages` branch or a `docs/` folder; set the base URL accordingly.
- **GitHub Releases + raw.githubusercontent.com** — simpler; no branch management.
- **Any CDN or object store** — S3, Cloudflare R2, Bunny, etc. Ensure `Content-Type: application/json` is served
  for `.json` files.

Kani requires `https://` for all repository URLs. HTTP-only hosts are rejected by the server before the TOFU
flow begins.

### Example: GitHub Pages workflow

```yaml
# .github/workflows/publish.yml
on:
  push:
    branches: [main]
    paths: ["extensions/**", "*.yaml"]

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install kani-cli
        run: cargo install kani-cli
      - name: Publish extension
        run: |
          kani-cli publish \
            --sign-key ./keys/maintainer.key \
            --repo-sign-key ./keys/maintainer.key \
            ./my-source.yaml
          kani-cli repo verify --repo-key ./keys/maintainer.pub
      - uses: peaceiris/actions-gh-pages@v4
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: .
```

Store `maintainer.key` as an encrypted GitHub Actions secret. Commit only `maintainer.pub` to the repository.

## Revoking an extension

Remove the entry from `index.json` and re-sign. Existing installs are not automatically uninstalled — users
must remove the source manually. Consider leaving a `deprecated: true` marker and `description` note before
removing the entry entirely.

## Key rotation

If your author key is compromised:

1. Generate a new author key.
2. Re-sign all your artifacts with the new key using `kani-cli publish --re-sign`.
3. Announce the key change through a trusted channel.

If the **maintainer key** is compromised:

1. Generate a new keypair.
2. Re-sign the entire repository index with the new maintainer key.
3. Notify your users — they will see a **409 Key Changed** warning in Kani and must manually re-confirm the
   new fingerprint after verifying it out-of-band.

## Writing an extension

See [YAML schema](./yaml-schema.md) for the declarative format (recommended for new sources) and
[DSL grammar](./dsl-grammar.md) for the expression language used in field selectors.
