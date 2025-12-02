use serde::{Deserialize, Serialize};

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
            let parts: Vec<String> = arr
                .into_iter()
                .filter_map(|v| {
                    v.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| v.as_str().map(|s| s.to_string()))
                })
                .collect();
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
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.stop, Some(vec!["stop".to_string()]));
    }

    #[test]
    fn test_deserialize_stop_array() {
        let json = r#"{
            "model": "test",
            "messages": [],
            "stop": ["stop1", "stop2"]
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
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
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.stop, None);
    }

    #[test]
    fn test_deserialize_content_array() {
        let json = r#"{
            "role": "user",
            "content": [{"type": "text", "text": "hello"}, {"type": "text", "text": "world"}]
        }"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content, "hello\nworld");
    }
}
