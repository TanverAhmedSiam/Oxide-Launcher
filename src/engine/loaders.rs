use super::download::{self, Job};
use super::install;
use super::java;
use super::paths::Paths;
use super::versions::*;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Stdio;

/// Mod loaders the launcher can install.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Loader {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
    OptiFine,
}

impl Loader {
    pub const ALL: [Loader; 5] =
        [Loader::Fabric, Loader::Quilt, Loader::Forge, Loader::NeoForge, Loader::OptiFine];

    pub fn name(self) -> &'static str {
        match self {
            Loader::Fabric => "Fabric",
            Loader::Quilt => "Quilt",
            Loader::Forge => "Forge",
            Loader::NeoForge => "NeoForge",
            Loader::OptiFine => "OptiFine",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Loader::Fabric => "fabric",
            Loader::Quilt => "quilt",
            Loader::Forge => "forge",
            Loader::NeoForge => "neoforge",
            Loader::OptiFine => "optifine",
        }
    }
    /// Forge and NeoForge are installed by running their official installer.
    pub fn uses_installer(self) -> bool {
        matches!(self, Loader::Forge | Loader::NeoForge)
    }
    /// Meta API for loaders that publish a ready-made profile (Fabric, Quilt).
    fn meta_base(self) -> &'static str {
        match self {
            Loader::Fabric => "https://meta.fabricmc.net/v2/versions/loader",
            Loader::Quilt => "https://meta.quiltmc.org/v3/versions/loader",
            Loader::Forge | Loader::NeoForge | Loader::OptiFine => "",
        }
    }
}

/// Id of the thing we install and launch: "1.21.1" or "1.21.1-fabric".
/// It does not contain the loader version, so updating the loader keeps the same
/// game folder (worlds, mods, settings).
pub fn profile_id(mc: &str, loader: Option<Loader>) -> String {
    match loader {
        Some(l) => format!("{mc}-{}", l.slug()),
        None => mc.to_string(),
    }
}

async fn get_text(client: &reqwest::Client, url: &str) -> Result<String> {
    Ok(client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("GET {url}"))?
        .text()
        .await?)
}

// ---------------- Fabric / Quilt ----------------

#[derive(Deserialize)]
struct Entry {
    loader: Info,
}
#[derive(Deserialize)]
struct Info {
    version: String,
    #[serde(default)]
    stable: Option<bool>,
}

async fn loader_versions(client: &reqwest::Client, l: Loader, mc: &str) -> Result<Vec<Info>> {
    let text = get_text(client, &format!("{}/{}", l.meta_base(), mc)).await?;
    let entries: Vec<Entry> = serde_json::from_str(&text)?;
    Ok(entries.into_iter().map(|e| e.loader).collect())
}

// ---------------- Forge ----------------

#[derive(Deserialize)]
struct Promos {
    promos: HashMap<String, String>,
}

static FORGE_PROMOS: tokio::sync::OnceCell<HashMap<String, String>> = tokio::sync::OnceCell::const_new();

/// "<mc>-recommended" / "<mc>-latest" -> Forge version. Fetched once per run.
async fn forge_promos(client: &reqwest::Client) -> Result<&'static HashMap<String, String>> {
    FORGE_PROMOS
        .get_or_try_init(|| async {
            let text = get_text(
                client,
                "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
            )
            .await?;
            let p: Promos = serde_json::from_str(&text)?;
            Ok::<_, anyhow::Error>(p.promos)
        })
        .await
}

/// Forge installs through its installer, which behaves consistently from 1.12.2 on.
fn forge_supported(mc: &str) -> bool {
    let mut it = mc.split('.');
    if it.next() != Some("1") {
        return false;
    }
    let Some(minor) = it.next().and_then(|s| s.parse::<u32>().ok()) else { return false };
    let patch = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    if it.next().is_some() {
        return false;
    }
    (minor, patch) >= (12, 2)
}

async fn forge_version(client: &reqwest::Client, mc: &str) -> Result<String> {
    let p = forge_promos(client).await?;
    p.get(&format!("{mc}-recommended"))
        .or_else(|| p.get(&format!("{mc}-latest")))
        .cloned()
        .with_context(|| format!("there is no Forge build for Minecraft {mc}"))
}

// ---------------- NeoForge ----------------

#[derive(Deserialize)]
struct NfVersions {
    versions: Vec<String>,
}

static NEOFORGE_VERSIONS: tokio::sync::OnceCell<Vec<String>> = tokio::sync::OnceCell::const_new();

/// Every NeoForge version, fetched once per run (maven API first, maven-metadata.xml as backup).
async fn neoforge_versions(client: &reqwest::Client) -> Result<&'static Vec<String>> {
    NEOFORGE_VERSIONS
        .get_or_try_init(|| async {
            if let Ok(text) = get_text(
                client,
                "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge",
            )
            .await
            {
                if let Ok(v) = serde_json::from_str::<NfVersions>(&text) {
                    if !v.versions.is_empty() {
                        return Ok::<_, anyhow::Error>(v.versions);
                    }
                }
            }
            let xml = get_text(
                client,
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
            )
            .await?;
            let list: Vec<String> = xml
                .split("<version>")
                .skip(1)
                .filter_map(|s| s.split("</version>").next())
                .map(|s| s.trim().to_string())
                .collect();
            if list.is_empty() {
                bail!("no NeoForge versions found");
            }
            Ok(list)
        })
        .await
}

