pub mod config;
pub mod crypto;
pub mod database;
pub mod embedded;
pub mod events;
pub mod federation;
pub mod handlers;
pub mod logging;
pub mod middleware;
pub mod proxy_rate;
pub mod routes;
pub mod service;
pub mod services;
pub mod turn;
pub mod utils;

use std::net::SocketAddr;
use std::sync::Arc;

use utils::encryption::ContentEncryption;

/// Main server logic - shared between console and service modes
pub async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();
    // On Windows, also try the installer-generated config file so that manual
    // runs of the binary (outside the service) pick up the correct settings.
    #[cfg(windows)]
    dotenvy::from_path(r"C:\ProgramData\Magnolia\magnolia.env").ok();

    // Initialize XDR-compatible logging
    let log_config = logging::LoggingConfig::from_env();
    logging::init_logging(&log_config)
        .map_err(|e| format!("Failed to initialize logging: {}", e))?;

    // Load settings
    let settings = Arc::new(config::Settings::from_env()?);
    tracing::info!("Settings loaded successfully");

    // Create database pool and run migrations
    let pool = database::create_pool(&settings.database_url).await?;
    let db_type = if settings.database_url.starts_with("postgres") {
        "PostgreSQL"
    } else {
        "SQLite"
    };
    tracing::info!("Database connected and migrations applied ({})", db_type);

    // Terminate any calls that were active when the server last stopped.
    // This prevents users from being stuck in a call that no longer exists.
    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "UPDATE call_participants SET status = 'left', left_at = $1 WHERE status = 'joined'",
    )
    .bind(&now)
    .execute(&pool)
    .await;
    let _ = sqlx::query("UPDATE call_participants SET status = 'missed' WHERE status = 'ringing'")
        .execute(&pool)
        .await;
    let _ = sqlx::query(
        "UPDATE calls SET status = 'ended', ended_at = $1, duration_seconds = 0
         WHERE status IN ('ringing', 'active')",
    )
    .bind(&now)
    .execute(&pool)
    .await;
    tracing::info!("Stale calls cleared on startup");

    // Start STUN server health-check service
    services::StunHealthCheckService::new(pool.clone()).start();
    tracing::info!("STUN health-check service started");

    // Initialize audit service for security logging (auth + admin endpoints)
    let audit_service =
        middleware::AuditService::new(middleware::AuditConfig::default(), pool.clone());
    tracing::info!("Security audit service initialized");

    // Start embedded TURN server (if enabled)
    let turn_config =
        turn::TurnConfig::from_env().map_err(|e| format!("TURN configuration error: {e}"))?;
    turn::start_turn_server(&turn_config).await;

    // Check whether first-run setup is still needed (no users in DB).
    // If setup is already done the routes are omitted entirely so the
    // endpoint is permanently unavailable, not just guarded at handler level.
    let setup_required = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM user_accounts WHERE user_id != '__fed__' AND user_id != '__proxy__'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0)
        == 0;

    // Build optional at-rest encryption (needed for federation identity storage)
    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .map(ContentEncryption::from_hex_key)
        .transpose()
        .map_err(|e| format!("Invalid ENCRYPTION_AT_REST_KEY: {:?}", e))?;

    // Load or generate the server's ML-DSA-87 identity (used for S2S signing)
    let identity = Arc::new(
        federation::identity::load_or_generate(&pool, enc.as_ref())
            .await
            .map_err(|e| format!("Failed to load server identity: {}", e))?,
    );
    tracing::info!("Server identity loaded");

    let s2s_client = federation::client::build_client();

    // Initialise the persistent S2S WebSocket hub.
    let hub_registry = federation::hub::new_registry();
    let hub_status = federation::hub::new_status_map();
    federation::hub::init_global(hub_registry.clone(), hub_status.clone());
    tokio::spawn(federation::hub::start_hub(
        pool.clone(),
        Arc::clone(&settings),
        Arc::clone(&identity),
        hub_registry.clone(),
        hub_status.clone(),
    ));

    // Create router
    let app = routes::create_router(
        pool,
        settings.clone(),
        Some(audit_service),
        setup_required,
        identity,
        s2s_client,
        hub_registry,
        hub_status,
    );

    let addr = format!("{}:{}", settings.host, settings.port);
    let public_listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server running on: {addr}");
    if let Ok(local_port) = std::env::var("LOCAL_PORT") {
        let local_addr = format!("127.0.0.1:{}", local_port);
        match tokio::net::TcpListener::bind(&local_addr).await {
            Ok(private_listener) => {
                let private_serve = axum::serve(private_listener, app.clone().into_make_service())
                    .with_graceful_shutdown(async {
                        tokio::signal::ctrl_c().await.ok();
                    });

                let public_serve = axum::serve(
                    public_listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    tokio::signal::ctrl_c().await.ok();
                    tracing::info!("Shutdown signal received — notifying federation peers");
                    federation::hub::broadcast_going_offline().await;
                });
                tracing::info!("Local routes running on: {local_addr}");
                tokio::try_join!(public_serve, private_serve)?;
            }
            Err(e) => {
                tracing::warn!(
                    "Could not bind LOCAL_PORT {local_addr}: {e} - running public listener only"
                );
                axum::serve(
                    public_listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    tokio::signal::ctrl_c().await.ok();
                    tracing::info!("Shutdown signal received, notifying federation peers");
                    federation::hub::broadcast_going_offline().await;
                })
                .await?;
            }
        }
    } else {
        axum::serve(
            public_listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("Shutdown signal received — notifying federation peers");
            federation::hub::broadcast_going_offline().await;
        })
        .await?;
    }

    Ok(())
}
