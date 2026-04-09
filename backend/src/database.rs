use chrono::Utc;
use sqlx::{AnyPool, Error, Executor};
use std::path::Path;

/// Create a database connection pool and run migrations
pub async fn create_pool(database_url: &str) -> Result<AnyPool, Error> {
    sqlx::any::install_default_drivers();

    // SQLite: ensure the database file exists before connecting
    if database_url.starts_with("sqlite:") {
        let path_str = database_url
            .trim_start_matches("sqlite://")
            .trim_start_matches("sqlite:");
        let db_path = Path::new(path_str);
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    sqlx::Error::Configuration(
                        format!("Failed to create database directory: {}", e).into(),
                    )
                })?;
            }
        }
        if !db_path.exists() {
            std::fs::File::create(db_path).map_err(|e| {
                sqlx::Error::Configuration(format!("Failed to create database file: {}", e).into())
            })?;
        }
    }

    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;

    run_migrations(&pool).await?;

    Ok(pool)
}

/// Run SQL migration files in order, tracking applied migrations to stay idempotent.
async fn run_migrations(pool: &AnyPool) -> Result<(), Error> {
    // Enable foreign keys for SQLite
    let _ = pool.execute("PRAGMA foreign_keys = ON").await;

    // Migration tracking table — created unconditionally so we can track state
    pool.execute(
        "CREATE TABLE IF NOT EXISTS _migrations \
 (name TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
    )
    .await?;

    let migrations: &[(&str, &str)] = &[
        (
            "001_user_accounts",
            include_str!("../migrations/001_user_accounts.sql"),
        ),
        (
            "002_sessions",
            include_str!("../migrations/002_sessions.sql"),
        ),
        (
            "003_email_verifications",
            include_str!("../migrations/003_email_verifications.sql"),
        ),
        (
            "004_password_resets",
            include_str!("../migrations/004_password_resets.sql"),
        ),
        ("005_invites", include_str!("../migrations/005_invites.sql")),
        (
            "006_register_applications",
            include_str!("../migrations/006_register_applications.sql"),
        ),
        ("007_posts", include_str!("../migrations/007_posts.sql")),
        (
            "008_post_contents",
            include_str!("../migrations/008_post_contents.sql"),
        ),
        (
            "009_post_tags",
            include_str!("../migrations/009_post_tags.sql"),
        ),
        (
            "010_comments",
            include_str!("../migrations/010_comments.sql"),
        ),
        ("011_media", include_str!("../migrations/011_media.sql")),
        (
            "012_favourites",
            include_str!("../migrations/012_favourites.sql"),
        ),
        (
            "013_theme_settings",
            include_str!("../migrations/013_theme_settings.sql"),
        ),
        (
            "014_site_config",
            include_str!("../migrations/014_site_config.sql"),
        ),
        (
            "015_email_settings",
            include_str!("../migrations/015_email_settings.sql"),
        ),
        (
            "016_email_logs",
            include_str!("../migrations/016_email_logs.sql"),
        ),
        (
            "017_site_pages",
            include_str!("../migrations/017_site_pages.sql"),
        ),
        (
            "018_conversations",
            include_str!("../migrations/018_conversations.sql"),
        ),
        (
            "019_conversation_members",
            include_str!("../migrations/019_conversation_members.sql"),
        ),
        (
            "020_messages",
            include_str!("../migrations/020_messages.sql"),
        ),
        (
            "021_message_deliveries",
            include_str!("../migrations/021_message_deliveries.sql"),
        ),
        (
            "022_messaging_preferences",
            include_str!("../migrations/022_messaging_preferences.sql"),
        ),
        (
            "023_user_blocks",
            include_str!("../migrations/023_user_blocks.sql"),
        ),
        (
            "024_conversation_favourites",
            include_str!("../migrations/024_conversation_favourites.sql"),
        ),
        (
            "025_message_attachments",
            include_str!("../migrations/025_message_attachments.sql"),
        ),
        (
            "026_conversation_backgrounds",
            include_str!("../migrations/026_conversation_backgrounds.sql"),
        ),
        (
            "027_audit_logs",
            include_str!("../migrations/027_audit_logs.sql"),
        ),
        ("028_calls", include_str!("../migrations/028_calls.sql")),
        (
            "029_call_participants",
            include_str!("../migrations/029_call_participants.sql"),
        ),
        (
            "030_link_previews",
            include_str!("../migrations/030_link_previews.sql"),
        ),
        (
            "031_global_call_participants",
            include_str!("../migrations/031_global_call_participants.sql"),
        ),
        (
            "032_server_identity",
            include_str!("../migrations/032_server_identity.sql"),
        ),
        (
            "033_server_connections",
            include_str!("../migrations/033_server_connections.sql"),
        ),
        (
            "034_discovery_hints",
            include_str!("../migrations/034_discovery_hints.sql"),
        ),
        (
            "035_federation_users",
            include_str!("../migrations/035_federation_users.sql"),
        ),
        (
            "036_user_federation_settings",
            include_str!("../migrations/036_user_federation_settings.sql"),
        ),
        (
            "037_user_federation_rules",
            include_str!("../migrations/037_user_federation_rules.sql"),
        ),
        (
            "038_federation_post_refs",
            include_str!("../migrations/038_federation_post_refs.sql"),
        ),
        (
            "039_user_external_bans",
            include_str!("../migrations/039_user_external_bans.sql"),
        ),
        (
            "040_cross_server_groups",
            include_str!("../migrations/040_cross_server_groups.sql"),
        ),
        (
            "041_cross_server_group_members",
            include_str!("../migrations/041_cross_server_group_members.sql"),
        ),
        (
            "042_cross_server_message_queue",
            include_str!("../migrations/042_cross_server_message_queue.sql"),
        ),
        (
            "043_federated_conversation_members",
            include_str!("../migrations/043_federated_conversation_members.sql"),
        ),
        (
            "044_user_events",
            include_str!("../migrations/044_user_events.sql"),
        ),
        (
            "045_stun_servers",
            include_str!("../migrations/045_stun_servers.sql"),
        ),
    ];
    println!("Pushing migrations into DB:");
    for (name, migration) in migrations {
        println!("- {name}");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations WHERE name = $1")
            .bind(*name)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

        if count > 0 {
            continue;
        }

        for statement in migration.split(';') {
            let trimmed = statement.trim();
            let has_sql = trimmed.lines().any(|line| {
                let l = line.trim();
                !l.is_empty() && !l.starts_with("--")
            });
            if !has_sql {
                continue;
            }
            pool.execute(trimmed).await?;
        }

        sqlx::query("INSERT INTO _migrations (name, applied_at) VALUES ($1, $2)")
            .bind(*name)
            .bind(Utc::now().to_rfc3339())
            .execute(pool)
            .await?;
    }

    Ok(())
}
