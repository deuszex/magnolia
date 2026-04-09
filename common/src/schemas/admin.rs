use serde::{Deserialize, Serialize};
use validator::Validate;

/// Full admin view of a user — includes status flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminUserListItem {
    pub user_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub username: String,
    pub avatar_url: Option<String>,
    pub verified: bool,
    pub admin: bool,
    pub active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminUserListResponse {
    pub users: Vec<AdminUserListItem>,
    pub total: i64,
}

/// Query parameters for admin user list
#[derive(Debug, Deserialize)]
pub struct AdminUserListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub q: Option<String>,
}

fn default_limit() -> i64 {
    50
}

/// Create a user directly (admin, bypasses email verification flow)
#[derive(Debug, Deserialize, Validate)]
pub struct AdminCreateUserRequest {
    #[validate(length(min = 3, max = 30))]
    pub username: String,

    pub email: Option<String>,

    #[validate(length(min = 8, max = 128))]
    pub password: String,

    #[serde(default)]
    pub admin: bool,

    /// Mark the account as email-verified immediately (defaults to true)
    #[serde(default = "default_true")]
    pub verified: bool,
}

fn default_true() -> bool {
    true
}

/// Update a user's flags (active / admin)
#[derive(Debug, Deserialize)]
pub struct AdminUpdateUserRequest {
    pub active: Option<bool>,
    pub admin: Option<bool>,
}

/// Create an invite link
#[derive(Debug, Deserialize)]
pub struct CreateInviteRequest {
    /// Optionally bind the invite to a specific email address
    pub email: Option<String>,
    /// Expiry in hours — defaults to 168 (7 days)
    pub expires_hours: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteResponse {
    pub invite_id: String,
    pub token: String,
    pub email: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub expires_at: String,
    pub used_at: Option<String>,
    pub used_by_user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteListResponse {
    pub invites: Vec<InviteResponse>,
    pub total: i64,
}

/// Query parameters for invite list
#[derive(Debug, Deserialize)]
pub struct InviteListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

/// Send invites via email to a list of addresses
#[derive(Debug, Deserialize)]
pub struct SendEmailInvitesRequest {
    /// List of email addresses to invite
    pub emails: Vec<String>,
    /// Expiry in hours — defaults to 168 (7 days)
    pub expires_hours: Option<i64>,
    /// Optional personal message to include in the invitation email
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SendEmailInvitesResponse {
    pub sent: Vec<String>,
    pub failed: Vec<String>,
}

// Registration applications

/// Submit a registration application (public endpoint, application mode only)
#[derive(Debug, Deserialize, Validate)]
pub struct SubmitApplicationRequest {
    #[validate(email)]
    pub email: String,

    #[validate(length(max = 50))]
    pub display_name: Option<String>,

    #[validate(length(max = 1000))]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationResponse {
    pub application_id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub message: Option<String>,
    pub status: String,
    pub created_at: String,
    pub expires_at: String,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
    pub is_expired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationListResponse {
    pub applications: Vec<ApplicationResponse>,
    pub total: i64,
}

#[derive(Debug, Deserialize)]
pub struct ApplicationListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    /// Filter by status: 'pending', 'approved', 'denied', 'expired' — omit for all
    pub status: Option<String>,
}

/// Returned when an application is approved and an account is created
#[derive(Debug, Serialize)]
pub struct ApproveApplicationResponse {
    pub user_id: String,
    pub email: String,
    /// Password setup link — only present when SMTP is not configured (admin must share manually)
    pub setup_link: Option<String>,
    pub email_sent: bool,
}
