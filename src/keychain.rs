use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

use crate::config::Config;

const SECURITY: &str = "security";
type CacheKey = (String, String);

pub fn is_not_found_error(error: &anyhow::Error) -> bool {
    error.to_string().contains("secret not found in Keychain")
}

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

    fn decode_security_password(stdout: &[u8]) -> Option<String> {
        let mut val = String::from_utf8_lossy(stdout).into_owned();
        if val.ends_with('\n') {
            val.pop();
            if val.ends_with('\r') {
                val.pop();
            }
        }
        if val.is_empty() {
            None
        } else {
            Some(val)
        }
    }

    fn fetch_with_service(svc: &str, key: &str) -> Option<String> {
        let output = Command::new(SECURITY)
            .args(["find-generic-password", "-s", svc, "-a", key, "-w"])
            .output()
            .ok()?;
        if output.status.success() {
            Self::decode_security_password(&output.stdout)
        } else {
            None
        }
    }

    /// Run a single `security find-generic-password` call for a project+key.
    fn fetch_from_keychain(project: &str, key: &str) -> Option<String> {
        let svc = Config::service_name(project);
        Self::fetch_with_service(&svc, key)
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
            self.value_cache
                .lock()
                .unwrap()
                .insert(cache_key, val.clone());
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
        let output = Command::new(SECURITY)
            .args(["delete-generic-password", "-s", &svc, "-a", key])
            .output()?;
        if output.status.success() {
            // Invalidate cache
            self.value_cache
                .lock()
                .unwrap()
                .remove(&(project.to_string(), key.to_string()));
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("could not be found") || stderr.contains("not be found") {
                Err(anyhow::anyhow!("secret not found in Keychain"))
            } else {
                Err(anyhow::anyhow!(
                    "failed to delete secret from Keychain: {}",
                    stderr.trim()
                ))
            }
        }
    }

    fn list_values(&self, project: &str, keys: &[String]) -> Vec<(String, String)> {
        let svc = Config::service_name(project);
        let project_key = project.to_string();
        let mut ordered_values: Vec<Option<String>> = vec![None; keys.len()];

        // Phase 1: collect cache hits and uncached keys in one lock acquisition
        let mut uncached: Vec<(usize, String)> = Vec::new();
        {
            let cache = self.value_cache.lock().unwrap();
            for (idx, key) in keys.iter().enumerate() {
                let ck = (project_key.clone(), key.clone());
                if let Some(val) = cache.get(&ck) {
                    ordered_values[idx] = Some(val.clone());
                } else {
                    uncached.push((idx, key.clone()));
                }
            }
        }

        // Phase 2: parallel fetch for uncached keys
        if !uncached.is_empty() {
            let worker_count = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(uncached.len());
            let chunk_size = uncached.len().div_ceil(worker_count);
            let fetched: Vec<(usize, String, String)> = std::thread::scope(|s| {
                let mut handles = Vec::with_capacity(worker_count);
                for chunk in uncached.chunks(chunk_size) {
                    let chunk = chunk.to_vec();
                    let svc = svc.clone();
                    handles.push(s.spawn(move || {
                        let mut chunk_results = Vec::with_capacity(chunk.len());
                        for (idx, key) in chunk {
                            if let Some(val) = Self::fetch_with_service(&svc, &key) {
                                chunk_results.push((idx, key, val));
                            }
                        }
                        chunk_results
                    }));
                }

                handles
                    .into_iter()
                    .flat_map(|h| h.join().unwrap_or_default())
                    .collect()
            });

            // Populate cache and results
            if !fetched.is_empty() {
                let mut cache = self.value_cache.lock().unwrap();
                for (idx, key, val) in fetched {
                    cache.insert((project_key.clone(), key), val.clone());
                    ordered_values[idx] = Some(val);
                }
            }
        }

        keys.iter()
            .enumerate()
            .filter_map(|(idx, key)| {
                ordered_values[idx]
                    .as_ref()
                    .map(|val| (key.clone(), val.clone()))
            })
            .collect()
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
