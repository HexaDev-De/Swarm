//! IPC for Settings → Cloud and the cloud machines in the VM panel.
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tauri::{ipc::Response, AppHandle, Emitter};

use super::{aws::Aws, aws_screen, cloudflare::Cloudflare, daytona::Daytona, http::block, split, Machine};
use crate::Result;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub provider: &'static str,
    pub connected: bool,
    /// Cloudflare: the sandbox Worker is deployed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandboxes: Option<bool>,
    /// Non-secret details shown in Settings, such as an API URL or account.
    pub detail: Option<String>,
}

pub fn connections() -> Vec<Connection> {
    let settings = super::settings();
    vec![
        Connection { provider: "daytona", connected: super::secret("daytona_key").is_some(), sandboxes: None, detail: settings.daytona_organization.or(settings.daytona_api_url) },
        Connection { provider: "cloudflare", connected: super::secret("cloudflare_token").is_some() && settings.cloudflare_account.is_some(), sandboxes: Some(super::secret("cloudflare_worker_token").is_some() && settings.cloudflare_worker_url.is_some()), detail: settings.cloudflare_account.clone() },
        Connection { provider: "aws", connected: settings.aws_region.is_some(), sandboxes: None, detail: settings.aws_region.map(|region| format!("{} · {region}", settings.aws_profile.unwrap_or_else(|| "default profile".into()))) },
        Connection { provider: "vercel", connected: super::secret("vercel_token").is_some(), sandboxes: None, detail: settings.vercel_team },
        Connection { provider: "railway", connected: super::secret("railway_token").is_some(), sandboxes: None, detail: settings.railway_workspace },
        Connection { provider: "gcloud", connected: settings.gcloud_project.is_some(), sandboxes: None, detail: settings.gcloud_project.map(|project| match settings.gcloud_region { Some(region) => format!("{project} · {region}"), None => project }) },
    ]
}

#[tauri::command]
pub async fn cloud_connections() -> Result<Vec<Connection>> {
    crate::blocking(|| Ok(connections())).await
}

/// Check the credentials with a real request before saving them.
#[tauri::command]
pub async fn cloud_connect(app: AppHandle, provider: String, key: String, options: Value) -> Result<Vec<Connection>> {
    crate::blocking(move || {
        let text = |name: &str| options[name].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
        match provider.as_str() {
            "daytona" => {
                let api = text("apiUrl");
                if api.as_deref().is_some_and(|url| !url.starts_with("https://")) {
                    return Err("The Daytona API URL must start with https://.".into());
                }
                let daytona = Daytona::new(api.clone(), key.trim().to_string(), text("organization"));
                block(daytona.list())?;
                super::set_secret("daytona_key", key.trim())?;
                let mut settings = super::settings();
                settings.daytona_api_url = api;
                settings.daytona_organization = text("organization");
                super::save_settings(&settings)?;
            }
            "cloudflare" => {
                let account = text("account").ok_or("Enter your Cloudflare account ID.")?;
                if !account.bytes().all(|b| b.is_ascii_hexdigit()) || account.len() != 32 {
                    return Err("A Cloudflare account ID is 32 hexadecimal characters (Workers & Pages → Account details).".into());
                }
                block(super::cloudflare::verify_token(key.trim(), &account))?;
                super::set_secret("cloudflare_token", key.trim())?;
                let mut settings = super::settings();
                settings.cloudflare_account = Some(account);
                super::save_settings(&settings)?;
            }
            "vercel" => {
                let team = text("team");
                block(verify_vercel(key.trim(), team.as_deref()))?;
                super::set_secret("vercel_token", key.trim())?;
                let mut settings = super::settings();
                settings.vercel_team = team;
                super::save_settings(&settings)?;
            }
            "railway" => {
                let workspace = text("workspace");
                block(verify_railway(RAILWAY_API, key.trim(), workspace.as_deref()))?;
                super::set_secret("railway_token", key.trim())?;
                let mut settings = super::settings();
                settings.railway_workspace = workspace;
                super::save_settings(&settings)?;
            }
            // Google Cloud uses gcloud's own login; PandaCode stores only the project and region.
            "gcloud" => {
                let project = text("project").ok_or("Enter your Google Cloud project ID.")?;
                if project.len() > 100 || project.starts_with('-') || !project.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.:".contains(&b)) {
                    return Err("That does not look like a Google Cloud project ID (for example my-app-123456).".into());
                }
                let region = text("region");
                if region.as_deref().is_some_and(|r| r.len() > 32 || !r.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')) {
                    return Err("That does not look like a Google Cloud region (for example us-central1).".into());
                }
                super::deploy::gcloud_verify(&project, region.as_deref())?;
                let mut settings = super::settings();
                settings.gcloud_project = Some(project);
                settings.gcloud_region = region;
                super::save_settings(&settings)?;
            }
            // AWS uses the CLI's own login; PandaCode stores only the profile and region.
            "aws" => {
                let region = text("region").ok_or("Choose an AWS region, such as us-east-1.")?;
                if !region.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') || region.len() > 32 {
                    return Err("That does not look like an AWS region (for example us-east-1).".into());
                }
                let profile = text("profile");
                if profile.as_deref().is_some_and(|p| p.len() > 128 || p.starts_with('-') || p.chars().any(char::is_whitespace)) {
                    return Err("That does not look like an AWS CLI profile name.".into());
                }
                Aws { profile: profile.clone(), region: region.clone() }.identity()?;
                let mut settings = super::settings();
                settings.aws_profile = profile;
                settings.aws_region = Some(region);
                super::save_settings(&settings)?;
            }
            _ => return Err("Unknown cloud provider.".into()),
        }
        let _ = app.emit("vm-changed", serde_json::json!({}));
        Ok(connections())
    })
    .await
}

