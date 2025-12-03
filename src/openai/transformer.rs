use crate::models::openai::{ChatCompletionChunk, ChatCompletionChunkChoice, DeltaMessage, Role};
use crate::openai::models::{
    BackendContent, BackendConversationRequest, BackendMessage, BackendMessageData, BackendSSEEvent,
};
use anyhow::Result;
use tracing::warn;
use uuid::Uuid;

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
        action: "next".to_string(),
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
            warn!("Failed to parse SSE event JSON: {} - data: {}", e, data_str);
            None
        }
    }
}

pub fn transform_sse_to_openai_chunk(
    event: &BackendSSEEvent,
    model: &str,
    request_id: &str,
) -> Option<ChatCompletionChunk> {
    // Fix timestamp overflow: clamp timestamp to prevent overflow
    let timestamp = chrono::Utc::now().timestamp();
    let created = timestamp.max(0) as u64;

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

    if event.event_type != "message" {
        return None;
    }

    let message_data: BackendMessageData = match serde_json::from_value(event.data.clone()) {
        Ok(data) => data,
        Err(e) => {
            warn!("Failed to parse backend message data: {}", e);
            return None;
        }
    };

    let content = message_data.message?.content;
    let content_str = match content {
        BackendContent::Text { parts, .. } => parts.join(""),
        BackendContent::String(s) => s,
    };

    // Fix timestamp overflow: clamp timestamp to prevent overflow
    let timestamp = chrono::Utc::now().timestamp();
    let created = timestamp.max(0) as u64;

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
