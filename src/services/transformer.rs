use crate::models::{
    openai::{
        ChatCompletionChoice, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, Role,
        Usage,
    },
    vertex::{Content, GenerateContentRequest, GenerateContentResponse, GenerationConfig, Part},
};
use anyhow::Result;
use tracing::warn;

pub fn transform_request(req: ChatCompletionRequest) -> Result<GenerateContentRequest> {
    // Collect all system messages and concatenate them
    let system_messages: Vec<String> = req
        .messages
        .iter()
        .filter(|m| matches!(m.role, Role::System))
        .map(|m| m.content.clone())
        .collect();

    let system_instruction_text = if !system_messages.is_empty() {
        Some(system_messages.join("\n\n"))
    } else {
        None
    };

    // Collect non-system messages, preserving role semantics
    // Note: Vertex API uses "user" and "model" roles, but we preserve Tool role as "user"
    // since Vertex doesn't have a Tool role equivalent
    let mut contents: Vec<Content> = Vec::new();

    for msg in req.messages.iter() {
        match msg.role {
            Role::System => {
                // System messages are already collected above
            }
            Role::User | Role::Tool => {
                contents.push(Content {
                    role: "user".to_string(),
                    parts: vec![Part {
                        text: Some(msg.content.clone()),
                    }],
                });
            }
            Role::Assistant => {
                contents.push(Content {
                    role: "model".to_string(),
                    parts: vec![Part {
                        text: Some(msg.content.clone()),
                    }],
                });
            }
        }
    }

    let vertex_req = GenerateContentRequest {
        contents,
        system_instruction: system_instruction_text.map(|text| Content {
            role: "system".to_string(), // Use "system" role for system instruction
            parts: vec![Part { text: Some(text) }],
        }),
        generation_config: Some(GenerationConfig {
            temperature: Some(req.temperature),
            top_p: Some(req.top_p),
            max_output_tokens: req.max_tokens,
            stop_sequences: req.stop,
            candidate_count: None,
        }),
        safety_settings: None,
    };

    Ok(vertex_req)
}

pub fn transform_response(
    vertex_res: GenerateContentResponse,
    model: String,
    request_id: String,
) -> Result<ChatCompletionResponse> {
    let candidate = vertex_res
        .candidates
        .as_ref()
        .and_then(|c| c.first())
        .ok_or_else(|| anyhow::anyhow!("No candidates in Vertex response"))?;

    let content = candidate
        .content
        .as_ref()
        .and_then(|c| c.parts.first())
        .and_then(|p| p.text.as_ref())
        .ok_or_else(|| anyhow::anyhow!("No content in Vertex response"))?
        .clone();

    let finish_reason = candidate.finish_reason.as_ref().map(|s| s.to_lowercase());

    // Fix error swallowing: Log detailed error information instead of silently continuing
    let usage = vertex_res.usage_metadata.as_ref().and_then(|u| {
        if u.prompt_token_count.is_none()
            || u.candidates_token_count.is_none()
            || u.total_token_count.is_none()
        {
            warn!(
                "Vertex response missing token counts (prompt: {:?}, candidates: {:?}, total: {:?}) - returning None. This may indicate API contract violation.",
                u.prompt_token_count, u.candidates_token_count, u.total_token_count
            );
            None
        } else {
            Some(Usage {
                prompt_tokens: u.prompt_token_count.unwrap_or(0),
                completion_tokens: u.candidates_token_count.unwrap_or(0),
                total_tokens: u.total_token_count.unwrap_or(0),
            })
        }
    });

    // Fix: Use SystemTime instead of chrono for timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let created = timestamp.max(0) as u64;

    Ok(ChatCompletionResponse {
        id: request_id,
        object: "chat.completion".to_string(),
        created,
        model,
        choices: vec![ChatCompletionChoice {
            index: candidate.index.unwrap_or(0),
            message: ChatMessage {
                role: Role::Assistant,
                content,
                name: None,
            },
            finish_reason,
        }],
        usage,
    })
}

