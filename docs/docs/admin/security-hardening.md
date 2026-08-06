# Security Hardening

## Internet-facing checklist

- Terminate TLS at a maintained reverse proxy and set `KANI_SECURE_COOKIES=true`.
- Enable `KANI_PUBLIC_INSTANCE=true` for the hardened runtime profile.
- Restrict direct access to Kani's port and set an explicit `KANI_CORS_ORIGIN`.
- Complete first-run setup before publishing the service; do not leave
  `KANI_ALLOW_REMOTE_SETUP=true` enabled.
- Disable self-registration unless it is intentionally offered.
- Require strong account passwords and enable TOTP for administrators.
- Review active sessions and revoke unfamiliar ones.
- Give automation a scoped API token instead of a user's cookie or password.
- Review role inheritance and grant only the permissions each role needs.
- Restrict extension installation and review `unrestricted_http` and required capabilities.
- Protect and back up encryption keys separately from ordinary logs and support bundles.
- Authenticate `/metrics` with a token scoped to `metrics:read`.

## Accounts, sessions, and two-factor authentication

Users manage passwords, active sessions, TOTP, and backup codes under the Account and Security
settings sections. Backup codes are shown once. Regenerating them invalidates the previous set.

Changing session-timeout settings requires a server restart. Logging out everywhere or revoking a
specific session is the immediate response to a suspected session compromise.

The public-instance profile tightens cookies, policy headers, and login controls and warns when an
administrator lacks two-factor authentication. It complements TLS and network controls; it does
not configure them.

## API and OPDS tokens

Tokens are shown only when created and are stored hashed. General API tokens use bearer
authentication and explicit scopes. OPDS reader tokens are accepted only by the OPDS surface.
Effective permissions are intersected with the owner's current permissions, so role removal also
reduces long-lived token access.

Use separate tokens per client, set an expiry where available, name the device or integration, and
revoke unused tokens from **Settings → Clients**.

## Key management

`KANI_SECRET_KEY` is not a session-cookie password. It encrypts stored credentials. If no explicit
key is supplied, Kani provisions `/data/secret.key`. Image-proxy signing uses a separate
`proxy.key` or `KANI_PROXY_SECRET`.

To rotate a credential key safely, first ensure the application can decrypt existing credentials,
then follow the release's supported rotation or migration procedure. Replacing the key file in
place makes previously encrypted values unreadable. Never improvise a rotation by deleting it.

## Extension isolation

WASM prevents direct host filesystem access, but extensions intentionally receive networking,
parsing, cache, preference, extraction, and optional scripting capabilities. An extension with
`unrestricted_http` is not restricted to its declared base host. Browser-based sources also add a
Chromium process and persistent profiles to the threat model.

Use signed repositories, verify TOFU fingerprints out of band, block untrusted repository URLs,
and set `KANI_SOURCE_INSTALL_ALLOWED=false` for a fixed-source deployment.