async fn verify_vercel(token: &str, team: Option<&str>) -> Result<()> {
    let client = super::http::client(Duration::from_secs(30))?;
    let url = match team { Some(team) => format!("https://api.vercel.com/v2/teams/{team}"), None => "https://api.vercel.com/v2/user".into() };
    super::http::send("Vercel", client.get(url).bearer_auth(token)).await.map(|_| ())
}

const RAILWAY_API: &str = "https://backboard.railway.com/graphql/v2";

/// An account token can read `me`; a workspace token only its own workspace.
async fn verify_railway(api: &str, token: &str, workspace: Option<&str>) -> Result<()> {
    let client = super::http::client(Duration::from_secs(30))?;
    let body = match workspace {
        Some(id) => serde_json::json!({"query": "query($id: String!) { workspace(workspaceId: $id) { id } }", "variables": {"id": id}}),
        None => serde_json::json!({"query": "{ me { id } }"}),
    };
    let reply = super::http::send("Railway", client.post(api).bearer_auth(token).json(&body)).await?;
    if reply["data"]["me"]["id"].is_string() || reply["data"]["workspace"]["id"].is_string() {
        return Ok(());
    }
    // GraphQL reports a bad token or an unknown workspace with HTTP 200 and an errors list.
    Err(match (workspace, reply["errors"][0]["message"].as_str()) {
        (Some(_), Some(message)) if !message.to_ascii_lowercase().contains("not authorized") => format!("Railway: {}", message.chars().take(200).collect::<String>()),
        (None, _) => "Railway refused the token. Use an account token, or a workspace token with its workspace ID.".into(),
        _ => "Railway refused the token for that workspace. Check the token and the workspace ID.".into(),
    })
}

/// Deploy PandaCode's sandbox Worker to the connected Cloudflare account (needs Docker).
#[tauri::command]
pub async fn cloud_setup_sandboxes(app: AppHandle) -> Result<Vec<Connection>> {
    crate::blocking(move || {
        let token = super::secret("cloudflare_token").ok_or("Connect Cloudflare first.")?;
        let account = super::settings().cloudflare_account.ok_or("Connect Cloudflare first.")?;
        let progress = |label: &str| { let _ = app.emit("cloud-setup", serde_json::json!({"provider": "cloudflare", "label": label})); };
        let (url, secret) = super::cloudflare::deploy(&token, &account, &progress)?;
        progress("Checking the Worker…");
        let worker = Cloudflare::new(url.clone(), secret.clone());
        // A fresh workers.dev route can take a few seconds to answer.
        let mut ready = Err(String::new());
        for _ in 0..10 {
            ready = block(worker.health());
            if ready.is_ok() { break; }
            std::thread::sleep(Duration::from_secs(3));
        }
        ready.map_err(|e| format!("The Worker deployed but did not answer yet ({e}). Try again in a minute."))?;
        super::set_secret("cloudflare_worker_token", &secret)?;
        let mut settings = super::settings();
        settings.cloudflare_worker_url = Some(url);
        super::save_settings(&settings)?;
        let _ = app.emit("vm-changed", serde_json::json!({}));
        Ok(connections())
    })
    .await
}

