use chrono::{DateTime, Utc};
use sqlx::AnyPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::ProxyUserAccount;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProxySession {
    pub session_id: String,
    pub proxy_id: String,
    pub expires_at: String,
    pub created_at: String,
}

impl ProxySession {
    pub fn new(proxy_id: String, duration_days: i64) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(duration_days);
        Self {
            session_id: Uuid::new_v4().to_string(),
            proxy_id,
            expires_at: expires_at.to_rfc3339(),
            created_at: now.to_rfc3339(),
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Ok(expires) = DateTime::parse_from_rfc3339(&self.expires_at) {
            expires.with_timezone(&Utc) < Utc::now()
        } else {
            true
        }
    }
}

#[derive(Clone)]
pub struct ProxyUserRepository {
    pool: AnyPool,
}

impl ProxyUserRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, proxy: &ProxyUserAccount) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO proxy_accounts
                (proxy_id, paired_user_id, active, display_name, username, password_hash, bio, avatar_media_id, public_key, e2e_key_blob, hmac_key, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(&proxy.proxy_id)
        .bind(&proxy.paired_user_id)
        .bind(proxy.active)
        .bind(&proxy.display_name)
        .bind(&proxy.username)
        .bind(&proxy.password_hash)
        .bind(&proxy.bio)
        .bind(&proxy.avatar_media_id)
        .bind(&proxy.public_key)
        .bind(&proxy.e2e_key_blob)
        .bind(&proxy.hmac_key)
        .bind(&proxy.created_at)
        .bind(&proxy.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_by_id(&self, proxy_id: &str) -> Result<Option<ProxyUserAccount>, AppError> {
        let proxy = sqlx::query_as::<_, ProxyUserAccount>(
            r#"
            SELECT proxy_id, paired_user_id, active, display_name, username,
                   password_hash, bio, avatar_media_id, public_key, e2e_key_blob, hmac_key,
                   created_at, updated_at
            FROM proxy_accounts
            WHERE proxy_id = $1
            "#,
        )
        .bind(proxy_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(proxy)
    }

    pub async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<ProxyUserAccount>, AppError> {
        let proxy = sqlx::query_as::<_, ProxyUserAccount>(
            r#"
            SELECT proxy_id, paired_user_id, active, display_name, username,
                   password_hash, bio, avatar_media_id, public_key, e2e_key_blob, hmac_key,
                   created_at, updated_at
            FROM proxy_accounts
            WHERE username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(proxy)
    }

    pub async fn find_by_paired_user(
        &self,
        user_id: &str,
    ) -> Result<Option<ProxyUserAccount>, AppError> {
        let proxy = sqlx::query_as::<_, ProxyUserAccount>(
            r#"
            SELECT proxy_id, paired_user_id, active, display_name, username,
                   password_hash, bio, avatar_media_id, public_key, e2e_key_blob, hmac_key,
                   created_at, updated_at
            FROM proxy_accounts
            WHERE paired_user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(proxy)
    }

    pub async fn list_all(&self) -> Result<Vec<ProxyUserAccount>, AppError> {
        let proxies = sqlx::query_as::<_, ProxyUserAccount>(
            r#"
            SELECT proxy_id, paired_user_id, active, display_name, username,
                   password_hash, bio, avatar_media_id, public_key, e2e_key_blob, hmac_key,
                   created_at, updated_at
            FROM proxy_accounts
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(proxies)
    }

    pub async fn update(
        &self,
        proxy_id: &str,
        display_name: Option<&str>,
        bio: Option<&str>,
        avatar_media_id: Option<&str>,
        password_hash: Option<&str>,
        public_key: Option<&str>,
        e2e_key_blob: Option<&str>,
        hmac_key: Option<&str>,
        active: Option<i32>,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE proxy_accounts
            SET display_name     = COALESCE($1, display_name),
                bio              = COALESCE($2, bio),
                avatar_media_id  = COALESCE($3, avatar_media_id),
                password_hash    = COALESCE($4, password_hash),
                public_key       = COALESCE($5, public_key),
                e2e_key_blob     = COALESCE($6, e2e_key_blob),
                hmac_key         = COALESCE($7, hmac_key),
                active           = COALESCE($8, active),
                updated_at       = $9
            WHERE proxy_id = $10
            "#,
        )
        .bind(display_name)
        .bind(bio)
        .bind(avatar_media_id)
        .bind(password_hash)
        .bind(public_key)
        .bind(e2e_key_blob)
        .bind(hmac_key)
        .bind(active)
        .bind(&now)
        .bind(proxy_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn set_hmac_key(&self, proxy_id: &str, hmac_key: &str) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE proxy_accounts
            SET hmac_key = $1, updated_at = $2
            WHERE proxy_id = $3
            "#,
        )
        .bind(hmac_key)
        .bind(&now)
        .bind(proxy_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Clear the password hash (disables session auth for this proxy)
    pub async fn clear_password(&self, proxy_id: &str) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE proxy_accounts
            SET password_hash = NULL, updated_at = $1
            WHERE proxy_id = $2
            "#,
        )
        .bind(&now)
        .bind(proxy_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, proxy_id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM proxy_accounts WHERE proxy_id = $1")
            .bind(proxy_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // --- Session management ---

    pub async fn create_session(&self, session: &ProxySession) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO proxy_sessions (session_id, proxy_id, expires_at, created_at)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(&session.session_id)
        .bind(&session.proxy_id)
        .bind(&session.expires_at)
        .bind(&session.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_session(&self, session_id: &str) -> Result<Option<ProxySession>, AppError> {
        let session = sqlx::query_as::<_, ProxySession>(
            r#"
            SELECT session_id, proxy_id, expires_at, created_at
            FROM proxy_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM proxy_sessions WHERE session_id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_proxy_sessions(&self, proxy_id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM proxy_sessions WHERE proxy_id = $1")
            .bind(proxy_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Given a slice of user/proxy IDs, return the subset that are proxy IDs.
    /// Used to set `is_proxy` on `MemberInfo` without N+1 queries.
    pub async fn filter_proxy_ids(
        &self,
        ids: &[String],
    ) -> Result<std::collections::HashSet<String>, AppError> {
        if ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
        let sql = format!(
            "SELECT proxy_id FROM proxy_accounts WHERE proxy_id IN ({})",
            placeholders.join(", ")
        );
        let mut query = sqlx::query_scalar::<_, String>(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().collect())
    }

    /// List all active proxies (for public listing / add-member UI).
    pub async fn list_active(&self) -> Result<Vec<ProxyUserAccount>, AppError> {
        let proxies = sqlx::query_as::<_, ProxyUserAccount>(
            r#"
            SELECT proxy_id, paired_user_id, active, display_name, username,
                   password_hash, bio, avatar_media_id, public_key, e2e_key_blob, hmac_key,
                   created_at, updated_at
            FROM proxy_accounts
            WHERE active = 1
            ORDER BY username ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(proxies)
    }

    /// Disable a proxy (set active = 0).
    pub async fn disable(&self, proxy_id: &str) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE proxy_accounts SET active = 0, updated_at = $1 WHERE proxy_id = $2")
            .bind(&now)
            .bind(proxy_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
