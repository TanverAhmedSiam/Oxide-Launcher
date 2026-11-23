//! Which versions are installed, read straight from disk (so it works offline and instantly).
use super::paths::Paths;
use std::collections::HashSet;

#[derive(Clone)]
pub struct InstalledVersion {
    pub id: String,
    /// "release" | "snapshot" | "old_beta" | "old_alpha"
    pub kind: String,
    pub release_time: String,
}

#[derive(Default)]
pub struct InstalledScan {
    /// Every installed profile id: "1.20.1", "1.20.1-fabric", ...
    pub ids: HashSet<String>,
    /// The base game versions among them, newest first.
    pub versions: Vec<InstalledVersion>,
}

pub fn scan(paths: &Paths) -> InstalledScan {
    let mut out = InstalledScan::default();
    let Ok(rd) = std::fs::read_dir(paths.root.join("versions")) else { return out };
    for e in rd.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let id = e.file_name().to_string_lossy().to_string();
        if !paths.is_installed(&id) {
            continue;
        }
        out.ids.insert(id.clone());
        let Ok(data) = std::fs::read(paths.version_json(&id)) else { continue };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&data) else { continue };
        if v.get("inheritsFrom").is_some() {
            continue; // a loader profile, not a base version
        }
        out.versions.push(InstalledVersion {
            id,
            kind: v.get("type").and_then(|x| x.as_str()).unwrap_or("release").to_string(),
            release_time: v.get("releaseTime").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        });
    }
    out.versions.sort_by(|a, b| b.release_time.cmp(&a.release_time));
    out
}
