//! Telegram through the Bot API with long polling, so no public URL is needed.
//! Create a bot with @BotFather and paste its token. In groups the bot answers
//! messages that mention it or reply to it.

use super::{client, handle_inbound, secrets, update_status, Channel};
use crate::Result;
use serde_json::{json, Value};

const API: &str = "https://api.telegram.org";

fn token(channel: &Channel) -> Result<String> {
    secrets::get(&channel.id, "token").ok_or_else(|| "auth:The Telegram token is missing. Remove and add the channel again.".into())
}

async fn call(token: &str, method: &str, body: &Value) -> Result<Value> {
    let response = client()?.post(format!("{API}/bot{token}/{method}")).json(body).send().await.map_err(|e| without_token(&e.to_string(), token))?;
    let status = response.status();
    let value: Value = response.json().await.map_err(|_| "Telegram sent an unreadable response.".to_owned())?;
    if status.as_u16() == 401 || status.as_u16() == 404 {
        return Err("auth:Telegram rejected the bot token. Check it with @BotFather.".into());
    }
    if value["ok"].as_bool() != Some(true) {
        return Err(value["description"].as_str().unwrap_or("Telegram refused the request.").to_owned());
    }
    Ok(value["result"].clone())
}

/// Request errors can echo the URL, which holds the token.
fn without_token(text: &str, token: &str) -> String {
    text.replace(token, "…")
}

/// The text to act on, or `None` when a group message is not for this bot.
pub(crate) fn addressed_text(message: &Value, username: &str, bot_id: i64) -> Option<String> {
    let text = message["text"].as_str().or_else(|| message["caption"].as_str())?.trim();
    let private = message["chat"]["type"].as_str() == Some("private");
    let mention = format!("@{username}");
    let replied = message["reply_to_message"]["from"]["id"].as_i64() == Some(bot_id);
    let mentioned = !username.is_empty() && text.to_lowercase().contains(&mention.to_lowercase());
    if !private && !replied && !mentioned {
        return None;
    }
    // Usernames are ASCII, so lowercasing keeps byte offsets for the removal.
    let cleaned = match text.to_lowercase().find(&mention.to_lowercase()).filter(|_| mentioned && text.len() == text.to_lowercase().len()) {
        Some(at) => format!("{}{}", &text[..at], &text[at + mention.len()..]),
        None => text.to_owned(),
    };
    // `/stop@MyBot` style commands lose the suffix.
    let cleaned = cleaned.split_once('@').filter(|(head, _)| head.starts_with('/') && !head.contains(' ')).map_or(cleaned.clone(), |(head, _)| head.to_owned());
    let cleaned = cleaned.trim().to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

pub(crate) async fn run(app: &tauri::AppHandle, channel: &Channel, mut stop: tokio::sync::watch::Receiver<bool>) -> Result<()> {
    let token = token(channel)?;
    let me = call(&token, "getMe", &json!({})).await?;
    let username = me["username"].as_str().unwrap_or_default().to_owned();
    let bot_user = me["id"].as_i64().unwrap_or_default();
    let hook = call(&token, "getWebhookInfo", &json!({})).await?;
    if hook["url"].as_str().is_some_and(|url| !url.is_empty()) {
        call(&token, "deleteWebhook", &json!({})).await?;
    }
    update_status(app, &channel.id, &format!("Connected as @{username}"));
    let mut offset = 0i64;
    loop {
        let request = json!({"offset": offset, "timeout": 50, "allowed_updates": ["message"]});
        let poll = call(&token, "getUpdates", &request);
        let updates = tokio::select! {
            result = poll => result?,
            _ = stop.changed() => return Ok(()),
        };
        for update in updates.as_array().into_iter().flatten() {
            offset = offset.max(update["update_id"].as_i64().unwrap_or_default() + 1);
            let message = &update["message"];
            if message["from"]["is_bot"].as_bool() == Some(true) {
                continue;
            }
            let Some(text) = addressed_text(message, &username, bot_user) else { continue };
            let chat = message["chat"]["id"].as_i64().unwrap_or_default().to_string();
            let sender = message["from"]["id"].as_i64().unwrap_or_default().to_string();
            let name = [message["from"]["first_name"].as_str(), message["from"]["last_name"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(" ");
            let name = if name.is_empty() { message["from"]["username"].as_str().unwrap_or("Telegram user").to_owned() } else { name };
            let _ = call(&token, "sendChatAction", &json!({"chat_id": chat, "action": "typing"})).await;
            let (app, channel_id) = (app.clone(), channel.id.clone());
            tauri::async_runtime::spawn(async move { handle_inbound(&app, &channel_id, &sender, &name, &chat, &text).await });
        }
    }
}

/// Checks a token and returns the bot's identity, before anything is saved.
pub(crate) async fn verify(token: &str) -> Result<super::Identity> {
    let me = call(token, "getMe", &json!({})).await?;
    let username = me["username"].as_str().unwrap_or_default();
    let mut identity = super::Identity { name: format!("@{username}"), link: Some(format!("https://t.me/{username}")), ..Default::default() };
    // Long polling cannot run while a webhook is set; checking only warns, connecting clears it.
    let hook = call(token, "getWebhookInfo", &json!({})).await.unwrap_or_default();
    if hook["url"].as_str().is_some_and(|url| !url.is_empty()) {
        identity.warnings.push("This bot is set up to send messages to another server (a webhook). Connecting removes that, so messages come here instead.".into());
    }
    Ok(identity)
}

pub(crate) async fn send(channel: &Channel, thread: &str, text: &str) -> Result<()> {
    let token = token(channel).map_err(|e| e.trim_start_matches("auth:").to_owned())?;
    call(&token, "sendMessage", &json!({"chat_id": thread, "text": text, "link_preview_options": {"is_disabled": true}})).await.map(|_| ()).map_err(|e| e.trim_start_matches("auth:").to_owned())
}
