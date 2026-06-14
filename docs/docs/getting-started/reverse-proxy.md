# Reverse Proxy

Kani runs on HTTP. Terminating TLS at a reverse proxy is the recommended way to serve it securely over the internet.

Set `KANI_SECURE_COOKIES=true` and `KANI_BIND=127.0.0.1:8242` when running behind a proxy.

## nginx

```nginx
server {
    listen 443 ssl http2;
    server_name kani.example.com;

    ssl_certificate     /etc/letsencrypt/live/kani.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/kani.example.com/privkey.pem;

    client_max_body_size 100M;

    location / {
        proxy_pass         http://127.0.0.1:8242;
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
        proxy_set_header   X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;

        proxy_http_version 1.1;
        proxy_set_header   Upgrade $http_upgrade;
        proxy_set_header   Connection "upgrade";
        proxy_read_timeout 300s;
    }
}
```

## Caddy

```caddy
kani.example.com {
    reverse_proxy 127.0.0.1:8242
}
```

Caddy obtains and renews TLS certificates automatically.

## Traefik

!!! note "TODO"
    Traefik labels example coming soon.

## Notes

- **WebSocket / SSE** — Kani uses Server-Sent Events for live download progress. Ensure your proxy
  does not buffer SSE responses (`proxy_buffering off` in nginx).
- **Upload size** — CBZ imports can be large; raise `client_max_body_size` in nginx accordingly.
