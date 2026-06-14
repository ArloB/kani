# Authentication

## Session-based auth

Kani uses HTTP-only session cookies. Obtain a session by POSTing credentials to `/rest/auth/login`.

### Login

```http
POST /rest/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "password"
}
```

Response `200 OK` sets a `kani_session` cookie. Pass this cookie in subsequent requests.

### Logout

```http
POST /rest/auth/logout
```

Invalidates the session and clears the cookie.

## OIDC / SSO

If `KANI_OIDC_ISSUER` is configured, the login page shows a "Sign in with SSO" button. The OIDC flow:

1. `GET /rest/auth/oidc/start` — redirects to the provider.
2. Provider redirects back to `GET /rest/auth/oidc/callback` with an authorization code.
3. Kani exchanges the code for tokens, creates or updates the user, and sets the session cookie.

## API tokens

!!! note "TODO"
    API token (bearer token) support is planned but not yet implemented. For automation use cases,
    create a dedicated user account and authenticate via the login endpoint.
