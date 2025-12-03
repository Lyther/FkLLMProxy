use crate::openai::models::BackendSSEEvent;
use crate::openai::transformer::parse_sse_event;

const DEFAULT_EVENT_TYPE: &str = "message";

fn finish_current_event(
    event_type: Option<String>,
    data: &mut Vec<String>,
) -> Option<(String, String)> {
    if event_type.is_some() || !data.is_empty() {
        let event = event_type.unwrap_or_else(|| DEFAULT_EVENT_TYPE.to_string());
        let data_str = data.join("\n");
        data.clear();
        Some((event, data_str))
    } else {
        None
    }
}

pub struct SSEParser {
    buffer: String,
    current_event: Option<String>,
    current_data: Vec<String>,
}

impl SSEParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            current_event: None,
            current_data: Vec::new(),
        }
    }

    pub fn parse_chunk(&mut self, chunk: &[u8]) -> Vec<BackendSSEEvent> {
        // Handle invalid UTF-8 explicitly instead of silently replacing
        let text = match String::from_utf8(chunk.to_vec()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Invalid UTF-8 in SSE chunk: {}", e);
                // Use lossy conversion but log the error
                String::from_utf8_lossy(chunk).to_string()
            }
        };
        self.buffer.push_str(&text);

        let mut events = Vec::new();
        // Use iterator directly to avoid allocation
        let all_lines: Vec<String> = self.buffer.lines().map(|s| s.to_string()).collect();

        let (complete_lines, incomplete) = if !text.ends_with('\n') && !text.ends_with('\r') {
            let len = all_lines.len();
            // Fix potential bug: check len > 1 before slicing to avoid empty slice when len == 1
            if len > 1 {
                let incomplete = all_lines.last().cloned();
                (all_lines[..len - 1].to_vec(), incomplete)
            } else if len == 1 {
                // Single incomplete line - keep it in buffer
                (Vec::new(), all_lines.last().cloned())
            } else {
                (Vec::new(), None)
            }
        } else {
            (all_lines, None)
        };

        self.buffer = incomplete.unwrap_or_default();

        for line in &complete_lines {
            if line.trim().is_empty() {
                if let Some((event_type, data_str)) =
                    finish_current_event(self.current_event.take(), &mut self.current_data)
                {
                    if let Some(event) = parse_sse_event(&event_type, &data_str) {
                        events.push(event);
                    }
                }
            } else if let Some(event_data) = line.strip_prefix("event:") {
                // SSE spec requires space after colon: "event: " not "event:"
                if let Some((event_type, data_str)) =
                    finish_current_event(self.current_event.take(), &mut self.current_data)
                {
                    if let Some(event) = parse_sse_event(&event_type, &data_str) {
                        events.push(event);
                    }
                }
                self.current_event = Some(event_data.trim().to_string());
            } else if let Some(data) = line.strip_prefix("data:") {
                // SSE spec requires space after colon: "data: " not "data:"
                self.current_data.push(data.trim().to_string());
            } else {
                // Log unknown SSE line format for debugging
                tracing::debug!("Unknown SSE line format: {}", line);
            }
        }

        events
    }
}

impl Default for SSEParser {
    fn default() -> Self {
        Self::new()
    }
}
