use rusqlite::params;
use super::channels::{self, discord, http, slack, telegram, web, whatsapp};
use super::{brief, gateway, guard, schedule, *};
use serde_json::json;

const MINUTE: i64 = 60_000;
const DAY: i64 = 24 * 60 * MINUTE;

struct Fixture {
    store: Store,
    root: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!("pandacode-bots-{}", crate::storage::id()));
    std::fs::create_dir_all(&root).unwrap();
    Fixture { store: Store::open(std::path::Path::new(":memory:")).unwrap(), root }
}

fn input(name: &str) -> BotInput {
    BotInput { name: name.into(), role: "Research assistant".into(), instructions: "Be brief.".into(), provider: "claude".into(), model: "default".into(), mode: "ask".into(), ..Default::default() }
}

fn bot(f: &Fixture) -> Bot {
    create(&f.store, &input("Scout"), &f.root).unwrap()
}

#[test]
fn creates_a_bot_with_its_own_workspace_project() {
    let f = fixture();
    let bot = bot(&f);
    assert_eq!(bot.name, "Scout");
    assert!(!bot.paused);
    assert_eq!(project_ids(&f.store).unwrap(), vec![bot.project_id.clone()]);
    let name: String = f.store.db.query_row("SELECT name FROM projects WHERE id=?1", [&bot.project_id], |r| r.get(0)).unwrap();
    assert_eq!(name, "Scout");
    assert!(std::fs::read_dir(&f.root).unwrap().count() == 1);
    assert_eq!(list(&f.store).unwrap().len(), 1);
    assert_eq!(activity(&f.store, &bot.id, None, 10).unwrap()[0].kind, "created");
}

#[test]
fn rejects_invalid_bot_settings() {
    let f = fixture();
    assert!(create(&f.store, &input("  "), &f.root).is_err());
    let mut bad = input("Scout");
    bad.mode = "yolo".into();
    assert!(create(&f.store, &bad, &f.root).is_err());
}

#[test]
fn memories_dedupe_and_bump_the_brief_version() {
    let f = fixture();
    let bot = bot(&f);
    let first = remember(&f.store, &bot.id, "Prefers metric units", "user").unwrap();
    let again = remember(&f.store, &bot.id, "prefers METRIC units", "bot").unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(memories(&f.store, &bot.id).unwrap().len(), 1);
    let version = get(&f.store, &bot.id).unwrap().memory_version;
    assert!(version > bot.memory_version);
    forget(&f.store, &bot.id, &first.id).unwrap();
    assert!(get(&f.store, &bot.id).unwrap().memory_version > version);
    assert!(forget(&f.store, &bot.id, &first.id).is_err());
    assert!(remember(&f.store, &bot.id, "   ", "user").is_err());
}

#[test]
fn editing_instructions_rebriefs_threads() {
    let f = fixture();
    let bot = bot(&f);
    let (session, created) = thread_session(&f.store, &bot, DESKTOP, DESKTOP, "Scout · You").unwrap();
    assert!(created);
    assert_eq!(brief_version_of(&f.store, &session).unwrap(), -1);
    mark_briefed(&f.store, &session, bot.memory_version).unwrap();
    let mut changed = input("Scout");
    changed.instructions = "Be thorough.".into();
    let updated = update(&f.store, &bot.id, &changed).unwrap();
    assert!(updated.memory_version > bot.memory_version);
    let (same, created) = thread_session(&f.store, &updated, DESKTOP, DESKTOP, "Scout · You").unwrap();
    assert_eq!((same.as_str(), created), (session.as_str(), false));
    assert_eq!(owner_of(&f.store, &session).unwrap().unwrap().0.id, bot.id);
}

