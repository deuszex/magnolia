//! Idempotent utility to create the initial administrator account.
//!
//! Connects directly to the database and inserts an admin user.
//! Safe to run multiple times — skips creation if any admin already exists.
//!
//! Usage:
//!   create_admin --email admin@example.com
//!       Prompts for the password interactively (input hidden).
//!
//!   create_admin --email admin@example.com --password-stdin
//!       Reads the password from stdin — use for scripts and installers:
//!       echo "s3cr3t" | create_admin --email admin@example.com --password-stdin

use magnolia_server::database;

#[tokio::main]
async fn main() {
    // Load from standard locations; failures are non-fatal
    dotenvy::dotenv().ok();
    dotenvy::from_path("/etc/magnolia/magnolia.env").ok();
    #[cfg(windows)]
    dotenvy::from_path(r"C:\ProgramData\Magnolia\magnolia.env").ok();

    let args: Vec<String> = std::env::args().collect();

    let email = get_arg(&args, "--email").unwrap_or_else(|| {
        eprintln!("Usage: create_admin --email <email> [--password-stdin]");
        eprintln!("  Password is prompted interactively unless --password-stdin is given.");
        std::process::exit(1);
    });

    let password_stdin = args.iter().any(|a| a == "--password-stdin");

    let password = if password_stdin {
        read_password_from_stdin()
    } else {
        prompt_password_interactive()
    };

    if password.is_empty() {
        eprintln!("Error: password must not be empty");
        std::process::exit(1);
    }

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:///var/lib/magnolia/magnolia.db".to_string());

    println!("Connecting to: {}", database_url);

    let pool = database::create_pool(&database_url)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error: Failed to connect to database: {}", e);
            std::process::exit(1);
        });

    // Idempotent: skip if any admin already exists
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_accounts WHERE admin = 1")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    if existing > 0 {
        println!("Admin account already exists.");
        return;
    }

    let password_hash =
        magnolia_server::utils::crypto::hash_password(&password).unwrap_or_else(|e| {
            eprintln!("Error: Failed to hash password: {:?}", e);
            std::process::exit(1);
        });

    let user_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO user_accounts \
         (user_id, email, password_hash, verified, admin, active, created_at, updated_at) \
         VALUES (?, ?, ?, 1, 1, 1, ?, ?)",
    )
    .bind(&user_id)
    .bind(&email)
    .bind(&password_hash)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Error: Failed to create admin account: {}", e);
        std::process::exit(1);
    });

    println!("Admin account created: {}", email);
}

fn read_password_from_stdin() -> String {
    use std::io::{self, BufRead};
    let stdin = io::stdin();
    stdin
        .lock()
        .lines()
        .next()
        .and_then(|l| l.ok())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn prompt_password_interactive() -> String {
    loop {
        let password = rpassword::prompt_password("Password: ").unwrap_or_else(|e| {
            eprintln!("Error reading password: {}", e);
            std::process::exit(1);
        });

        let confirm = rpassword::prompt_password("Confirm password: ").unwrap_or_else(|e| {
            eprintln!("Error reading password: {}", e);
            std::process::exit(1);
        });

        if password == confirm {
            return password;
        }

        eprintln!("Passwords do not match, try again.");
    }
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}
