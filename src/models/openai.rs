use serde::{Deserialize, Serialize};
use std::result::Result;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: Role,
    #[serde(deserialize_with = "deserialize_content")]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

fn deserialize_content<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Content {
        String(String),
        Array(Vec<serde_json::Value>),
    }

    let content = Content::deserialize(deserializer)?;
    match content {
        Content::String(s) => Ok(s),
        Content::Array(arr) => {
            // Fix content deserialization limitation: Document that we only support text content
            // Multimodal content (images, etc.) is not supported - only extracts "text" fields
            // This is a known limitation of the current implementation
            let parts: Vec<String> = arr
                .into_iter()
                .filter_map(|v| {
                    // Extract text field if present (for text content)
                    v.get("text")
                        .and_then(|t| t.as_str())
                        .map(std::string::ToString::to_string)
                        // Fallback: if value is a string, use it directly
                        .or_else(|| v.as_str().map(std::string::ToString::to_string))
                })
                .collect();
            // Fix: Document that joining with "\n" may not be correct for all content types
            // For text-only content, newline separator is appropriate
            // For multimodal content, this would lose structure - limitation documented
            Ok(parts.join("\n"))
        }
    }
}

fn deserialize_stop<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stop {
        String(String),
        Array(Vec<String>),
        None,
    }

    let stop = Option::<Stop>::deserialize(deserializer)?;
    match stop {
        Some(Stop::String(s)) => Ok(Some(vec![s])),
        Some(Stop::Array(arr)) => Ok(Some(arr)),
        Some(Stop::None) | None => Ok(None),
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    pub max_tokens: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_stop")]
    pub stop: Option<Vec<String>>,
}

impl ChatCompletionRequest {
    /// Validates the chat completion request parameters.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - Model field is empty
    /// - Messages array is empty
    /// - Temperature is outside the valid range [0, 2]
    /// - Top-p is outside the valid range [0, 1]
    /// - Max tokens is 0 or negative
    pub fn validate(&self) -> Result<(), String> {
        // Validate model name
        if self.model.is_empty() {
            return Err("model field cannot be empty".to_string());
        }

        // Validate messages
        if self.messages.is_empty() {
            return Err("messages field cannot be empty".to_string());
        }

        // Validate temperature range (0-2)
        if self.temperature < 0.0 || self.temperature > 2.0 {
            return Err(format!(
                "temperature must be between 0 and 2, got {}",
                self.temperature
            ));
        }

        // Validate top_p range (0-1)
        if self.top_p < 0.0 || self.top_p > 1.0 {
            return Err(format!("top_p must be between 0 and 1, got {}", self.top_p));
        }

        // Validate max_tokens
        if let Some(max) = self.max_tokens {
            if max == 0 {
                return Err("max_tokens must be greater than 0".to_string());
            }
        }

        Ok(())
    }
}

fn default_temperature() -> f32 {
    1.0
}

fn default_top_p() -> f32 {
    1.0
}

#[derive(Debug, Serialize, Clone)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ChatCompletionChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatCompletionChunkChoice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatCompletionChunkChoice {
    pub index: u32,
    pub delta: DeltaMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeltaMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_stop_string() {
        let json = r#"{
            "model": "test",
            "messages": [],
            "stop": "stop"
        }"#;
        let req: ChatCompletionRequest =
            serde_json::from_str(json).expect("chat completion request should deserialize");
        assert_eq!(req.stop, Some(vec!["stop".to_string()]));
    }

    #[test]
    fn test_deserialize_stop_array() {
        let json = r#"{
            "model": "test",
            "messages": [],
            "stop": ["stop1", "stop2"]
        }"#;
        let req: ChatCompletionRequest =
            serde_json::from_str(json).expect("chat completion request should deserialize");
        assert_eq!(
            req.stop,
            Some(vec!["stop1".to_string(), "stop2".to_string()])
        );
    }

    #[test]
    fn test_deserialize_stop_null() {
        let json = r#"{
            "model": "test",
            "messages": [],
            "stop": null
        }"#;
        let req: ChatCompletionRequest =
            serde_json::from_str(json).expect("chat completion request should deserialize");
        assert_eq!(req.stop, None);
    }

    #[test]
    fn test_deserialize_content_array() {
        let json = r#"{
            "role": "user",
            "content": [{"type": "text", "text": "hello"}, {"type": "text", "text": "world"}]
        }"#;
        let msg: ChatMessage = serde_json::from_str(json).expect("chat message should deserialize");
        assert_eq!(msg.content, "hello\nworld");
    }
}