#[test]
fn reset_wipes_conversations_memories_schedules_and_activity() {
    let f = fixture();
    let bot = bot(&f);
    let (session, _) = thread_session(&f.store, &bot, DESKTOP, DESKTOP, "t").unwrap();
    remember(&f.store, &bot.id, "Lives in Lisbon", "user").unwrap();
    let schedule_input = schedule::ScheduleInput { prompt: "Check the inbox".into(), kind: "interval".into(), interval_minutes: Some(60), ..Default::default() };
    schedule::create(&f.store, &bot.id, &schedule_input, "user", 0).unwrap();
    reset(&f.store, &bot.id).unwrap();
    assert!(memories(&f.store, &bot.id).unwrap().is_empty());
    assert!(schedule::list(&f.store, &bot.id).unwrap().is_empty());
    assert!(threads(&f.store, &bot.id).unwrap().is_empty());
    assert!(f.store.session(&session).is_err());
    assert!(activity(&f.store, &bot.id, None, 10).unwrap().iter().all(|a| a.kind == "reset"));
}

#[test]
fn delete_removes_the_bot_and_its_channels() {
    let f = fixture();
    let bot = bot(&f);
    let channel = channels::add(&f.store, &bot.id, &channels::ChannelInput { kind: "web".into(), ..Default::default() }).unwrap();
    assert_eq!(channels::web_token(&f.store, &channel.id).map(|token| token.len()), Some(64));
    assert!(channel.secrets.is_empty(), "the web link is not a keychain secret");
    assert!(!serde_json::to_string(&channel).unwrap().contains(&channels::web_token(&f.store, &channel.id).unwrap()), "the link token never reaches the webview through channel data");
    delete(&f.store, &bot.id).unwrap();
    assert!(get(&f.store, &bot.id).is_err());
    assert!(channels::get(&f.store, &channel.id).is_err());
    assert!(channels::web_token(&f.store, &channel.id).is_none());
}

#[test]
fn glob_rules_match_whole_text_and_substrings() {
    assert!(guard::matches("npm test*", "npm test -- --watch"));
    assert!(!guard::matches("npm test*", "run npm test"));
    assert!(guard::matches("*deploy*", "Run the deploy script"));
    assert!(guard::matches("a*b*c", "aXXbYYc"));
    assert!(!guard::matches("a*b*c", "aXXbYY"));
    assert!(guard::matches("git status", "Run: git status --short"));
    assert!(!guard::matches("", "anything"));
}

#[test]
fn deny_beats_allow_and_dangerous_actions_always_ask() {
    let f = fixture();
    let bot = bot(&f);
    assert_eq!(guard::decide(&f.store, &bot.id, "ls -la").unwrap(), guard::Decision::Ask);
    guard::add_rule(&f.store, &bot.id, "*", "allow").unwrap_err();
    guard::add_rule(&f.store, &bot.id, "ls*", "allow").unwrap();
    guard::add_rule(&f.store, &bot.id, "git*", "allow").unwrap();
    assert_eq!(guard::decide(&f.store, &bot.id, "ls -la").unwrap(), guard::Decision::Allow);
    assert_eq!(guard::decide(&f.store, &bot.id, "git push --force origin main").unwrap(), guard::Decision::Ask);
    guard::add_rule(&f.store, &bot.id, "ls -la", "deny").unwrap();
    assert_eq!(guard::decide(&f.store, &bot.id, "ls -la").unwrap(), guard::Decision::Deny);
    assert_eq!(guard::pattern_for("npm run build --prod\nmore"), "npm run build*");
    assert_eq!(guard::pattern_for("ls"), "ls");
}

#[test]
fn approval_replies_only_count_while_something_is_waiting() {
    assert_eq!(gateway::parse_command("yes", true), gateway::Command::Approve(true, false));
    assert_eq!(gateway::parse_command("Always.", true), gateway::Command::Approve(true, true));
    assert_eq!(gateway::parse_command("no", true), gateway::Command::Approve(false, false));
    assert_eq!(gateway::parse_command("yes", false), gateway::Command::Message);
    assert_eq!(gateway::parse_command("/STOP", false), gateway::Command::Stop);
    assert_eq!(gateway::parse_command("/new", false), gateway::Command::NewThread);
    assert_eq!(gateway::parse_command("/help", false), gateway::Command::Help);
}

