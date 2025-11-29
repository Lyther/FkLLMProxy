use crate::models::openai::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionRequest,
    ChatCompletionResponse, ChatMessage, DeltaMessage, Role, Usage,
};
use crate::models::vertex::{
    Content, GenerateContentRequest, GenerateContentResponse, GenerationConfig, Part,
};
use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn transform_request(req: ChatCompletionRequest) -> Result<GenerateContentRequest> {
    let mut contents = Vec::new();
    let mut system_instruction = None;

    for msg in req.messages {
        match msg.role {
            Role::System => {
                // Vertex supports system_instruction separately
                system_instruction = Some(Content {
                    role: "user".to_string(), // System instruction is technically "user" role in some contexts or special field
                    parts: vec![Part {
                        text: Some(msg.content),
                    }],
                });
            }
            Role::User => {
                contents.push(Content {
                    role: "user".to_string(),
                    parts: vec![Part {
                        text: Some(msg.content),
                    }],
                });
            }
            Role::Assistant => {
                contents.push(Content {
                    role: "model".to_string(),
                    parts: vec![Part {
                        text: Some(msg.content),
                    }],
                });
            }
            Role::Tool => {
                // TODO: Handle tool outputs
            }
        }
    }

    let generation_config = Some(GenerationConfig {
        temperature: Some(req.temperature),
        top_p: Some(req.top_p),
        max_output_tokens: req.max_tokens,
        stop_sequences: req.stop,
        candidate_count: Some(1),
    });

    Ok(GenerateContentRequest {
        contents,
        system_instruction,
        generation_config,
        safety_settings: None, // Use defaults
    })
}

pub fn transform_response(
    vertex_res: GenerateContentResponse,
    model: String,
    id: String,
) -> Result<ChatCompletionResponse> {
    let created = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let choices = vertex_res
        .candidates
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let content = c
                .content
                .and_then(|c| c.parts.first().and_then(|p| p.text.clone()))
                .unwrap_or_default();

            ChatCompletionChoice {
                index: i as u32,
                message: ChatMessage {
                    role: Role::Assistant,
                    content,
                    name: None,
                },
                finish_reason: c.finish_reason.map(|s| s.to_lowercase()),
            }
        })
        .collect();

    let usage = vertex_res.usage_metadata.map(|u| Usage {
        prompt_tokens: u.prompt_token_count.unwrap_or(0),
        completion_tokens: u.candidates_token_count.unwrap_or(0),
        total_tokens: u.total_token_count.unwrap_or(0),
    });

    Ok(ChatCompletionResponse {
        id,
        object: "chat.completion".to_string(),
        created,
        model,
        choices,
        usage,
    })
}

pub fn transform_stream_chunk(
    vertex_res: GenerateContentResponse,
    model: String,
    id: String,
) -> Result<ChatCompletionChunk> {
    let created = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let choices = vertex_res
        .candidates
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let content = c
                .content
                .and_then(|c| c.parts.first().and_then(|p| p.text.clone()));

            ChatCompletionChunkChoice {
                index: i as u32,
                delta: DeltaMessage {
                    role: if content.is_some() {
                        Some(Role::Assistant)
                    } else {
                        None
                    }, // Only send role on first chunk ideally, but here we might send it often. OpenAI usually sends role only in first chunk.
                    // Actually, Vertex might send empty content for finish reason.
                    content,
                },
                finish_reason: c.finish_reason.map(|s| s.to_lowercase()),
            }
        })
        .collect();

    Ok(ChatCompletionChunk {
        id,
        object: "chat.completion.chunk".to_string(),
        created,
        model,
        choices,
    })
}
