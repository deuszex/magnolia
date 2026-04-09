use sqlx::AnyPool;

use crate::errors::AppError;
use crate::models::{EmailVerification, PasswordReset, Session, UserAccount};

#[derive(Clone)]
pub struct UserRepository {
    pool: AnyPool,
}

impl UserRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    // User operations
    pub async fn create_user(&self, user: &UserAccount) -> Result<(), AppError> {
        sqlx::query(
 r#"
 INSERT INTO user_accounts (user_id, email, username, password_hash, verified, admin, active, email_visible, created_at, updated_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
 "#,
 )
 .bind(&user.user_id)
 .bind(&user.email)
 .bind(&user.username)
 .bind(&user.password_hash)
 .bind(user.verified)
 .bind(user.admin)
 .bind(user.active)
 .bind(user.email_visible)
 .bind(&user.created_at)
 .bind(&user.updated_at)
 .execute(&self.pool)
 .await?;

        Ok(())
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<UserAccount>, AppError> {
        let user = sqlx::query_as::<_, UserAccount>(
            r#"
 SELECT user_id, email, password_hash, verified, admin, active,
 display_name, username, bio, avatar_media_id, location, website,
 public_key, e2e_key_blob, email_visible, created_at, updated_at
 FROM user_accounts
 WHERE email = $1
 "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn find_by_id(&self, user_id: &str) -> Result<Option<UserAccount>, AppError> {
        let user = sqlx::query_as::<_, UserAccount>(
            r#"
 SELECT user_id, email, password_hash, verified, admin, active,
 display_name, username, bio, avatar_media_id, location, website,
 public_key, e2e_key_blob, email_visible, created_at, updated_at
 FROM user_accounts
 WHERE user_id = $1
 "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn mark_verified(&self, user_id: &str) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
 UPDATE user_accounts
 SET verified = TRUE, updated_at = $1
 WHERE user_id = $2
 "#,
        )
        .bind(&now)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // Session operations
    pub async fn create_session(&self, session: &Session) -> Result<(), AppError> {
        sqlx::query(
            r#"
 INSERT INTO sessions (session_id, user_id, expires_at, created_at)
 VALUES ($1, $2, $3, $4)
 "#,
        )
        .bind(&session.session_id)
        .bind(&session.user_id)
        .bind(&session.expires_at)
        .bind(&session.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_session(&self, session_id: &str) -> Result<Option<Session>, AppError> {
        let session = sqlx::query_as::<_, Session>(
            r#"
 SELECT session_id, user_id, expires_at, created_at
 FROM sessions
 WHERE session_id = $1
 "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
 DELETE FROM sessions
 WHERE session_id = $1
 "#,
        )
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_user_sessions(&self, user_id: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
 DELETE FROM sessions
 WHERE user_id = $1
 "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // Email verification operations
    pub async fn create_verification_token(
        &self,
        token: &EmailVerification,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
 INSERT INTO email_verifications (token, user_id, expires_at, used, created_at)
 VALUES ($1, $2, $3, $4, $5)
 "#,
        )
        .bind(&token.token)
        .bind(&token.user_id)
        .bind(&token.expires_at)
        .bind(token.used)
        .bind(&token.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_verification_token(
        &self,
        token: &str,
    ) -> Result<Option<EmailVerification>, AppError> {
        let verification = sqlx::query_as::<_, EmailVerification>(
            r#"
 SELECT token, user_id, expires_at, used, created_at
 FROM email_verifications
 WHERE token = $1
 "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        Ok(verification)
    }

    pub async fn mark_token_used(&self, token: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
 UPDATE email_verifications
 SET used = TRUE
 WHERE token = $1
 "#,
        )
        .bind(token)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // Cleanup expired sessions
    pub async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query(
            r#"
 DELETE FROM sessions
 WHERE expires_at < $1
 "#,
        )
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    // Password reset operations
    pub async fn create_password_reset_token(&self, token: &PasswordReset) -> Result<(), AppError> {
        sqlx::query(
            r#"
 INSERT INTO password_resets (token, user_id, expires_at, used, created_at)
 VALUES ($1, $2, $3, $4, $5)
 "#,
        )
        .bind(&token.token)
        .bind(&token.user_id)
        .bind(&token.expires_at)
        .bind(token.used)
        .bind(&token.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_password_reset_token(
        &self,
        token: &str,
    ) -> Result<Option<PasswordReset>, AppError> {
        let reset = sqlx::query_as::<_, PasswordReset>(
            r#"
 SELECT token, user_id, expires_at, used, created_at
 FROM password_resets
 WHERE token = $1
 "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        Ok(reset)
    }

    pub async fn mark_password_reset_used(&self, token: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
 UPDATE password_resets
 SET used = TRUE
 WHERE token = $1
 "#,
        )
        .bind(token)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn invalidate_user_password_resets(&self, user_id: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
 UPDATE password_resets
 SET used = TRUE
 WHERE user_id = $1 AND used = FALSE
 "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_password(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
 UPDATE user_accounts
 SET password_hash = $1, updated_at = $2
 WHERE user_id = $3
 "#,
        )
        .bind(password_hash)
        .bind(&now)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // Admin user management operations

    /// Find all users with pagination
    pub async fn find_all_paginated(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<UserAccount>, i64), AppError> {
        let users = sqlx::query_as::<_, UserAccount>(
            r#"
 SELECT user_id, email, password_hash, verified, admin, active,
 display_name, username, bio, avatar_media_id, location, website,
 public_key, e2e_key_blob, email_visible, created_at, updated_at
 FROM user_accounts
 WHERE user_id != '__fed__'
 ORDER BY created_at DESC
 LIMIT $1 OFFSET $2
 "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let total: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_accounts WHERE user_id != '__fed__'")
                .fetch_one(&self.pool)
                .await?;

        Ok((users, total.0))
    }

    /// Search users by email (partial match)
    pub async fn search_by_email(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<UserAccount>, i64), AppError> {
        let pattern = format!("%{}%", query);

        let users = sqlx::query_as::<_, UserAccount>(
            r#"
 SELECT user_id, email, password_hash, verified, admin, active,
 display_name, username, bio, avatar_media_id, location, website,
 public_key, e2e_key_blob, email_visible, created_at, updated_at
 FROM user_accounts
 WHERE user_id != '__fed__'
 AND (email LIKE $1 OR display_name LIKE $1 OR username LIKE $1)
 ORDER BY created_at DESC
 LIMIT $2 OFFSET $3
 "#,
        )
        .bind(&pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let total: (i64,) = sqlx::query_as(
 "SELECT COUNT(*) FROM user_accounts WHERE user_id != '__fed__' AND (email LIKE $1 OR display_name LIKE $1 OR username LIKE $1)",
 )
 .bind(&pattern)
 .fetch_one(&self.pool)
 .await?;

        Ok((users, total.0))
    }

    /// Count total users
    pub async fn count_all(&self) -> Result<i64, AppError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_accounts")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    /// Count verified users
    pub async fn count_verified(&self) -> Result<i64, AppError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_accounts WHERE verified = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    /// Count admin users
    pub async fn count_admins(&self) -> Result<i64, AppError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_accounts WHERE admin = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    /// Count active users
    pub async fn count_active(&self) -> Result<i64, AppError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_accounts WHERE active = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    /// Set user active status (activate/deactivate)
    pub async fn set_active_status(&self, user_id: &str, active: bool) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        let active_val: i32 = if active { 1 } else { 0 };

        sqlx::query(
            r#"
 UPDATE user_accounts
 SET active = $1, updated_at = $2
 WHERE user_id = $3
 "#,
        )
        .bind(active_val)
        .bind(&now)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Set user admin status
    pub async fn set_admin_status(&self, user_id: &str, admin: bool) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        let admin_val: i32 = if admin { 1 } else { 0 };

        sqlx::query(
            r#"
 UPDATE user_accounts
 SET admin = $1, updated_at = $2
 WHERE user_id = $3
 "#,
        )
        .bind(admin_val)
        .bind(&now)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user profile fields
    pub async fn update_profile(
        &self,
        user_id: &str,
        display_name: Option<&str>,
        bio: Option<&str>,
        avatar_media_id: Option<&str>,
        location: Option<&str>,
        website: Option<&str>,
        updated_at: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
 UPDATE user_accounts
 SET display_name = $1, bio = $2, avatar_media_id = $3,
 location = $4, website = $5, updated_at = $6
 WHERE user_id = $7
 "#,
        )
        .bind(display_name)
        .bind(bio)
        .bind(avatar_media_id)
        .bind(location)
        .bind(website)
        .bind(updated_at)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find user by email OR username — used for login with either identifier.
    pub async fn find_by_identifier(
        &self,
        identifier: &str,
    ) -> Result<Option<UserAccount>, AppError> {
        let user = sqlx::query_as::<_, UserAccount>(
            r#"
 SELECT user_id, email, password_hash, verified, admin, active,
 display_name, username, bio, avatar_media_id, location, website,
 public_key, e2e_key_blob, email_visible, created_at, updated_at
 FROM user_accounts
 WHERE (email = $1 OR username = $1)
 AND user_id != '__fed__'
 LIMIT 1
 "#,
        )
        .bind(identifier)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    /// Find user by username
    pub async fn find_by_username(&self, username: &str) -> Result<Option<UserAccount>, AppError> {
        let user = sqlx::query_as::<_, UserAccount>(
            r#"
 SELECT user_id, email, password_hash, verified, admin, active,
 display_name, username, bio, avatar_media_id, location, website,
 public_key, e2e_key_blob, email_visible, created_at, updated_at
 FROM user_accounts
 WHERE username = $1
 "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    /// Set email visibility preference
    pub async fn update_email_visible(&self, user_id: &str, visible: bool) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE user_accounts SET email_visible = $1, updated_at = $2 WHERE user_id = $3",
        )
        .bind(if visible { 1i32 } else { 0i32 })
        .bind(&now)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update the user's E2E public key
    pub async fn update_public_key(&self, user_id: &str, public_key: &str) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"
 UPDATE user_accounts
 SET public_key = $1, updated_at = $2
 WHERE user_id = $3
 "#,
        )
        .bind(public_key)
        .bind(&now)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Fetch only the E2E key blob for a user (lightweight — avoids loading the full row)
    pub async fn get_e2e_key_blob(&self, user_id: &str) -> Result<Option<String>, AppError> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT e2e_key_blob FROM user_accounts WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(blob,)| blob))
    }

    /// Persist the user's passphrase-encrypted E2E key blob
    pub async fn set_e2e_key_blob(&self, user_id: &str, blob: &str) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE user_accounts SET e2e_key_blob = $1, updated_at = $2 WHERE user_id = $3",
        )
        .bind(blob)
        .bind(&now)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a user account and all related data
    pub async fn delete_user(&self, user_id: &str) -> Result<(), AppError> {
        // Delete sessions
        sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        // Delete email verifications
        sqlx::query("DELETE FROM email_verifications WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        // Delete password resets
        sqlx::query("DELETE FROM password_resets WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        // Finally delete the user account
        sqlx::query("DELETE FROM user_accounts WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
