use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use reqwest::StatusCode;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::{
    io::{self, AsyncBufReadExt, BufReader},
    sync::oneshot,
};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use vertex_bridge::config::AppConfig;
use vertex_bridge::handlers::{chat, health, metrics};
use vertex_bridge::middleware::{
    api_version::api_version_middleware,
    auth::auth_middleware,
    rate_limit::{rate_limit_middleware, RateLimiter},
    security_headers::security_headers_middleware,
};
use vertex_bridge::openai::circuit_breaker::CircuitBreaker;
use vertex_bridge::openai::metrics::Metrics;
use vertex_bridge::services::auth::TokenManager;
use vertex_bridge::services::cache::Cache;
use vertex_bridge::services::providers::ProviderRegistry;
use vertex_bridge::state::AppState;

type ServicesInit = (
    TokenManager,
    RateLimiter,
    Arc<CircuitBreaker>,
    Arc<Metrics>,
    Arc<ProviderRegistry>,
    Arc<Cache>,
);

type LogReloadHandle =
    tracing_subscriber::reload::Handle<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>;

struct CommandResult {
    message: String,
    shutdown: bool,
}

#[derive(Clone)]
struct CliContext {
    state: AppState,
    log_handle: Option<LogReloadHandle>,
}

fn parse_command(input: &str) -> (&str, Vec<&str>) {
    let trimmed = input.trim();
    let mut parts = trimmed.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    let args: Vec<&str> = parts.collect();
    (cmd, args)
}

fn format_provider_status(state: &AppState) -> String {
    let mut lines = Vec::new();

    // Vertex (Gemini via Vertex/AI Studio)
    let vertex_status = if state.config.vertex.api_key.is_some() {
        "api_key"
    } else if state.config.vertex.credentials_file.is_some()
        || std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok()
    {
        "service_account"
    } else {
        "unauthenticated"
    };
    lines.push(format!(
        "Vertex (gemini-*) - status: {vertex_status}, region: {}",
        state.config.vertex.region
    ));

    // Anthropic bridge
    lines.push(format!(
        "Anthropic (claude-*) - bridge URL: {}",
        state.config.anthropic.bridge_url
    ));

    // OpenAI via harvester
    lines.push(format!(
        "OpenAI (gpt-*) - harvester URL: {}",
        state.config.openai.harvester_url
    ));

    // Gemini CLI
    lines.push(format!(
        "Gemini CLI (gemini-*, local) - enabled: {}, path: {}",
        state.config.gemini_cli.enabled,
        state
            .config
            .gemini_cli
            .cli_path
            .clone()
            .unwrap_or_else(|| "gemini".to_string())
    ));

    lines.join("\n")
}

fn list_supported_models() -> String {
    [
        ("gpt-*", "OpenAI via Harvester backend"),
        ("gemini-*", "Google Vertex/AI Studio or Gemini CLI"),
        ("claude-*", "Anthropic Bridge"),
    ]
    .iter()
    .map(|(prefix, desc)| format!("{prefix} -> {desc}"))
    .collect::<Vec<_>>()
    .join("\n")
}

async fn fetch_local(
    ctx: &CliContext,
    path: &str,
    require_auth: bool,
) -> Result<(StatusCode, String), String> {
    let base = format!(
        "http://{}:{}",
        ctx.state.config.server.host, ctx.state.config.server.port
    );
    let url = format!("{base}{path}");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let mut req = client.get(&url);
    if require_auth
        && ctx.state.config.auth.require_auth
        && !ctx.state.config.auth.master_key.is_empty()
    {
        req = req.bearer_auth(&ctx.state.config.auth.master_key);
    }

    let res = req
        .send()
        .await
        .map_err(|e| format!("Request to {url} failed: {e}"))?;
    let status = res.status();
    let body = res
        .text()
        .await
        .unwrap_or_else(|e| format!("Failed to read body: {e}"));
    Ok((status, body))
}