#[test]
fn schedules_compute_their_next_local_run() {
    // Thursday 1970-01-01 10:00 UTC, no offset.
    let thursday = 10 * 60 * MINUTE;
    assert_eq!(schedule::next_run("interval", Some(30), None, thursday, 0), Some(thursday + 30 * MINUTE));
    assert_eq!(schedule::next_run("interval", Some(1), None, thursday, 0), None);
    assert_eq!(schedule::next_run("daily", None, Some("09:00"), thursday, 0), Some(DAY + 9 * 60 * MINUTE));
    assert_eq!(schedule::next_run("daily", None, Some("11:30"), thursday, 0), Some(11 * 60 * MINUTE + 30 * MINUTE));
    // Friday 10:00 -> weekdays at 09:00 skips the weekend to Monday.
    let friday = DAY + thursday;
    assert_eq!(schedule::next_run("weekdays", None, Some("09:00"), friday, 0), Some(4 * DAY + 9 * 60 * MINUTE));
    // A +2h zone: 09:00 local is 07:00 UTC.
    assert_eq!(schedule::next_run("daily", None, Some("09:00"), thursday, 2 * 60 * MINUTE), Some(DAY + 7 * 60 * MINUTE));
    assert_eq!(schedule::next_run("daily", None, Some("25:00"), thursday, 0), None);
}

#[test]
fn schedules_validate_and_advance() {
    let f = fixture();
    let bot = bot(&f);
    let once = schedule::ScheduleInput { title: "Reminder".into(), prompt: "Remind me to stretch".into(), kind: "once".into(), run_at: Some(10 * MINUTE), ..Default::default() };
    let created = schedule::create(&f.store, &bot.id, &once, "bot", 0).unwrap();
    assert_eq!(created.created_by, "bot");
    let empty = schedule::ScheduleInput { kind: "daily".into(), time_of_day: Some("09:00".into()), ..Default::default() };
    assert!(schedule::create(&f.store, &bot.id, &empty, "user", 0).is_err());
    let (due, _) = schedule::due(&f.store, 11 * MINUTE).unwrap();
    assert_eq!(due.len(), 1);
    schedule::advance(&f.store, &due[0], 11 * MINUTE).unwrap();
    let after = &schedule::list(&f.store, &bot.id).unwrap()[0];
    assert!(!after.enabled, "one-off schedules turn off after running");
    assert!(schedule::due(&f.store, 20 * MINUTE).unwrap().0.is_empty());
}

#[test]
fn paused_bots_skip_heartbeats() {
    let f = fixture();
    let mut with_heartbeat = input("Watcher");
    with_heartbeat.heartbeat_minutes = 60;
    let bot = create(&f.store, &with_heartbeat, &f.root).unwrap();
    let due_at = bot.next_heartbeat_at.unwrap();
    assert_eq!(schedule::due(&f.store, due_at + 1).unwrap().1.len(), 1);
    set_paused(&f.store, &bot.id, true).unwrap();
    assert!(schedule::due(&f.store, due_at + 1).unwrap().1.is_empty());
}

#[test]
fn brief_carries_identity_memories_and_channel_etiquette() {
    let f = fixture();
    let bot = bot(&f);
    remember(&f.store, &bot.id, "Works at Acme\nin sales", "user").unwrap();
    let text = brief::compose(&get(&f.store, &bot.id).unwrap(), &memories(&f.store, &bot.id).unwrap(), "telegram", "Ana");
    assert!(text.starts_with("\n\nBot brief:\nYou are Scout, Research assistant"));
    assert!(text.contains("- Works at Acme in sales"));
    assert!(text.contains("phone chat app"));
    assert!(text.contains("from Ana"));
    assert!(brief::compose(&bot, &[], "schedule", "").contains(gateway::NOTHING));
}

