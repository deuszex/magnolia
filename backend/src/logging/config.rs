use std::env;

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Output format: json or pretty
    pub format: LogFormat,
    /// Where to send logs: stdout, file, or both
    pub output: LogOutput,
    /// Path for file output (optional)
    pub file_path: Option<String>,
    /// Include source file and line number in logs
    pub include_source_location: bool,
    /// Service name for log context
    pub service_name: String,
    /// Service version for log context
    pub service_version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat {
    Json,
    Pretty,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogOutput {
    Stdout,
    File,
    Both,
}

impl LoggingConfig {
    /// Create logging config from environment variables
    pub fn from_env() -> Self {
        let format = match env::var("LOG_FORMAT")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "pretty" => LogFormat::Pretty,
            _ => LogFormat::Json,
        };

        let output = match env::var("LOG_OUTPUT")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "file" => LogOutput::File,
            "both" => LogOutput::Both,
            _ => LogOutput::Stdout,
        };

        Self {
            format,
            output,
            file_path: env::var("LOG_FILE_PATH").ok(),
            include_source_location: env::var("LOG_INCLUDE_SOURCE")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            service_name: "magnolia".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Create a default development configuration (pretty stdout)
    pub fn development() -> Self {
        Self {
            format: LogFormat::Pretty,
            output: LogOutput::Stdout,
            file_path: None,
            include_source_location: true,
            service_name: "magnolia".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Create a default production configuration (JSON stdout)
    pub fn production() -> Self {
        Self {
            format: LogFormat::Json,
            output: LogOutput::Stdout,
            file_path: None,
            include_source_location: false,
            service_name: "magnolia".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}
