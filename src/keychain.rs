use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

use crate::config::Config;

const SECURITY: &str = "security";
type CacheKey = (String, String);

/// Backend trait for Keychain operations. Allows swapping the real
/// macOS Keychain for a mock in tests. Must be `Send + Sync` so it can
/// be shared with background value-fetch threads.
pub trait KeychainBackend: Send + Sync {
    fn get(&self, project: &str, key: &str) -> Option<String>;
    fn set(&self, project: &str, key: &str, value: &str) -> anyhow::Result<()>;
    fn delete(&self, project: &str, key: &str) -> anyhow::Result<()>;
    fn list_values(&self, project: &str, keys: &[String]) -> Vec<(String, String)>;
}

/// Real macOS Keychain backend — calls the `security` CLI.
///
/// Uses an in-memory value cache to avoid repeated subprocess calls.
/// Uncached lookups in `list_values` are parallelized via `thread::scope`.
pub struct RealKeychain {
    value_cache: Mutex<HashMap<CacheKey, String>>,
}

impl RealKeychain {
    pub fn new() -> Self {
        Self {
            value_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Run a single `security find-generic-password` call for a project+key.
    fn fetch_from_keychain(project: &str, key: &str) -> Option<String> {
        let svc = Config::service_name(project);
        let output = Command::new(SECURITY)
            .args(["find-generic-password", "-s", &svc, "-a", key, "-w"])
            .output()
            .ok()?;
        if output.status.success() {
            let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if val.is_empty() { None } else { Some(val) }
        } else {
            None
        }
    }
}

impl KeychainBackend for RealKeychain {
    fn get(&self, project: &str, key: &str) -> Option<String> {
        let cache_key = (project.to_string(), key.to_string());
        // Check cache first
        if let Some(val) = self.value_cache.lock().unwrap().get(&cache_key) {
            return Some(val.clone());
        }
        // Fetch from Keychain
        if let Some(val) = Self::fetch_from_keychain(project, key) {
            self.value_cache.lock().unwrap().insert(cache_key, val.clone());
            Some(val)
        } else {
            None
        }
    }

    fn set(&self, project: &str, key: &str, value: &str) -> anyhow::Result<()> {
        let svc = Config::service_name(project);
        let status = Command::new(SECURITY)
            .args([
                "add-generic-password",
                "-s",
                &svc,
                "-a",
                key,
                "-w",
                value,
                "-U",
            ])
            .status()?;
        if status.success() {
            // Update cache
            self.value_cache
                .lock()
                .unwrap()
                .insert((project.to_string(), key.to_string()), value.to_string());
            Ok(())
        } else {
            Err(anyhow::anyhow!("failed to save secret to Keychain"))
        }
    }

    fn delete(&self, project: &str, key: &str) -> anyhow::Result<()> {
        let svc = Config::service_name(project);
        let status = Command::new(SECURITY)
            .args(["delete-generic-password", "-s", &svc, "-a", key])
            .status()?;
        if status.success() {
            // Invalidate cache
            self.value_cache
                .lock()
                .unwrap()
                .remove(&(project.to_string(), key.to_string()));
            Ok(())
        } else {
            Err(anyhow::anyhow!("secret not found in Keychain"))
        }
    }

    fn list_values(&self, project: &str, keys: &[String]) -> Vec<(String, String)> {
        let svc = Config::service_name(project);

        // Phase 1: collect cache hits and uncached keys in one lock acquisition
        let mut results: Vec<(String, String)> = Vec::with_capacity(keys.len());
        let mut uncached: Vec<String> = Vec::new();
        {
            let cache = self.value_cache.lock().unwrap();
            for key in keys {
                let ck = (project.to_string(), key.clone());
                if let Some(val) = cache.get(&ck) {
                    results.push((key.clone(), val.clone()));
                } else {
                    uncached.push(key.clone());
                }
            }
        }

        // Phase 2: parallel fetch for uncached keys
        if !uncached.is_empty() {
            let fetched: Vec<(String, String)> = std::thread::scope(|s| {
                let handles: Vec<_> = uncached
                    .iter()
                    .map(|key| {
                        let k = key.clone();
                        let svc = svc.clone();
                        s.spawn(move || {
                            let output = Command::new(SECURITY)
                                .args([
                                    "find-generic-password",
                                    "-s",
                                    &svc,
                                    "-a",
                                    &k,
                                    "-w",
                                ])
                                .output()
                                .ok()?;
                            if output.status.success() {
                                let val =
                                    String::from_utf8_lossy(&output.stdout).trim().to_string();
                                if !val.is_empty() {
                                    return Some((k, val));
                                }
                            }
                            None
                        })
                    })
                    .collect();

                handles
                    .into_iter()
                    .filter_map(|h| h.join().ok()?)
                    .collect()
            });

            // Populate cache and results
            if !fetched.is_empty() {
                let mut cache = self.value_cache.lock().unwrap();
                for (key, val) in &fetched {
                    cache.insert((project.to_string(), key.clone()), val.clone());
                    results.push((key.clone(), val.clone()));
                }
            }
        }

        results
    }
}

/// In-memory mock Keychain for testing.
#[cfg_attr(not(test), allow(dead_code))]
pub struct MockKeychain {
    store: Mutex<HashMap<(String, String), String>>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl MockKeychain {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }
}

impl KeychainBackend for MockKeychain {
    fn get(&self, project: &str, key: &str) -> Option<String> {
        self.store
            .lock()
            .unwrap()
            .get(&(project.to_string(), key.to_string()))
            .cloned()
    }

    fn set(&self, project: &str, key: &str, value: &str) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert((project.to_string(), key.to_string()), value.to_string());
        Ok(())
    }

    fn delete(&self, project: &str, key: &str) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .remove(&(project.to_string(), key.to_string()));
        Ok(())
    }

