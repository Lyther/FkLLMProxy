use crate::models::{
    openai::{
        ChatCompletionChoice, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, Role,
        Usage,
    },
    vertex::{Content, GenerateContentRequest, GenerateContentResponse, GenerationConfig, Part},
};
use anyhow::{Context, Result};
use chrono::Utc;

pub fn transform_request(req: ChatCompletionRequest) -> Result<GenerateContentRequest> {
    let contents: Result<Vec<Content>> = req
        .messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "model",
                Role::System => "user", // System messages become user messages in Vertex
                Role::Tool => "user",   // Tool messages become user messages
            };

            Ok(Content {
                role: role.to_string(),
                parts: vec![Part {
                    text: Some(msg.content.clone()),
                }],
            })
        })
        .collect();

    let mut vertex_req = GenerateContentRequest {
        contents: contents?,
        system_instruction: None,
        generation_config: Some(GenerationConfig {
            temperature: Some(req.temperature),
            top_p: Some(req.top_p),
            max_output_tokens: req.max_tokens,
            stop_sequences: req.stop,
            candidate_count: None,
        }),
        safety_settings: None,
    };

    // Extract system message if present
    if let Some(system_msg) = req.messages.iter().find(|m| matches!(m.role, Role::System)) {
        vertex_req.system_instruction = Some(Content {
            role: "user".to_string(),
            parts: vec![Part {
                text: Some(system_msg.content.clone()),
            }],
        });
        // Remove system message from contents
        let system_content = system_msg.content.clone();
        vertex_req.contents.retain(|c| {
            if let Some(text) = c.parts.first().and_then(|p| p.text.as_ref()) {
                system_content != *text
            } else {
                true
            }
        });
    }

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
        .context("No candidates in Vertex response")?;

    let content = candidate
        .content
        .as_ref()
        .and_then(|c| c.parts.first())
        .and_then(|p| p.text.as_ref())
        .context("No content in Vertex response")?
        .clone();

    let finish_reason = candidate.finish_reason.as_ref().map(|s| s.to_lowercase());

    let usage = vertex_res.usage_metadata.as_ref().map(|u| Usage {
        prompt_tokens: u.prompt_token_count.unwrap_or(0),
        completion_tokens: u.candidates_token_count.unwrap_or(0),
        total_tokens: u.total_token_count.unwrap_or(0),
    });

    Ok(ChatCompletionResponse {
        id: request_id,
        object: "chat.completion".to_string(),
        created: Utc::now().timestamp() as u64,
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
        .context("No candidates in Vertex response")?;

    let content = candidate
        .content
        .as_ref()
        .and_then(|c| c.parts.first())
        .and_then(|p| p.text.as_ref())
        .cloned();

    let finish_reason = candidate.finish_reason.as_ref().map(|s| s.to_lowercase());

    Ok(crate::models::openai::ChatCompletionChunk {
        id: request_id,
        object: "chat.completion.chunk".to_string(),
        created: Utc::now().timestamp() as u64,
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