#[tauri::command]
pub async fn cloud_disconnect(app: AppHandle, provider: String) -> Result<Vec<Connection>> {
    crate::blocking(move || {
        match provider.as_str() {
            "daytona" => {
                super::remove_secret("daytona_key");
                let mut settings = super::settings();
                settings.daytona_api_url = None;
                settings.daytona_organization = None;
                super::save_settings(&settings)?;
            }
            // The Worker stays in the account; Settings says how to remove it.
            "cloudflare" => {
                super::remove_secret("cloudflare_worker_token");
                super::remove_secret("cloudflare_token");
                let mut settings = super::settings();
                settings.cloudflare_worker_url = None;
                settings.cloudflare_account = None;
                settings.cloudflare_sandboxes.clear();
                super::save_settings(&settings)?;
            }
            "vercel" => {
                super::remove_secret("vercel_token");
                let mut settings = super::settings();
                settings.vercel_team = None;
                super::save_settings(&settings)?;
            }
            "railway" => {
                super::remove_secret("railway_token");
                let mut settings = super::settings();
                settings.railway_workspace = None;
                super::save_settings(&settings)?;
            }
            "gcloud" => {
                let mut settings = super::settings();
                settings.gcloud_project = None;
                settings.gcloud_region = None;
                super::save_settings(&settings)?;
            }
            "aws" => {
                aws_screen::close(None);
                let mut settings = super::settings();
                settings.aws_profile = None;
                settings.aws_region = None;
                super::save_settings(&settings)?;
            }
            _ => return Err("Unknown cloud provider.".into()),
        }
        let _ = app.emit("vm-changed", serde_json::json!({}));
        Ok(connections())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Machines {
    pub machines: Vec<Machine>,
    /// Providers that could not be listed, with why.
    pub errors: Vec<String>,
}

/// Every connected provider's machines; one failing provider does not hide the others.
pub fn machines() -> Machines {
    let mut result = Machines { machines: Vec::new(), errors: Vec::new() };
    if let Ok(daytona) = Daytona::connected() {
        match block(daytona.list()) {
            Ok(list) => result.machines.extend(list),
            Err(error) => result.errors.push(error),
        }
    }
    if let Ok(cloudflare) = Cloudflare::connected() {
        match block(cloudflare.list(&super::settings().cloudflare_sandboxes)) {
            Ok(list) => result.machines.extend(list),
            Err(error) => result.errors.push(error),
        }
    }
    if let Ok(aws) = Aws::connected() {
        match aws.list() {
            Ok(list) => result.machines.extend(list),
            Err(error) => result.errors.push(error),
        }
        // AgentCore may not be offered in every region or account; that is not an error worth showing.
        if let Ok(list) = aws.interpreter_list() {
            result.machines.extend(list);
        }
    }
    result
}

#[tauri::command]
pub async fn cloud_machines() -> Result<Machines> {
    crate::blocking(|| Ok(machines())).await
}

pub fn create(provider: &str, name: &str) -> Result<Machine> {
    match provider {
        "daytona" => block(Daytona::connected()?.create(name)),
        "cloudflare" => {
            let machine = block(Cloudflare::connected()?.create(name))?;
            let mut settings = super::settings();
            if !settings.cloudflare_sandboxes.iter().any(|n| n == name) {
                settings.cloudflare_sandboxes.push(name.to_string());
                super::save_settings(&settings)?;
            }
            Ok(machine)
        }
        "aws-linux" | "aws-windows" => {
            let (machine, password) = Aws::connected()?.create(&provider[4..], name)?;
            let instance = machine.id.trim_start_matches("aws:").to_string();
            super::set_secret(&aws_screen::password_key(&instance), &password)?;
            Ok(machine)
        }
        "agentcore" => Aws::connected()?.interpreter_start(name),
        _ => Err("Unknown cloud provider.".into()),
    }
}
/// An AWS desktop's OS, from its tags.
fn aws_os(aws: &Aws, instance: &str) -> Result<String> {
    let found = aws.call(&["ec2", "describe-instances", "--instance-ids", instance], Duration::from_secs(60))?;
    let item = &found["Reservations"][0]["Instances"][0];
    if item.is_null() { return Err("AWS has no such instance.".into()); }
    Ok(Aws::machine(item).os)
}
pub fn power(id: &str, action: &str) -> Result<()> {
    match split(id) {
        Some(("daytona", native)) => block(Daytona::connected()?.power(native, action)),
        Some(("cloudflare", native)) => {
            block(Cloudflare::connected()?.power(native, action))?;
            if action == "delete" {
                let mut settings = super::settings();
                settings.cloudflare_sandboxes.retain(|n| n != native);
                super::save_settings(&settings)?;
            }
            Ok(())
        }
        Some(("aws", native)) => {
            if action != "start" { aws_screen::close(Some(native)); }
            Aws::connected()?.power(native, action)?;
            if action == "delete" { super::remove_secret(&aws_screen::password_key(native)); }
            Ok(())
        }
        Some(("agentcore", native)) if matches!(action, "stop" | "delete") => Aws::connected()?.interpreter_stop(native),
        Some(("agentcore", _)) => Err("AgentCore sessions cannot be restarted; create a new one.".into()),
        _ => Err("Unknown cloud machine.".into()),
    }
}
pub fn exec(id: &str, command: &str, timeout: Duration) -> Result<(Option<i32>, String)> {
    match split(id) {
        Some(("daytona", native)) => block(Daytona::connected()?.exec(native, command, timeout)),
        Some(("cloudflare", native)) => block(Cloudflare::connected()?.exec(native, command, timeout)),
        Some(("aws", native)) => { let aws = Aws::connected()?; let windows = aws_os(&aws, native)? == "windows"; aws.exec(native, windows, command, timeout) }
        Some(("agentcore", native)) => { let aws = Aws::connected()?; let credentials = aws.credentials()?; block(aws.interpreter_exec(native, command, &credentials, None)) }
        _ => Err("Unknown cloud machine.".into()),
    }
}
pub fn screenshot(id: &str) -> Result<crate::vm::screen::Shot> {
    match split(id) {
        Some(("daytona", native)) => block(Daytona::connected()?.screenshot(native)),
        Some(("aws", native)) => { let aws = Aws::connected()?; let os = aws_os(&aws, native)?; aws_screen::screenshot(&aws, native, &os) }
        _ => Err("This machine has no screen.".into()),
    }
}
pub fn act(id: &str, action: &crate::vm::screen::Action) -> Result<String> {
    match split(id) {
        Some(("daytona", native)) => block(Daytona::connected()?.act(native, action)),
        Some(("aws", native)) => { let aws = Aws::connected()?; let os = aws_os(&aws, native)?; aws_screen::act(&aws, native, &os, action) }
        _ => Err("This machine has no screen.".into()),
    }
}

#[tauri::command]
pub async fn cloud_create(app: AppHandle, provider: String, name: String) -> Result<Machine> {
    crate::blocking(move || {
        let machine = create(&provider, &name)?;
        let _ = app.emit("vm-changed", serde_json::json!({}));
        Ok(machine)
    })
    .await
}
#[tauri::command]
pub async fn cloud_power(app: AppHandle, id: String, action: String) -> Result<()> {
    crate::blocking(move || {
        power(&id, &action)?;
        let _ = app.emit("vm-changed", serde_json::json!({}));
        Ok(())
    })
    .await
}
#[tauri::command]
pub async fn cloud_frame(id: String) -> Result<Response> {
    crate::blocking(move || Ok(Response::new(screenshot(&id)?.jpeg))).await
}
#[tauri::command]
pub async fn cloud_input(id: String, input: Value) -> Result<String> {
    let action = crate::vm::tools::action(&input)?;
    crate::blocking(move || act(&id, &action)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::http::mock;

    #[tokio::test]
    async fn verifies_railway_account_and_workspace_tokens() {
        let (base, seen) = mock::serve(|request, _| match request.authorization.as_deref() {
            Some("Bearer account") if request.body.contains("me {") => (200, r#"{"data":{"me":{"id":"u1"}}}"#),
            Some("Bearer workspace") if request.body.contains(r#""id":"ws-1""#) => (200, r#"{"data":{"workspace":{"id":"ws-1"}}}"#),
            Some("Bearer workspace") => (200, r#"{"data":{"workspace":null},"errors":[{"message":"Workspace not found"}]}"#),
            _ => (200, r#"{"data":null,"errors":[{"message":"Not Authorized"}]}"#),
        });
        let api = format!("{base}/graphql/v2");
        verify_railway(&api, "account", None).await.unwrap();
        verify_railway(&api, "workspace", Some("ws-1")).await.unwrap();
        assert!(verify_railway(&api, "workspace", None).await.unwrap_err().contains("workspace token with its workspace ID"));
        assert_eq!(verify_railway(&api, "workspace", Some("ws-2")).await.unwrap_err(), "Railway: Workspace not found");
        assert!(verify_railway(&api, "bad", Some("ws-1")).await.unwrap_err().contains("refused the token"));
        let seen = seen.lock().unwrap();
        assert!(seen.iter().all(|r| r.method == "POST" && r.path == "/graphql/v2"));
    }
}