/// NeoForge versions start with the Minecraft version minus the leading "1.":
/// 1.21 -> "21.0.", 1.21.1 -> "21.1.", 1.20.4 -> "20.4." and, for the newer year-based
/// Minecraft versions, 26.1 -> "26.1.0.". Supported from 1.20.2 (1.20.1 is a different
/// artifact and the NeoForge team recommends Forge there).
fn neoforge_prefix(mc: &str) -> Option<String> {
    let nums: Vec<u32> = mc.split('.').map(|p| p.parse::<u32>().ok()).collect::<Option<Vec<u32>>>()?;
    match nums.as_slice() {
        [1, minor, rest @ ..] if rest.len() <= 1 => {
            let patch = rest.first().copied().unwrap_or(0);
            if (*minor, patch) >= (20, 2) {
                Some(format!("{minor}.{patch}."))
            } else {
                None
            }
        }
        [major, minor, rest @ ..] if *major != 1 && rest.len() <= 1 => {
            let patch = rest.first().copied().unwrap_or(0);
            Some(format!("{major}.{minor}.{patch}."))
        }
        _ => None,
    }
}

/// Newest stable NeoForge build for this Minecraft version (a beta only if nothing else exists).
async fn neoforge_version(client: &reqwest::Client, mc: &str) -> Result<String> {
    let prefix = neoforge_prefix(mc)
        .with_context(|| format!("NeoForge supports Minecraft 1.20.2 and newer, not {mc}"))?;
    let all = neoforge_versions(client).await?;
    all.iter()
        .filter(|v| v.starts_with(&prefix))
        .max_by_key(|v| {
            let rest = &v.as_str()[prefix.len()..];
            let stable = !rest.contains('-');
            let n: u64 = rest
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            (stable, n)
        })
        .cloned()
        .with_context(|| format!("there is no NeoForge build for Minecraft {mc}"))
}

// ---------------- availability and install ----------------

/// Which loaders support this Minecraft version.
pub async fn available(client: &reqwest::Client, mc: &str) -> Vec<Loader> {
    let mut out = Vec::new();
    for l in Loader::ALL {
        let ok = match l {
            Loader::Forge => forge_supported(mc) && forge_version(client, mc).await.is_ok(),
            Loader::NeoForge => neoforge_version(client, mc).await.is_ok(),
            Loader::OptiFine => super::optifine::available(client, mc).await,
            _ => loader_versions(client, l, mc).await.map_or(false, |v| !v.is_empty()),
        };
        if ok {
            out.push(l);
        }
    }
    out
}

/// Installs the base game if needed, then the loader's profile and libraries.
pub async fn install_loader(
    client: &reqwest::Client,
    paths: &Paths,
    entry: &ManifestEntry,
    l: Loader,
    on_progress: impl Fn(usize, usize),
) -> Result<()> {
    if l == Loader::OptiFine {
        bail!("OptiFine needs the file you download from optifine.net");
    }
    if !paths.is_installed(&entry.id) {
        install::install(client, paths, entry, &on_progress).await?;
    }
    if l.uses_installer() {
        return install_via_installer(client, paths, entry, l, on_progress).await;
    }

    let versions = loader_versions(client, l, &entry.id)
        .await
        .with_context(|| format!("{} may not support {}", entry.id, l.name()))?;
    // Newest stable loader (Quilt has no `stable` flag, so treat "-beta" style names as unstable).
    let chosen = versions
        .iter()
        .find(|v| v.stable.unwrap_or(!v.version.contains('-')))
        .or(versions.first())
        .context("no loader versions available")?;

    let url = format!("{}/{}/{}/profile/json", l.meta_base(), entry.id, chosen.version);
    let text = get_text(client, &url).await?;
    let mut v: serde_json::Value = serde_json::from_str(&text)?;
    let pid = profile_id(&entry.id, Some(l));
    v["id"] = serde_json::Value::String(pid.clone());
    v["inheritsFrom"] = serde_json::Value::String(entry.id.clone());

    let dir = paths.version_dir(&pid);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(paths.version_json(&pid), serde_json::to_vec_pretty(&v)?)?;

    // Merged view (base game + loader); download whatever is missing.
    let meta = load_local(paths, &pid)?;
    let res = install::resolve_libraries(&meta, paths);
    download::fetch_all(client, res.jobs, on_progress).await?;

    std::fs::write(dir.join(".installed"), b"1")?;
    Ok(())
}

