//! Email sending via SMTP using the lettre crate

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{MultiPart, SinglePart, header::ContentType},
    transport::smtp::authentication::Credentials,
};

use crate::config::Settings;
use magnolia_common::models::EmailSettings;

/// Returns true if SMTP is configured in the server environment settings.
pub fn smtp_is_configured(settings: &Settings) -> bool {
    !settings.smtp_host.is_empty() && !settings.smtp_username.is_empty()
}

/// Returns true if SMTP is configured via the DB email settings.
pub fn smtp_is_configured_db(es: &EmailSettings) -> bool {
    !es.smtp_host.is_empty() && !es.smtp_username.is_empty()
}

/// Send a plain-text email via the configured SMTP settings.
/// Returns `Ok(())` on success or `Err(reason)` on failure.
pub async fn send_email(
    settings: &Settings,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    if !smtp_is_configured(settings) {
        return Err("SMTP is not configured".to_string());
    }

    let from: lettre::Address = settings
        .smtp_from
        .parse()
        .map_err(|e: lettre::address::AddressError| format!("Invalid SMTP_FROM: {e}"))?;

    let to_addr: lettre::Address = to
        .parse()
        .map_err(|e: lettre::address::AddressError| format!("Invalid to address '{to}': {e}"))?;

    let email = Message::builder()
        .from(lettre::message::Mailbox::new(None, from))
        .to(lettre::message::Mailbox::new(None, to_addr))
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| format!("Failed to build email: {e}"))?;

    let creds = Credentials::new(
        settings.smtp_username.clone(),
        settings.smtp_password.clone(),
    );
    let port = settings.smtp_port;

    // Use TLS relay by default (port 465); STARTTLS for port 587; plain for anything else
    let mailer: AsyncSmtpTransport<Tokio1Executor> = if port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&settings.smtp_host)
            .map_err(|e| format!("SMTP relay error: {e}"))?
            .port(port)
            .credentials(creds)
            .build()
    } else if port == 587 {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&settings.smtp_host)
            .map_err(|e| format!("SMTP STARTTLS error: {e}"))?
            .port(port)
            .credentials(creds)
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&settings.smtp_host)
            .port(port)
            .credentials(creds)
            .build()
    };

    mailer
        .send(email)
        .await
        .map_err(|e| format!("Failed to send email: {e}"))?;

    Ok(())
}

/// Send a plain-text email using SMTP settings stored in the database.
/// `smtp_secure`: "tls" = STARTTLS, "ssl" = implicit TLS, anything else = plain.
pub async fn send_email_db(
    es: &EmailSettings,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    if !smtp_is_configured_db(es) {
        return Err("SMTP is not configured".to_string());
    }

    // Mailbox::from_str supports both "email@host" and "Name <email@host>"
    let from: lettre::message::Mailbox = es
        .smtp_from
        .parse()
        .map_err(|e| format!("Invalid smtp_from '{}': {e}", es.smtp_from))?;

    let to_addr: lettre::Address = to
        .parse()
        .map_err(|e: lettre::address::AddressError| format!("Invalid to address '{to}': {e}"))?;

    let email = Message::builder()
        .from(from)
        .to(lettre::message::Mailbox::new(None, to_addr))
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| format!("Failed to build email: {e}"))?;

    let creds = Credentials::new(es.smtp_username.clone(), es.smtp_password.clone());
    let port = es.smtp_port as u16;

    let mailer: AsyncSmtpTransport<Tokio1Executor> = match es.smtp_secure.as_str() {
        "ssl" => AsyncSmtpTransport::<Tokio1Executor>::relay(&es.smtp_host)
            .map_err(|e| format!("SMTP relay error: {e}"))?
            .port(port)
            .credentials(creds)
            .build(),
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&es.smtp_host)
            .map_err(|e| format!("SMTP STARTTLS error: {e}"))?
            .port(port)
            .credentials(creds)
            .build(),
        _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&es.smtp_host)
            .port(port)
            .credentials(creds)
            .build(),
    };

    mailer
        .send(email)
        .await
        .map_err(|e| format!("Failed to send email: {e}"))?;

    Ok(())
}

/// Send a multipart text+HTML email via the server environment SMTP settings.
pub async fn send_email_html(
    settings: &Settings,
    to: &str,
    subject: &str,
    text_body: &str,
    html_body: &str,
) -> Result<(), String> {
    if !smtp_is_configured(settings) {
        return Err("SMTP is not configured".to_string());
    }

    let from: lettre::Address = settings
        .smtp_from
        .parse()
        .map_err(|e: lettre::address::AddressError| format!("Invalid SMTP_FROM: {e}"))?;

    let to_addr: lettre::Address = to
        .parse()
        .map_err(|e: lettre::address::AddressError| format!("Invalid to address '{to}': {e}"))?;

    let email = Message::builder()
        .from(lettre::message::Mailbox::new(None, from))
        .to(lettre::message::Mailbox::new(None, to_addr))
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(text_body.to_string()),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html_body.to_string()),
                ),
        )
        .map_err(|e| format!("Failed to build email: {e}"))?;

    let creds = Credentials::new(
        settings.smtp_username.clone(),
        settings.smtp_password.clone(),
    );
    let port = settings.smtp_port;

    let mailer: AsyncSmtpTransport<Tokio1Executor> = if port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&settings.smtp_host)
            .map_err(|e| format!("SMTP relay error: {e}"))?
            .port(port)
            .credentials(creds)
            .build()
    } else if port == 587 {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&settings.smtp_host)
            .map_err(|e| format!("SMTP STARTTLS error: {e}"))?
            .port(port)
            .credentials(creds)
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&settings.smtp_host)
            .port(port)
            .credentials(creds)
            .build()
    };

    mailer
        .send(email)
        .await
        .map_err(|e| format!("Failed to send email: {e}"))?;

    Ok(())
}

/// Send a multipart text+HTML email using SMTP settings stored in the database.
pub async fn send_email_db_html(
    es: &EmailSettings,
    to: &str,
    subject: &str,
    text_body: &str,
    html_body: &str,
) -> Result<(), String> {
    if !smtp_is_configured_db(es) {
        return Err("SMTP is not configured".to_string());
    }

    let from: lettre::message::Mailbox = es
        .smtp_from
        .parse()
        .map_err(|e| format!("Invalid smtp_from '{}': {e}", es.smtp_from))?;

    let to_addr: lettre::Address = to
        .parse()
        .map_err(|e: lettre::address::AddressError| format!("Invalid to address '{to}': {e}"))?;

    let email = Message::builder()
        .from(from)
        .to(lettre::message::Mailbox::new(None, to_addr))
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(text_body.to_string()),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html_body.to_string()),
                ),
        )
        .map_err(|e| format!("Failed to build email: {e}"))?;

    let creds = Credentials::new(es.smtp_username.clone(), es.smtp_password.clone());
    let port = es.smtp_port as u16;

    let mailer: AsyncSmtpTransport<Tokio1Executor> = match es.smtp_secure.as_str() {
        "ssl" => AsyncSmtpTransport::<Tokio1Executor>::relay(&es.smtp_host)
            .map_err(|e| format!("SMTP relay error: {e}"))?
            .port(port)
            .credentials(creds)
            .build(),
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&es.smtp_host)
            .map_err(|e| format!("SMTP STARTTLS error: {e}"))?
            .port(port)
            .credentials(creds)
            .build(),
        _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&es.smtp_host)
            .port(port)
            .credentials(creds)
            .build(),
    };

    mailer
        .send(email)
        .await
        .map_err(|e| format!("Failed to send email: {e}"))?;

    Ok(())
}
