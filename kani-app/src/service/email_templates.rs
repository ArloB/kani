fn base_layout(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
  body {{ font-family: sans-serif; background: #0f0f17; color: #ddddf0; margin: 0; padding: 32px 16px; }}
  .card {{ max-width: 520px; margin: 0 auto; background: #18181f; border-radius: 12px; padding: 32px; }}
  .header {{ font-size: 22px; font-weight: 700; color: #e8545a; margin-bottom: 24px; }}
  p {{ line-height: 1.6; color: #b0b0c8; }}
  .footer {{ margin-top: 28px; font-size: 12px; color: #7878a0; }}
  a {{ color: #e8545a; }}
</style>
</head>
<body>
<div class="card">
  <div class="header">Kani</div>
  {body}
  <div class="footer">You received this email from your Kani instance.</div>
</div>
</body>
</html>"#
    )
}

fn action_button(label: &str, url: &str) -> String {
    format!(
        r#"<p style="text-align:center;margin:28px 0;">
  <a href="{url}" style="background:#e8545a;color:#fff;text-decoration:none;padding:12px 28px;border-radius:8px;font-weight:600;display:inline-block;">{label}</a>
</p>
<p style="text-align:center;font-size:12px;color:#7878a0;">Or copy this link: <a href="{url}">{url}</a></p>"#
    )
}

pub fn password_reset_email(username: &str, reset_url: &str) -> (String, String) {
    let subject = "Reset your Kani password".to_string();
    let body = format!(
        r#"<p>Hi <strong>{username}</strong>,</p>
<p>We received a request to reset your password. Click the button below to set a new one. This link expires in <strong>1 hour</strong>.</p>
{btn}
<p>If you did not request a password reset, you can safely ignore this email — your password will not change.</p>"#,
        btn = action_button("Reset Password", reset_url)
    );
    (subject, base_layout("Reset your Kani password", &body))
}

pub fn email_verification_email(username: &str, verify_url: &str) -> (String, String) {
    let subject = "Verify your Kani email address".to_string();
    let body = format!(
        r#"<p>Hi <strong>{username}</strong>,</p>
<p>Please verify your email address by clicking the button below. This link expires in <strong>24 hours</strong>.</p>
{btn}
<p>If you did not create a Kani account, you can safely ignore this email.</p>"#,
        btn = action_button("Verify Email", verify_url)
    );
    (subject, base_layout("Verify your email address", &body))
}

pub fn password_changed_email(username: &str) -> (String, String) {
    let subject = "Your Kani password was changed".to_string();
    let body = format!(
        r#"<p>Hi <strong>{username}</strong>,</p>
<p>This is a confirmation that your Kani password was successfully changed.</p>
<p>If you did not make this change, please contact your administrator immediately.</p>"#
    );
    (subject, base_layout("Password changed", &body))
}

pub fn admin_password_reset_email(username: &str) -> (String, String) {
    let subject = "An administrator reset your Kani password".to_string();
    let body = format!(
        r#"<p>Hi <strong>{username}</strong>,</p>
<p>An administrator has initiated a password reset for your account. You should receive a separate email with a reset link shortly.</p>
<p>If you did not request this, please contact your administrator.</p>"#
    );
    (subject, base_layout("Admin-initiated password reset", &body))
}

pub fn welcome_email(username: &str) -> (String, String) {
    let subject = "Welcome to Kani".to_string();
    let body = format!(
        r#"<p>Hi <strong>{username}</strong>,</p>
<p>Your Kani account has been created. You can now sign in and start exploring your library.</p>"#
    );
    (subject, base_layout("Welcome to Kani", &body))
}

pub fn test_email() -> (String, String) {
    let subject = "Kani email test".to_string();
    let body = "<p>This is a test email from your Kani instance. If you received it, your email configuration is working correctly.</p>".to_string();
    (subject, base_layout("Email test", &body))
}
