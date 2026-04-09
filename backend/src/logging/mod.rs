pub mod config;
pub mod context;
pub mod events;

pub use config::{LogFormat, LogOutput, LoggingConfig};
pub use context::RequestContext;
pub use events::SecurityEvent;

use std::io;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the logging system with XDR-compatible JSON output
pub fn init_logging(config: &LoggingConfig) -> Result<(), String> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("magnolia=info,magnolia_common=info,tower_http=info,security=error")
    });

    let registry = tracing_subscriber::registry().with(env_filter);

    match (&config.format, &config.output) {
        (LogFormat::Json, LogOutput::Stdout) => {
            let json_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_file(config.include_source_location)
                .with_line_number(config.include_source_location)
                .with_thread_ids(false)
                .with_thread_names(false);

            registry.with(json_layer).init();
        }
        (LogFormat::Json, LogOutput::File) => {
            let file_appender = create_file_appender(config)?;
            let json_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_file(config.include_source_location)
                .with_line_number(config.include_source_location)
                .with_writer(file_appender);

            registry.with(json_layer).init();
        }
        (LogFormat::Json, LogOutput::Both) => {
            let file_appender = create_file_appender(config)?;

            let stdout_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_file(config.include_source_location)
                .with_line_number(config.include_source_location);

            let file_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_file(config.include_source_location)
                .with_line_number(config.include_source_location)
                .with_writer(file_appender)
                .with_ansi(false);

            registry.with(stdout_layer).with(file_layer).init();
        }
        (LogFormat::Pretty, LogOutput::Stdout) => {
            let pretty_layer = tracing_subscriber::fmt::layer().pretty().with_target(true);

            registry.with(pretty_layer).init();
        }
        (LogFormat::Pretty, LogOutput::File) => {
            let file_appender = create_file_appender(config)?;
            let layer = tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_writer(file_appender)
                .with_ansi(false);

            registry.with(layer).init();
        }
        (LogFormat::Pretty, LogOutput::Both) => {
            let file_appender = create_file_appender(config)?;

            let stdout_layer = tracing_subscriber::fmt::layer().pretty().with_target(true);

            let file_layer = tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_writer(file_appender)
                .with_ansi(false);

            registry.with(stdout_layer).with(file_layer).init();
        }
    }

    Ok(())
}

fn create_file_appender(config: &LoggingConfig) -> Result<RollingFileAppender, String> {
    let (dir, prefix) = match &config.file_path {
        Some(path) => {
            let path = std::path::Path::new(path);
            let dir = path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            let prefix = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "magnolia.log".to_string());
            (dir, prefix)
        }
        None => (".".to_string(), "magnolia.log".to_string()),
    };

    Ok(RollingFileAppender::new(Rotation::DAILY, dir, prefix))
}
