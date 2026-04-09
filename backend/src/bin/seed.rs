//! Test data seeder.
//!
//! Seeds the database with sample users, posts, comments, conversations,
//! and messages for local development and testing.
//!
//! Usage:
//! cargo run-p magnolia_server--bin seed

use magnolia_server::database;
use sqlx::Executor;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:./magnolia.db".to_string());

    println!("Seeding database: {}", database_url);

    let pool = database::create_pool(&database_url)
        .await
        .expect("Failed to connect to database");

    // user_accounts
    // psswords are all "password123" hashed with argon2
    let password_hash = hash_password("password123");
    let now = chrono::Utc::now().to_rfc3339();

    let users = vec![
        ("Primary", "prim@ary.com", 1, 1),     // admin, verified
        ("Secondary", "second@ary.com", 1, 0), // verified
        ("Tertiary", "terti@ary.com", 1, 0),   // verified
    ];

    for (id, email, verified, admin) in &users {
        let res = pool
            .execute(
                sqlx::query(
                    "INSERT OR IGNORE INTO user_accounts \
 (user_id, email, username, password_hash, verified, admin, active, created_at, updated_at) \
 VALUES ($1, $2, $1, $3, $4, $5, 1, $6, $6)",
                )
                .bind(id)
                .bind(email)
                .bind(&password_hash)
                .bind(verified)
                .bind(admin)
                .bind(&now),
            )
            .await
            .expect("Failed to insert user");
        if res.rows_affected() > 0 {
            println!("- User: {} ({})", email, id);
        } else {
            println!(" = User: {} already exists", email);
        }
    }

    // user_profiles
    let profiles: Vec<(&str, &str, &str, Option<&str>)> = vec![
        (
            "Primary",
            "Primary",
            "Primary",
            Some("https://www.youtube.com/watch?v=iR2TEgs8qnA"),
        ),
        ("Secondary", "Secondary", "Secondary", None),
        ("Tertiary", "Tertiary", "Tertiary", None),
    ];

    for (user_id, display_name, username, bio) in &profiles {
        pool.execute(
 sqlx::query(
 "UPDATE user_accounts SET display_name = $1, username = $2, bio = $3, updated_at = $4 WHERE user_id = $5",
 )
 .bind(display_name)
 .bind(username)
 .bind(bio)
 .bind(&now)
 .bind(user_id),
 )
 .await
 .expect("Failed to update user profile");
        println!("- Profile: {} ({})", display_name, user_id);
    }

    // posts
    let posts = vec![(
        "post-0001",
        "Primary",
        1,
        "Magnolia is a large genus of about 210 to 340 flowering plant species in the subfamily Magnolioideae of the family Magnoliaceae. The natural range of Magnolia species is disjunct, with a main center in east, south and southeast Asia and a secondary center in South America, Central America, the West Indies, and some species in eastern North America.",
    )];

    for (post_id, author_id, published, text_content) in &posts {
        let content_id = format!("{}-content", post_id);
        let res = pool
            .execute(
                sqlx::query(
                    "INSERT OR IGNORE INTO posts \
 (post_id, author_id, is_published, created_at, updated_at) \
 VALUES ($1, $2, $3, $4, $4)",
                )
                .bind(post_id)
                .bind(author_id)
                .bind(published)
                .bind(&now),
            )
            .await
            .expect("Failed to insert post");

        if res.rows_affected() > 0 {
            pool.execute(
                sqlx::query(
                    "INSERT OR IGNORE INTO post_contents \
 (content_id, post_id, content_type, display_order, content, created_at) \
 VALUES ($1, $2, 'text', 0, $3, $4)",
                )
                .bind(&content_id)
                .bind(post_id)
                .bind(text_content)
                .bind(&now),
            )
            .await
            .expect("Failed to insert post content");
            println!("- Post: {} by {}", post_id, author_id);
        } else {
            println!(" = Post: {} already exists", post_id);
        }
    }

    // post-tags

    let post_tags: Vec<(&str, &str)> = vec![
        ("post-0001", "welcome"),
        ("post-0001", "introduction"),
        ("post-0001", "magnolia"),
    ];

    for (post_id, tag) in &post_tags {
        let res = pool
            .execute(
                sqlx::query("INSERT OR IGNORE INTO post_tags (post_id, tag) VALUES ($1, $2)")
                    .bind(post_id)
                    .bind(tag),
            )
            .await
            .expect("Failed to insert post tag");
        if res.rows_affected() > 0 {
            println!("- Tag: {} on {}", tag, post_id);
        }
    }

    // post-comments

    let comments = vec![(
        "comment-0001",
        "post-0001",
        "Primary",
        None::<&str>,
        "Yes, the quote is straight from Wikipedia.",
    )];

    for (cid, post_id, author_id, parent, content) in &comments {
        let res = pool
 .execute(
 sqlx::query(
 "INSERT OR IGNORE INTO comments \
 (comment_id, post_id, author_id, parent_comment_id, content_type, content, is_deleted, created_at, updated_at) \
 VALUES ($1, $2, $3, $4, 'text', $5, 0, $6, $6)",
 )
 .bind(cid)
 .bind(post_id)
 .bind(author_id)
 .bind(parent)
 .bind(content)
 .bind(&now),
 )
 .await
 .expect("Failed to insert comment");
        if res.rows_affected() > 0 {
            println!("- Comment: {} on {}", cid, post_id);
        }
    }

    // messaging preferences (incomplete feature)
    for (id, _, _, _) in &users {
        pool.execute(
            sqlx::query(
                "INSERT OR IGNORE INTO messaging_preferences \
 (user_id, accept_messages, created_at, updated_at) \
 VALUES ($1, 1, $2, $2)",
            )
            .bind(id)
            .bind(&now),
        )
        .await
        .expect("Failed to insert messaging preferences");
    }
    println!("- Messaging preferences for all users");

    // conversations
    let dm_id = "Yoooooooooo";
    let res = pool
        .execute(
            sqlx::query(
                "INSERT OR IGNORE INTO conversations \
 (conversation_id, conversation_type, name, created_by, created_at, updated_at) \
 VALUES ($1, 'direct', NULL, $2, $3, $3)",
            )
            .bind(dm_id)
            .bind("Primary")
            .bind(&now),
        )
        .await
        .expect("Failed to insert DM conversation");
    if res.rows_affected() > 0 {
        for (member_id, uid, role) in [
            ("member-dm-1", "Primary", "owner"),
            ("member-dm-2", "Secondary", "member"),
        ] {
            pool.execute(
                sqlx::query(
                    "INSERT OR IGNORE INTO conversation_members \
 (id, conversation_id, user_id, role, joined_at) \
 VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(member_id)
                .bind(dm_id)
                .bind(uid)
                .bind(role)
                .bind(&now),
            )
            .await
            .expect("Failed to insert DM member");
        }
    }

    // group chat
    let group_id = "Org";
    let res = pool
        .execute(
            sqlx::query(
                "INSERT OR IGNORE INTO conversations \
 (conversation_id, conversation_type, name, created_by, created_at, updated_at) \
 VALUES ($1, 'group', 'The Crew', $2, $3, $3)",
            )
            .bind(group_id)
            .bind("Primary")
            .bind(&now),
        )
        .await
        .expect("Failed to insert group conversation");
    if res.rows_affected() > 0 {
        for (member_id, uid, role) in [
            ("member-grp-1", "Primary", "owner"),
            ("member-grp-2", "Secondary", "member"),
            ("member-grp-3", "Tertiary", "member"),
        ] {
            pool.execute(
                sqlx::query(
                    "INSERT OR IGNORE INTO conversation_members \
 (id, conversation_id, user_id, role, joined_at) \
 VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(member_id)
                .bind(group_id)
                .bind(uid)
                .bind(role)
                .bind(&now),
            )
            .await
            .expect("Failed to insert group member");
        }
    }

    println!();
    println!("Done! Test data seeded.");
    println!();
    println!("Test accounts (password: password123):");
    println!("- prim@ary.com (admin, verified)");
    println!("- second@ary.com (member, verified)");
    println!("- terti@ary.com (member, verified)");
}

fn hash_password(password: &str) -> String {
    use argon2::{
        Argon2, PasswordHasher,
        password_hash::{SaltString, rand_core::OsRng},
    };
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("Failed to hash password")
        .to_string()
}
