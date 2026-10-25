//! Channel tokens and signing secrets in the OS secure store. They never enter
//! prompts, logs, the database or the webview; only native channel code reads them.
use crate::secret_store;

const SERVICE: &str = "app.pandacode.bots";
/// Every secret name a channel may hold, so deleting a channel removes them all.
pub(crate) const NAMES: &[&str] = &["token", "app_token", "signing_secret", "access_token"];

fn account(channel_id: &str, name: &str) -> String {
    format!("{channel_id}:{name}")
}

pub(crate) fn set(channel_id: &str, name: &str, value: &str) -> crate::Result<()> {
    secret_store::set(SERVICE, &account(channel_id, name), value)
}

pub(crate) fn get(channel_id: &str, name: &str) -> Option<String> {
    secret_store::get(SERVICE, &account(channel_id, name))
}

pub(crate) fn remove(channel_id: &str, name: &str) {
    secret_store::remove(SERVICE, &account(channel_id, name));
}

pub(crate) fn remove_all(channel_id: &str) {
    for name in NAMES {
        remove(channel_id, name);
    }
}