fn command_help(args: &[&str]) -> CommandResult {
    let verbose = args.first().is_some_and(|v| *v == "verbose");
    let message = if verbose {
        serde_json::json!({
            "commands": [
                "/help [verbose]",
                "/status",
                "/models [filter]",
                "/providers|/proxies",
                "/health",
                "/metrics",
                "/rate-limit",
                "/cache stats|clear",
                "/circuit",
                "/logs level <trace|debug|info|warn|error>",
                "/reload",
                "/connections",
                "/test <model> <text>",
                "/quit"
            ]
        })
        .to_string()
    } else {
        "/help - show commands\n/status - show service status\n/models [filter] - list supported model prefixes\n/providers - show provider/proxy configuration\n/health - call local health endpoint\n/metrics - fetch metrics summary\n/rate-limit - show rate limiter stats\n/cache stats|clear - show or clear cache\n/circuit - show circuit breaker status\n/logs level <level> - change log level\n/reload - validate config reload (dry-run)\n/connections - check backend reachability\n/test <model> <text> - send a local probe request\n/quit - stop the service"
            .to_string()
    };

    CommandResult {
        message,
        shutdown: false,
    }
}

fn command_status(ctx: &CliContext) -> CommandResult {
    let providers = ctx
        .state
        .provider_registry
        .list_providers()
        .iter()
        .map(|p| format!("{p:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let provider_summary = if providers.is_empty() {
        "none".to_string()
    } else {
        providers
    };

    CommandResult {
        message: format!(
            "Service status:\n- Address: {}:{}\n- Auth required: {}\n- Providers: {}",
            ctx.state.config.server.host,
            ctx.state.config.server.port,
            ctx.state.config.auth.require_auth,
            provider_summary
        ),
        shutdown: false,
    }
}

fn command_models(args: &[&str]) -> CommandResult {
    let filter = args.first().map(|s| s.to_lowercase());
    let listing = list_supported_models();
    let filtered = if let Some(f) = filter {
        listing
            .lines()
            .filter(|l| l.to_lowercase().contains(&f))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        listing
    };

    CommandResult {
        message: filtered,
        shutdown: false,
    }
}

fn command_providers(ctx: &CliContext) -> CommandResult {
    CommandResult {
        message: format_provider_status(&ctx.state),
        shutdown: false,
    }
}

async fn command_health(ctx: &CliContext) -> CommandResult {
    match fetch_local(ctx, "/health", false).await {
        Ok((status, body)) => CommandResult {
            message: format!("GET /health -> {status}\n{body}"),
            shutdown: false,
        },
        Err(e) => CommandResult {
            message: e,
            shutdown: false,
        },
    }
}

async fn command_metrics(ctx: &CliContext) -> CommandResult {
    match fetch_local(ctx, "/metrics", true).await {
        Ok((status, body)) => {
            if status.is_success() {
                match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(v) => {
                        let get_f64 = |key: &str| -> f64 {
                            v.get(key)
                                .and_then(serde_json::Value::as_f64)
                                .unwrap_or(0.0)
                        };
                        let get_u64 = |key: &str| -> u64 {
                            v.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0)
                        };
                        CommandResult {
                            message: format!(
                                "Metrics: total={} failed={} success_rate={:.2}% avg_latency_ms={:.2} cache_hit_rate={:.2}%",
                                get_u64("total_requests"),
                                get_u64("failed_requests"),
                                get_f64("success_rate"),
                                get_f64("avg_latency_ms"),
                                get_f64("cache_hit_rate")
                            ),
                            shutdown: false,
                        }
                    }
                    Err(_) => CommandResult {
                        message: format!("Metrics response {status}: {body}"),
                        shutdown: false,
                    },
                }
            } else {
                CommandResult {
                    message: format!("Metrics request failed: {status} {body}"),
                    shutdown: false,
                }
            }
        }
        Err(e) => CommandResult {
            message: e,
            shutdown: false,
        },
    }
}

