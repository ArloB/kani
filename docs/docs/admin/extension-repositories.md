# Extension Repositories

Extension repositories are signed, git-hostable catalogs of Kani extensions. Adding a repository lets you browse and install extensions from it in one click, with automatic signature and integrity verification.

## Trust model

Every repository has a **maintainer keypair** (Ed25519). When you add a repo for the first time, Kani fetches its `index.json` and shows you the maintainer's key fingerprint. You must explicitly confirm that fingerprint before the repo is trusted — this is called **Trust-on-First-Use (TOFU)**. Once confirmed, Kani pins the fingerprint and rejects any future index signed by a different key, preventing silent key substitution.

Each extension inside the index is also signed individually by its **author keypair**. Kani verifies both the index signature and the per-extension signature before writing a single byte to disk.

```
index.json   ←  signed by maintainer key
├── extension A  ←  signed by author A's key
└── extension B  ←  signed by author B's key
```

!!! warning "Review permissions before installing"
    Inspect the `unrestricted_http` and `requires_capabilities` fields of any extension before
    installing from a third-party repository. Kani sandboxes extensions in WASM, but
    `unrestricted_http` permits arbitrary outbound HTTP requests.

## Adding a repository

### Via the web UI

1. Go to **Settings → Sources → Repositories**.
2. Click **Add repository** and paste the repository URL.
3. Kani fetches the index and displays the maintainer key fingerprint:

    ```
    SHA256:AbCdEfGhIjKlMnOpQrStUvWxYz0123456789ABCDEF=
    ```

4. Verify this fingerprint out-of-band (the repository's README, its GitHub releases page, etc.) and click **Trust & add**.

### Via the REST API

```http
POST /rest/sources/repos
Content-Type: application/json

{ "url": "https://extensions.example.com" }
```

On first add the server returns **428 Precondition Required**:

```json
{
  "error": "TOFU_CONFIRMATION_REQUIRED",
  "fingerprint": "SHA256:AbCdEf...",
  "repo_url": "https://extensions.example.com"
}
```

Confirm by re-submitting with the fingerprint. You can provide it either in the request body or as a header:

=== "Body field"

    ```http
    POST /rest/sources/repos
    Content-Type: application/json

    {
      "url": "https://extensions.example.com",
      "confirm_fingerprint": "SHA256:AbCdEf..."
    }
    ```

=== "Header"

    ```http
    POST /rest/sources/repos
    Content-Type: application/json
    X-Confirm-Key-Fingerprint: SHA256:AbCdEf...

    { "url": "https://extensions.example.com" }
    ```

A successful add returns **200 OK** with `{ "id": 3, "name": "Example Extensions" }`.

### Key change after trust

If a repository's maintainer key changes after you've already trusted it, Kani returns **409 Conflict**:

```json
{
  "error": "REPO_KEY_CHANGED",
  "old_fingerprint": "SHA256:OldKey...",
  "new_fingerprint": "SHA256:NewKey...",
  "repo_url": "https://extensions.example.com"
}
```

Treat this as a serious security event. Confirm the new key through a trusted channel (the repo maintainer's announcement, signed release notes, etc.) before re-confirming.

## Browsing and installing extensions

1. Go to **Settings → Sources → Repositories → [repo name]**.
2. Extensions available in the repo are listed with name, version, and a description.
3. Click **Install** on any extension. Kani downloads the artifact, verifies its SHA-256 hash and the author's signature, then loads it into the runtime.

If the installed version is behind the latest in the repo, an **Update available** badge appears on the source card.

### Via the REST API

```http
# List extensions available in a repo
GET /rest/sources/repos/{id}/extensions

# Install an extension
POST /rest/sources/install
Content-Type: application/json

{ "repo_id": 3, "extension_id": "my-source" }

# Update an installed source to the latest repo version
POST /rest/sources/{source_id}/update
Content-Type: application/json

{ "repo_id": 3, "extension_id": "my-source" }
```

## Refreshing a repository

Kani caches the repository index. To pick up new or updated extensions, refresh the repo:

- **Web UI**: Settings → Sources → Repositories → [repo name] → **Refresh**.
- **API**: `POST /rest/sources/repos/{id}/refresh`

## Removing a repository

Removing a repository deletes the trust record but does **not** uninstall extensions that were already installed from it. Those sources continue to work normally.

- **Web UI**: Settings → Sources → Repositories → [repo name] → **Remove**.
- **API**: `DELETE /rest/sources/repos/{id}`

## Blocking repositories

Admins can block a repository URL so no user can add it. Blocked repos are checked before the TOFU flow runs.

```http
POST /rest/admin/sources/blocked-repos
Content-Type: application/json

{
  "url": "https://untrusted.example.com",
  "reason": "Violated content policy"
}
```

List or delete blocks:

```http
GET    /rest/admin/sources/blocked-repos
DELETE /rest/admin/sources/blocked-repos/{id}
```

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `KANI_SOURCE_INSTALL_ALLOWED` | `true` | Set to `false` to disable all source installation (upload, fetch, and repo-based). Existing sources continue to work. |
| `KANI_OFFICIAL_REPO_URL` | *(empty)* | URL of the official Kani extension repository. When set alongside `KANI_OFFICIAL_REPO_KEY`, Kani bootstraps this repo with `trusted_level = official` on first startup without requiring manual TOFU confirmation. |
| `KANI_OFFICIAL_REPO_KEY` | *(baked in)* | Base64-encoded Ed25519 public key that overrides the key compiled into the binary for the official repository. Useful for self-hosted or air-gapped deployments. |

### Locking down source installation

To run Kani with a fixed, administrator-controlled set of sources and prevent any further installation:

```bash
KANI_SOURCE_INSTALL_ALLOWED=false
```

This blocks all install paths: direct WASM upload, URL fetch, and repository-based install. Reload (`POST /rest/sources/{id}/reload`) is not affected, so the operator can still hot-swap on-disk files.

## Security considerations

- All repository and artifact fetches go through Kani's SSRF-protected HTTP client. Private IPs, loopback addresses, and other RFC-1918 ranges are blocked.
- Extension artifacts are verified against their SHA-256 hash **and** the author's Ed25519 signature before any bytes are written to disk. A verification failure leaves the existing source and database row unchanged.
- The `trusted_level` column distinguishes `official` (bootstrapped from `KANI_OFFICIAL_REPO_URL`) from `community` repos. The UI may surface this visually.
- Backup and restore include the `repo_trust` and `blocked_repos` tables, so TOFU pins are preserved across migrations.

## Publishing your own repository

See [Publishing & Distribution](../extension-authoring/publishing.md) for how to create, sign, and host a repository.
