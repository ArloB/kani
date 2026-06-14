# Observability

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Logs

Kani uses the `tracing` crate. Control log level with `RUST_LOG`:

```bash
RUST_LOG=kani=debug,tower_http=warn
```

View logs in Docker:

```bash
docker compose logs -f kani
```

In-app log viewer: **Settings → Admin → Logs**.

## Metrics

<!-- TODO: Prometheus metrics endpoint (if/when added) -->

## Health endpoint

`GET /health` returns `200 OK` with body `ok`. Use this for load balancer health checks.

## Tracing

<!-- TODO: OpenTelemetry / Jaeger integration (if/when added) -->
