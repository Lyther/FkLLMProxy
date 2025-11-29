use config::{Config, ConfigError};
use serde::Deserialize;
use std::env;
use validator::Validate;

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct ServerConfig {
    #[validate(length(min = 1))]
    pub host: String,
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub require_auth: bool,
    pub master_key: String,
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct VertexConfig {
    pub project_id: Option<String>,
    pub region: String,
    pub api_key: Option<String>,
    pub credentials_file: Option<String>, // Added for explicit passing
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct LogConfig {
    pub level: String,
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct AppConfig {
    #[validate(nested)]
    pub server: ServerConfig,
    pub auth: AuthConfig,
    #[validate(nested)]
    pub vertex: VertexConfig,
    #[validate(nested)]
    pub log: LogConfig,
}

impl AppConfig {
    pub fn new() -> Result<Self, ConfigError> {
        // 1. Load .env file if present (dotenvy)
        dotenvy::dotenv().ok();

        let _run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            // Start with default values
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 4000)?
            .set_default("auth.require_auth", false)?
            .set_default("auth.master_key", "")?
            .set_default("vertex.region", "us-central1")?
            .set_default("log.level", "info")?
            // Load from Environment Variables
            // APP_SERVER__PORT=4000 maps to server.port
            .add_source(
                config::Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true),
            )
            // Map specific Google env vars
            // GOOGLE_API_KEY -> vertex.api_key
            .add_source(
                config::Environment::with_prefix("GOOGLE")
                    .separator("_")
                    .try_parsing(true),
            )
            // Explicit overrides for env vars (config crate can be finicky with case)
            .set_override_option("server.host", env::var("APP_SERVER__HOST").ok())?
            .set_override_option(
                "server.port",
                env::var("APP_SERVER__PORT")
                    .ok()
                    .and_then(|v| v.parse::<i64>().ok()),
            )?
            .set_override_option(
                "auth.require_auth",
                env::var("APP_AUTH__REQUIRE_AUTH")
                    .ok()
                    .map(|v| v.to_lowercase() == "true"),
            )?
            .set_override_option("auth.master_key", env::var("APP_AUTH__MASTER_KEY").ok())?
            .set_override_option("vertex.api_key", env::var("GOOGLE_API_KEY").ok())?
            .build()?;

        let config: AppConfig = s.try_deserialize()?;

        // 2. Validate
        if let Err(e) = config.validate() {
            return Err(ConfigError::Message(format!("Validation error: {}", e)));
        }

        // 3. Custom Logic Validation
        if config.auth.require_auth && config.auth.master_key.is_empty() {
            return Err(ConfigError::Message(
                "APP_AUTH__MASTER_KEY is required when APP_AUTH__REQUIRE_AUTH=true".into(),
            ));
        }

        if config.vertex.api_key.is_none() && config.vertex.project_id.is_none() {
            // Check if GOOGLE_APPLICATION_CREDENTIALS is set in env if not in config
            if env::var("GOOGLE_APPLICATION_CREDENTIALS").is_err() {
                return Err(ConfigError::Message(
                    "Missing configuration: Must provide either GOOGLE_API_KEY or (APP_VERTEX__PROJECT_ID + GOOGLE_APPLICATION_CREDENTIALS)".into()
                ));
            }
        }

        Ok(config)
    }
}
