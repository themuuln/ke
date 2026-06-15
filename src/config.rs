use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const INDEX_DIR_OLD: &str = "keychain-env";
const INDEX_DIR: &str = "ke";
const ICLOUD_DIR: &str = "ke";

pub struct Config {
    pub index_dir: PathBuf,
    pub icloud_dir: Option<PathBuf>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let home = home_dir()?;
        let base = home.join(".config");

        // Try new path (~/.config/ke) first, fall back to old (~/.config/keychain-env)
        let new = base.join(INDEX_DIR);
        let old = base.join(INDEX_DIR_OLD);
        let index_dir = if new.exists() { new } else { old };

        // Resolve symlink so we detect iCloud Drive redirect
        let resolved = index_dir.canonicalize().unwrap_or(index_dir.clone());

        // Detect iCloud Drive
        let icloud_dir = detect_icloud(&home, &resolved);

        fs::create_dir_all(&index_dir)?;
        Ok(Self { index_dir, icloud_dir })
    }

    /// Path to iCloud Drive ke folder (whether or not it exists yet).
    pub fn icloud_ke_path() -> Option<PathBuf> {
        let home = home_dir().ok()?;
        let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
        if icloud.exists() {
            Some(icloud.join(ICLOUD_DIR))
        } else {
            None
        }
    }

    /// Whether the index_dir is currently synced via iCloud Drive.
    pub fn is_icloud_synced(&self) -> bool {
        self.icloud_dir.is_some()
    }

    /// Move index from ~/.config/ke to iCloud Drive and symlink back.
    pub fn enable_icloud_sync(&mut self) -> anyhow::Result<()> {
        let icloud_path = Self::icloud_ke_path()
            .ok_or_else(|| anyhow::anyhow!("iCloud Drive not found at ~/Library/Mobile Documents/com~apple~CloudDocs"))?;

        if self.is_icloud_synced() {
            return Ok(()); // already set up
        }

        let config_path = self.index_dir.clone();

        // If iCloud target already exists, merge (copy non-conflicting)
        if icloud_path.exists() {
            if let Ok(entries) = fs::read_dir(&config_path) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let target = icloud_path.join(&name);
                    if entry.file_type().map(|t| t.is_file()).unwrap_or(false) && !target.exists() {
                        let _ = fs::copy(entry.path(), &target);
                    }
                }
            }
        } else {
            // Fresh setup — copy all index files to iCloud
            fs::create_dir_all(&icloud_path)?;
            if let Ok(entries) = fs::read_dir(&config_path) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                        let _ = fs::copy(entry.path(), icloud_path.join(&name));
                    }
                }
            }
        }

        // Symlink the active config path to iCloud
        let backup = config_path.with_extension("ke.bak");
        if backup.exists() {
            fs::remove_dir_all(&backup)?;
        }
        fs::rename(&config_path, &backup)?;
        std::os::unix::fs::symlink(&icloud_path, &config_path)?;
        fs::remove_dir_all(&backup)?;

        // Also symlink the secondary path if it's a plain directory (not already a symlink)
        let home = home_dir()?;
        let base = home.join(".config");
        for candidate in [base.join(INDEX_DIR), base.join(INDEX_DIR_OLD)] {
            if candidate.exists() && candidate.is_dir() && !candidate.is_symlink() && candidate != config_path {
                let cand_backup = candidate.with_extension("ke.bak");
                let _ = fs::rename(&candidate, &cand_backup);
                let _ = std::os::unix::fs::symlink(&icloud_path, &candidate);
                let _ = fs::remove_dir_all(&cand_backup);
            }
        }

        self.index_dir = config_path;
        self.icloud_dir = Some(icloud_path);
        Ok(())
    }

    /// Return projects with their key status: (project, total_keys, keys_with_values_here).
    pub fn status(&self) -> anyhow::Result<Vec<(String, usize, usize)>> {
        use crate::keychain::Keychain;
        let projects = self.list_projects()?;
        let mut results = Vec::new();
        for p in &projects {
            let keys = self.list_keys(p)?;
            let total = keys.len();
            let have = keys.iter().filter(|k| Keychain::get(p, k).is_some()).count();
            results.push((p.clone(), total, have));
        }
        Ok(results)
    }

    /// List all known projects (from index files).
    pub fn list_projects(&self) -> anyhow::Result<Vec<String>> {
        let mut projects = Vec::new();
        for entry in fs::read_dir(&self.index_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    projects.push(name.to_string());
                }
            }
        }
        projects.sort();
        Ok(projects)
    }

    /// List key names for a project.
    pub fn list_keys(&self, project: &str) -> anyhow::Result<Vec<String>> {
        let path = self.index_file(project);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path)?;
        let keys: Vec<String> = content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        Ok(keys)
    }

    /// Add a key to a project's index.
    pub fn add_key(&self, project: &str, key: &str) -> anyhow::Result<()> {
        let mut keys: BTreeSet<String> = self.list_keys(project)?.into_iter().collect();
        keys.insert(key.to_string());
        self.write_index(project, &keys.into_iter().collect::<Vec<_>>())
    }

    /// Remove a key from a project's index.
    pub fn remove_key(&self, project: &str, key: &str) -> anyhow::Result<()> {
        let keys: Vec<String> = self
            .list_keys(project)?
            .into_iter()
            .filter(|k| k != key)
            .collect();
        if keys.is_empty() {
            let path = self.index_file(project);
            let _ = fs::remove_file(&path);
        } else {
            self.write_index(project, &keys)?;
        }
        Ok(())
    }

    pub fn remove_project(&self, project: &str) -> anyhow::Result<()> {
        let path = self.index_file(project);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn index_file(&self, project: &str) -> PathBuf {
        self.index_dir.join(project)
    }

    fn write_index(&self, project: &str, keys: &[String]) -> anyhow::Result<()> {
        let path = self.index_file(project);
        let content = keys.join("\n") + "\n";
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn service_name(project: &str) -> String {
        format!("keychain-env-{}", project)
    }
}

fn home_dir() -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var("HOME").map_err(|_| anyhow::anyhow!("$HOME not set"))?,
    ))
}

fn detect_icloud(home: &Path, resolved_index: &Path) -> Option<PathBuf> {
    let icloud_base = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if !icloud_base.exists() {
        return None;
    }
    let icloud_ke = icloud_base.join(ICLOUD_DIR);
    if resolved_index.starts_with(&icloud_base) {
        Some(icloud_ke)
    } else {
        None
    }
}
