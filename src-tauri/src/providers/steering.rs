//! Steering: a message for an agent while its turn is running. Claude Code takes
//! it on stream input at the next tool boundary (or as a following turn); Codex
//! takes it through `turn/steer` on the active turn; Pi through its `steer` command,
//! delivered before its next model call. Other harnesses decline, and
//! the app keeps the message queued for after the turn instead.

use super::goals::{Purpose, RpcReply};
use super::*;

const MAX_STEER_BYTES: usize = 128 * 1024;

impl Protocol {
    fn claude_family(&self) -> bool {
        matches!(self.config.provider.as_str(), "claude" | "glm" | "mimo")
    }

    /// Claude echoes the opening prompt first; each later echo means a steering message was consumed.
    pub(super) fn claude_replay(&mut self) {
        if !self.prompt_replayed {
            self.prompt_replayed = true;
        } else {
            self.steers_pending = self.steers_pending.saturating_sub(1);
        }
    }

    pub(super) fn steer(&mut self, text: String, reply: RpcReply) -> Step {
        let mut step = Step::default();
        let text = text.trim().to_owned();
        if text.is_empty() || text.len() > MAX_STEER_BYTES {
            let _ = reply.send(Err("Write a steering message under 128 KiB.".into()));
            return step;
        }
        if self.claude_family() {
            if self.phase == 0 {
                let _ = reply.send(Err("The agent has not started its turn yet.".into()));
                return step;
            }
            let mut request = json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":text}]},"parent_tool_use_id":null});
            if !self.remote_id.is_empty() {
                request["session_id"] = json!(self.remote_id);
            }
            step.write.push(request);
            self.steers_pending += 1;
            let _ = reply.send(Ok(json!({"steered": true})));
            return step;
        }
        if self.config.provider == "codex" {
            if self.remote_id.is_empty() || self.turn_id.is_empty() {
                let _ = reply.send(Err("The agent has no active turn to steer.".into()));
                return step;
            }
            let params = json!({"threadId":self.remote_id,"expectedTurnId":self.turn_id,"input":[{"type":"text","text":text,"text_elements":[]}]});
            // A refused request drops the reply, which the caller reports as not accepted;
            // it must never fail the running turn.
            let _ = self.goal_request("turn/steer", params, Purpose::Rpc(reply), &mut step);
            return step;
        }
        if self.config.provider == "pi" {
            return self.pi_steer(text, reply);
        }
        let _ = reply.send(Err("This agent cannot be steered mid-turn; the message stays queued.".into()));
        step
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::config;
    use super::*;

    fn steer(protocol: &mut Protocol, text: &str) -> (Step, tokio::sync::oneshot::Receiver<Result<Value, String>>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (protocol.steer(text.into(), tx), rx)
    }

    #[test]
    fn claude_steers_on_stream_input_and_waits_for_a_late_steer_turn() {
        let mut p = Protocol::new(config("claude"));
        p.phase = 1;
        p.remote_id = "session".into();
        let (step, mut reply) = steer(&mut p, "  also add tests  ");
        assert_eq!(step.write.len(), 1);
        assert_eq!(step.write[0]["type"], "user");
        assert_eq!(step.write[0]["message"]["content"][0]["text"], "also add tests");
        assert_eq!(step.write[0]["session_id"], "session");
        assert!(reply.try_recv().unwrap().is_ok());
        // The opening prompt is echoed first; it is not a steering message.
        p.receive(json!({"type":"user","isReplay":true,"message":{"role":"user","content":[{"type":"text","text":"original"}]}})).unwrap();
        assert_eq!(p.steers_pending, 1);
        // A result before the steer is echoed means Claude will answer it in another turn.
        let early = p.receive(json!({"type":"result","subtype":"success","result":"first answer"})).unwrap();
        assert!(!early.done, "the run stays open for the steered turn");
        p.receive(json!({"type":"user","isReplay":true,"message":{"role":"user","content":[{"type":"text","text":"also add tests"}]}})).unwrap();
        assert_eq!(p.steers_pending, 0);
        assert!(p.receive(json!({"type":"result","subtype":"success","result":"done"})).unwrap().done);
    }

    #[test]
    fn claude_steer_consumed_mid_turn_finishes_with_one_result() {
        let mut p = Protocol::new(config("glm"));
        p.phase = 1;
        let _ = steer(&mut p, "steer");
        for text in ["prompt", "steer"] {
            p.receive(json!({"type":"user","isReplay":true,"message":{"role":"user","content":[{"type":"text","text":text}]}})).unwrap();
        }
        assert!(p.receive(json!({"type":"result","subtype":"success","result":"ok"})).unwrap().done);
    }

    #[test]
    fn pi_steers_the_running_turn_and_reports_the_reply() {
        let mut idle = Protocol::new(config("pi"));
        let (step, mut reply) = steer(&mut idle, "too early");
        assert!(step.write.is_empty());
        assert!(reply.try_recv().unwrap().is_err());

        let mut p = Protocol::new(config("pi"));
        p.phase = 1;
        let (step, mut accepted) = steer(&mut p, " check the docs ");
        assert_eq!(step.write[0]["type"], "steer");
        assert_eq!(step.write[0]["message"], "check the docs");
        let id = step.write[0]["id"].as_str().unwrap().to_owned();
        let (step, mut refused) = steer(&mut p, "second");
        let refused_id = step.write[0]["id"].clone();
        p.receive(json!({"type":"response","command":"steer","id":id,"success":true,"data":{"disposition":"queued"}})).unwrap();
        assert!(accepted.try_recv().unwrap().is_ok());
        // A refused steer stays queued in the app and never fails the run.
        let step = p.receive(json!({"type":"response","command":"steer","id":refused_id,"success":false,"error":"Not streaming"})).unwrap();
        assert!(!step.done);
        assert!(refused.try_recv().unwrap().is_err());
        p.receive(json!({"type":"agent_settled","aborted":false})).unwrap();
        let (step, mut late) = steer(&mut p, "after settling");
        assert!(step.write.is_empty());
        assert!(late.try_recv().unwrap().is_err());
    }

    #[test]
    fn codex_steers_the_active_turn_and_other_harnesses_decline() {
        let mut p = Protocol::new(config("codex"));
        p.phase = 1;
        p.remote_id = "thread-1".into();
        p.turn_id = "turn-7".into();
        let (step, _reply) = steer(&mut p, "focus on the parser");
        let request = step.write.iter().find(|request| request["method"] == "turn/steer").expect("turn/steer request");
        assert_eq!(request["params"]["expectedTurnId"], "turn-7");
        assert_eq!(request["params"]["threadId"], "thread-1");
        assert_eq!(request["params"]["input"][0]["text"], "focus on the parser");

        let mut idle = Protocol::new(config("codex"));
        idle.remote_id = "thread-1".into();
        let (step, mut reply) = steer(&mut idle, "too early");
        assert!(step.write.is_empty());
        assert!(reply.try_recv().unwrap().is_err());

        for provider in ["kimi", "opencode", "grok", "antigravity"] {
            let mut p = Protocol::new(config(provider));
            p.phase = 1;
            let (step, mut reply) = steer(&mut p, "hello");
            assert!(step.write.is_empty(), "{provider} queues instead");
            assert!(reply.try_recv().unwrap().is_err());
        }
        let mut p = Protocol::new(config("claude"));
        p.phase = 1;
        let (_, mut reply) = steer(&mut p, "   ");
        assert!(reply.try_recv().unwrap().is_err());
    }
}
