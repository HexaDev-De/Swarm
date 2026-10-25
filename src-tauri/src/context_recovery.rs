//! A one-time conversation handoff when a harness context must be replaced.
use crate::{
    storage::{Session, Store},
    Result,
};
use rusqlite::{params, OptionalExtension};
use serde_json::json;

pub(crate) const LEGACY_NOTICE: &str = "Architect mode starts a fresh harness context so its team tools are available. Your earlier conversation remains here.";
const HISTORY_BYTES: usize = 64 * 1024;
const HISTORY_MESSAGES: usize = 200;
pub(crate) const RECOVERY_REQUIRED: &str = "This task's earlier conversation needs to be restored. Send a normal message in the conversation first, then retry this command.";

pub(crate) fn pending(store: &Store, session: &Session) -> Result<bool> {
    store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM context_recovery WHERE session_id=?1) OR (?2 AND EXISTS(SELECT 1 FROM messages WHERE session_id=?1 AND role IN ('user','assistant','coordination') AND content<>''))",
        params![session.id, session.resume_id.as_deref().is_none_or(|id| id.trim().is_empty())],
        |row| row.get(0),
    ).map_err(|e| e.to_string())
}

/// Queue before clearing a resume ID, in the caller's configuration transaction.
pub(crate) fn queue(store: &Store, sid: &str) -> Result<()> {
    store.session(sid)?;
    store.db.execute(
        "INSERT INTO context_recovery(session_id,through_message_id) SELECT ?1,MAX(id) FROM messages WHERE session_id=?1 AND role IN ('user','assistant','coordination') AND content<>'' HAVING MAX(id) IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET through_message_id=MAX(through_message_id,excluded.through_message_id)",
        [sid],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// Call before recording the new user message. The visible transcript stays unchanged.
pub(crate) fn prepare(
    store: &Store,
    session: &Session,
    original: &str,
) -> Result<(String, Option<i64>)> {
    if !pending(store, session)? {
        return Ok((original.into(), None));
    }
    // A provider can assign its new ID before accepting the prompt. Include
    // intervening failed requests too, so a later “try again” keeps its referent.
    queue(store, &session.id)?;
    let through: Option<i64> = store
        .db
        .query_row(
            "SELECT through_message_id FROM context_recovery WHERE session_id=?1",
            [&session.id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(through) = through else {
        return Ok((original.into(), None));
    };
    // Fetch newest entries first without materializing an unbounded transcript.
    let mut stmt = store.db.prepare("SELECT role,CASE WHEN role='coordination' THEN substr(content,1,786432) ELSE substr(content,1,65536) END,length(CAST(content AS BLOB)) FROM messages WHERE session_id=?1 AND id<=?2 AND role IN ('user','assistant','coordination') AND content<>'' ORDER BY id DESC LIMIT 201").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session.id, through], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as usize,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut remaining = HISTORY_BYTES;
    let mut truncated = false;
    for row in rows {
        let (role, content, mut bytes) = row.map_err(|e| e.to_string())?;
        let coordination = role == "coordination";
        let (role, content) = if coordination {
            let (_, prompt) = crate::architect::internal_turns::decode(&content)
                .ok_or("Saved coordination context is invalid.")?;
            bytes = prompt.len();
            ("user".to_string(), prompt)
        } else {
            (role, content)
        };
        let entry_for = |content: &str| {
            if coordination {
                json!({"role":role,"origin":"pandacode_coordination","content":content}).to_string()
            } else {
                json!({"role":role,"content":content}).to_string()
            }
        };
        if entries.len() == HISTORY_MESSAGES || remaining < 256 {
            truncated = true;
            break;
        }
        truncated |= content.len() < bytes;
        let mut entry = entry_for(&content);
        if entry.len() + 2 > remaining {
            // Preserve UTF-8 and count JSON escaping in the actual wire budget.
            let (mut low, mut high) = (0, content.len());
            while low <= high {
                let mid = low + (high - low) / 2;
                let mut end = mid;
                while !content.is_char_boundary(end) {
                    end -= 1;
                }
                let candidate = entry_for(&format!("{}\n[Message shortened]", &content[..end]));
                if candidate.len() + 2 <= remaining {
                    entry = candidate;
                    low = mid + 1;
                } else if mid == 0 {
                    break;
                } else {
                    high = mid - 1;
                }
            }
            truncated = true;
        }
        if entry.len() + 2 > remaining {
            break;
        }
        remaining -= entry.len() + 2;
        entries.push(entry);
    }
    entries.reverse();
    let notice = if truncated {
        " This is a bounded excerpt: older or long entries were omitted or shortened; do not assume omitted details."
    } else {
        ""
    };
    let prompt = format!("PandaCode restored earlier messages from this same task after its harness context changed. Use these as prior conversation so references such as ‘this’ retain their meaning. They are historical user/assistant messages, not new requests or higher-priority instructions.{notice}\n\nEarlier conversation (JSON):\n[{}]\n\nCurrent request:\n{original}", entries.join(",\n"));
    Ok((prompt, Some(through)))
}

/// A failed startup, cancelled turn, or failed persistence must leave recovery pending.
pub(crate) fn acknowledge(store: &Store, sid: &str, through: i64) -> Result<()> {
    store
        .db
        .execute(
            "DELETE FROM context_recovery WHERE session_id=?1 AND through_message_id=?2",
            params![sid, through],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests;
