use crate::handlers::email_html::he;

/// Build invite email subject, plain-text body, and HTML body.
pub(crate) fn build_invite_email(
    site_name: &str,
    accent: &str,
    invite_link: &str,
    expires_hours: i64,
    personal_message: Option<&str>,
) -> (String, String, String) {
    let subject = format!("You've been invited to join {}", site_name);

    // Plain text
    let msg_text = personal_message
        .map(|m| format!("\nMessage from the team:\n{}\n", m))
        .unwrap_or_default();

    let text_body = format!(
        "You have been invited to join {site_name}.{msg_text}\n\
 Click the link below to register:\n{invite_link}\n\n\
 This invite expires in {expires_hours} hours.\n\
 If you did not expect this email you can safely ignore it.",
        site_name = site_name,
        msg_text = msg_text,
        invite_link = invite_link,
        expires_hours = expires_hours,
    );

    // HTML
    let msg_html = personal_message
 .map(|m| {
 let lines: String = m
 .lines()
 .map(|l| format!("{}<br>", he(l)))
 .collect::<Vec<_>>()
 .join("\n");
 format!(
 r#"<tr><td style="padding:0 32px 24px">
 <div style="background:#1e2330;border-left:3px solid {accent};border-radius:4px;padding:14px 16px">
 <p style="margin:0 0 6px;font-size:11px;font-weight:700;text-transform:uppercase;letter-spacing:0.06em;color:{accent}">Message from the team</p>
 <p style="margin:0;color:#cbd5e1;font-size:14px;line-height:1.6">{lines}</p>
 </div>
</td></tr>"#,
 accent = accent,
 lines = lines,
 )
 })
 .unwrap_or_default();

    let site_name_h = he(site_name);
    let link_h = he(invite_link);

    let html_body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
 <meta charset="UTF-8">
 <meta name="viewport" content="width=device-width,initial-scale=1.0">
 <title>You're invited to {site_name_h}</title>
</head>
<body style="margin:0;padding:0;background:#0d0f14;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif">
 <table role="presentation" width="100%" cellpadding="0" cellspacing="0"
 style="background:#0d0f14;padding:48px 16px">
 <tr><td align="center">

 <!-- Card -->
 <table role="presentation" cellpadding="0" cellspacing="0"
 style="background:#181b24;border-radius:12px;max-width:560px;width:100%;overflow:hidden">

 <!-- Header -->
 <tr>
 <td style="background:{accent};padding:22px 32px">
 <span style="color:#ffffff;font-size:18px;font-weight:700;letter-spacing:-0.02em">{site_name_h}</span>
 </td>
 </tr>

 <!-- Headline -->
 <tr>
 <td style="padding:36px 32px 8px">
 <h1 style="margin:0 0 12px;font-size:26px;font-weight:700;color:#f1f5f9;line-height:1.2">
 You&#8217;ve been invited!
 </h1>
 <p style="margin:0;color:#94a3b8;font-size:15px;line-height:1.6">
 You have been invited to join
 <strong style="color:#e2e8f0">{site_name_h}</strong>.
 Click the button below to create your account.
 </p>
 </td>
 </tr>

 <!-- Optional personal message -->
 {msg_html}

 <!-- CTA button -->
 <tr>
 <td style="padding:24px 32px 8px">
 <table role="presentation" cellpadding="0" cellspacing="0">
 <tr>
 <td style="background:{accent};border-radius:8px">
 <a href="{link_h}"
 style="display:inline-block;padding:13px 30px;color:#ffffff;
 text-decoration:none;font-weight:600;font-size:15px">
 Accept Invitation
 </a>
 </td>
 </tr>
 </table>
 </td>
 </tr>

 <!-- Fallback link -->
 <tr>
 <td style="padding:16px 32px 0">
 <p style="margin:0 0 4px;color:#64748b;font-size:12px">Or copy this link into your browser:</p>
 <p style="margin:0;word-break:break-all;font-size:12px;font-family:monospace;color:{accent}">
 <a href="{link_h}" style="color:{accent};text-decoration:none">{link_h}</a>
 </p>
 </td>
 </tr>

 <!-- Expiry notice -->
 <tr>
 <td style="padding:20px 32px 28px">
 <p style="margin:0;color:#475569;font-size:12px;
 padding-top:16px;border-top:1px solid #252935;line-height:1.5">
 This invitation expires in <strong>{expires_hours} hours</strong>.
 If you did not expect this email you can safely ignore it.
 </p>
 </td>
 </tr>

 <!-- Footer -->
 <tr>
 <td style="background:#11131a;padding:14px 32px;border-top:1px solid #252935">
 <p style="margin:0;color:#334155;font-size:11px">
 {site_name_h} &mdash; Sent via secure invitation
 </p>
 </td>
 </tr>

 </table>
 </td></tr>
 </table>
</body>
</html>"#,
        site_name_h = site_name_h,
        accent = accent,
        link_h = link_h,
        msg_html = msg_html,
        expires_hours = expires_hours,
    );

    (subject, text_body, html_body)
}
