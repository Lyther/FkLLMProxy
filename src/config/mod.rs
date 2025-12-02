use config::{Config, ConfigError};
use serde::Deserialize;
use std::env;
use validator::Validate;

const DEFAULT_MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024;
const DEFAULT_ACCESS_TOKEN_TTL_SECS: u64 = 3600;
const DEFAULT_ARKOSE_TOKEN_TTL_SECS: u64 = 120;
const DEFAULT_CACHE_TTL_SECS: u64 = 3600;

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct ServerConfig {
    #[validate(length(min = 1))]
    pub host: String,
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,
    #[serde(default = "default_max_request_size")]
    pub max_request_size: usize,
}

fn default_max_request_size() -> usize {
    DEFAULT_MAX_REQUEST_SIZE
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
    pub credentials_file: Option<String>,
    #[validate(length(min = 1))]
    pub api_key_base_url: Option<String>,
    #[validate(length(min = 1))]
    pub oauth_base_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Validate)]
pub struct LogConfig {
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
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
    DEFAULT_CACHE_TTL_SECS
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

fn parse_bool(value: &str) -> bool {
    let lower = value.to_lowercase();
    matches!(lower.as_str(), "true" | "1" | "yes" | "on")
}

fn parse_port(value: &str) -> Result<i64, ConfigError> {
    value.parse::<i64>().map_err(|e| {
        ConfigError::Message(format!(
            "Invalid port value '{}': {}. Port must be a number between 1 and 65535.",
            value, e
        ))
    })
}

impl AppConfig {
    pub fn new() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let s = Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 4000)?
            .set_default("server.max_request_size", DEFAULT_MAX_REQUEST_SIZE as i64)?
            .set_default("auth.require_auth", false)?
            .set_default("auth.master_key", "")?
            .set_default("vertex.region", "us-central1")?
            .set_default("log.level", "info")?
            .set_default("log.format", "pretty")?
            .set_default("openai.harvester_url", "http://localhost:3001")?
            .set_default(
                "openai.access_token_ttl_secs",
                DEFAULT_ACCESS_TOKEN_TTL_SECS,
            )?
            .set_default(
                "openai.arkose_token_ttl_secs",
                DEFAULT_ARKOSE_TOKEN_TTL_SECS,
            )?
            .set_default("anthropic.bridge_url", "http://localhost:4001")?
            .set_default("rate_limit.capacity", 100)?
            .set_default("rate_limit.refill_per_second", 10)?
            .set_default("circuit_breaker.failure_threshold", 10)?
            .set_default("circuit_breaker.timeout_secs", 60)?
            .set_default("circuit_breaker.success_threshold", 3)?
            .set_default("cache.enabled", false)?
            .set_default("cache.default_ttl_secs", DEFAULT_CACHE_TTL_SECS)?
            .add_source(
                config::Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true),
            )
            .add_source(
                config::Environment::with_prefix("GOOGLE")
                    .separator("_")
                    .try_parsing(true),
            )
            .set_override_option("server.host", env::var("APP_SERVER__HOST").ok())?
            .set_override_option(
                "server.port",
                env::var("APP_SERVER__PORT")
                    .ok()
                    .map(|v| parse_port(&v))
                    .transpose()?,
            )?
            .set_override_option(
                "auth.require_auth",
                env::var("APP_AUTH__REQUIRE_AUTH")
                    .ok()
                    .map(|v| parse_bool(&v)),
            )?
            .set_override_option("auth.master_key", env::var("APP_AUTH__MASTER_KEY").ok())?
            .set_override_option("vertex.api_key", env::var("GOOGLE_API_KEY").ok())?
            .build()?;

        let config: AppConfig = s.try_deserialize()?;

        if let Err(e) = config.validate() {
            return Err(ConfigError::Message(format!("Validation error: {}", e)));
        }

        if config.auth.require_auth && config.auth.master_key.is_empty() {
            return Err(ConfigError::Message(
                "APP_AUTH__MASTER_KEY is required when APP_AUTH__REQUIRE_AUTH=true".into(),
            ));
        }

        if config.vertex.api_key.is_none()
            && config.vertex.project_id.is_none()
            && env::var("GOOGLE_APPLICATION_CREDENTIALS").is_err()
        {
            return Err(ConfigError::Message(
                "Missing configuration: Must provide either GOOGLE_API_KEY or (APP_VERTEX__PROJECT_ID + GOOGLE_APPLICATION_CREDENTIALS)".into()
            ));
        }

        Ok(config)
    }
}
