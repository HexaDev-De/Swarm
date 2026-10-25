//! Reconcile streamed text with completed snapshots without retaining a second
//! copy of long responses. A bounded per-message digest verifies each prefix.
use std::{
    collections::{HashMap, VecDeque},
    hash::{DefaultHasher, Hasher},
};

const CACHE_LIMIT: usize = 128;

#[derive(Default)]
struct Message {
    bytes: usize,
    digest: DefaultHasher,
    phase: Option<String>,
}

pub(super) struct Update {
    pub boundary: Option<(String, Option<String>)>,
    pub text: String,
}

#[derive(Default)]
pub(super) struct TextStreams {
    messages: HashMap<String, Message>,
    order: VecDeque<String>,
    active_id: Option<String>,
}

impl TextStreams {
    fn message(&mut self, id: &str, phase: Option<&str>) -> &mut Message {
        let id = super::limited(if id.is_empty() { "anonymous" } else { id }, 256);
        if !self.messages.contains_key(&id) {
            if self.messages.len() >= CACHE_LIMIT {
                if let Some(oldest) = self.order.pop_front() {
                    self.messages.remove(&oldest);
                }
            }
            self.order.push_back(id.clone());
        }
        let message = self.messages.entry(id).or_default();
        if let Some(phase) = phase.filter(|phase| !phase.is_empty()) {
            message.phase = Some(super::limited(phase, 64));
        }
        message
    }

    pub fn remember_phase(&mut self, id: &str, phase: Option<&str>) {
        self.message(id, phase);
    }

    pub fn delta(&mut self, id: &str, phase: Option<&str>, text: &str) -> Option<Update> {
        if text.is_empty() {
            return None;
        }
        let id = super::limited(if id.is_empty() { "anonymous" } else { id }, 256);
        let new_segment = self.active_id.as_deref() != Some(&id);
        let message = self.message(&id, phase);
        let boundary =
            (message.bytes == 0 || new_segment).then(|| (id.clone(), message.phase.clone()));
        message.digest.write(text.as_bytes());
        message.bytes = message.bytes.saturating_add(text.len());
        self.active_id = Some(id);
        Some(Update {
            boundary,
            text: text.to_owned(),
        })
    }

    pub fn snapshot(&mut self, id: &str, phase: Option<&str>, text: &str) -> Option<Update> {
        if text.is_empty() {
            return None;
        }
        let message = self.message(id, phase);
        let consumed = message.bytes;
        let prefix = text.get(..consumed)?;
        let mut digest = DefaultHasher::new();
        digest.write(prefix.as_bytes());
        // A changed snapshot is not an append operation. Preserve the emitted
        // stream rather than duplicate or splice unrelated replacement text.
        if consumed > 0 && digest.finish() != message.digest.finish() {
            return None;
        }
        self.delta(id, phase, &text[consumed..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_reconciliation_is_independent_of_chunk_boundaries_and_utf8() {
        let mut streams = TextStreams::default();
        streams.remember_phase("message", Some("commentary"));
        assert_eq!(
            streams.delta("message", None, "Hello ").unwrap().boundary,
            Some(("message".into(), Some("commentary".into())))
        );
        assert!(streams
            .delta("message", None, "🦀")
            .unwrap()
            .boundary
            .is_none());
        let tail = streams.snapshot("message", None, "Hello 🦀!\n").unwrap();
        assert_eq!(tail.text, "!\n");
        assert!(tail.boundary.is_none());
        assert!(streams.snapshot("message", None, "Hello 🦀!\n").is_none());
        assert!(streams
            .snapshot("message", None, "Unrelated final text")
            .is_none());
    }

    #[test]
    fn empty_updates_do_not_open_messages_and_tracking_stays_bounded() {
        let mut streams = TextStreams::default();
        assert!(streams.delta("empty", None, "").is_none());
        assert!(streams.snapshot("empty", None, "").is_none());
        for index in 0..300 {
            let id = format!("message-{index}");
            let update = streams.snapshot(&id, None, "whole response").unwrap();
            assert!(update.boundary.is_some());
            assert!(streams.snapshot(&id, None, "whole response").is_none());
        }
        assert_eq!(streams.messages.len(), CACHE_LIMIT);
        assert_eq!(streams.order.len(), CACHE_LIMIT);
    }

    #[test]
    fn interleaved_message_ids_open_distinct_contiguous_segments() {
        let mut streams = TextStreams::default();
        streams.delta("first", None, "Part ").unwrap();
        streams.delta("second", None, "Other").unwrap();
        let resumed = streams.delta("first", None, "two").unwrap();
        assert_eq!(resumed.boundary, Some(("first".into(), None)));
        assert_eq!(resumed.text, "two");
        assert!(streams.snapshot("first", None, "Part two").is_none());
    }
}