async fn command_rate_limit(ctx: &CliContext) -> CommandResult {
    let stats = ctx.state.rate_limiter.stats().await;
    CommandResult {
        message: format!(
            "Rate limiter: capacity={}, refill_per_second={}, active_keys={}",
            stats.capacity, stats.refill_per_second, stats.active_keys
        ),
        shutdown: false,
    }
}

async fn command_cache(args: &[&str], ctx: &CliContext) -> CommandResult {
    if matches!(args.first(), Some(cmd) if *cmd == "clear") {
        ctx.state.cache.clear().await;
        return CommandResult {
            message: "Cache cleared".to_string(),
            shutdown: false,
        };
    }

    let stats = ctx.state.cache.stats().await;
    CommandResult {
        message: format!(
            "Cache: enabled={}, total_entries={}, active_entries={}, expired_entries={}",
            stats.enabled, stats.total_entries, stats.active_entries, stats.expired_entries
        ),
        shutdown: false,
    }
}

async fn command_circuit(ctx: &CliContext) -> CommandResult {
    let stats = ctx.state.circuit_breaker.stats().await;
    CommandResult {
        message: format!(
            "Circuit breaker: state={:?}, failures={}/{}, successes={}/{}, timeout={}s",
            stats.state,
            stats.failure_count,
            stats.failure_threshold,
            stats.success_count,
            stats.success_threshold,
            stats.timeout_secs
        ),
        shutdown: false,
    }
}

fn command_logs(args: &[&str], ctx: &CliContext) -> CommandResult {
    if args.len() == 2 && args[0] == "level" {
        let level = args[1].to_lowercase();
        if let Some(handle) = &ctx.log_handle {
            let filter_str = format!("{level},tower_http=debug");
            return match EnvFilter::try_new(filter_str) {
                Ok(filter) => {
                    if handle.reload(filter).is_ok() {
                        CommandResult {
                            message: format!("Log level set to {level}"),
                            shutdown: false,
                        }
                    } else {
                        CommandResult {
                            message: "Failed to update log level".to_string(),
                            shutdown: false,
                        }
                    }
                }
                Err(e) => CommandResult {
                    message: format!("Invalid log level: {e}"),
                    shutdown: false,
                },
            };
        }

        return CommandResult {
            message: "Log level reload not available in this build".to_string(),
            shutdown: false,
        };
    }

    CommandResult {
        message: "Usage: /logs level <trace|debug|info|warn|error>".to_string(),
        shutdown: false,
    }
}

fn command_reload() -> CommandResult {
    match AppConfig::new() {
        Ok(new_config) => CommandResult {
            message: format!(
                "Config reload validated (not applied): host {}:{}, auth_required={}, region={}",
                new_config.server.host,
                new_config.server.port,
                new_config.auth.require_auth,
                new_config.vertex.region
            ),
            shutdown: false,
        },
        Err(e) => CommandResult {
            message: format!("Config reload failed: {e}"),
            shutdown: false,
        },
    }
}

async fn command_connections(ctx: &CliContext) -> CommandResult {
    let mut lines = Vec::new();

    let harvester = &ctx.state.config.openai.harvester_url;
    lines.push(format!(
        "Harvester: {}",
        check_url(harvester).await.unwrap_or_else(|e| e)
    ));

    let bridge = &ctx.state.config.anthropic.bridge_url;
    lines.push(format!(
        "Anthropic bridge: {}",
        check_url(bridge).await.unwrap_or_else(|e| e)
    ));

    let cli_path = ctx
        .state
        .config
        .gemini_cli
        .cli_path
        .clone()
        .unwrap_or_else(|| "gemini".to_string());
    let cli_status = tokio::fs::metadata(&cli_path).await.map_or_else(
        |e| format!("Gemini CLI path check failed ({cli_path}): {e}"),
        |_| format!("Gemini CLI path ok: {cli_path}"),
    );
    lines.push(cli_status);

    CommandResult {
        message: lines.join("\n"),
        shutdown: false,
    }
}

