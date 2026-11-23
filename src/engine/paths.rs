use anyhow::{anyhow, Result};
use std::path::PathBuf;

/// Shared files (libraries, assets, versions) live under %APPDATA%\RustMCLauncher.
/// Game folders (saves, mods, resourcepacks, ...) live in `instances_dir`,
/// which the user can change in Settings.
#[derive(Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub instances_dir: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let base = directories::BaseDirs::new().ok_or_else(|| anyhow!("no home directory"))?;
        let data = base.data_dir();
        let root = data.join("OxideLauncher");
        let old = data.join("RustMCLauncher");
        // First start after the rename: move the old data folder over (versions, mods, accounts...).
        // If that fails (a file is locked), keep using the old folder so nothing is lost.
        if !root.exists() && old.exists() && std::fs::rename(&old, &root).is_err() {
            return Ok(Self { instances_dir: old.join("instances"), root: old });
        }
        Ok(Self { instances_dir: root.join("instances"), root })
    }
    pub fn is_installed(&self, id: &str) -> bool {
        self.version_dir(id).join(".installed").exists()
    }
    pub fn libraries(&self) -> PathBuf { self.root.join("libraries") }
    pub fn assets(&self) -> PathBuf { self.root.join("assets") }
    pub fn version_dir(&self, id: &str) -> PathBuf { self.root.join("versions").join(id) }
    pub fn version_json(&self, id: &str) -> PathBuf { self.version_dir(id).join(format!("{id}.json")) }
    pub fn version_jar(&self, id: &str) -> PathBuf { self.version_dir(id).join(format!("{id}.jar")) }
    pub fn natives(&self, id: &str) -> PathBuf { self.version_dir(id).join("natives") }
    /// The game directory for one version.
    pub fn instance(&self, id: &str) -> PathBuf { self.instances_dir.join(id) }
    pub fn log_config(&self, file_id: &str) -> PathBuf {
        self.assets().join("log_configs").join(file_id)
    }
}
