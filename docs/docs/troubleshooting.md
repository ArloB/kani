# Troubleshooting

## Server won't start

**Symptom:** Container exits immediately.

- Check `docker compose logs kani` for the error.
- `KANI_SECRET_KEY` must be set and at least 32 characters.
- The `/data` and `/library` volumes must be writable by the container user.

## Extension fails to fetch

**Symptom:** A source returns no results or shows an error.

Extensions talk to external websites. If the site changed its structure, the extension may need an
update — this is usually not a Kani bug.

1. Check **Settings → Sources → [source name]** for the installed version.
2. Try updating the source.
3. Open **Settings → Admin → Logs** and look for error lines from the source.
4. If the source is still broken after updating, file a report on the
   [extension's issue tracker](https://github.com/ArloB/kani/issues) using the "Extension broken" template.

## Database is locked

**Symptom:** `SqliteError: database is locked`

SQLite WAL mode should prevent this in normal use. Causes:

- A backup tool or external process has the database open.
- The `/data` volume is on a network filesystem (NFS/CIFS) — SQLite WAL is not safe on these.
  Use a local volume.

## Pages load slowly / images fail to load

- Check the source's page-load action in the extension settings.
- Some sources require a valid session or cookies — see the source's readme.

## Login fails

- Confirm the username and password are correct.
- If OIDC is configured, ensure `KANI_OIDC_ISSUER`, `KANI_OIDC_CLIENT_ID`, and `KANI_OIDC_CLIENT_SECRET` are all set.
- Check `/rest/system/info` — if `registration_enabled: false` and you've lost your admin password,
  you'll need to reset the database. There is no generated password to recover: on a new server the
  first account is created through the setup screen, and that window closes once it exists.

## The setup screen says setup is unavailable

A new server shows a setup screen to create the administrator. It is refused in two cases:

- **An account already exists.** Setup closes permanently the moment the first account is created.
  Sign in instead; if nobody knows the password, reset the database.
- **You are not on the local network.** Setup is only accepted from a loopback or private address,
  so an instance exposed to the internet before its owner reaches it cannot be claimed by a
  stranger. Reach it over the LAN or an SSH tunnel, or start the server with
  `KANI_ALLOW_REMOTE_SETUP=true` if it genuinely must be done over the internet. Behind a reverse
  proxy the proxy's address is what Kani sees, so the proxy is the boundary.

## Getting more help

- Search or open a [GitHub Discussion](https://github.com/ArloB/kani/discussions).
- Check [existing issues](https://github.com/ArloB/kani/issues).
- Enable debug logging with `RUST_LOG=kani=debug` and attach relevant lines to your report.
