# Install with Docker

## Choose a deployment source

The repository includes a supported Dockerfile and Compose file. Building a pinned release tag
locally does not depend on a registry image:

```bash
git clone https://github.com/ArloB/kani.git
cd kani
git checkout <release-tag>
docker compose up --build -d
```

If a release publishes a container image, use the exact image coordinates and verification
instructions from that GitHub Release. Do not infer a registry name or assume that a moving
`latest` tag exists.

## Compose configuration

The essential shape of the included Compose service is:

```yaml
services:
  kani:
    build: .
    ports:
      - "8242:8242"
    volumes:
      - ./data:/data
      - ./library:/library
    environment:
      KANI_BIND: "0.0.0.0:8242"
      KANI_SECURE_COOKIES: "false"
    restart: unless-stopped
```

Start it and follow the logs:

```bash
docker compose up --build -d
docker compose logs -f kani
```

## Volumes and ownership

| Mount | Contents |
|---|---|
| `/data` | SQLite database, installed extensions, browser profiles, and generated key files |
| `/library` | Covers, downloaded chapter data, and archives |

Both mounts must be writable by UID and GID 1000. `/data/secret.key` protects stored credentials,
and `/data/proxy.key` signs image-proxy URLs. Losing either key can make the corresponding stored
data unusable, so back them up with `kani.db`.

Do not put the SQLite database on NFS, SMB, or another filesystem that does not provide SQLite's
required locking semantics. The library may live on a separate large disk.

## Ports and binding

Kani listens on TCP port 8242 by default. `KANI_BIND` controls both the address and port:

```yaml
environment:
  KANI_BIND: "127.0.0.1:8242"
```

Binding to loopback is appropriate for a host-networked reverse proxy. Inside a normal Compose
network, leave Kani listening on `0.0.0.0` and publish the host port only where required.

## Common environment variables

| Variable | Default | Purpose |
|---|---|---|
| `KANI_BIND` | `0.0.0.0:8242` | Listen address and port |
| `KANI_DATA_DIR` | process working directory | Database and generated-key directory |
| `KANI_LIBRARY_DIR` | setting value | Override the library directory at startup |
| `KANI_STATIC_DIR` | `static` | Frontend asset directory for nonstandard binary layouts |
| `KANI_SECURE_COOKIES` | `false` | Mark cookies Secure; enable behind HTTPS |
| `KANI_CORS_ORIGIN` | request origin | Restrict browser API access to one origin |
| `KANI_PUBLIC_INSTANCE` | `false` | Enable the internet-facing hardened profile |
| `KANI_ALLOW_REMOTE_SETUP` | `false` | Permit first-account setup outside private networks |
| `KANI_ALLOW_REGISTRATION` | setting value | Startup override for self-registration |
| `KANI_SECRET_KEY` | generated file | Inline 64-character hex credential-encryption key |
| `KANI_SECRET_KEY_FILE` | — | Read the credential key from a mounted secret |
| `KANI_PROXY_SECRET` | generated file | Base64url 32-byte image-proxy signing key |
| `KANI_LOG_FORMAT` | `text` | `text` or structured `json` output |
| `RUST_LOG` | `error` | `tracing` filter |

See [Configuration](../admin/configuration.md) for rate limits, browser support, extension
installation policy, database pools, and which values should instead be changed in the UI.

## Optional image features

The Dockerfile supports a build argument for a larger optional dependency:

```yaml
build:
  args:
    INSTALL_KCC: "true"
```

`INSTALL_KCC` adds Kindle Comic Converter for MOBI/AZW3 export. It increases image size and should
be enabled only when needed.

Sources that capture browser payloads need no build argument: that work runs in the solver, which
is a separate container. See [Configuration](../admin/configuration.md) for the solver image and
its key.

## Health and restart behavior

The image health check calls `GET /health`. Kani also exposes `/healthz`, `/ready`, and `/readyz`;
readiness can fail while the process is still alive.

Exit code 0 is a clean shutdown. Exit code 42 requests a supervised restart, which
`restart: unless-stopped` handles.

## Upgrade

Take a backup, fetch the intended release, review its notes, then rebuild the container:

```bash
git fetch --tags
git checkout <release-tag>
docker compose up --build -d
```

Database migrations run during startup. Do not assume that a migrated database can be opened by an
older binary; follow the release's migration and rollback policy. See [Upgrades](../admin/upgrade.md).