async fn command_test(args: &[&str], ctx: &CliContext) -> CommandResult {
    if args.len() < 2 {
        return CommandResult {
            message: "Usage: /test <model> <text>".to_string(),
            shutdown: false,
        };
    }

    let model = args[0];
    let text = args[1..].join(" ");
    match send_probe(ctx, model, &text).await {
        Ok(msg) => CommandResult {
            message: msg,
            shutdown: false,
        },
        Err(e) => CommandResult {
            message: e,
            shutdown: false,
        },
    }
}

fn command_quit() -> CommandResult {
    CommandResult {
        message: "Shutting down service...".to_string(),
        shutdown: true,
    }
}

fn command_unknown() -> CommandResult {
    CommandResult {
        message: "Unknown command. Type /help for a list of commands.".to_string(),
        shutdown: false,
    }
}

async fn process_command(input: &str, ctx: &CliContext) -> CommandResult {
    let (cmd, args) = parse_command(input);

    match cmd {
        "/help" | "help" => command_help(&args),
        "/status" | "status" => command_status(ctx),
        "/models" | "models" => command_models(&args),
        "/providers" | "providers" | "/proxies" | "proxies" => command_providers(ctx),
        "/health" | "health" => command_health(ctx).await,
        "/metrics" | "metrics" => command_metrics(ctx).await,
        "/rate-limit" | "rate-limit" => command_rate_limit(ctx).await,
        "/cache" | "cache" => command_cache(&args, ctx).await,
        "/circuit" | "circuit" => command_circuit(ctx).await,
        "/logs" | "logs" => command_logs(&args, ctx),
        "/reload" | "reload" => command_reload(),
        "/connections" | "connections" => command_connections(ctx).await,
        "/test" | "test" => command_test(&args, ctx).await,
        "/quit" | "/exit" | "quit" | "exit" => command_quit(),
        _ => command_unknown(),
    }
}

async fn check_url(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    match client.get(url).send().await {
        Ok(res) => Ok(format!("{url} -> {}", res.status())),
        Err(e) => Err(format!("{url} unreachable: {e}")),
    }
}

async fn send_probe(ctx: &CliContext, model: &str, text: &str) -> Result<String, String> {
    let url = format!(
        "http://{}:{}/v1/chat/completions",
        ctx.state.config.server.host, ctx.state.config.server.port
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let mut req = client.post(&url).json(&serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": text}],
        "stream": false,
        "max_tokens": 16
    }));
    if ctx.state.config.auth.require_auth && !ctx.state.config.auth.master_key.is_empty() {
        req = req.bearer_auth(&ctx.state.config.auth.master_key);
    }
    let res = req.send().await.map_err(|e| format!("Probe failed: {e}"))?;
    let status = res.status();
    let body = res
        .text()
        .await
        .unwrap_or_else(|e| format!("Failed to read probe body: {e}"));
    Ok(format!(
        "Probe {model:?} -> {status}\n{}",
        body.chars().take(400).collect::<String>()
    ))
}

async fn setup_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => Some(s),
            Err(e) => {
                warn!("Failed to register SIGTERM handler: {}", e);
                None
            }
        };

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, initiating graceful shutdown");
            }
            () = async {
                if let Some(ref mut sigterm) = sigterm {
                    if sigterm.recv().await.is_none() {
                        warn!("SIGTERM signal stream closed unexpectedly");
                    }
                }
            } => {
                info!("Received SIGTERM, initiating graceful shutdown");
            }
        }
    }
    #[cfg(not(unix))]
    {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to install Ctrl+C handler: {}", e);
            return;
        }
        info!("Received Ctrl+C, initiating graceful shutdown");
    }
}

