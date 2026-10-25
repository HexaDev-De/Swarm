//! Team rules go to each conversation once, then again only when they change, when the
//! harness starts a new conversation, or as a periodic reminder. Other turns get a
//! one-line reminder and only the messages they have not seen.
use super::*;
use std::hash::{Hash, Hasher};

/// Full rules on every eighth turn, so a compacted conversation never loses them for long.
const FULL_EVERY: u32 = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Briefing {
    /// A hash of the rules text this conversation last received in full.
    pub rules: u64,
    /// The newest addressed mailbox message already put in this conversation's prompt.
    pub message_id: i64,
    /// Turns since the rules were last sent in full.
    pub since_full: u32,
}

pub(super) fn fingerprint(rules: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    rules.hash(&mut hasher);
    hasher.finish()
}

/// Whether this turn needs the full rules. `resumed` is false when the harness starts a
/// new conversation, which has never seen them.
pub(super) fn needs_full(previous: Option<&Briefing>, rules: u64, resumed: bool) -> bool {
    match previous {
        Some(previous) if resumed => previous.rules != rules || previous.since_full + 1 >= FULL_EVERY,
        _ => true,
    }
}

pub(super) fn next(previous: Option<&Briefing>, rules: u64, message_id: i64, full: bool) -> Briefing {
    Briefing {
        rules,
        message_id,
        since_full: if full { 0 } else { previous.map_or(0, |b| b.since_full) + 1 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_go_out_again_only_when_needed() {
        let rules = fingerprint("rules");
        assert!(needs_full(None, rules, true));
        let first = next(None, rules, 0, true);
        assert!(needs_full(Some(&first), rules, false), "a new harness conversation");
        assert!(needs_full(Some(&first), fingerprint("changed"), true), "changed rules");
        let mut briefing = first;
        for _ in 1..FULL_EVERY {
            assert!(!needs_full(Some(&briefing), rules, true));
            briefing = next(Some(&briefing), rules, 0, false);
        }
        assert!(needs_full(Some(&briefing), rules, true), "periodic reminder");
    }
}