#[test]
fn channels_require_their_secrets_and_valid_kinds() {
    let f = fixture();
    let bot = bot(&f);
    let missing = channels::ChannelInput { kind: "telegram".into(), ..Default::default() };
    assert!(channels::add(&f.store, &bot.id, &missing).unwrap_err().contains("token"));
    let unknown = channels::ChannelInput { kind: "fax".into(), ..Default::default() };
    assert!(channels::add(&f.store, &bot.id, &unknown).is_err());
    let whatsapp_bad = channels::ChannelInput { kind: "whatsapp".into(), config: json!({"phoneNumberId": "abc"}), ..Default::default() };
    assert!(channels::add(&f.store, &bot.id, &whatsapp_bad).is_err());
    let telegram = channels::ChannelInput { kind: "telegram".into(), label: "Phone".into(), secrets: [("token".to_owned(), "123:abc".to_owned())].into(), ..Default::default() };
    let channel = channels::add(&f.store, &bot.id, &telegram).unwrap();
    assert_eq!(channel.secrets, vec!["token".to_owned()]);
    assert_eq!(channels::list(&f.store, &bot.id).unwrap().len(), 1);
    let serialized = serde_json::to_string(&channel).unwrap();
    assert!(!serialized.contains("123:abc"), "secrets never leave native code");
}

#[test]
fn pairing_codes_are_stable_until_decided() {
    let f = fixture();
    let bot = bot(&f);
    let telegram = channels::ChannelInput { kind: "telegram".into(), secrets: [("token".to_owned(), "1:x".to_owned())].into(), ..Default::default() };
    let channel = channels::add(&f.store, &bot.id, &telegram).unwrap();
    let code = channels::request_pairing(&f.store, &channel.id, "42", "Ana").unwrap().unwrap();
    assert_eq!(code.len(), 4);
    assert_eq!(channels::request_pairing(&f.store, &channel.id, "42", "Ana").unwrap().unwrap(), code);
    assert!(!channels::is_approved(&f.store, &channel.id, "42").unwrap());
    channels::decide_pairing(&f.store, &channel.id, "42", "approved").unwrap();
    assert!(channels::is_approved(&f.store, &channel.id, "42").unwrap());
    channels::decide_pairing(&f.store, &channel.id, "42", "blocked").unwrap();
    assert!(channels::request_pairing(&f.store, &channel.id, "42", "Ana").unwrap().is_none());
    assert!(channels::decide_pairing(&f.store, &channel.id, "42", "maybe").is_err());
    assert_eq!(channels::pairings(&f.store, &bot.id).unwrap()[0].status, "blocked");
    assert_eq!(channels::direct_thread("discord", "42"), "dm:42");
}

#[test]
fn long_replies_split_on_natural_boundaries() {
    let text = format!("{}\n\n{}", "a".repeat(30), "b".repeat(30));
    assert_eq!(channels::chunks(&text, 40), vec!["a".repeat(30), "b".repeat(30)]);
    let words = "word ".repeat(30);
    let parts = channels::chunks(&words, 32);
    assert!(parts.iter().all(|part| part.chars().count() <= 32 && !part.ends_with(' ')));
    assert_eq!(parts.join(" ").split_whitespace().count(), 30);
    assert_eq!(channels::chunks("ünïcödé".repeat(10).as_str(), 7).len(), 10);
    assert!(channels::chunks("   ", 10).is_empty());
}

#[test]
fn telegram_answers_private_chats_mentions_and_replies() {
    let private = json!({"text": "hello", "chat": {"type": "private"}, "from": {"id": 1}});
    assert_eq!(telegram::addressed_text(&private, "scout_bot", 99).as_deref(), Some("hello"));
    let group = json!({"text": "hello all", "chat": {"type": "group"}});
    assert_eq!(telegram::addressed_text(&group, "scout_bot", 99), None);
    let mention = json!({"text": "@Scout_Bot what's up", "chat": {"type": "group"}});
    assert_eq!(telegram::addressed_text(&mention, "scout_bot", 99).as_deref(), Some("what's up"));
    let reply = json!({"text": "and then?", "chat": {"type": "supergroup"}, "reply_to_message": {"from": {"id": 99}}});
    assert_eq!(telegram::addressed_text(&reply, "scout_bot", 99).as_deref(), Some("and then?"));
    let command = json!({"text": "/stop@scout_bot", "chat": {"type": "group"}});
    assert_eq!(telegram::addressed_text(&command, "scout_bot", 99).as_deref(), Some("/stop"));
}

