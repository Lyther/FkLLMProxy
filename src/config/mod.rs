use config::{Config, ConfigError};
use serde::Deserialize;
use std::env;
use std::fs;
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

/// Configuration for the Gemini CLI provider.
///
/// Enables integration with Google's Gemini CLI for local AI processing.
/// Requires `gemini` CLI to be installed and authenticated.
#[derive(Debug, Deserialize, Clone, Validate)]
pub struct GeminiCliConfig {
    #[serde(default = "default_gemini_cli_enabled")]
    pub enabled: bool,
    pub cli_path: Option<String>,
    #[serde(default = "default_gemini_cli_timeout")]
    #[validate(range(min = 1))]
    pub timeout_secs: u64,
    #[serde(default = "default_gemini_cli_max_concurrency")]
    #[validate(range(min = 1))]
    pub max_concurrency: usize,
}

impl Default for GeminiCliConfig {
    fn default() -> Self {
        Self {
            enabled: default_gemini_cli_enabled(),
            cli_path: None,
            timeout_secs: default_gemini_cli_timeout(),
            max_concurrency: default_gemini_cli_max_concurrency(),
        }
    }
}

fn default_gemini_cli_enabled() -> bool {
    false
}

fn default_gemini_cli_timeout() -> u64 {
    30
}

