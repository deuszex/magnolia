pub mod invite_email;
pub mod reset_email;

/// Minimal HTML escaping for embedding user content in email templates.
fn he(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
