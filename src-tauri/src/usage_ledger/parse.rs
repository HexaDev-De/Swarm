//! Line parsers for the agents' own local session logs. Each returns token
//! counts only; prompts, replies and tool output are never retained.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hash::{Hash, Hasher};

/// Prompt size classes, so long-context rates can be applied at pricing time
/// without keeping each request's size: 0 = up to 200k, 1 = up to 272k, 2 = larger.
pub(crate) fn size_class(prompt_tokens: u64) -> u8 {
    match prompt_tokens {
        0..=200_000 => 0,
        200_001..=272_000 => 1,
        _ => 2,
    }
}

/// Token counts for one request (Claude) or one hour of one model (Codex).
/// `input` excludes cached and cache-write tokens for both providers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Record {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<u64>,
    pub hour: i64,
    pub model: String,
    pub class: u8,
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    #[serde(default)]
    pub cache_write_1h: u64,
    pub output: u64,
}
impl Record {
    pub(crate) fn tokens(&self) -> u64 {
        self.input + self.cache_read + self.cache_write + self.cache_write_1h + self.output
    }
    fn absorb(&mut self, other: &Record) {
        self.input += other.input;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.cache_write_1h += other.cache_write_1h;
        self.output += other.output;
    }
}

/// State carried between appended chunks of one Codex rollout file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct CodexState {
    pub model: Option<String>,
    pub total: Option<u64>,
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix seconds for an RFC 3339 timestamp such as `2026-09-26T08:58:01.123Z` or `…+02:00`.
pub(crate) fn unix_seconds(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || !matches!(bytes[10], b'T' | b't' | b' ') {
        return None;
    }
    let number = |range: std::ops::Range<usize>| value.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let zone = value[19..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match zone {
        "Z" | "z" => 0,
        _ if zone.len() == 6 && matches!(&zone[0..1], "+" | "-") && &zone[3..4] == ":" => {
            let minutes = zone[1..3].parse::<i64>().ok()? * 60 + zone[4..6].parse::<i64>().ok()?;
            if zone.starts_with('+') { minutes * 60 } else { -minutes * 60 }
        }
        _ => return None,
    };
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

fn count(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

/// Normalizes a reported model name; `None` for placeholders that never reach an API.
fn model_name(value: &Value) -> Option<String> {
    let name = value.as_str()?.trim();
    if name.is_empty() || name.starts_with('<') || name.len() > 128 {
        return None;
    }
    Some(name.to_ascii_lowercase())
}

/// One assistant reply from a Claude Code project log. Streaming writes the same
/// reply several times and resumed sessions copy history, so replies carry a key
/// (message id + request id) for global de-duplication.
pub(crate) fn claude_line(line: &str) -> Option<Record> {
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return None;
    }
    let entry: Value = serde_json::from_str(line).ok()?;
    if entry["type"] != "assistant" {
        return None;
    }
    let message = &entry["message"];
    let usage = message.get("usage")?;
    let model = model_name(&message["model"])?;
    let seconds = unix_seconds(entry["timestamp"].as_str()?)?;
    let key = match (message["id"].as_str(), entry["requestId"].as_str()) {
        (Some(id), Some(request)) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (id, request).hash(&mut hasher);
            Some(hasher.finish())
        }
        _ => None,
    };
    let input = count(&usage["input_tokens"]);
    let cache_read = count(&usage["cache_read_input_tokens"]);
    let written = count(&usage["cache_creation_input_tokens"]);
    // Newer logs split cache writes by lifetime; one-hour writes bill at a higher rate.
    let hour_writes = count(&usage["cache_creation"]["ephemeral_1h_input_tokens"]).min(written);
    let record = Record {
        key,
        hour: seconds.div_euclid(3600),
        class: size_class(input + cache_read + written),
        model,
        input,
        cache_read,
        cache_write: written - hour_writes,
        cache_write_1h: hour_writes,
        output: count(&usage["output_tokens"]),
    };
    (record.tokens() > 0).then_some(record)
}

/// Rollout lines lead with their type, so a short prefix rejects the large
/// message and tool-output lines without parsing them.
fn codex_kind(line: &str) -> Option<&'static str> {
    let head = &line.as_bytes()[..line.len().min(200)];
    let has = |needle: &[u8]| head.windows(needle.len()).any(|window| window == needle);
    if has(b"\"turn_context\"") {
        Some("context")
    } else if has(b"\"token_count\"") {
        Some("tokens")
    } else {
        None
    }
}

