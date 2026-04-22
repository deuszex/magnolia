use crate::handlers::email_html::he;

pub(crate) fn build_reset_email(
    site_name: &str,
    accent: &str,
    reset_link: String,
) -> (String, String) {
    let text_body = format!(
        "Hello,\n\nA password reset was requested for your account on {}.\n\n\
             Click the link below to set a new password. This link expires in 1 hour.\n\n\
             {reset_link}\n\n\
             If you did not request this, you can safely ignore this email.",
        site_name
    );

    let html_body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1"></head>
<body style="margin:0;padding:0;background:#f4f4f5;font-family:sans-serif;">
  <table width="100%" cellpadding="0" cellspacing="0" style="background:#f4f4f5;padding:40px 0;">
    <tr><td align="center">
      <table width="480" cellpadding="0" cellspacing="0" style="background:#ffffff;border-radius:8px;overflow:hidden;box-shadow:0 2px 8px rgba(0,0,0,.08);">
        <tr><td style="background:{accent};padding:24px 32px;">
          <span style="color:#ffffff;font-size:20px;font-weight:700;">{site_name_escaped}</span>
        </td></tr>
        <tr><td style="padding:32px;">
          <p style="margin:0 0 16px;font-size:16px;color:#111827;">Hello,</p>
          <p style="margin:0 0 16px;font-size:15px;color:#374151;">
            A password reset was requested for your account on
            <strong>{site_name_escaped}</strong>.
            Click the button below to set a new password.
            This link expires in <strong>1 hour</strong>.
          </p>
          <p style="margin:24px 0;text-align:center;">
            <a href="{reset_link_escaped}" style="display:inline-block;background:{accent};color:#ffffff;text-decoration:none;padding:12px 28px;border-radius:6px;font-size:15px;font-weight:600;">Reset Password</a>
          </p>
          <p style="margin:0 0 8px;font-size:13px;color:#6b7280;">
            Or copy this link into your browser:
          </p>
          <p style="margin:0 0 24px;font-size:13px;color:#6b7280;word-break:break-all;">
            <a href="{reset_link_escaped}" style="color:{accent};">{reset_link_escaped}</a>
          </p>
          <p style="margin:0;font-size:13px;color:#9ca3af;">
            If you did not request a password reset, you can safely ignore this email.
          </p>
        </td></tr>
      </table>
    </td></tr>
  </table>
</body>
</html>"#,
        accent = he(&accent),
        site_name_escaped = he(&site_name),
        reset_link_escaped = he(&reset_link),
    );
    (text_body, html_body)
}
