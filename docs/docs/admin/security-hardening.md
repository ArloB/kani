# Security Hardening

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Checklist

- [ ] Set a strong, random `KANI_SECRET_KEY` (32+ characters).
- [ ] Enable `KANI_SECURE_COOKIES=true` when serving over HTTPS.
- [ ] Run behind a reverse proxy with TLS termination (see [Reverse proxy](../getting-started/reverse-proxy.md)).
- [ ] Set `KANI_BIND=127.0.0.1:8242` so the port is not exposed directly to the internet.
- [ ] Complete first-run setup before publishing the instance. A server with no accounts shows a
      setup screen; it is restricted to loopback and private addresses, but if you set
      `KANI_ALLOW_REMOTE_SETUP=true` the first person to reach it can claim it.
- [ ] Disable `registration_enabled` once all accounts are created.
- [ ] Review extension permissions — sources with `unrestricted_http` can make arbitrary outbound requests.

## Secret key rotation

<!-- TODO: procedure for rotating KANI_SECRET_KEY (invalidates all sessions) -->

## Network isolation

<!-- TODO: Docker network segmentation, firewall rules -->