pub fn transform_stream_chunk(
    vertex_res: GenerateContentResponse,
    model: String,
    request_id: String,
) -> Result<crate::models::openai::ChatCompletionChunk> {
    let candidate = vertex_res
        .candidates
        .as_ref()
        .and_then(|c| c.first())
        .ok_or_else(|| anyhow::anyhow!("No candidates in Vertex response"))?;

    let content = candidate
        .content
        .as_ref()
        .and_then(|c| c.parts.first())
        .and_then(|p| p.text.as_ref())
        .cloned();

    let finish_reason = candidate.finish_reason.as_ref().map(|s| s.to_lowercase());

    // Fix: Use SystemTime instead of chrono for timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let created = timestamp.max(0) as u64;

    Ok(crate::models::openai::ChatCompletionChunk {
        id: request_id,
        object: "chat.completion.chunk".to_string(),
        created,
        model,
        choices: vec![crate::models::openai::ChatCompletionChunkChoice {
            index: candidate.index.unwrap_or(0),
            delta: crate::models::openai::DeltaMessage {
                role: None,
                content,
            },
            finish_reason,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::openai::{ChatMessage, Role};
    use crate::models::vertex::{Candidate, UsageMetadata};

    #[test]
    fn test_transform_request_basic() {
        let req = ChatCompletionRequest {
            model: "gemini-pro".to_string(),
            messages: vec![
                ChatMessage {
                    role: Role::User,
                    content: "Hello".to_string(),
                    name: None,
                },
                ChatMessage {
                    role: Role::Assistant,
                    content: "Hi there".to_string(),
                    name: None,
                },
            ],
            stream: false,
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: Some(100),
            stop: None,
        };

        let vertex_req = transform_request(req).unwrap();
        assert_eq!(vertex_req.contents.len(), 2);
        assert_eq!(vertex_req.contents[0].role, "user");
        assert_eq!(vertex_req.contents[1].role, "model");
        assert_eq!(
            vertex_req.generation_config.as_ref().unwrap().temperature,
            Some(0.7)
        );
        assert_eq!(
            vertex_req
                .generation_config
                .as_ref()
                .unwrap()
                .max_output_tokens,
            Some(100)
        );
    }

    #[test]
    fn test_transform_request_with_system() {
        let req = ChatCompletionRequest {
            model: "gemini-pro".to_string(),
            messages: vec![
                ChatMessage {
                    role: Role::System,
                    content: "You are a helpful assistant".to_string(),
                    name: None,
                },
                ChatMessage {
                    role: Role::User,
                    content: "Hello".to_string(),
                    name: None,
                },
            ],
            stream: false,
            temperature: 1.0,
            top_p: 1.0,
            max_tokens: None,
            stop: None,
        };

        let vertex_req = transform_request(req).unwrap();
        assert!(vertex_req.system_instruction.is_some());
        assert_eq!(vertex_req.contents.len(), 1);
        assert_eq!(vertex_req.contents[0].role, "user");
    }

    #[test]
    fn test_transform_response() {
        let vertex_res = GenerateContentResponse {
            candidates: Some(vec![Candidate {
                content: Some(Content {
                    role: "model".to_string(),
                    parts: vec![Part {
                        text: Some("Hello, world!".to_string()),
                    }],
                }),
                finish_reason: Some("STOP".to_string()),
                index: Some(0),
            }]),
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: Some(10),
                candidates_token_count: Some(5),
                total_token_count: Some(15),
            }),
        };

        let response =
            transform_response(vertex_res, "gemini-pro".to_string(), "test-id".to_string())
                .unwrap();
        assert_eq!(response.id, "test-id");
        assert_eq!(response.model, "gemini-pro");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Hello, world!");
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
        assert!(response.usage.is_some());
        assert_eq!(response.usage.unwrap().total_tokens, 15);
    }

    #[test]
    fn test_transform_response_no_candidates() {
        let vertex_res = GenerateContentResponse {
            candidates: None,
            usage_metadata: None,
        };

        let result =
            transform_response(vertex_res, "gemini-pro".to_string(), "test-id".to_string());
        assert!(result.is_err());
    }
}
