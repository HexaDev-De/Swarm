//! Discord through the Gateway WebSocket, so no public URL is needed. The bot
//! needs the Message Content intent enabled in the Developer Portal. It answers
//! direct messages, and server messages that mention it.

use super::{client, handle_inbound, secrets, update_status, Channel};
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

const API: &str = "https://discord.com/api/v10";
/// GUILD_MESSAGES | DIRECT_MESSAGES | MESSAGE_CONTENT.
const INTENTS: u64 = (1 << 9) | (1 << 12) | (1 << 15);

async fn rest(token: &str, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
    let mut request = client()?.request(method, format!("{API}{path}")).header("Authorization", format!("Bot {token}"));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    match status {
        200..=299 => Ok(value),
        401 => Err("auth:Discord rejected the bot token. Reset it in the Developer Portal and add the channel again.".into()),
        _ => Err(format!("Discord: {}", value["message"].as_str().unwrap_or("request failed"))),
    }
}

/// What a MESSAGE_CREATE asks for: (sender, name, thread, text), or `None` to ignore it.
pub(crate) fn inbound(message: &Value, bot_user: &str) -> Option<(String, String, String, String)> {
    if message["author"]["bot"].as_bool() == Some(true) {
        return None;
    }
    let direct = message["guild_id"].is_null();
    let mentioned = message["mentions"].as_array().into_iter().flatten().any(|user| user["id"].as_str() == Some(bot_user));
    if !direct && !mentioned {
        return None;
    }
    let text = message["content"].as_str()?.replace(&format!("<@{bot_user}>"), "").replace(&format!("<@!{bot_user}>"), "").trim().to_owned();
    if text.is_empty() {
        return None;
    }
    let author = &message["author"];
    let name = author["global_name"].as_str().or(author["username"].as_str()).unwrap_or("Discord user").to_owned();
    Some((author["id"].as_str()?.to_owned(), name, message["channel_id"].as_str()?.to_owned(), text))
}

