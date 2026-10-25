//! Support is user-initiated navigation to the public GitHub issue tracker.
//! No customer or workspace data is sent, and nothing is contacted automatically.
use crate::Result;
use serde::Serialize;

const SUPPORT_URL: &str = "https://github.com/HexaDev-De/Swarm/issues";

#[tauri::command]
pub async fn open_support() -> Result<()> {
    crate::open_external_url(SUPPORT_URL.into()).await
}

#[derive(Serialize)]
pub struct SupportDetails {
    version: &'static str,
    platform: &'static str,
    architecture: &'static str,
}

/// An explicit allowlist: no logs, paths, accounts or agent output.
#[tauri::command]
pub fn support_details() -> SupportDetails {
    SupportDetails {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_has_no_identity_or_workspace_payload() {
        let details = serde_json::to_value(support_details()).unwrap();
        assert_eq!(details.as_object().unwrap().len(), 3);
        assert_eq!(details["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(details["platform"], std::env::consts::OS);
        assert_eq!(details["architecture"], std::env::consts::ARCH);
        let url = tauri::Url::parse(SUPPORT_URL).unwrap();
        assert_eq!(url.as_str(), "https://github.com/HexaDev-De/Swarm/issues");
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
    }
}
