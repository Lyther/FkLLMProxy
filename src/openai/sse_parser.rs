use crate::openai::models::BackendSSEEvent;
use crate::openai::transformer::parse_sse_event;

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
        let text = String::from_utf8_lossy(chunk);
        self.buffer.push_str(&text);

        let mut events = Vec::new();
        let all_lines: Vec<String> = self.buffer.lines().map(|s| s.to_string()).collect();

        let (complete_lines, incomplete) = if !text.ends_with('\n') && !text.ends_with('\r') {
            let len = all_lines.len();
            if len > 0 {
                let incomplete = all_lines.last().cloned();
                (all_lines[..len.saturating_sub(1)].to_vec(), incomplete)
            } else {
                (Vec::new(), None)
            }
        } else {
            (all_lines, None)
        };

        self.buffer = incomplete.unwrap_or_default();

        for line in &complete_lines {
            if line.trim().is_empty() {
                if self.current_event.is_some() || !self.current_data.is_empty() {
                    let event_type = self
                        .current_event
                        .take()
                        .unwrap_or_else(|| "message".to_string());
                    let data_str = self.current_data.join("\n");
                    self.current_data.clear();

                    if let Some(event) = parse_sse_event(&event_type, &data_str) {
                        events.push(event);
                    }
                }
            } else if let Some(event_data) = line.strip_prefix("event:") {
                if self.current_event.is_some() || !self.current_data.is_empty() {
                    let event_type = self
                        .current_event
                        .take()
                        .unwrap_or_else(|| "message".to_string());
                    let data_str = self.current_data.join("\n");
                    self.current_data.clear();

                    if let Some(event) = parse_sse_event(&event_type, &data_str) {
                        events.push(event);
                    }
                }
                self.current_event = Some(event_data.trim().to_string());
            } else if let Some(data) = line.strip_prefix("data:") {
                self.current_data.push(data.trim().to_string());
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