fn default_gemini_cli_max_concurrency() -> usize {
    4
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
    #[serde(default)]
    #[validate(nested)]
    pub gemini_cli: GeminiCliConfig,
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

fn parse_port(value: &str) -> Result<u16, ConfigError> {
    // Parse as i64 first to catch negative numbers
    let port_i64 = value.parse::<i64>().map_err(|e| {
        ConfigError::Message(format!(
            "Invalid port value '{value}': {e}. Port must be a number between 1 and 65535."
        ))
    })?;

    // Explicitly validate range before conversion to u16
    if !(1..=65535).contains(&port_i64) {
        return Err(ConfigError::Message(format!(
            "Port value '{port_i64}' is out of range. Port must be between 1 and 65535."
        )));
    }

    // Safe to convert: we've validated the range 1..=65535
    u16::try_from(port_i64).map_err(|_| {
        ConfigError::Message("Port value out of u16 range (this should not happen)".into())
    })
}

fn non_empty_env_var(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn extract_project_id_from_credentials(path: &str) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    json.get("project_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

fn resolve_project_id(
    configured_project_id: Option<String>,
    credentials_path: Option<&str>,
) -> Option<String> {
    // Priority: explicit config > GOOGLE_CLOUD_PROJECT env > credentials file
    if let Some(pid) = configured_project_id {
        if !pid.is_empty() {
            return Some(pid);
        }
    }

    if let Ok(env_pid) = env::var("GOOGLE_CLOUD_PROJECT") {
        if !env_pid.is_empty() {
            return Some(env_pid);
        }
    }

    if let Some(path) = credentials_path {
        return extract_project_id_from_credentials(path);
    }

    None
}

fn load_env_file() {
    if let Err(e) = dotenvy::dotenv() {
        tracing::debug!("Failed to load .env file (this is optional): {}", e);
    }
}

fn build_config_from_sources() -> Result<AppConfig, ConfigError> {
    Config::builder()
        .set_default("server.host", "127.0.0.1")?
        .set_default("server.port", 4000)?
        .set_default(
            "server.max_request_size",
            i64::try_from(DEFAULT_MAX_REQUEST_SIZE).unwrap_or(i64::MAX),
        )?
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
        .set_override_option("vertex.api_key", non_empty_env_var("GOOGLE_API_KEY"))?
        .set_override_option(
            "vertex.project_id",
            non_empty_env_var("APP_VERTEX__PROJECT_ID")
                .or_else(|| non_empty_env_var("GOOGLE_CLOUD_PROJECT")),
        )?
        .build()?
        .try_deserialize()
}

fn normalize_vertex_config(config: &mut AppConfig) {
    if config
        .vertex
        .api_key
        .as_deref()
        .is_some_and(|k| k.trim().is_empty())
    {
        config.vertex.api_key = None;
    }

    if config
        .vertex
        .project_id
        .as_deref()
        .is_some_and(|p| p.trim().is_empty())
    {
        config.vertex.project_id = None;
    }
}

fn validate_config_values(config: &AppConfig) -> Result<(), ConfigError> {
    if let Err(e) = config.validate() {
        return Err(ConfigError::Message(format!("Validation error: {e}")));
    }
    Ok(())
}

fn validate_auth_config(config: &AppConfig) -> Result<(), ConfigError> {
    if config.auth.require_auth && config.auth.master_key.is_empty() {
        return Err(ConfigError::Message(
            "APP_AUTH__MASTER_KEY is required when APP_AUTH__REQUIRE_AUTH=true".into(),
        ));
    }
    if config.auth.require_auth && config.auth.master_key.len() < 16 {
        return Err(ConfigError::Message(
            "APP_AUTH__MASTER_KEY must be at least 16 characters long when APP_AUTH__REQUIRE_AUTH=true"
                .into(),
        ));
    }
    Ok(())
}

fn ensure_vertex_credentials(
    config: &AppConfig,
    credentials_path_env: Option<&str>,
) -> Result<(), ConfigError> {
    let has_credentials_file = config
        .vertex
        .credentials_file
        .as_ref()
        .is_some_and(|f| std::path::Path::new(f).exists());
    let has_env_credentials = credentials_path_env.is_some();

    if config.vertex.api_key.is_none() && !has_credentials_file && !has_env_credentials {
        return Err(ConfigError::Message(
            "Missing configuration: Must provide either GOOGLE_API_KEY or GOOGLE_APPLICATION_CREDENTIALS or APP_VERTEX__CREDENTIALS_FILE".into()
        ));
    }

    if config.vertex.api_key.is_none() {
        let credentials_path = config
            .vertex
            .credentials_file
            .as_deref()
            .or(credentials_path_env);

        if resolve_project_id(config.vertex.project_id.clone(), credentials_path).is_none() {
            return Err(ConfigError::Message(
                "Missing project ID for Vertex service account authentication: set APP_VERTEX__PROJECT_ID or GOOGLE_CLOUD_PROJECT, or provide credentials containing project_id"
                    .into(),
            ));
        }
    }

    Ok(())
}

impl AppConfig {
    /// Create a new application configuration from environment variables.
    ///
    /// Loads configuration from environment variables with APP_ prefix.
    /// Falls back to sensible defaults for missing values.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if:
    /// - Configuration parsing fails
    /// - Validation fails (invalid ranges, missing required values)
    /// - Authentication configuration is invalid
    /// - Port values are out of valid range
    pub fn new() -> Result<Self, ConfigError> {
        load_env_file();

        let mut config = build_config_from_sources()?;

        normalize_vertex_config(&mut config);
        validate_config_values(&config)?;
        validate_auth_config(&config)?;

        let credentials_path_env = env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
        ensure_vertex_credentials(&config, credentials_path_env.as_deref())?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_errors_without_project_id_for_service_account() {
        let creds_path = std::env::temp_dir().join(format!(
            "creds-missing-project-{}.json",
            uuid::Uuid::new_v4()
        ));
        fs::write(&creds_path, r#"{"type":"service_account"}"#)
            .expect("failed to write temp credentials file");

        temp_env::with_vars(
            [
                ("GOOGLE_API_KEY", Some("")),
                (
                    "GOOGLE_APPLICATION_CREDENTIALS",
                    Some(
                        creds_path
                            .to_str()
                            .expect("temp path should be valid UTF-8"),
                    ),
                ),
                ("GOOGLE_CLOUD_PROJECT", Some("")),
                ("APP_VERTEX__PROJECT_ID", Some("")),
                ("APP_AUTH__REQUIRE_AUTH", Some("false")),
            ],
            || {
                let result = AppConfig::new();
                assert!(
                    result.is_err(),
                    "config creation should fail without project id for service account"
                );
                let err = format!("{}", result.unwrap_err());
                assert!(
                    err.contains("project ID"),
                    "error message should mention missing project id, got: {err}"
                );
            },
        );

        let _ = std::fs::remove_file(&creds_path);
    }

    #[test]
    fn app_config_accepts_project_id_from_env() {
        let creds_path = std::env::temp_dir().join(format!(
            "creds-with-env-project-{}.json",
            uuid::Uuid::new_v4()
        ));
        fs::write(&creds_path, r#"{"type":"service_account"}"#)
            .expect("failed to write temp credentials file");

        temp_env::with_vars(
            [
                ("GOOGLE_API_KEY", Some("")),
                (
                    "GOOGLE_APPLICATION_CREDENTIALS",
                    Some(
                        creds_path
                            .to_str()
                            .expect("temp path should be valid UTF-8"),
                    ),
                ),
                ("GOOGLE_CLOUD_PROJECT", Some("test-project")),
                ("APP_VERTEX__PROJECT_ID", Some("")),
                ("APP_AUTH__REQUIRE_AUTH", Some("false")),
            ],
            || {
                let config = AppConfig::new().expect("config should load when project id provided");
                assert_eq!(
                    config.vertex.project_id.as_deref(),
                    Some("test-project"),
                    "project id should be sourced from GOOGLE_CLOUD_PROJECT"
                );
            },
        );

        let _ = std::fs::remove_file(&creds_path);
    }
}
