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
                if let Some((event_type, data_str)) =
                    finish_current_event(self.current_event.take(), &mut self.current_data)
                {
                    if let Some(event) = parse_sse_event(&event_type, &data_str) {
                        events.push(event);
                    }
                }
            } else if let Some(event_data) = line.strip_prefix("event:") {
                if let Some((event_type, data_str)) =
                    finish_current_event(self.current_event.take(), &mut self.current_data)
                {
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
