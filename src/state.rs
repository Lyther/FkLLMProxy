use crate::config::AppConfig;
use crate::services::auth::TokenManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub token_manager: TokenManager,
}
