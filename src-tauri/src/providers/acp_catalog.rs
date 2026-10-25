//! Positive ACP model metadata shared by settings and command discovery.
//! Commands themselves remain scoped to their project directory.
use super::{acp, ModelInfo};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant, SystemTime},
};

const TTL: Duration = Duration::from_secs(5 * 60);
const LIMIT: usize = 16;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Key {
    provider: String,
    path: PathBuf,
    modified: Option<SystemTime>,
    length: u64,
}

fn key(provider: &str, path: &Path) -> Key {
    let metadata = path.metadata().ok();
    Key {
        provider: provider.into(),
        path: path.into(),
        modified: metadata.as_ref().and_then(|value| value.modified().ok()),
        length: metadata.map_or(0, |value| value.len()),
    }
}

#[derive(Default)]
struct Cache(HashMap<Key, (Instant, Vec<ModelInfo>)>, u64);

pub(crate) struct Discovery {
    key: Key,
    revision: u64,
}

impl Cache {
    fn begin(&self, key: Key) -> Discovery {
        Discovery {
            key,
            revision: self.1,
        }
    }

    fn commit(&mut self, discovery: Discovery, metadata: &Value, now: Instant) {
        if discovery.revision == self.1 {
            self.store(discovery.key, metadata, now);
        }
    }

    fn invalidate(&mut self, provider: &str) {
        self.1 = self.1.wrapping_add(1);
        self.0.retain(|key, _| key.provider != provider);
    }

    fn get(&self, key: &Key, now: Instant) -> Option<Vec<ModelInfo>> {
        self.0
            .get(key)
            .filter(|(at, _)| now.saturating_duration_since(*at) < TTL)
            .map(|(_, models)| models.clone())
    }

    fn store(&mut self, key: Key, metadata: &Value, now: Instant) {
        let models = acp::parse_models(metadata);
        // A default-only response is not a catalog and must not overwrite a
        // previously discovered usable list.
        if models.len() <= 1 {
            return;
        }
        self.0.retain(|existing, (at, _)| {
            now.saturating_duration_since(*at) < TTL
                && !(existing.provider == key.provider && existing.path == key.path)
        });
        if self.0.len() >= LIMIT {
            if let Some(oldest) = self
                .0
                .iter()
                .min_by_key(|(_, (at, _))| at)
                .map(|(key, _)| key.clone())
            {
                self.0.remove(&oldest);
            }
        }
        self.0.insert(key, (now, models));
    }
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

pub(crate) fn get(provider: &str, path: &Path) -> Option<Vec<ModelInfo>> {
    cache()
        .lock()
        .ok()?
        .get(&key(provider, path), Instant::now())
}

/// Capture before starting a process so Refresh can invalidate in-flight work,
/// and replacing an executable cannot attach its old response to the new file.
pub(crate) fn begin(provider: &str, path: &Path) -> Option<Discovery> {
    Some(cache().lock().ok()?.begin(key(provider, path)))
}

pub(crate) fn store(discovery: Option<Discovery>, metadata: &Value) {
    let Some(discovery) = discovery else {
        return;
    };
    if key(&discovery.key.provider, &discovery.key.path) != discovery.key {
        return;
    }
    if let Ok(mut cache) = cache().lock() {
        cache.commit(discovery, metadata, Instant::now());
    }
}

pub(crate) fn invalidate(provider: &str) {
    if let Ok(mut cache) = cache().lock() {
        cache.invalidate(provider);
    }
}

/// Callers recheck the positive cache after acquiring this guard. Weak entries
/// disappear when callers finish, so changing an executable never grows a
/// permanent lock registry or blocks a different executable's discovery.
pub(crate) async fn refresh_lock(provider: &str, path: &Path) -> tokio::sync::OwnedMutexGuard<()> {
    type Locks = HashMap<Key, Weak<tokio::sync::Mutex<()>>>;
    static LOCKS: OnceLock<Mutex<Locks>> = OnceLock::new();
    let lock = {
        let mut locks = LOCKS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        locks.retain(|_, lock| lock.strong_count() > 0);
        let weak = locks.entry(key(provider, path)).or_default();
        weak.upgrade().unwrap_or_else(|| {
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            *weak = Arc::downgrade(&lock);
            lock
        })
    };
    lock.lock_owned().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn positive_catalog_is_bounded_expires_and_follows_executable_identity() {
        let now = Instant::now();
        let identity = key("antigravity", Path::new("/fixture/agy"));
        let metadata =
            json!({"models":{"availableModels":[{"modelId":"model-a","name":"Model A"}]}});
        let mut cache = Cache::default();
        cache.store(identity.clone(), &metadata, now);
        assert_eq!(cache.get(&identity, now).unwrap()[1].id, "model-a");
        cache.store(identity.clone(), &json!({}), now);
        assert!(cache.get(&identity, now).is_some());
        assert!(cache.get(&identity, now + TTL).is_none());
        let mut replaced = identity.clone();
        replaced.length = 42;
        assert!(cache.get(&replaced, now).is_none());
        assert!(cache
            .get(&key("grok", Path::new("/fixture/agy")), now)
            .is_none());
        for index in 0..30 {
            cache.store(
                key("antigravity", Path::new(&format!("/fixture/{index}"))),
                &metadata,
                now,
            );
        }
        assert_eq!(cache.0.len(), LIMIT);
    }

    #[tokio::test]
    async fn refresh_lock_coalesces_one_executable_without_blocking_another() {
        let path = Path::new("/fixture/coalescing");
        let first = refresh_lock("antigravity", path).await;
        let waiting = tokio::spawn(async move { refresh_lock("antigravity", path).await });
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        let other = tokio::time::timeout(
            Duration::from_millis(100),
            refresh_lock("antigravity", Path::new("/fixture/different")),
        )
        .await
        .unwrap();
        drop(other);
        drop(first);
        let _next = tokio::time::timeout(Duration::from_millis(100), waiting)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn explicit_refresh_invalidates_only_the_requested_provider() {
        let provider = format!("fixture-{}", uuid::Uuid::new_v4());
        let other = format!("other-{}", uuid::Uuid::new_v4());
        let path = Path::new("/fixture/refresh");
        let metadata =
            json!({"models":{"availableModels":[{"modelId":"model-a","name":"Model A"}]}});
        store(begin(&provider, path), &metadata);
        store(begin(&other, path), &metadata);
        assert!(get(&provider, path).is_some());
        invalidate(&provider);
        assert!(get(&provider, path).is_none());
        assert!(get(&other, path).is_some());
        invalidate(&other);
    }

    #[test]
    fn refresh_cannot_be_repopulated_by_discovery_started_before_it() {
        let now = Instant::now();
        let identity = key("antigravity", Path::new("/fixture/in-flight"));
        let old = json!({"models":{"availableModels":[{"modelId":"old-model"}]}});
        let new = json!({"models":{"availableModels":[{"modelId":"new-model"}]}});
        let mut cache = Cache::default();
        let in_flight = cache.begin(identity.clone());
        cache.invalidate("antigravity");
        cache.commit(in_flight, &old, now);
        assert!(cache.get(&identity, now).is_none());
        let refreshed = cache.begin(identity.clone());
        cache.commit(refreshed, &new, now);
        assert_eq!(cache.get(&identity, now).unwrap()[1].id, "new-model");
    }
}
