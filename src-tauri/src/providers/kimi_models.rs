//! Query the installed CLI, which already owns the user's configured model catalog.
//! `kimi provider list --help` documents `--json`; model names are documented at
//! https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/config-files#models
use super::{effort_info, ModelInfo, ProcessGroup};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

const OUTPUT_LIMIT: usize = 1024 * 1024;
const MODEL_LIMIT: usize = 200;

// The CLI's JSON also includes provider credentials. Deserialize only this
// whitelist; never return raw output or CLI errors to the webview or logs.
#[derive(Deserialize)]
struct Catalog {
    models: BTreeMap<String, ConfiguredModel>,
}

#[derive(Deserialize)]
struct ConfiguredModel {
    model: String,
    #[serde(default, rename = "displayName", alias = "display_name")]
    display_name: Option<String>,
    #[serde(default, rename = "maxContextSize", alias = "max_context_size")]
    max_context_size: Option<u64>,
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    #[serde(default, rename = "defaultEffort", alias = "default_effort")]
    default_effort: Option<String>,
    #[serde(default)]
    overrides: Option<ModelOverrides>,
}

#[derive(Deserialize)]
struct ModelOverrides {
    #[serde(default, rename = "displayName", alias = "display_name")]
    display_name: Option<String>,
    #[serde(default, rename = "maxContextSize", alias = "max_context_size")]
    max_context_size: Option<u64>,
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    #[serde(default, rename = "defaultEffort", alias = "default_effort")]
    default_effort: Option<String>,
}

