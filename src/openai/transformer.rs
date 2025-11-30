use crate::models::openai::{ChatCompletionChunk, ChatCompletionChunkChoice, DeltaMessage, Role};
use crate::openai::models::{
    BackendContent, BackendConversationRequest, BackendMessage, BackendMessageData, BackendSSEEvent,
};
use anyhow::Result;
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

    let data: serde_json::Value = serde_json::from_str(data_str).ok()?;

    Some(BackendSSEEvent {
        event_type: event_type.to_string(),
        data,
    })
}

pub fn transform_sse_to_openai_chunk(
    event: &BackendSSEEvent,
    model: &str,
    request_id: &str,
) -> Option<ChatCompletionChunk> {
    if event.event_type == "done" {
        return Some(ChatCompletionChunk {
            id: request_id.to_string(),
            object: "chat.completion.chunk".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
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

    let message_data: BackendMessageData = serde_json::from_value(event.data.clone()).ok()?;

    let content = message_data.message?.content;
    let content_str = match content {
        BackendContent::Text { parts, .. } => parts.join(""),
        BackendContent::String(s) => s,
    };

    Some(ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
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