    fn list_values(&self, project: &str, keys: &[String]) -> Vec<(String, String)> {
        keys.iter()
            .filter_map(|k| self.get(project, k).map(|v| (k.clone(), v)))
            .collect()
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl MockKeychain {
    /// Mutable set — convenience for tests that already hold `&mut MockKeychain`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_mut(&mut self, project: &str, key: &str, value: &str) {
        self.store
            .lock()
            .unwrap()
            .insert((project.to_string(), key.to_string()), value.to_string());
    }

    /// Mutable delete — convenience for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn delete_mut(&mut self, project: &str, key: &str) {
        self.store
            .lock()
            .unwrap()
            .remove(&(project.to_string(), key.to_string()));
    }
}

// ─── Legacy static API (delegates to RealKeychain) ─────────────────────

pub struct Keychain;

impl Keychain {
    pub fn get(project: &str, key: &str) -> Option<String> {
        RealKeychain::new().get(project, key)
    }

    pub fn set(project: &str, key: &str, value: &str) -> anyhow::Result<()> {
        RealKeychain::new().set(project, key, value)
    }

    pub fn delete(project: &str, key: &str) -> anyhow::Result<()> {
        RealKeychain::new().delete(project, key)
    }

    #[allow(dead_code)]
    pub fn list_values(project: &str, keys: &[String]) -> Vec<(String, String)> {
        RealKeychain::new().list_values(project, keys)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_keychain_roundtrip() {
        let mut mock = MockKeychain::new();
        mock.set_mut("myproject", "DB_URL", "postgres://localhost/mydb");

        let got = mock.get("myproject", "DB_URL");
        assert_eq!(got, Some("postgres://localhost/mydb".into()));

        // Non-existent key
        assert_eq!(mock.get("myproject", "MISSING"), None);
    }

    #[test]
    fn mock_keychain_delete() {
        let mut mock = MockKeychain::new();
        mock.set_mut("myproject", "SECRET", "value123");
        assert!(mock.get("myproject", "SECRET").is_some());

        mock.delete_mut("myproject", "SECRET");
        assert!(mock.get("myproject", "SECRET").is_none());
    }

    #[test]
    fn mock_keychain_list_values() {
        let mut mock = MockKeychain::new();
        mock.set_mut("p", "A", "1");
        mock.set_mut("p", "B", "2");

        let keys = vec!["A".into(), "B".into(), "MISSING".into()];
        let values = mock.list_values("p", &keys);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0], ("A".into(), "1".into()));
        assert_eq!(values[1], ("B".into(), "2".into()));
    }
}