/// One line of a Codex rollout. Token events carry the session's running total;
/// an event whose total has not moved repeats the previous turn and is skipped.
pub(crate) fn codex_line(line: &str, state: &mut CodexState) -> Option<Record> {
    let kind = codex_kind(line)?;
    let entry: Value = serde_json::from_str(line).ok()?;
    let payload = &entry["payload"];
    if kind == "context" {
        if entry["type"] == "turn_context" {
            if let Some(model) = model_name(&payload["model"]) {
                state.model = Some(model);
            }
        }
        return None;
    }
    if payload["type"] != "token_count" {
        return None;
    }
    let info = payload.get("info").filter(|info| info.is_object())?;
    let total = info["total_token_usage"]["total_tokens"].as_u64();
    if total.is_some() && total == state.total {
        return None;
    }
    state.total = total;
    let last = &info["last_token_usage"];
    let seconds = unix_seconds(entry["timestamp"].as_str()?)?;
    // OpenAI reports cached and cache-write tokens inside input_tokens, and
    // reasoning tokens inside output_tokens.
    let prompt = count(&last["input_tokens"]);
    let cache_read = count(&last["cached_input_tokens"]).min(prompt);
    let cache_write = count(&last["cache_write_input_tokens"]).min(prompt - cache_read);
    let record = Record {
        key: None,
        hour: seconds.div_euclid(3600),
        model: state.model.clone().unwrap_or_else(|| "unknown".into()),
        class: size_class(prompt),
        input: prompt - cache_read - cache_write,
        cache_read,
        cache_write,
        cache_write_1h: 0,
        output: count(&last["output_tokens"]),
    };
    (record.tokens() > 0).then_some(record)
}

/// Codex records only need hourly totals per model and size class.
pub(crate) fn merge_hourly(records: &mut Vec<Record>, record: Record) {
    match records.iter_mut().rev().take(8).find(|existing| {
        existing.hour == record.hour && existing.class == record.class && existing.model == record.model
    }) {
        Some(existing) => existing.absorb(&record),
        None => records.push(record),
    }
}

/// Parses Antigravity ACP or brain transcript logs.
pub(crate) fn antigravity_line(line: &str) -> Option<Record> {
    if !line.contains("\"created_at\"") {
        return None;
    }
    let entry: Value = serde_json::from_str(line).ok()?;
    let seconds = unix_seconds(entry["created_at"].as_str()?)?;
    let hour = seconds.div_euclid(3600);

    let (input, output, cache_read) = if let Some(usage) = entry.get("usage").filter(|u| u.is_object()) {
        (
            count(&usage["input_tokens"]),
            count(&usage["output_tokens"]),
            count(&usage["cached_tokens"]),
        )
    } else {
        let t = entry["type"].as_str().unwrap_or("");
        if t == "PLANNER_RESPONSE" {
            let len = entry["thinking"].as_str().map_or(0, |s| s.len())
                + entry["content"].as_str().map_or(0, |s| s.len());
            (0, (len / 4).max(80) as u64, 0)
        } else if t == "USER_INPUT" {
            let len = entry["content"].as_str().map_or(0, |s| s.len());
            ((len / 4).max(120) as u64, 0, 0)
        } else {
            return None;
        }
    };

    if input == 0 && output == 0 {
        return None;
    }

    let model = entry["model"]
        .as_str()
        .or_else(|| entry["model_name"].as_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "gemini-3.8-flash".into());

    let record = Record {
        key: None,
        hour,
        model,
        class: size_class(input),
        input,
        cache_read,
        cache_write: 0,
        cache_write_1h: 0,
        output,
    };
    (record.tokens() > 0).then_some(record)
}

