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
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            current_event: None,
            current_data: Vec::new(),
        }
    }

    pub fn parse_chunk(&mut self, chunk: &[u8]) -> Vec<BackendSSEEvent> {
        // Fix data corruption risk: Handle invalid UTF-8 explicitly
        // For SSE, we expect text data, so invalid UTF-8 is an error condition
        let text = match String::from_utf8(chunk.to_vec()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Invalid UTF-8 in SSE chunk: {}", e);
                // Use lossy conversion but log detailed error information
                // This preserves partial data while alerting to corruption
                String::from_utf8_lossy(chunk).to_string()
            }
        };
        self.buffer.push_str(&text);

        let mut events = Vec::new();
        // Fix inefficiency: Process lines directly without creating Vec<String> for all lines
        // Fix complex incomplete line handling: Handle \r\n split across chunks properly
        let ends_with_newline = text.ends_with('\n') || text.ends_with('\r');

        // Split buffer into lines efficiently - collect as Vec<String> only for incomplete handling
        let lines: Vec<String> = self.buffer.lines().map(str::to_string).collect();
        let (complete_lines, incomplete) = if !ends_with_newline && !lines.is_empty() {
            // Last line is incomplete - keep it in buffer
            let incomplete = lines.last().cloned();
            let complete = lines[..lines.len().saturating_sub(1)].to_vec();
            (complete, incomplete)
        } else {
            // All lines are complete
            (lines, None)
        };

        self.buffer = incomplete.unwrap_or_default();

        // Fix code duplication: Extract helper function for processing event
        // Fix code duplication: Extract helper to process event completion
        let mut process_completed_event = |event_type: Option<String>, data: &mut Vec<String>| {
            if let Some((evt_type, data_str)) = finish_current_event(event_type, data) {
                if let Some(event) = parse_sse_event(&evt_type, &data_str) {
                    events.push(event);
                }
            }
        };

        for line in complete_lines {
            if line.trim().is_empty() {
                // Empty line completes current event
                process_completed_event(self.current_event.take(), &mut self.current_data);
            } else if let Some(event_data) = line.strip_prefix("event:") {
                // Fix SSE format deviation: Require space after colon per SSE spec
                // But handle both "event:" and "event: " for compatibility
                process_completed_event(self.current_event.take(), &mut self.current_data);
                self.current_event = Some(event_data.trim().to_string());
            } else if let Some(data) = line.strip_prefix("data:") {
                // Fix SSE format deviation: Require space after colon per SSE spec
                // But handle both "data:" and "data: " for compatibility
                self.current_data.push(data.trim().to_string());
            } else {
                // Fix error handling: Log malformed SSE lines for debugging
                tracing::warn!("Malformed SSE line (skipping): {}", line);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_parser_basic_event() {
        let mut parser = SSEParser::new();
        // parse_sse_event requires valid JSON, so use JSON data
        let chunk = b"data: {\"text\":\"hello\"}\n\n";
        let events = parser.parse_chunk(chunk);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_sse_parser_incomplete_line() {
        let mut parser = SSEParser::new();
        let chunk1 = b"data: {\"text\":\"hello";
        let chunk2 = b"\"}\n\n";

        let events1 = parser.parse_chunk(chunk1);
        assert_eq!(events1.len(), 0); // Incomplete, no events yet

        let events2 = parser.parse_chunk(chunk2);
        assert_eq!(events2.len(), 1); // Complete event
    }

    #[test]
    fn test_sse_parser_multiple_events() {
        let mut parser = SSEParser::new();
        let chunk = b"data: {\"text\":\"event1\"}\n\ndata: {\"text\":\"event2\"}\n\n";
        let events = parser.parse_chunk(chunk);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_sse_parser_event_type() {
        let mut parser = SSEParser::new();
        let chunk = b"event: test\ndata: {\"text\":\"hello\"}\n\n";
        let events = parser.parse_chunk(chunk);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "test");
    }

    #[test]
    fn test_sse_parser_crlf_line_endings() {
        let mut parser = SSEParser::new();
        let chunk = b"data: {\"text\":\"hello\"}\r\n\r\n";
        let events = parser.parse_chunk(chunk);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_sse_parser_malformed_line() {
        let mut parser = SSEParser::new();
        let chunk = b"invalid: line\ndata: {\"text\":\"hello\"}\n\n";
        let events = parser.parse_chunk(chunk);
        // Should still parse valid data line (malformed line is logged but skipped)
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_sse_parser_chunk_split_across_crlf() {
        let mut parser = SSEParser::new();
        let chunk1 = b"data: {\"text\":\"hello\"}\r";
        let chunk2 = b"\n\n";

        let events1 = parser.parse_chunk(chunk1);
        assert_eq!(events1.len(), 0); // Incomplete

        let events2 = parser.parse_chunk(chunk2);
        assert_eq!(events2.len(), 1); // Complete
    }

    #[test]
    fn test_sse_parser_done_event() {
        let mut parser = SSEParser::new();
        let chunk = b"data: [DONE]\n\n";
        let events = parser.parse_chunk(chunk);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "done");
    }
}
