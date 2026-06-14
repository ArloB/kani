# Email / SMTP

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Configuration

Configure SMTP under **Settings → Admin → Email**.

| Setting | Description |
|---------|-------------|
| Host | SMTP server hostname |
| Port | SMTP port (typically 587 for STARTTLS, 465 for SSL) |
| Username | SMTP login username |
| Password | SMTP login password |
| From address | `From:` header for outbound emails |
| TLS mode | None / STARTTLS / TLS |

## Uses

- Password reset emails
- Email verification (if `email_verification_required` is enabled)
- New chapter notifications (if enabled per-user)

## Testing

After configuring, click **Send test email** to verify delivery.