#[test]
fn slack_handles_mentions_in_threads_and_direct_messages() {
    let mention = json!({"type": "app_mention", "user": "U1", "channel": "C1", "ts": "1.2", "text": "<@B1> summarize"});
    assert_eq!(slack::inbound(&mention, "B1"), Some(("U1".into(), "C1|1.2".into(), "summarize".into())));
    let threaded = json!({"type": "app_mention", "user": "U1", "channel": "C1", "ts": "1.3", "thread_ts": "1.0", "text": "<@B1> more"});
    assert_eq!(slack::inbound(&threaded, "B1").unwrap().1, "C1|1.0");
    let direct = json!({"type": "message", "channel_type": "im", "user": "U1", "channel": "D1", "ts": "2", "text": "hi"});
    assert_eq!(slack::inbound(&direct, "B1").unwrap().1, "D1");
    let channel_message = json!({"type": "message", "channel_type": "channel", "user": "U1", "channel": "C1", "ts": "3", "text": "hi"});
    assert_eq!(slack::inbound(&channel_message, "B1"), None);
    let own = json!({"type": "message", "channel_type": "im", "bot_id": "B1", "channel": "D1", "ts": "4", "text": "echo"});
    assert_eq!(slack::inbound(&own, "B1"), None);
}

#[test]
fn discord_answers_dms_and_mentions_but_not_bots() {
    let dm = json!({"author": {"id": "7", "username": "ana"}, "channel_id": "55", "content": "hi"});
    assert_eq!(discord::inbound(&dm, "1"), Some(("7".into(), "ana".into(), "55".into(), "hi".into())));
    let guild = json!({"guild_id": "9", "author": {"id": "7", "username": "ana"}, "channel_id": "55", "content": "hi", "mentions": []});
    assert_eq!(discord::inbound(&guild, "1"), None);
    let mentioned = json!({"guild_id": "9", "author": {"id": "7", "global_name": "Ana"}, "channel_id": "55", "content": "<@1> status", "mentions": [{"id": "1"}]});
    assert_eq!(discord::inbound(&mentioned, "1").unwrap().3, "status");
    let bot = json!({"author": {"id": "8", "bot": true}, "channel_id": "55", "content": "hi"});
    assert_eq!(discord::inbound(&bot, "1"), None);
}

#[test]
fn whatsapp_checks_signatures_and_reads_text_messages() {
    use hmac::{Hmac, Mac};
    let body = br#"{"entry":[{"changes":[{"value":{"contacts":[{"wa_id":"351900","profile":{"name":"Ana"}}],"messages":[{"from":"351900","type":"text","text":{"body":"hello"}},{"from":"351900","type":"image"}]}}]}]}"#;
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"app-secret").unwrap();
    mac.update(body);
    let signature: String = mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
    assert!(whatsapp::signature_valid("app-secret", body, &format!("sha256={signature}")));
    assert!(!whatsapp::signature_valid("other", body, &format!("sha256={signature}")));
    assert!(!whatsapp::signature_valid("app-secret", body, &signature));
    let payload: serde_json::Value = serde_json::from_slice(body).unwrap();
    assert_eq!(whatsapp::messages(&payload), vec![("351900".into(), "Ana".into(), "hello".into())]);
}

#[test]
fn local_server_parses_requests_and_guards_hosts() {
    let request = http::parse_head("GET /api/chat/abc/events?after=12&x=a%20b+c HTTP/1.1\r\nHost: 127.0.0.1:47321\r\nAuthorization: Bearer t0k").unwrap();
    assert_eq!(request.path, "/api/chat/abc/events");
    assert_eq!(request.query["after"], "12");
    assert_eq!(request.query["x"], "a b c");
    assert_eq!(request.bearer(), Some("t0k"));
    assert!(request.loopback_host());
    let rebound = http::parse_head("GET / HTTP/1.1\r\nHost: evil.example:47321").unwrap();
    assert!(!rebound.loopback_host());
    assert!(http::parse_head("garbage").is_none());
    assert!(http::same("abc", "abc") && !http::same("abc", "abd") && !http::same("abc", "ab"));
    assert_eq!(web::visible("hi\n\nBot brief:\nYou are Scout"), "hi");
    assert_eq!(web::new_token().len(), 64);
    assert_eq!(web::page("not a channel!").status, 404);
    let engine = http::Response::markdown_engine();
    assert_eq!((engine.status, engine.content_type), (200, "text/javascript; charset=utf-8"));
    assert!(std::str::from_utf8(&engine.body).unwrap().trim_end().ends_with("export default createMd4cModule;"));
}

