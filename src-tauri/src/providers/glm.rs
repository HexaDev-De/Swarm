//! Z.AI's documented Anthropic-compatible Coding Plan endpoint. Overrides are
//! private to this child; the user's normal Claude configuration is untouched.
use super::ModelInfo;
use tokio::process::Command;

pub(super) const ENDPOINT: &str = "https://api.z.ai/api/anthropic";
pub(super) const DEFAULT_MODEL: &str = "glm-4.7";

fn resolve_key(
    saved: Result<Option<String>, String>,
    environment: Option<String>,
) -> Result<String, String> {
    let saved = saved.and_then(|key| {
        key.map(|key| crate::provider_settings::validate_key(&key).map(str::to_owned))
            .transpose()
    });
    if let Ok(Some(key)) = &saved {
        return Ok(key.clone());
    }
    // A locked or unavailable desktop keyring must not disable environment
    // credentials (including Linux sessions without a Secret Service).
    if let Some(key) = environment.filter(|key| !key.trim().is_empty()) {
        return crate::provider_settings::validate_key(&key).map(str::to_owned);
    }
    saved?.ok_or_else(|| {
        "Add your Z.AI Coding Plan API key in Settings → Agents, or set ZAI_API_KEY.".into()
    })
}

pub(super) fn environment() -> Result<Vec<(String, String)>, String> {
    let key = resolve_key(
        crate::provider_settings::glm_key(),
        std::env::var("ZAI_API_KEY").ok(),
    )?;
    Ok(vec![
        ("ANTHROPIC_AUTH_TOKEN".into(), key),
        ("ANTHROPIC_BASE_URL".into(), ENDPOINT.into()),
        ("ANTHROPIC_MODEL".into(), DEFAULT_MODEL.into()),
        (
            "ANTHROPIC_DEFAULT_SONNET_MODEL".into(),
            DEFAULT_MODEL.into(),
        ),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), DEFAULT_MODEL.into()),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), "glm-4.5-air".into()),
    ])
}
pub(super) fn configure(command: &mut Command) -> Result<String, String> {
    let env = environment()?;
    let key = env
        .iter()
        .find(|(name, _)| name == "ANTHROPIC_AUTH_TOKEN")
        .unwrap()
        .1
        .clone();
    apply_environment(command, &key);
    Ok(key)
}

pub(super) fn arguments() -> Vec<String> {
    // Claude applies settings.json env after inherited env. Exclude those files
    // so an unrelated Anthropic credential or gateway cannot replace this key.
    // Managed enterprise policies remain enforced by Claude itself.
    vec!["--setting-sources".into(), "".into(), "--settings".into(), serde_json::json!({"env":{
        "ANTHROPIC_BASE_URL": ENDPOINT,
        "ANTHROPIC_API_KEY":"", "CLAUDE_CODE_OAUTH_TOKEN":"",
        "CLAUDE_CODE_USE_BEDROCK":"", "CLAUDE_CODE_USE_VERTEX":"", "CLAUDE_CODE_USE_FOUNDRY":"",
        "ANTHROPIC_DEFAULT_SONNET_MODEL":DEFAULT_MODEL,"ANTHROPIC_DEFAULT_OPUS_MODEL":DEFAULT_MODEL,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL":"glm-4.5-air", "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB":"1"
    }}).to_string()]
}

fn apply_environment(command: &mut Command, key: &str) {
    command.args(arguments());
    command
        .env("ANTHROPIC_BASE_URL", ENDPOINT)
        .env("ANTHROPIC_AUTH_TOKEN", key)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("CLAUDE_CODE_OAUTH_TOKEN")
        .env_remove("ZAI_API_KEY")
        .env("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB", "1")
        .env_remove("CLAUDE_CODE_USE_BEDROCK")
        .env_remove("CLAUDE_CODE_USE_VERTEX")
        .env_remove("CLAUDE_CODE_USE_FOUNDRY")
        .env("ANTHROPIC_MODEL", DEFAULT_MODEL)
        .env("ANTHROPIC_DEFAULT_SONNET_MODEL", DEFAULT_MODEL)
        .env("ANTHROPIC_DEFAULT_OPUS_MODEL", DEFAULT_MODEL)
        .env("ANTHROPIC_DEFAULT_HAIKU_MODEL", "glm-4.5-air");
}

pub(super) fn models() -> Vec<ModelInfo> {
    // Documented Coding Plan IDs, not Claude aliases or inferred versions.
    // Z.AI does not expose plan entitlements through Claude's model catalog.
    // Keep custom IDs available in the picker as the service evolves.
    [
        ("glm-5.3", "GLM-5.3"),
        ("glm-5.3-flash", "GLM-5.3-Flash"),
        ("glm-4.7", "GLM-4.7"),
        ("glm-5.1", "GLM-5.1"),
        ("glm-5-turbo", "GLM-5-Turbo"),
        ("glm-4.5-air", "GLM-4.5-Air"),
    ]
    .into_iter()
    .map(|(id, name)| ModelInfo {
        id: id.into(),
        name: name.into(),
        description: "Z.AI Coding Plan via Claude Code. Availability depends on your plan.".into(),
        reasoning_efforts: Vec::new(),
        default_reasoning_effort: None,
        context_window: None,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn environment_key_recovers_from_unavailable_secure_store() {
        assert_eq!(
            resolve_key(Err("Keyring is locked".into()), Some(" env-key ".into())).unwrap(),
            "env-key"
        );
        assert_eq!(
            resolve_key(Ok(Some("saved-key".into())), Some("env-key".into())).unwrap(),
            "saved-key"
        );
        assert_eq!(
            resolve_key(Ok(None), Some("env-key".into())).unwrap(),
            "env-key"
        );
        assert_eq!(
            resolve_key(Err("Keyring is locked".into()), None).unwrap_err(),
            "Keyring is locked"
        );
        for invalid in ["bad\nkey".into(), "x".repeat(4097), " ".into()] {
            assert!(resolve_key(Ok(None), Some(invalid)).is_err());
        }
        assert_eq!(
            resolve_key(Ok(Some("bad\nkey".into())), Some("env-key".into())).unwrap(),
            "env-key"
        );
    }
    #[test]
    fn credentials_are_environment_only_and_do_not_change_parent_process() {
        let mut command = Command::new("claude");
        apply_environment(&mut command, "test-secret");
        let env: std::collections::HashMap<_, _> = command.as_std().get_envs().collect();
        assert_eq!(
            env[std::ffi::OsStr::new("ANTHROPIC_AUTH_TOKEN")],
            Some(std::ffi::OsStr::new("test-secret"))
        );
        assert_eq!(
            env[std::ffi::OsStr::new("ANTHROPIC_BASE_URL")],
            Some(std::ffi::OsStr::new(ENDPOINT))
        );
        assert_eq!(env[std::ffi::OsStr::new("ANTHROPIC_API_KEY")], None);
        assert!(command
            .as_std()
            .get_args()
            .all(|arg| !arg.to_string_lossy().contains("test-secret")));
        assert!(models()
            .iter()
            .all(|model| model.id.starts_with("glm-") && model.name.starts_with("GLM-")));
    }
}
