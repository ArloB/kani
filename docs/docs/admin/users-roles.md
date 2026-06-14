# Users & Roles

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Overview

Kani uses role-based access control (RBAC). Permissions are `resource:action` strings assigned to
roles, and roles are assigned to users.

## Built-in roles

<!-- TODO: document default roles (admin, user, read-only) and their permission sets -->

## Creating users

Navigate to **Settings → Admin → Users** and click **Add user**.

## Permissions reference

| Permission | Description |
|------------|-------------|
| `admin:manage` | Full admin access |
| `library:view` | Browse the library |
| `library:manage` | Add and remove library entries |
| `source:install` | Install and update extensions |
| `download:manage` | Queue and cancel downloads |

## OIDC / SSO

Set the `KANI_OIDC_ISSUER`, `KANI_OIDC_CLIENT_ID`, and `KANI_OIDC_CLIENT_SECRET` environment
variables to enable OIDC single sign-on. Users logging in via OIDC are created automatically on
first login.

<!-- TODO: detailed OIDC setup walkthrough -->
