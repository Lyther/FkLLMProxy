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
    #[serde(default = "default_max_request_size")]
    pub max_request_size: usize, // Maximum request body size in bytes
}

fn default_max_request_size() -> usize {
    10 * 1024 * 1024 // 10MB default
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
    #[validate(length(min = 1))]
    pub api_key_base_url: Option<String>, // Optional: override for testing/mocking
    #[validate(length(min = 1))]
    pub oauth_base_url: Option<String>, // Optional: override for testing/mocking
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct LogConfig {
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String, // "json" or "pretty"
}

fn default_log_format() -> String {
    "pretty".to_string()
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct OpenAIConfig {
    pub harvester_url: String,
    #[validate(range(min = 1))]
    pub access_token_ttl_secs: u64,
    #[validate(range(min = 1))]
    pub arkose_token_ttl_secs: u64,
    #[serde(default = "default_tls_fingerprint_enabled")]
    pub tls_fingerprint_enabled: bool,
    #[serde(default = "default_tls_fingerprint_target")]
    pub tls_fingerprint_target: String, // e.g., "chrome120", "firefox120"
}

fn default_tls_fingerprint_enabled() -> bool {
    false
}

fn default_tls_fingerprint_target() -> String {
    "chrome120".to_string()
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct AnthropicConfig {
    #[validate(length(min = 1))]
    pub bridge_url: String,
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct RateLimitConfig {
    #[validate(range(min = 1))]
    pub capacity: u32,
    #[validate(range(min = 1))]
    pub refill_per_second: u32,
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct CircuitBreakerConfig {
    #[validate(range(min = 1))]
    pub failure_threshold: u32,
    #[validate(range(min = 1))]
    pub timeout_secs: u64,
    #[validate(range(min = 1))]
    pub success_threshold: u32,
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct CacheConfig {
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
    #[validate(range(min = 1))]
    #[serde(default = "default_cache_ttl")]
    pub default_ttl_secs: u64,
}

fn default_cache_enabled() -> bool {
    false
}

fn default_cache_ttl() -> u64 {
    3600 // 1 hour
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
    #[validate(nested)]
    pub openai: OpenAIConfig,
    #[validate(nested)]
    pub anthropic: AnthropicConfig,
    #[validate(nested)]
    pub rate_limit: RateLimitConfig,
    #[validate(nested)]
    pub circuit_breaker: CircuitBreakerConfig,
    #[validate(nested)]
    pub cache: CacheConfig,
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
            .set_default("server.max_request_size", 10_485_760i64)? // 10MB in bytes
            .set_default("auth.require_auth", false)?
            .set_default("auth.master_key", "")?
            .set_default("vertex.region", "us-central1")?
            .set_default("log.level", "info")?
            .set_default("log.format", "pretty")?
            .set_default("openai.harvester_url", "http://localhost:3001")?
            .set_default("openai.access_token_ttl_secs", 3600)?
            .set_default("openai.arkose_token_ttl_secs", 120)?
            .set_default("openai.tls_fingerprint_enabled", false)?
            .set_default("openai.tls_fingerprint_target", "chrome120")?
            .set_default("anthropic.bridge_url", "http://localhost:4001")?
            .set_default("rate_limit.capacity", 100)?
            .set_default("rate_limit.refill_per_second", 10)?
            .set_default("circuit_breaker.failure_threshold", 10)?
            .set_default("circuit_breaker.timeout_secs", 60)?
            .set_default("circuit_breaker.success_threshold", 3)?
            .set_default("cache.enabled", false)?
            .set_default("cache.default_ttl_secs", 3600)?
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
