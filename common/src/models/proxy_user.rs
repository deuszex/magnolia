use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProxyUserAccount {
    pub proxy_id: String,
    /// Id of the user this proxy is tied to. In case it's missing it's handled by the admins. If paired the user can manage.
    pub paired_user_id: Option<String>,
    pub active: i32,
    pub display_name: Option<String>,
    pub username: String,
    pub password_hash: Option<String>,
    pub bio: Option<String>,
    pub avatar_media_id: Option<String>,
    pub public_key: Option<String>,
    /* Store each user's passphrase-encrypted E2E key blob server-side.
    The blob is opaque to the server: it is AES-256-GCM ciphertext wrapped
    around the user's ECDH private key JWK, keyed by a PBKDF2-derived wrapping
    key that the server never sees.  Only the owning user can decrypt it.
    Use the passphrase on the proxys client to decrypt the key,
    so the proxy is capable of communications.*/
    pub e2e_key_blob: Option<String>,
    /// HMAC shared secret for one-shot request signing (hex-encoded)
    pub hmac_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ProxyUserAccount {
    pub fn new(username: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            proxy_id: uuid::Uuid::new_v4().to_string(),
            paired_user_id: None,
            active: 0,
            display_name: None,
            username,
            password_hash: None,
            bio: None,
            avatar_media_id: None,
            public_key: None,
            e2e_key_blob: None,
            hmac_key: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
