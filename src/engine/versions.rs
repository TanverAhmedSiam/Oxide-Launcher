use super::paths::Paths;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub const MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<ManifestEntry>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    /// "release" | "snapshot" | "old_beta" | "old_alpha"
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: Option<String>,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

pub async fn fetch_manifest(client: &reqwest::Client, paths: &Paths) -> Result<Manifest> {
    let cache = paths.root.join("version_manifest_v2.json");
    match client.get(MANIFEST_URL).send().await.and_then(|r| r.error_for_status()) {
        Ok(r) => {
            let text = r.text().await?;
            tokio::fs::create_dir_all(&paths.root).await?;
            tokio::fs::write(&cache, &text).await?;
            Ok(serde_json::from_str(&text)?)
        }
        Err(e) => {
            // Offline: fall back to the cached manifest.
            let text = tokio::fs::read_to_string(&cache)
                .await
                .with_context(|| format!("offline and no cached manifest ({e})"))?;
            Ok(serde_json::from_str(&text)?)
        }
    }
}

// ---------- per-version JSON ----------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    pub id: String,
    pub main_class: String,
    pub asset_index: AssetIndexRef,
    pub downloads: Downloads,
    pub libraries: Vec<Library>,
    pub arguments: Option<Arguments>,
    pub minecraft_arguments: Option<String>,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
    #[serde(rename = "type")]
    pub kind: String,
    /// Which version's jar and natives to use. Same as `id` for vanilla; the base game
    /// version for loader profiles (e.g. "1.21.1" for "1.21.1-fabric").
    #[serde(skip)]
    pub jar_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: Option<String>,
    pub url: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Downloads {
    pub client: FileRef,
}
#[derive(Debug, Clone, Deserialize)]
pub struct FileRef {
    pub sha1: Option<String>,
    pub url: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: u32,
}

// Log4j config shipped by Mojang. Passing it is the Log4Shell mitigation for 1.7-1.18.
#[derive(Debug, Clone, Deserialize)]
pub struct Logging {
    pub client: Option<LoggingClient>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LogFile,
}
#[derive(Debug, Clone, Deserialize)]
pub struct LogFile {
    pub id: String,
    pub sha1: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    /// Maven repository base URL (Fabric/Quilt style libraries have this instead of `downloads`).
    pub url: Option<String>,
    pub downloads: Option<LibDownloads>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<HashMap<String, String>>,
    pub extract: Option<Extract>,
    /// Old Forge profiles mark server-only libraries with `"clientreq": false`.
    #[serde(default)]
    pub clientreq: Option<bool>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct LibDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub sha1: Option<String>,
    pub url: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Extract {
    pub exclude: Option<Vec<String>>,
}

// ---------- rules ----------

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

fn current_arch() -> &'static str {
    if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

fn rule_matches(r: &Rule) -> bool {
    // Feature-gated rules (demo mode, custom resolution, ...) are skipped for now.
    if r.features.as_ref().map_or(false, |f| !f.is_empty()) {
        return false;
    }
    if let Some(os) = &r.os {
        if os.name.as_deref().map_or(false, |n| n != "windows") {
            return false;
        }
        if os.arch.as_deref().map_or(false, |a| a != current_arch()) {
            return false;
        }
    }
    true
}

/// Mojang semantics: no rules = allowed; otherwise the last matching rule wins.
pub fn rules_allow(rules: &Option<Vec<Rule>>) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        if rule_matches(r) {
            allowed = r.action == "allow";
        }
    }
    allowed
}

// ---------- launch arguments ----------

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Arg>,
    #[serde(default)]
    pub jvm: Vec<Arg>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Arg {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: OneOrMany },
}
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

pub fn collect_args(args: &[Arg]) -> Vec<String> {
    let mut out = Vec::new();
    for a in args {
        match a {
            Arg::Plain(s) => out.push(s.clone()),
            Arg::Conditional { rules, value } => {
                if rules_allow(&Some(rules.iter().map(clone_rule).collect())) {
                    match value {
                        OneOrMany::One(s) => out.push(s.clone()),
                        OneOrMany::Many(v) => out.extend(v.iter().cloned()),
                    }
                }
            }
        }
    }
    out
}

fn clone_rule(r: &Rule) -> Rule {
    Rule {
        action: r.action.clone(),
        os: r.os.as_ref().map(|o| OsRule { name: o.name.clone(), arch: o.arch.clone() }),
        features: r.features.clone(),
    }
}

/// Loader profiles ("inheritsFrom") only describe what differs from the base game.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Overlay {
    id: String,
    main_class: Option<String>,
    arguments: Option<Arguments>,
    minecraft_arguments: Option<String>,
    #[serde(default)]
    libraries: Vec<Library>,
}

fn lib_key(name: &str) -> String {
    let mut p = name.split(':');
    let g = p.next().unwrap_or("");
    let a = p.next().unwrap_or("");
    let _version = p.next();
    match p.next() {
        Some(c) => format!("{g}:{a}:{c}"),
        None => format!("{g}:{a}"),
    }
}

fn merge(mut base: VersionMeta, o: Overlay) -> VersionMeta {
    base.id = o.id; // jar_id stays the base game's
    if let Some(mc) = o.main_class {
        base.main_class = mc;
    }
    if let Some(oa) = o.arguments {
        if let Some(b) = base.arguments.as_mut() {
            b.game.extend(oa.game);
            b.jvm.extend(oa.jvm);
        } else {
            base.arguments = Some(oa);
        }
    }
    if o.minecraft_arguments.is_some() {
        base.minecraft_arguments = o.minecraft_arguments;
    }
    // Loader libraries first; a loader library replaces a base one with the same name.
    let keys: HashSet<String> = o.libraries.iter().map(|l| lib_key(&l.name)).collect();
    let mut libs = o.libraries;
    libs.extend(base.libraries.into_iter().filter(|l| !keys.contains(&lib_key(&l.name))));
    base.libraries = libs;
    base
}

pub fn load_local(paths: &Paths, id: &str) -> Result<VersionMeta> {
    let p = paths.version_json(id);
    let data = std::fs::read(&p)
        .with_context(|| format!("{id} is not installed (run `rmcl install {id}`)"))?;
    let v: serde_json::Value = serde_json::from_slice(&data)?;
    let parent = v.get("inheritsFrom").and_then(|p| p.as_str()).map(String::from);
    if let Some(parent) = parent {
        let overlay: Overlay = serde_json::from_value(v)?;
        let base = load_local(paths, &parent)?;
        return Ok(merge(base, overlay));
    }
    let mut m: VersionMeta = serde_json::from_value(v)?;
    m.jar_id = m.id.clone();
    Ok(m)
}