fn valid(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

fn parse(bytes: &[u8]) -> Result<Vec<ModelInfo>, String> {
    if bytes.len() > OUTPUT_LIMIT {
        return Err("Kimi model catalog exceeded its output limit".into());
    }
    let catalog: Catalog = serde_json::from_slice(bytes)
        .map_err(|_| "Kimi returned an invalid model catalog".to_owned())?;
    if catalog.models.len() > MODEL_LIMIT {
        return Err("Kimi model catalog contains too many models".into());
    }
    let mut models = vec![ModelInfo::default_choice()];
    for (alias, model) in catalog.models {
        if alias == "default" || !valid(&alias) || !valid(&model.model) {
            continue;
        }
        let overrides = model.overrides;
        let capabilities = overrides
            .as_ref()
            .and_then(|settings| settings.capabilities.as_ref())
            .or(model.capabilities.as_ref());
        let has_capability = |name: &str| {
            capabilities.is_some_and(|values| values.iter().any(|capability| capability == name))
        };
        let always_thinking = has_capability("always_thinking");
        let mut efforts = Vec::new();
        if has_capability("thinking") || always_thinking {
            // The installed ACP adapter only exposes on/off. Advertising the
            // catalog's low/high/max would imply controls ACP cannot apply.
            let default = overrides
                .as_ref()
                .and_then(|settings| settings.default_effort.as_deref())
                .or(model.default_effort.as_deref())
                .filter(|id| valid(id));
            let description = default
                .map(|id| format!("Use the model’s configured {id} effort"))
                .unwrap_or_else(|| "Use the model’s configured thinking effort".into());
            efforts.push(effort_info("on", Some(&description)));
            if !always_thinking {
                efforts.push(effort_info("off", Some("Disable thinking")));
            }
        }
        let context_window = overrides
            .as_ref()
            .and_then(|settings| settings.max_context_size)
            .or(model.max_context_size)
            .filter(|window| *window > 0 && *window <= 1_000_000_000);
        let name = overrides
            .and_then(|overrides| overrides.display_name)
            .filter(|name| valid(name))
            .or_else(|| model.display_name.filter(|name| valid(name)))
            .unwrap_or_else(|| model.model.clone());
        models.push(ModelInfo {
            // ACP set_model accepts the configured alias, not the wire model ID.
            id: alias,
            name,
            description: model.model,
            reasoning_efforts: efforts,
            default_reasoning_effort: if always_thinking {
                Some("on".into())
            } else {
                None
            },
            context_window,
        });
    }
    Ok(models)
}

pub(super) async fn list(path: &Path) -> Result<Vec<ModelInfo>, String> {
    let mut command = super::command(path)?;
    command
        .args(["provider", "list", "--json"])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| "Could not start Kimi model discovery".to_owned())?;
    let group = ProcessGroup::register(child.id());
    let stdout = child.stdout.take().ok_or("Kimi model output unavailable")?;
    let mut bytes = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(8), async {
        stdout
            .take((OUTPUT_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Could not read Kimi model catalog".to_owned())?;
        if bytes.len() > OUTPUT_LIMIT {
            return Err("Kimi model catalog exceeded its output limit".into());
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "Could not finish Kimi model discovery".to_owned())?;
        if !status.success() {
            return Err("Kimi model discovery is unavailable; check the CLI configuration or enter a custom model ID".into());
        }
        parse(&bytes)
    })
    .await
    .unwrap_or_else(|_| Err("Kimi model discovery timed out; enter a custom model ID".into()));
    // The raw config never leaves this function. Clear the buffer once only the
    // model projection remains, and reap the CLI on every exit path.
    bytes.fill(0);
    drop(group);
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn uses_cli_display_names_and_preserves_invocation_aliases() {
        let models = parse(
            br#"{"models":{
            "kimi-code/k3":{"model":"k3","displayName":"K3"},
            "kimi-code/kimi-for-coding":{"model":"kimi-for-coding","displayName":"K2.7 Coding"}
        }}"#,
        )
        .unwrap();
        assert_eq!(models[0].id, "default");
        assert_eq!(models[1].id, "kimi-code/k3");
        assert_eq!(models[1].name, "K3");
        assert_eq!(models[2].name, "K2.7 Coding");
        assert_eq!(models[2].description, "kimi-for-coding");
    }

    #[test]
    fn supports_effective_name_overrides_and_exact_id_fallbacks() {
        let models = parse(br#"{"models":{
            "a":{"model":"custom-model-v2","display_name":"Original","overrides":{"display_name":"Custom v2"}},
            "b":{"model":"custom-model-v3"},
            "c":{"model":"custom-model-v4","displayName":"","overrides":{"displayName":"\n"}}
        }}"#).unwrap();
        assert_eq!(models[1].name, "Custom v2");
        assert_eq!(models[2].name, "custom-model-v3");
        assert_eq!(models[3].name, "custom-model-v4");
    }

    #[test]
    fn context_and_thinking_use_declared_metadata_without_exposing_uncontrollable_efforts() {
        let models = parse(br#"{"models":{
            "a":{"model":"model-a","maxContextSize":1048576,"capabilities":["thinking","always_thinking"],"supportEfforts":["low","high","max"],"defaultEffort":"high"},
            "b":{"model":"model-b","max_context_size":262144,"capabilities":["thinking"],"overrides":{"max_context_size":200000}},
            "c":{"model":"sounds-like-thinking-but-unknown","maxContextSize":2000000000}
        }}"#).unwrap();
        assert_eq!(models[1].context_window, Some(1048576));
        assert_eq!(
            models[1]
                .reasoning_efforts
                .iter()
                .map(|effort| effort.id.as_str())
                .collect::<Vec<_>>(),
            vec!["on"]
        );
        assert!(models[1].reasoning_efforts[0].description.contains("high"));
        assert_eq!(models[1].default_reasoning_effort.as_deref(), Some("on"));
        assert_eq!(models[2].context_window, Some(200000));
        assert_eq!(
            models[2]
                .reasoning_efforts
                .iter()
                .map(|effort| effort.id.as_str())
                .collect::<Vec<_>>(),
            vec!["on", "off"]
        );
        assert!(models[3].reasoning_efforts.is_empty());
        assert_eq!(models[3].context_window, None);
    }

    #[test]
    fn excludes_credentials_and_rejects_malformed_metadata_without_echoing_it() {
        let fixture = br#"{"providers":{"managed":{"api_key":"PRIVATE_TEST_SECRET","oauth":{"access_token":"PRIVATE_TEST_TOKEN"}}},"models":{"alias":{"model":"model-v2","displayName":"Model v2","api_key":"PRIVATE_TEST_SECRET"}}}"#;
        let models = parse(fixture).unwrap();
        let output = serde_json::to_string(&models).unwrap();
        assert!(!output.contains("PRIVATE_TEST"));
        assert!(!output.contains("api_key"));
        let error = parse(br#"{"models":"PRIVATE_TEST_SECRET"}"#).unwrap_err();
        assert_eq!(error, "Kimi returned an invalid model catalog");
    }

    #[test]
    fn bounds_catalog_size_and_filters_invalid_ids() {
        assert!(parse(&vec![b' '; OUTPUT_LIMIT + 1]).is_err());
        let models: BTreeMap<_, _> = (0..=MODEL_LIMIT)
            .map(|index| (index.to_string(), json!({"model":"model-v2"})))
            .collect();
        assert!(parse(&serde_json::to_vec(&json!({"models":models})).unwrap()).is_err());
        let models = parse(
            br#"{"models":{
            "default":{"model":"reserved"},
            "":{"model":"empty"},
            "bad\n":{"model":"control"},
            "good":{"model":"model-v2"}
        }}"#,
        )
        .unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[1].id, "good");
    }

    #[tokio::test]
    #[ignore = "Read-only model discovery against an installed Kimi CLI"]
    async fn installed_kimi_catalog() {
        let path = super::super::executable("kimi").expect("Kimi is not installed");
        let models = list(&path).await.expect("Kimi catalog unavailable");
        assert_eq!(models[0].id, "default");
        assert!(models
            .iter()
            .skip(1)
            .all(|model| valid(&model.id) && valid(&model.name)));
        eprintln!("Discovered {} configured Kimi models", models.len() - 1);
    }
}
