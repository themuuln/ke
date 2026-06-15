use std::process::Command;

use crate::config::Config;

const SECURITY: &str = "security";

pub struct Keychain;

impl Keychain {
    /// Get a secret value from the Keychain.
    pub fn get(project: &str, key: &str) -> Option<String> {
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

    /// Set a secret value in the Keychain.
    pub fn set(project: &str, key: &str, value: &str) -> anyhow::Result<()> {
        let svc = Config::service_name(project);
        let status = Command::new(SECURITY)
            .args([
                "add-generic-password",
                "-s", &svc,
                "-a", key,
                "-w", value,
                "-U", // Update if exists
            ])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("failed to save secret to Keychain"))
        }
    }

    /// Delete a secret from the Keychain.
    pub fn delete(project: &str, key: &str) -> anyhow::Result<()> {
        let svc = Config::service_name(project);
        let status = Command::new(SECURITY)
            .args(["delete-generic-password", "-s", &svc, "-a", key])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("secret not found in Keychain"))
        }
    }

    /// Get all key-values for a project.
    pub fn list_values(project: &str, keys: &[String]) -> Vec<(String, String)> {
        keys.iter()
            .filter_map(|k| {
                Self::get(project, k).map(|v| (k.clone(), v))
            })
            .collect()
    }
}