pub(crate) async fn run(app: &tauri::AppHandle, channel: &Channel, mut stop: tokio::sync::watch::Receiver<bool>) -> Result<()> {
    let token = secrets::get(&channel.id, "token").ok_or("auth:The Discord bot token is missing.")?;
    let gateway = rest(&token, reqwest::Method::GET, "/gateway/bot", None).await?;
    let url = format!("{}/?v=10&encoding=json", gateway["url"].as_str().ok_or("Discord did not return a gateway URL.")?);
    let (mut socket, _) = tokio_tungstenite::connect_async(url.as_str()).await.map_err(|e| format!("Could not open Discord's gateway: {e}"))?;
    let mut sequence = Value::Null;
    // Replaced on Hello; a plain interval would tick at once and heartbeat before Hello.
    let mut heartbeat = tokio::time::interval_at(tokio::time::Instant::now() + Duration::from_secs(3600), Duration::from_secs(3600));
    let mut bot_user = String::new();
    loop {
        let frame = tokio::select! {
            frame = socket.next() => frame,
            _ = heartbeat.tick() => {
                socket.send(Message::text(json!({"op": 1, "d": sequence}).to_string())).await.map_err(|e| e.to_string())?;
                continue;
            }
            _ = stop.changed() => { let _ = socket.close(None).await; return Ok(()); }
        };
        let text = match frame {
            Some(Ok(Message::Text(text))) => text.to_string(),
            Some(Ok(Message::Close(Some(close)))) => {
                return Err(match u16::from(close.code) {
                    4004 => "auth:Discord rejected the bot token.".into(),
                    4014 => "auth:Turn on the Message Content intent for this bot in Discord's Developer Portal.".into(),
                    code => format!("Discord closed the connection ({code})."),
                });
            }
            Some(Ok(Message::Close(None))) | None => return Ok(()),
            Some(Ok(_)) => continue,
            Some(Err(error)) => return Err(error.to_string()),
        };
        let payload: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !payload["s"].is_null() {
            sequence = payload["s"].clone();
        }
        match payload["op"].as_u64() {
            Some(10) => {
                let every = payload["d"]["heartbeat_interval"].as_u64().unwrap_or(41_250);
                heartbeat = tokio::time::interval_at(tokio::time::Instant::now() + Duration::from_millis(every), Duration::from_millis(every));
                let identify = json!({"op": 2, "d": {"token": token, "intents": INTENTS, "properties": {"os": std::env::consts::OS, "browser": "Swarm", "device": "Swarm"}}});
                socket.send(Message::text(identify.to_string())).await.map_err(|e| e.to_string())?;
            }
            Some(1) => socket.send(Message::text(json!({"op": 1, "d": sequence}).to_string())).await.map_err(|e| e.to_string())?,
            // Reconnect or invalid session: start a fresh connection.
            Some(7) | Some(9) => return Ok(()),
            Some(0) => match payload["t"].as_str() {
                Some("READY") => {
                    bot_user = payload["d"]["user"]["id"].as_str().unwrap_or_default().to_owned();
                    update_status(app, &channel.id, &format!("Connected as {}", payload["d"]["user"]["username"].as_str().unwrap_or("bot")));
                }
                Some("MESSAGE_CREATE") => {
                    let Some((sender, name, thread, text)) = inbound(&payload["d"], &bot_user) else { continue };
                    let _ = rest(&token, reqwest::Method::POST, &format!("/channels/{thread}/typing"), None).await;
                    let (app, channel_id) = (app.clone(), channel.id.clone());
                    tauri::async_runtime::spawn(async move { handle_inbound(&app, &channel_id, &sender, &name, &thread, &text).await });
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// Permissions the invite link asks for: view channels, send messages, read history, send in threads.
const INVITE_PERMISSIONS: u64 = 1024 | 2048 | 65536 | (1 << 38);
/// The application flags that mean Message Content is allowed.
const MESSAGE_CONTENT_FLAGS: u64 = (1 << 18) | (1 << 19);

pub(crate) fn invite_url(application_id: &str) -> String {
    format!("https://discord.com/oauth2/authorize?client_id={application_id}&scope=bot&permissions={INVITE_PERMISSIONS}")
}

/// Checks the token, builds the server invite link, and warns when Message Content is off.
pub(crate) async fn verify(token: &str) -> Result<super::Identity> {
    let me = rest(token, reqwest::Method::GET, "/users/@me", None).await?;
    let application = rest(token, reqwest::Method::GET, "/oauth2/applications/@me", None).await?;
    let id = application["id"].as_str().map(snowflake).transpose()?.ok_or("Discord did not return the application ID.")?;
    let mut identity = super::Identity { name: me["username"].as_str().unwrap_or("bot").to_owned(), invite: Some(invite_url(id)), ..Default::default() };
    if application["flags"].as_u64().is_some_and(|flags| flags & MESSAGE_CONTENT_FLAGS == 0) {
        identity.warnings.push(format!("Message Content intent is off, so the bot cannot read messages. Turn it on at https://discord.com/developers/applications/{id}/bot under Privileged Gateway Intents."));
    }
    Ok(identity)
}

fn snowflake(id: &str) -> Result<&str> {
    if id.is_empty() || id.len() > 24 || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("That Discord ID looks wrong.".into());
    }
    Ok(id)
}

/// Sends to a channel ID, or to a user's direct messages with `dm:<user id>`.
pub(crate) async fn send(channel: &Channel, thread: &str, text: &str) -> Result<()> {
    let token = secrets::get(&channel.id, "token").ok_or("The Discord bot token is missing.")?;
    let target = match thread.strip_prefix("dm:") {
        Some(user) => {
            let dm = rest(&token, reqwest::Method::POST, "/users/@me/channels", Some(json!({"recipient_id": snowflake(user)?}))).await?;
            dm["id"].as_str().ok_or("Discord did not open a direct message.")?.to_owned()
        }
        None => snowflake(thread)?.to_owned(),
    };
    rest(&token, reqwest::Method::POST, &format!("/channels/{target}/messages"), Some(json!({"content": text, "allowed_mentions": {"parse": []}})))
        .await
        .map(|_| ())
        .map_err(|e| e.trim_start_matches("auth:").to_owned())
}