fn setup_logging(config: &AppConfig) -> LogReloadHandle {
    let log_format = config.log.format.as_str();
    let filter = EnvFilter::try_new(format!(
        "{level},tower_http=debug",
        level = config.log.level
    ))
    .unwrap_or_else(|_| EnvFilter::new(&config.log.level));

    let (filter_layer, reload_handle) = tracing_subscriber::reload::Layer::new(filter);

    match log_format {
        "json" => {
            tracing_subscriber::registry()
                .with(filter_layer)
                .with(
                    tracing_subscriber::fmt::layer()
                        .json()
                        .with_target(false)
                        .with_file(true)
                        .with_line_number(true)
                        .with_current_span(true)
                        .with_span_list(true),
                )
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(filter_layer)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(true)
                        .with_file(true)
                        .with_line_number(true),
                )
                .init();
        }
    }

    reload_handle
}

fn initialize_services(config: &AppConfig) -> anyhow::Result<ServicesInit> {
    let token_manager = TokenManager::new(
        config.vertex.api_key.clone(),
        config.vertex.credentials_file.clone(),
        config.vertex.project_id.clone(),
    )
    .map_err(|e| {
        error!("Failed to initialize TokenManager: {e}");
        anyhow::anyhow!("TokenManager initialization failed: {e}")
    })?;

    let rate_limiter = RateLimiter::new(
        config.rate_limit.capacity,
        config.rate_limit.refill_per_second,
    );
    let circuit_breaker = Arc::new(CircuitBreaker::new(
        config.circuit_breaker.failure_threshold,
        config.circuit_breaker.timeout_secs,
        config.circuit_breaker.success_threshold,
    ));
    let metrics = Arc::new(Metrics::new());
    let provider_registry = Arc::new(ProviderRegistry::with_config(
        &Some(config.anthropic.bridge_url.clone()),
        &Some(config.gemini_cli.clone()),
    ));
    let cache = Arc::new(Cache::new(
        config.cache.enabled,
        config.cache.default_ttl_secs,
    ));

    Ok((
        token_manager,
        rate_limiter,
        circuit_breaker,
        metrics,
        provider_registry,
        cache,
    ))
}

fn create_app_router(config: &AppConfig, state: AppState, rate_limiter: RateLimiter) -> Router {
    let public_routes = Router::new().route("/health", get(health::health_check));

    let protected_routes = Router::new()
        .route("/metrics", get(metrics::metrics_handler))
        .route(
            "/metrics/prometheus",
            get(metrics::prometheus_metrics_handler),
        )
        .route("/v1/chat/completions", post(chat::chat_completions))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_middleware,
        ));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            config.server.max_request_size,
        ))
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(middleware::from_fn(api_version_middleware))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

async fn run_server(
    app: Router,
    host: &str,
    port: u16,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid server address {host}:{port}: {e}"))?;

    info!("Listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let shutdown = async move {
        tokio::select! {
            () = setup_shutdown_signal() => {},
            _ = &mut shutdown_rx => {},
        }
    };

    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown);

    if let Err(e) = server.await {
        error!("Server error: {e}");
        return Err(anyhow::anyhow!("Server failed: {e}"));
    }

    info!("Server shutdown complete");
    Ok(())
}

