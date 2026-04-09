use std::env;

#[derive(Debug, Clone)]
pub struct Settings {
    pub env: String,
    pub database_url: String,
    pub host: String,
    pub port: u16,
    pub base_url: String,
    pub session_duration_days: i64,

    // SMTP
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_from: String,

    // Rate limiting
    pub rate_limit_global: usize,
    pub rate_limit_auth: usize,

    // Encryption at rest (optional, 64-char hex = 32 bytes AES-256 key)
    pub encryption_at_rest_key: Option<String>,

    // Trusted reverse proxy IP for X-Forwarded-For check
    pub trusted_proxy: Option<String>,
}

impl Settings {
    pub fn from_env() -> Result<Self, String> {
        Ok(Settings {
            env: env::var("ENV").unwrap_or_else(|_| "development".to_string()),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./magnolia.db".to_string()),
            host: env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .map_err(|_| "Invalid PORT")?,
            base_url: env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
            session_duration_days: env::var("SESSION_DURATION_DAYS")
                .unwrap_or_else(|_| "7".to_string())
                .parse()
                .map_err(|_| "Invalid SESSION_DURATION_DAYS")?,

            smtp_host: env::var("SMTP_HOST").unwrap_or_default(),
            smtp_port: env::var("SMTP_PORT")
                .unwrap_or_else(|_| "587".to_string())
                .parse()
                .map_err(|_| "Invalid SMTP_PORT")?,
            smtp_username: env::var("SMTP_USERNAME").unwrap_or_default(),
            smtp_password: env::var("SMTP_PASSWORD").unwrap_or_default(),
            smtp_from: env::var("SMTP_FROM").unwrap_or_default(),

            rate_limit_global: env::var("RATE_LIMIT_GLOBAL")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .map_err(|_| "Invalid RATE_LIMIT_GLOBAL")?,
            rate_limit_auth: env::var("RATE_LIMIT_AUTH")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .map_err(|_| "Invalid RATE_LIMIT_AUTH")?,

            encryption_at_rest_key: env::var("ENCRYPTION_AT_REST_KEY")
                .ok()
                .filter(|k| !k.is_empty()),

            trusted_proxy: env::var("TRUSTED_PROXY").ok().filter(|s| !s.is_empty()),
        })
    }
}
