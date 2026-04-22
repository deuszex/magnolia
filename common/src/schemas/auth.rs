use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    /// Email address, optional. If omitted a no-reply placeholder is stored.
    pub email: Option<String>,

    /// Username, required, primary login identifier and display handle.
    #[validate(length(min = 3, max = 30))]
    pub username: String,

    #[validate(length(min = 8, max = 128))]
    pub password: String,

    #[validate(must_match(other = "password"))]
    pub password_confirm: String,

    /// Required when registration_mode is 'invite_only'
    pub invite_token: Option<String>,
}

/// Lightweight public endpoint response, tells the frontend how registration works
/// and which password-reset methods are available.
#[derive(Debug, Serialize)]
pub struct AuthConfigResponse {
    /// 'open', 'invite_only', or 'application'
    pub registration_mode: String,
    /// Email-token reset is enabled by admin and SMTP is configured
    pub password_reset_email_available: bool,
    /// Signing-key reset is enabled by admin
    pub password_reset_signing_key_available: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    /// Email address or username, either can be used to log in.
    #[validate(length(min = 1))]
    pub identifier: String,

    #[validate(length(min = 1))]
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResendVerificationRequest {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RequestPasswordResetRequest {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ValidatePasswordResetRequest {
    pub token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordRequest {
    pub token: String,

    #[validate(length(min = 8, max = 128))]
    pub password: String,

    #[validate(must_match(other = "password"))]
    pub password_confirm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse {
    pub user_id: String,
    /// Only present for the user's own profile or when email_visible = 1
    pub email: Option<String>,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub user: UserResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_current: bool,
}

/// Minimal user info for user listings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListItem {
    pub user_id: String,
    /// Only present when email_visible = 1
    pub email: Option<String>,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListResponse {
    pub users: Vec<UserListItem>,
    pub total: i64,
}

#[derive(Debug, Deserialize)]
pub struct UserListQuery {
    #[serde(default = "default_user_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub q: Option<String>,
}

fn default_user_limit() -> i64 {
    100
}

/// Update user profile
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProfileRequest {
    #[validate(length(max = 50))]
    pub display_name: Option<String>,

    #[validate(length(max = 500))]
    pub bio: Option<String>,

    pub avatar_media_id: Option<String>,

    #[validate(length(max = 100))]
    pub location: Option<String>,

    #[validate(length(max = 200))]
    pub website: Option<String>,
}

/// Full profile response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub user_id: String,
    /// Only present for own profile or when email_visible = 1
    pub email: Option<String>,
    pub email_visible: bool,
    pub display_name: Option<String>,
    pub username: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub location: Option<String>,
    pub website: Option<String>,
    pub public_key: Option<String>,
    pub created_at: String,
}

/// Update email visibility preference
#[derive(Debug, Deserialize)]
pub struct UpdateEmailVisibleRequest {
    pub email_visible: bool,
}

/// Upload own E2E public key
#[derive(Debug, Deserialize, Validate)]
pub struct UpdatePublicKeyRequest {
    #[validate(length(min = 1, max = 4096))]
    pub public_key: String,
}

/// Response for GET /api/auth/me/e2e-key
#[derive(Debug, Serialize)]
pub struct E2EKeyBlobResponse {
    /// The passphrase-encrypted E2E key blob, or null if none has been stored yet.
    pub e2e_key_blob: Option<String>,
}

/// Request body for PUT /api/auth/me/e2e-key
#[derive(Debug, Deserialize, Validate)]
pub struct SetE2EKeyBlobRequest {
    /// Passphrase-encrypted E2E key blob.  The server stores it opaquely and never decrypts it.
    /// Max 8 KiB, a P-256 JWK wrapped in AES-256-GCM is well under 2 KiB.
    #[validate(length(min = 1, max = 8192))]
    pub e2e_key_blob: String,
}

/// Reset password using a downloaded HMAC signing key.
/// POST /api/auth/reset-password-with-key
#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordWithKeyRequest {
    /// The user_id embedded in the downloaded key file.
    #[validate(length(min = 1))]
    pub user_id: String,
    /// Unix timestamp (seconds) included in the HMAC payload; must be within ±5 min.
    pub timestamp: i64,
    /// The new password in plain text; included in the HMAC so the signature is
    /// bound to this specific password value.
    #[validate(length(min = 8, max = 128))]
    pub new_password: String,
    #[validate(must_match(other = "new_password"))]
    pub new_password_confirm: String,
    /// HMAC-SHA256( signing_key, "magnolia-reset|{timestamp}|{new_password}" ) - base64.
    pub signature: String,
}

/// Response for GET /api/auth/me/password-reset-key - the key file the user downloads.
#[derive(Debug, Serialize)]
pub struct PasswordResetKeyResponse {
    /// The user's local user_id, included so the reset form can pre-fill it.
    pub user_id: String,
    pub username: String,
    /// Raw signing key bytes encoded as standard base64.
    pub key: String,
    /// ISO-8601 timestamp of when this key was (re-)generated.
    pub generated_at: String,
}

/// Status response for GET /api/auth/me/password-reset-key/status
#[derive(Debug, Serialize)]
pub struct PasswordResetKeyStatus {
    pub has_key: bool,
}

/// Change password while logged in
#[derive(Debug, Deserialize, Validate)]
pub struct ChangePasswordRequest {
    pub current_password: String,

    #[validate(length(min = 8, max = 128))]
    pub new_password: String,

    #[validate(must_match(other = "new_password"))]
    pub new_password_confirm: String,
}

// Address and Billing DTOs
#[derive(Debug, Deserialize, Validate)]
pub struct CreateAddressRequest {
    #[validate(length(min = 1, max = 100))]
    pub street_line1: String,

    #[validate(length(max = 100))]
    pub street_line2: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub city: String,

    #[validate(length(max = 100))]
    pub state_province: Option<String>,

    #[validate(length(min = 1, max = 20))]
    pub postal_code: String,

    #[validate(length(min = 2, max = 2))]
    pub country: String,

    #[validate(length(min = 1, max = 20))]
    pub phone: String,

    #[validate(length(max = 500))]
    pub delivery_instructions: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateBillingRequest {
    #[validate(length(min = 1, max = 100))]
    pub full_name: String,

    #[validate(length(max = 50))]
    pub tax_code: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub street_line1: String,

    #[validate(length(max = 100))]
    pub street_line2: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub city: String,

    #[validate(length(max = 100))]
    pub state_province: Option<String>,

    #[validate(length(min = 1, max = 20))]
    pub postal_code: String,

    #[validate(length(min = 2, max = 2))]
    pub country: String,
}