#[test]
fn only_registered_bot_tools_skip_approval() {
    assert!(super::tools::self_scoped("pandacode_bot_remember"));
    assert!(super::tools::self_scoped("pandacode_bot_notify"));
    assert!(!super::tools::self_scoped("pandacode_bot_anything_else"));
    assert!(!super::tools::self_scoped("pandacode_open_browser"));
    assert!(super::tools::catalog().iter().all(|tool| tool["name"].as_str().is_some_and(super::tools::self_scoped)));
}

#[test]
fn setup_catches_misplaced_and_truncated_tokens_before_any_request() {
    let input = |kind: &str, secrets: &[(&str, &str)], config: serde_json::Value| channels::ChannelInput { kind: kind.into(), config, secrets: secrets.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(), ..Default::default() };
    assert!(channels::check_format(&input("telegram", &[("token", "123456789:AAE1234567890abcdefghijklmnopqrstu")], json!({}))).is_ok());
    assert!(channels::check_format(&input("telegram", &[("token", "123456789")], json!({}))).unwrap_err().contains("BotFather"));
    assert!(channels::check_format(&input("slack", &[("token", "xapp-1"), ("app_token", "xoxb-1")], json!({}))).unwrap_err().contains("swapped"));
    assert!(channels::check_format(&input("slack", &[("token", "xoxp-1"), ("app_token", "xapp-1")], json!({}))).unwrap_err().contains("xoxb-"));
    assert!(channels::check_format(&input("slack", &[("token", "xoxb-1"), ("app_token", "xapp-1")], json!({}))).is_ok());
    assert!(channels::check_format(&input("discord", &[("token", "client-secret")], json!({}))).unwrap_err().contains("Reset Token"));
    assert!(channels::check_format(&input("discord", &[("token", &format!("{}.{}.{}", "a".repeat(24), "b".repeat(6), "c".repeat(38)))], json!({}))).is_ok());
    assert!(channels::check_format(&input("whatsapp", &[], json!({"phoneNumberId": "+351 912"}))).unwrap_err().contains("not the phone number"));
    assert!(discord::invite_url("123").starts_with("https://discord.com/oauth2/authorize?client_id=123&scope=bot&permissions="));
}

#[test]
fn whatsapp_gets_a_shown_verify_token_and_settings_are_validated() {
    let f = fixture();
    let bot = bot(&f);
    let whatsapp = channels::ChannelInput { kind: "whatsapp".into(), config: json!({"phoneNumberId": "123456"}), secrets: [("access_token".to_owned(), "EAAG".to_owned()), ("signing_secret".to_owned(), "s3cret".to_owned())].into(), ..Default::default() };
    let channel = channels::add(&f.store, &bot.id, &whatsapp).unwrap();
    assert!(channel.config["verifyToken"].as_str().unwrap().starts_with("swarm-"));
    assert_eq!(channel.secrets, vec!["access_token".to_owned(), "signing_secret".to_owned()]);
    let configured = channels::configure(&f.store, &channel.id, &json!({"publicUrl": "https://quiet-river.trycloudflare.com/"})).unwrap();
    assert_eq!(configured.config["publicUrl"], "https://quiet-river.trycloudflare.com");
    assert!(channels::configure(&f.store, &channel.id, &json!({"publicUrl": "http://insecure.example"})).is_err());
    assert!(channels::configure(&f.store, &channel.id, &json!({"publicUrl": "https://user:pw@example.com"})).is_err());
    assert!(channels::configure(&f.store, &channel.id, &json!({"remote": true})).is_err(), "remote access is a web chat setting");
    let web = channels::add(&f.store, &bot.id, &channels::ChannelInput { kind: "web".into(), ..Default::default() }).unwrap();
    assert_eq!(channels::configure(&f.store, &web.id, &json!({"remote": true})).unwrap().config["remote"], true);
    assert!(channels::configure(&f.store, &web.id, &json!({"secret_names": "[]"})).is_err());
}

