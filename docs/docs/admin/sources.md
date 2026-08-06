# Sources

A source is an extension that teaches Kani how to browse, search, refresh, and download from an
external service. Sources run as WASM components or through the declarative YAML backend and can
use only host capabilities exposed by Kani.

## Install methods

Depending on permissions and operator policy, Kani can install:

- A signed extension from a configured repository.
- A YAML extension file or HTTPS URL.
- A WASM component upload or HTTPS URL.

Repository installation is preferred for third-party distribution because it verifies the
repository index, artifact digest, and author signature and provides an update relationship.
Direct installation is intended for development or an operator who already controls the artifact.

`KANI_SOURCE_INSTALL_ALLOWED=false` blocks every new installation path while leaving existing
sources and operator reload workflows available.

## Review capabilities

Before installation, review the extension's identity, version, minimum Kani version, required host
capabilities, declared hosts, scripting or browser requirement, and `unrestricted_http` flag.
WASM prevents arbitrary filesystem access, but granted host functions still perform real network,
cache, parsing, and scripting work on the extension's behalf.

An extension with `unrestricted_http` can request hosts beyond its base URL. A browser-based source
can persist a Chromium profile and requires Chromium in the server image.

## Configure a source

Open the source details page to enable or disable it, mark it as a favourite, configure declared
preferences, inspect filters and capabilities, and browse its content. Preferences are defined by
the extension and can include booleans, selections, numbers, text, and multi-value lists.

Disabling a source preserves local library data and downloads but prevents remote calls. Re-enable
it before refresh, migration, or remote-only reading.

## Health and recovery

**Settings → Source health** shows load state, recent failures, circuit breakers, browser support,
and relevant limits. Reload asks Kani to replace the in-memory backend from the on-disk artifact.
Use it after an operator has deliberately replaced a local development artifact.

When a source stops working:

1. Check whether its upstream site is reachable and changed.
2. Inspect source health and the recorded error.
3. Refresh its trusted repository and review any update.
4. Test search, manga details, chapters, and pages after updating.
5. Preserve the previous artifact until the new version is proven.

## Updates and compatibility

Repository-backed sources can report an available version and update through their repository.
Kani validates the digest, signature, minimum host version, and required capabilities before
installation. A failed update must not replace the working database row and artifact.

Source updates are independent from server upgrades. Review both sets of release notes when an
extension begins requiring a newer host capability.

See [Extension repositories](extension-repositories.md) and
[Extension authoring](../extension-authoring/yaml-schema.md).
