#[cfg(test)]
mod tests {
    use crate::models::openai::Role;
    use crate::models::vertex::{Candidate, Content, GenerateContentResponse, Part};
    use crate::services::transformer::transform_stream_chunk;

    #[test]
    fn test_transform_stream_chunk() {
        let vertex_res = GenerateContentResponse {
            candidates: Some(vec![Candidate {
                content: Some(Content {
                    role: "model".to_string(),
                    parts: vec![Part {
                        text: Some("Hello".to_string()),
                    }],
                }),
                finish_reason: None,
                index: Some(0),
            }]),
            usage_metadata: None,
        };

        let chunk = transform_stream_chunk(
            vertex_res,
            "gemini-flash".to_string(),
            "req-123".to_string(),
        )
        .unwrap();

        assert_eq!(chunk.id, "req-123");
        assert_eq!(chunk.model, "gemini-flash");
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
        assert_eq!(chunk.choices[0].delta.role, Some(Role::Assistant));
    }
}