#[test]
fn summaries_name_the_desktop_conversation_once_it_exists() {
    let f = fixture();
    let bot = bot(&f);
    assert_eq!(summaries(&f.store).unwrap()[0].desktop_session, None);
    let (session, _) = thread_session(&f.store, &bot, DESKTOP, DESKTOP, "Scout · You").unwrap();
    thread_session(&f.store, &bot, "channel", "chat", "Scout · Ana").unwrap();
    let json = serde_json::to_value(&summaries(&f.store).unwrap()[0]).unwrap();
    assert_eq!(json["desktopSession"], session.as_str());
    assert_eq!(json["name"], "Scout", "bot fields stay at the top level");
}

#[test]
fn summaries_carry_a_one_line_preview_of_the_latest_message() {
    let f = fixture();
    let bot = bot(&f);
    assert!(summaries(&f.store).unwrap()[0].last_message.is_none());
    let (session, _) = thread_session(&f.store, &bot, DESKTOP, DESKTOP, "Scout · You").unwrap();
    let insert = |role: &str, content: &str, at: i64| {
        f.store.db.execute("INSERT INTO messages(session_id,role,content,created_at) VALUES(?1,?2,?3,?4)", params![session, role, content, at]).unwrap();
    };
    insert("user", "What changed today?", 10);
    insert("assistant", "## Done\n\nInbox at **zero**.   Two `replies` ready.", 20);
    insert("system", "Run finished", 30);
    let json = serde_json::to_value(&summaries(&f.store).unwrap()[0]).unwrap();
    assert_eq!(json["lastMessage"]["role"], "assistant");
    assert_eq!(json["lastMessage"]["text"], "Done Inbox at zero. Two replies ready.");
    assert_eq!(json["lastMessage"]["at"], 20);
    insert("user", &"x".repeat(400), 40);
    let long = summaries(&f.store).unwrap()[0].last_message.clone().unwrap();
    assert_eq!(long.role, "user");
    assert!(long.text.chars().count() <= 161, "previews stay short");
}

#[test]
fn schema_version_includes_bots() {
    let f = fixture();
    let version: i64 = f.store.db.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, crate::storage::SCHEMA_VERSION);
}

/// Talks to the real services with made-up credentials: proves the requests are well-formed
/// and that each rejection becomes a message a person can act on. No account needed.
/// cargo test --lib bots::tests::live_services_reject_fake_credentials_clearly -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn live_services_reject_fake_credentials_clearly() {
    let input = |kind: &str, secrets: &[(&str, &str)], config: serde_json::Value| channels::ChannelInput { kind: kind.into(), config, secrets: secrets.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(), ..Default::default() };
    let telegram = channels::verify(&input("telegram", &[("token", "123456789:AAE_fake_token_for_pandacode_tests_00")], json!({}))).await.unwrap_err();
    println!("telegram: {telegram}");
    assert!(telegram.contains("Telegram rejected the bot token"), "{telegram}");
    let slack = channels::verify(&input("slack", &[("token", "xoxb-1234567890123-1234567890123-abcdefghijklmnopqrstuvwx"), ("app_token", "xapp-1-A0000000000-1234567890123-abcdef")], json!({}))).await.unwrap_err();
    println!("slack: {slack}");
    assert!(slack.contains("Slack didn't accept the bot token"), "{slack}");
    let discord = channels::verify(&input("discord", &[("token", &format!("{}.{}.{}", "MTA".repeat(8), "Gabcde", "x".repeat(38)))], json!({}))).await.unwrap_err();
    println!("discord: {discord}");
    assert!(discord.contains("Discord rejected the bot token"), "{discord}");
    let whatsapp = channels::verify(&input("whatsapp", &[("access_token", "EAAG_fake_token")], json!({"phoneNumberId": "123456789012345"}))).await.unwrap_err();
    println!("whatsapp: {whatsapp}");
    assert!(whatsapp.contains("Meta rejected the access token") || whatsapp.starts_with("WhatsApp:"), "{whatsapp}");
}
