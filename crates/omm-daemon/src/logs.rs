//! Bounded, in-memory tail shared by the log collector and new subscribers.
use std::collections::VecDeque;

use mihomo_api::model::LogLine;
use serde::Serialize;

const CAPACITY: usize = 400;
const MAX_PAYLOAD_BYTES: usize = 16 * 1024;

#[derive(Clone, Serialize)]
pub struct Entry {
    #[serde(flatten)]
    pub line: LogLine,
    pub timestamp: i64,
}

#[derive(Default)]
pub struct Tail(VecDeque<Entry>);

impl Tail {
    pub fn push(&mut self, mut line: LogLine, timestamp: i64) -> Entry {
        if line.payload.len() > MAX_PAYLOAD_BYTES {
            let mut end = MAX_PAYLOAD_BYTES;
            while !line.payload.is_char_boundary(end) {
                end -= 1;
            }
            line.payload.truncate(end);
            line.payload.push('…');
        }
        let entry = Entry { line, timestamp };
        if self.0.len() == CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back(entry.clone());
        entry
    }

    pub fn snapshot(&self) -> Vec<Entry> {
        self.0.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_recent_lines_in_order_with_wire_level_and_timestamp() {
        let mut tail = Tail::default();
        for i in 0..405 {
            tail.push(LogLine { level: "info".into(), payload: i.to_string() }, i);
        }
        let lines = tail.snapshot();
        assert_eq!(lines.len(), 400);
        assert_eq!(lines[0].line.payload, "5");
        let last = serde_json::to_value(lines.last().unwrap()).unwrap();
        assert_eq!(last, serde_json::json!({"type":"info", "payload":"404", "timestamp":404}));
    }

    #[test]
    fn bounds_large_unicode_messages() {
        let entry = Tail::default().push(LogLine { level: "warning".into(), payload: "界".repeat(20000) }, 1);
        assert!(entry.line.payload.len() <= MAX_PAYLOAD_BYTES + 3);
        assert!(entry.line.payload.ends_with('…'));
    }
}