async fn run_command_loop(ctx: CliContext, shutdown_tx: oneshot::Sender<()>) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let mut shutdown_tx = Some(shutdown_tx);

    println!("Interactive CLI ready. Type /help for available commands.");

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let result = process_command(&line, &ctx).await;
        println!("{}", result.message);

        if result.shutdown {
            if let Some(tx) = shutdown_tx.take() {
                let _ = tx.send(());
            }
            break;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vertex_bridge::services::flags::FeatureFlags::init();

    let config = AppConfig::new()
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to load configuration: {e}. Please check your environment variables and configuration."
            )
        })?;

    let log_handle = Some(setup_logging(&config));

    info!("Starting Vertex Bridge v{}", env!("CARGO_PKG_VERSION"));
    info!(
        "Config loaded: Host={}, Port={}",
        config.server.host, config.server.port
    );

    let (token_manager, rate_limiter, circuit_breaker, metrics, provider_registry, cache) =
        initialize_services(&config)?;

    let state = AppState {
        config: Arc::new(config.clone()),
        token_manager,
        provider_registry,
        rate_limiter: rate_limiter.clone(),
        circuit_breaker,
        metrics,
        cache,
    };

    let app = create_app_router(&config, state.clone(), rate_limiter);

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let cli_context = CliContext {
        state: state.clone(),
        log_handle,
    };
    tokio::spawn(async move {
        if let Err(e) = run_command_loop(cli_context, shutdown_tx).await {
            warn!("CLI loop terminated with error: {e}");
        }
    });

    run_server(app, &config.server.host, config.server.port, shutdown_rx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_state() -> AppState {
        let config = AppConfig {
            server: vertex_bridge::config::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 4000,
                max_request_size: 1024 * 1024,
            },
            auth: vertex_bridge::config::AuthConfig {
                require_auth: false,
                master_key: "test".to_string(),
            },
            vertex: vertex_bridge::config::VertexConfig {
                project_id: None,
                region: "us-central1".to_string(),
                api_key: None,
                credentials_file: None,
                api_key_base_url: None,
                oauth_base_url: None,
            },
            log: vertex_bridge::config::LogConfig {
                level: "info".to_string(),
                format: "pretty".to_string(),
            },
            openai: vertex_bridge::config::OpenAIConfig {
                harvester_url: "http://localhost:3001".to_string(),
                access_token_ttl_secs: 3600,
                arkose_token_ttl_secs: 120,
            },
            anthropic: vertex_bridge::config::AnthropicConfig {
                bridge_url: "http://localhost:4001".to_string(),
            },
            gemini_cli: vertex_bridge::config::GeminiCliConfig::default(),
            rate_limit: vertex_bridge::config::RateLimitConfig {
                capacity: 100,
                refill_per_second: 10,
            },
            circuit_breaker: vertex_bridge::config::CircuitBreakerConfig {
                failure_threshold: 10,
                timeout_secs: 60,
                success_threshold: 3,
            },
            cache: vertex_bridge::config::CacheConfig {
                enabled: false,
                default_ttl_secs: 3600,
            },
        };

        let token_manager =
            TokenManager::new(None, None, None).expect("TokenManager should initialize for tests");
        let rate_limiter = RateLimiter::new(100, 10);
        let circuit_breaker = Arc::new(CircuitBreaker::new(10, 60, 3));
        let metrics = Arc::new(Metrics::new());
        let provider_registry = Arc::new(ProviderRegistry::with_config(&None, &None));
        let cache = Arc::new(Cache::new(false, 3600));

        AppState {
            config: Arc::new(config),
            token_manager,
            provider_registry,
            rate_limiter,
            circuit_breaker,
            metrics,
            cache,
        }
    }

    fn make_test_ctx() -> CliContext {
        CliContext {
            state: make_test_state(),
            log_handle: None,
        }
    }

    #[tokio::test]
    async fn command_help_returns_commands() {
        let ctx = make_test_ctx();
        let result = process_command("/help", &ctx).await;
        assert!(result.message.contains("/help"));
        assert!(!result.shutdown);
    }

    #[tokio::test]
    async fn command_quit_requests_shutdown() {
        let ctx = make_test_ctx();
        let result = process_command("/quit", &ctx).await;
        assert!(result.shutdown);
    }

    #[tokio::test]
    async fn command_models_lists_prefixes() {
        let ctx = make_test_ctx();
        let result = process_command("/models", &ctx).await;
        assert!(result.message.contains("gpt-*"));
        assert!(result.message.contains("gemini-*"));
        assert!(result.message.contains("claude-*"));
        assert!(!result.shutdown);
    }
}
