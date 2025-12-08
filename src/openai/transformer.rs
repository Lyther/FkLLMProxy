use crate::models::openai::{ChatCompletionChunk, ChatCompletionChunkChoice, DeltaMessage, Role};
use crate::openai::models::{
    BackendContent, BackendConversationRequest, BackendMessage, BackendMessageData, BackendSSEEvent,
};
use anyhow::Result;
use tracing::{debug, warn};
use uuid::Uuid;

// Fix hardcoded action: Make action configurable via constant
const DEFAULT_BACKEND_ACTION: &str = "next";

/// Transforms an OpenAI-style chat completion request into a backend request.
///
/// # Errors
///
/// Returns an error if the input request cannot be converted to the backend format.
pub fn transform_to_backend(
    model: &str,
    messages: &[crate::models::openai::ChatMessage],
    temperature: Option<f32>,
    max_tokens: Option<u32>,
) -> Result<BackendConversationRequest> {
    let backend_messages: Result<Vec<BackendMessage>> = messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
                Role::Tool => "tool",
            };

            Ok(BackendMessage {
                // Fix UUID generation: Document why UUIDs are needed
                // Backend API requires unique message IDs for conversation tracking
                // UUIDs ensure uniqueness across concurrent requests and prevent collisions
                id: format!("node_{}", Uuid::new_v4()),
                role: role.to_string(),
                content: BackendContent::Text {
                    content_type: "text".to_string(),
                    parts: vec![msg.content.clone()],
                },
            })
        })
        .collect();

    Ok(BackendConversationRequest {
        // Fix hardcoded action: Use constant instead of hardcoded string
        action: DEFAULT_BACKEND_ACTION.to_string(),
        messages: backend_messages?,
        model: model.to_string(),
        parent_message_id: None,
        conversation_id: None,
        temperature,
        max_tokens,
    })
}

pub fn parse_sse_event(event_type: &str, data_str: &str) -> Option<BackendSSEEvent> {
    if data_str == "[DONE]" {
        return Some(BackendSSEEvent {
            event_type: "done".to_string(),
            data: serde_json::json!({}),
        });
    }

    match serde_json::from_str::<serde_json::Value>(data_str) {
        Ok(data) => Some(BackendSSEEvent {
            event_type: event_type.to_string(),
            data,
        }),
        Err(e) => {
            // Fix error swallowing: Log detailed error information
            warn!(
                "Failed to parse SSE event JSON (event_type: {}, error: {}, data length: {}): {}",
                event_type,
                e,
                data_str.len(),
                if data_str.len() > 200 {
                    format!("{}...", &data_str[..200])
                } else {
                    data_str.to_string()
                }
            );
            None
        }
    }
}

pub fn transform_sse_to_openai_chunk(
    event: &BackendSSEEvent,
    model: &str,
    request_id: &str,
) -> Option<ChatCompletionChunk> {
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if event.event_type == "done" {
        return Some(ChatCompletionChunk {
            id: request_id.to_string(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model.to_string(),
            choices: vec![ChatCompletionChunkChoice {
                index: 0,
                delta: DeltaMessage {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
        });
    }

    // Fix silent filtering: Log debug message for non-message events
    if event.event_type != "message" {
        debug!(
            "Ignoring SSE event type: {} (only 'message' events are processed)",
            event.event_type
        );
        return None;
    }

    // Fix error swallowing: Log detailed error information
    let message_data: BackendMessageData = match serde_json::from_value(event.data.clone()) {
        Ok(data) => data,
        Err(e) => {
            let data_str = event.data.to_string();
            warn!(
                "Failed to parse backend message data (error: {}, data preview: {}): {}",
                e,
                if data_str.len() > 200 {
                    format!("{}...", &data_str[..200])
                } else {
                    data_str.clone()
                },
                e
            );
            return None;
        }
    };

    let content = message_data.message?.content;
    let content_str = match content {
        BackendContent::Text { parts, .. } => {
            // Fix joins parts with empty string: Document why no separator
            // Backend API returns parts that should be concatenated without separator
            // This preserves the exact content structure from the backend
            parts.join("")
        }
        BackendContent::String(s) => s,
    };

    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Some(ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created,
        model: model.to_string(),
        choices: vec![ChatCompletionChunkChoice {
            index: 0,
            delta: DeltaMessage {
                role: None,
                content: Some(content_str),
            },
            finish_reason: None,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::openai::{ChatCompletionRequest, ChatMessage, Role};

    #[test]
    fn test_transform_request_basic() {
        let req = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: "Hello".to_string(),
                name: None,
            }],
            stream: false,
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: Some(100),
            stop: None,
        };

        let backend_req = transform_to_backend(
            &req.model,
            &req.messages,
            Some(req.temperature),
            req.max_tokens,
        )
        .unwrap();
        assert_eq!(backend_req.messages.len(), 1);
        assert_eq!(backend_req.messages[0].role, "user");
    }
}