/// Downloads the official installer (Forge or NeoForge), runs it without a window against our
/// data folder (it patches the game and downloads the libraries), then registers the profile
/// it created under our own id, e.g. "1.20.1-forge" or "1.21.1-neoforge".
async fn install_via_installer(
    client: &reqwest::Client,
    paths: &Paths,
    entry: &ManifestEntry,
    l: Loader,
    on_progress: impl Fn(usize, usize),
) -> Result<()> {
    let mc = &entry.id;
    let slug = l.slug();

    let (ver, url) = match l {
        Loader::Forge => {
            if !forge_supported(mc) {
                bail!("Forge is supported for Minecraft 1.12.2 and newer");
            }
            let fv = forge_version(client, mc).await?;
            let full = format!("{mc}-{fv}");
            let url = format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"
            );
            (fv, url)
        }
        Loader::NeoForge => {
            let nv = neoforge_version(client, mc).await?;
            let url = format!(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/{nv}/neoforge-{nv}-installer.jar"
            );
            (nv, url)
        }
        _ => bail!("{} does not use an installer", l.name()),
    };
    let jar = paths.root.join("installers").join(format!("{slug}-{mc}-{ver}-installer.jar"));

    // Both maven servers publish a checksum next to every file.
    let sha1 = match client.get(format!("{url}.sha1")).send().await.and_then(|r| r.error_for_status()) {
        Ok(r) => r.text().await.ok().and_then(|t| t.split_whitespace().next().map(str::to_string)),
        Err(_) => None,
    };
    download::fetch_retry(client, &Job { url, path: jar.clone(), sha1 }).await?;
    on_progress(0, 0);

    // The installers insist on a launcher_profiles.json in the target folder.
    let lp = paths.root.join("launcher_profiles.json");
    if !lp.exists() {
        std::fs::write(&lp, br#"{"profiles":{},"version":3}"#)?;
    }

    // Run it with the Java this Minecraft version needs.
    let base = load_local(paths, mc)?;
    let major = base.java_version.as_ref().map_or(8, |j| j.major_version);
    let (java_exe, found) = java::find_java(paths, major, false);
    let log_name = format!("{slug}-install.log");
    let log_path = paths.root.join("logs").join(&log_name);
    let root = paths.root.clone();
    let java_for_err = java_exe.clone();
    // NeoForge's installer accepts both spellings of the option; try the second if the first fails.
    let flags: &'static [&'static str] =
        if l == Loader::Forge { &["--installClient"] } else { &["--installClient", "--install-client"] };
    let status = tokio::task::spawn_blocking(move || -> Result<std::process::ExitStatus> {
        if let Some(d) = log_path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut last = None;
        for flag in flags {
            let log = std::fs::File::create(&log_path)?;
            let mut cmd = std::process::Command::new(&java_exe);
            cmd.arg("-jar")
                .arg(&jar)
                .arg(flag)
                .arg(&root)
                .current_dir(&root)
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
            let s = cmd.status()?;
            let ok = s.success();
            last = Some(s);
            if ok {
                break;
            }
        }
        last.context("no installer option to try")
    })
    .await?
    .with_context(|| {
        let hint = if found { "" } else { " (bundled Java not found, tried system Java)" };
        format!("could not run Java {major} ({}){hint}", java_for_err.display())
    })?;
    if !status.success() {
        bail!("The {} installer failed ({status}). See logs\\{log_name}", l.name());
    }

    // Find the profile the installer wrote and register it under our own id.
    let pid = profile_id(mc, Some(l));
    let src = find_installer_profile(paths, mc, &ver, slug, &log_name)?;
    let text = std::fs::read_to_string(paths.version_json(&src))?;
    let mut v: serde_json::Value = serde_json::from_str(&text)?;
    v["id"] = serde_json::Value::String(pid.clone());
    v["inheritsFrom"] = serde_json::Value::String(mc.clone());
    let dir = paths.version_dir(&pid);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(paths.version_json(&pid), serde_json::to_vec_pretty(&v)?)?;

    // Fetch anything the installer did not already put in place.
    let meta = load_local(paths, &pid)?;
    let res = install::resolve_libraries(&meta, paths);
    download::fetch_all(client, res.jobs, on_progress).await?;

    std::fs::write(dir.join(".installed"), b"1")?;
    Ok(())
}

/// The installers name their profile like "1.20.1-forge-47.2.0" or "neoforge-21.1.172".
/// Find it by name, preferring the exact loader version, then the newest.
fn find_installer_profile(paths: &Paths, mc: &str, ver: &str, keyword: &str, log_name: &str) -> Result<String> {
    let ours: Vec<String> = Loader::ALL.iter().map(|l| profile_id(mc, Some(*l))).collect();
    let mut found: Vec<(bool, std::time::SystemTime, String)> = Vec::new();
    for e in std::fs::read_dir(paths.root.join("versions"))?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let lname = name.to_lowercase();
        if ours.contains(&name) || !lname.contains(keyword) {
            continue;
        }
        if keyword == "forge" && lname.contains("neoforge") {
            continue; // a NeoForge profile is not a Forge profile
        }
        if !(name.contains(ver) || name.contains(mc)) || !paths.version_json(&name).exists() {
            continue;
        }
        let mtime = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        found.push((name.contains(ver), mtime, name));
    }
    found.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
    found
        .into_iter()
        .next()
        .map(|f| f.2)
        .with_context(|| format!("the installer finished but its profile was not found. See logs\\{log_name}"))
}
