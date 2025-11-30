use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arkose_token: Option<String>,
    pub expires_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HealthResponse {
    pub browser_alive: bool,
    pub session_valid: bool,
    pub last_token_refresh: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackendConversationRequest {
    pub action: String,
    pub messages: Vec<BackendMessage>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackendMessage {
    pub id: String,
    pub role: String,
    pub content: BackendContent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum BackendContent {
    Text {
        content_type: String,
        parts: Vec<String>,
    },
    String(String),
}

#[derive(Debug, Deserialize, Clone)]
pub struct BackendSSEEvent {
    #[serde(rename = "event")]
    pub event_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BackendMessageData {
    pub message: Option<BackendMessageResponse>,
    pub conversation_id: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BackendMessageResponse {
    pub id: String,
    pub content: BackendContent,
    pub role: Option<String>,
}
