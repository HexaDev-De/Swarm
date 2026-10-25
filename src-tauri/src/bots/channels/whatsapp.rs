//! WhatsApp through Meta's Cloud API. Meta delivers messages to a webhook, so the
//! local server's `/whatsapp/<channel id>` route must be reachable over HTTPS, for
//! example through `cloudflared tunnel --url http://127.0.0.1:47321` or Tailscale
//! Funnel. Each call is checked against the app secret's signature; the verify token
//! answers Meta's subscription check. Replies use the phone number ID and an access token.

use super::http::{same, Request, Response};
use super::{client, handle_inbound, secrets, Channel};
use crate::{AppState, Result};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use tauri::Manager;

const GRAPH: &str = "https://graph.facebook.com/v21.0";

/// Checks `X-Hub-Signature-256: sha256=<hex>` against the app secret.
pub(crate) fn signature_valid(secret: &str, body: &[u8], header: &str) -> bool {
    let Some(given) = header.strip_prefix("sha256=") else { return false };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else { return false };
    mac.update(body);
    let expected: String = mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
    same(&expected, &given.to_ascii_lowercase())
}

/// Text messages in a webhook payload: (sender phone, name, text).
pub(crate) fn messages(payload: &Value) -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    for change in payload["entry"].as_array().into_iter().flatten().flat_map(|entry| entry["changes"].as_array().into_iter().flatten()) {
        let value = &change["value"];
        let names: Vec<(&str, &str)> = value["contacts"].as_array().into_iter().flatten().filter_map(|c| Some((c["wa_id"].as_str()?, c["profile"]["name"].as_str()?))).collect();
        for message in value["messages"].as_array().into_iter().flatten() {
            let Some(from) = message["from"].as_str() else { continue };
            let text = match message["type"].as_str() {
                Some("text") => message["text"]["body"].as_str(),
                Some("button") => message["button"]["text"].as_str(),
                Some("interactive") => message["interactive"]["button_reply"]["title"].as_str(),
                _ => None,
            };
            let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { continue };
            let name = names.iter().find(|(id, _)| *id == from).map_or(from, |(_, name)| name);
            found.push((from.to_owned(), name.to_owned(), text.to_owned()));
        }
    }
    found
}

fn channel(app: &tauri::AppHandle, id: &str) -> Option<Channel> {
    let state = app.state::<AppState>();
    let store = state.store.lock().ok()?;
    super::get(&store, id).ok().filter(|c| c.kind == "whatsapp" && c.enabled)
}

pub(crate) async fn webhook(app: &tauri::AppHandle, channel_id: &str, request: &Request) -> Response {
    let Some(channel) = channel(app, channel_id) else { return Response::text(404, "Not found.") };
    match request.method.as_str() {
        "GET" => {
            let token = channel.config["verifyToken"].as_str().unwrap_or_default().to_owned();
            let given = request.query.get("hub.verify_token").map(String::as_str).unwrap_or_default();
            if request.query.get("hub.mode").map(String::as_str) == Some("subscribe") && !token.is_empty() && same(given, &token) {
                super::update_status(app, &channel.id, "Webhook verified");
                Response::text(200, request.query.get("hub.challenge").map(String::as_str).unwrap_or_default())
            } else {
                Response::text(403, "Verification failed.")
            }
        }
        "POST" => {
            let secret = secrets::get(&channel.id, "signing_secret").unwrap_or_default();
            if secret.is_empty() || !signature_valid(&secret, &request.body, request.header("x-hub-signature-256").unwrap_or_default()) {
                return Response::text(401, "Bad signature.");
            }
            let payload: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            for (from, name, text) in messages(&payload) {
                let (app, channel_id) = (app.clone(), channel.id.clone());
                tauri::async_runtime::spawn(async move { handle_inbound(&app, &channel_id, &from, &name, &from, &text).await });
            }
            super::update_status(app, &channel.id, "Receiving messages");
            Response::text(200, "OK")
        }
        _ => Response::text(405, "Not supported."),
    }
}

/// Checks the access token against the phone number ID and returns the business number.
pub(crate) async fn verify(phone_id: &str, token: &str) -> Result<super::Identity> {
    if phone_id.is_empty() || !phone_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Enter the WhatsApp phone number ID (digits only) from Meta's API Setup page.".into());
    }
    let response = client()?.get(format!("{GRAPH}/{phone_id}?fields=display_phone_number,verified_name")).bearer_auth(token).send().await.map_err(|e| e.to_string())?;
    let ok = response.status().is_success();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    if !ok {
        return Err(match value["error"]["code"].as_i64() {
            Some(190) => "Meta rejected the access token. Create a new one (temporary tokens expire after 24 hours; a system user token does not).".into(),
            Some(100) => "Meta does not know that phone number ID for this token. Copy it again from WhatsApp → API Setup.".into(),
            _ => format!("WhatsApp: {}", value["error"]["message"].as_str().unwrap_or("request failed")),
        });
    }
    let number = value["display_phone_number"].as_str().unwrap_or_default();
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    let name = value["verified_name"].as_str().map_or_else(|| number.to_owned(), |name| format!("{name} ({number})"));
    Ok(super::Identity { name, link: (!digits.is_empty()).then(|| format!("https://wa.me/{digits}")), ..Default::default() })
}

pub(crate) async fn send(channel: &Channel, thread: &str, text: &str) -> Result<()> {
    let token = secrets::get(&channel.id, "access_token").ok_or("The WhatsApp access token is missing.")?;
    let phone_id = channel.config["phoneNumberId"].as_str().filter(|id| id.chars().all(|c| c.is_ascii_digit())).ok_or("The WhatsApp phone number ID is missing.")?;
    if thread.is_empty() || !thread.chars().all(|c| c.is_ascii_digit()) {
        return Err("That WhatsApp number looks wrong.".into());
    }
    let body = json!({"messaging_product": "whatsapp", "recipient_type": "individual", "to": thread, "type": "text", "text": {"preview_url": false, "body": text}});
    let response = client()?.post(format!("{GRAPH}/{phone_id}/messages")).bearer_auth(token).json(&body).send().await.map_err(|e| e.to_string())?;
    if response.status().is_success() {
        return Ok(());
    }
    let value: Value = response.json().await.unwrap_or(Value::Null);
    // Outside the 24-hour customer service window WhatsApp only accepts templates.
    Err(match value["error"]["code"].as_i64() {
        Some(131047) => "WhatsApp only allows replies within 24 hours of the person's last message.".into(),
        Some(190) => "WhatsApp rejected the access token. Create a new one in Meta's app dashboard.".into(),
        _ => format!("WhatsApp: {}", value["error"]["message"].as_str().unwrap_or("request failed")),
    })
}
