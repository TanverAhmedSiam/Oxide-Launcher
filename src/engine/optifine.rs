//! OptiFine. Its website makes you download the file yourself (ad page), and it is not on
//! Modrinth or CurseForge, so it cannot be fetched automatically. Instead the user downloads
//! the official jar, and we run OptiFine's own installer against a scratch folder, then
//! import the profile and libraries it creates into our data folder.
use super::download;
use super::install;
use super::java;
use super::loaders::{profile_id, Loader};
use super::paths::Paths;
use super::versions::*;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const DOWNLOADS_URL: &str = "https://optifine.net/downloads";

// ---------------- which versions have OptiFine ----------------

static PAGE: tokio::sync::OnceCell<Option<HashSet<String>>> = tokio::sync::OnceCell::const_new();

/// Minecraft versions listed on OptiFine's public downloads page (read only, nothing is downloaded).
async fn listed_versions(client: &reqwest::Client) -> &'static Option<HashSet<String>> {
    PAGE.get_or_init(|| async {
        match client.get(DOWNLOADS_URL).send().await.and_then(|r| r.error_for_status()) {
            Ok(r) => match r.text().await {
                Ok(t) => {
                    let s = parse_page(&t);
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    })
    .await
}

/// Finds file names like "OptiFine_1.20.1_HD_U_I6.jar" and collects their Minecraft versions.
/// Preview builds ("preview_OptiFine_...") are skipped.
fn parse_page(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut rest = html;
    while let Some(i) = rest.find("OptiFine_") {
        let preview = rest[..i].ends_with("preview_");
        let after = &rest[i + "OptiFine_".len()..];
        let end = after.find('_').unwrap_or(after.len());
        let mc = &after[..end];
        if !preview
            && !mc.is_empty()
            && mc.chars().all(|c| c.is_ascii_digit() || c == '.')
            && after[end..].starts_with("_HD")
        {
            out.insert(mc.to_string());
        }
        rest = &after[end..];
    }
    out
}

/// If the page could not be read, assume release versions from 1.8 on.
fn guess(mc: &str) -> bool {
    let nums: Option<Vec<u32>> = mc.split('.').map(|p| p.parse::<u32>().ok()).collect();
    matches!(nums.as_deref(), Some([1, minor, ..]) if *minor >= 8)
}

pub async fn available(client: &reqwest::Client, mc: &str) -> bool {
    match listed_versions(client).await {
        Some(set) => set.contains(mc),
        None => guess(mc),
    }
}

// ---------------- the downloaded file ----------------

/// A matching file already sitting in the user's Downloads folder (newest first).
pub fn find_in_downloads(mc: &str) -> Option<PathBuf> {
    let dir = directories::UserDirs::new()?.download_dir()?.to_path_buf();
    let prefix = format!("OptiFine_{mc}_");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) && name.ends_with(".jar") {
            if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                if best.as_ref().map_or(true, |b| m > b.0) {
                    best = Some((m, e.path()));
                }
            }
        }
    }
    best.map(|b| b.1)
}

pub fn check_jar(path: &Path, mc: &str) -> Result<()> {
    if !path.is_file() {
        bail!("That file does not exist.");
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !name.starts_with(&format!("OptiFine_{mc}_")) || !name.ends_with(".jar") {
        bail!("This is not the OptiFine file for Minecraft {mc}. Its name should look like OptiFine_{mc}_HD_U_….jar");
    }
    Ok(())
}

// ---------------- install ----------------

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}

/// Installs the base game if needed, opens OptiFine's own installer window for the user
/// (pointed at a scratch ".minecraft" folder), waits until it is closed, then imports the
/// profile it made as "<mc>-optifine".
pub async fn install(
    client: &reqwest::Client,
    paths: &Paths,
    entry: &ManifestEntry,
    jar: &Path,
    on_progress: impl Fn(usize, usize),
) -> Result<()> {
    let mc = &entry.id;
    check_jar(jar, mc)?;
    if !paths.is_installed(mc) {
        install::install(client, paths, entry, &on_progress).await?;
    }
    on_progress(0, 0);

    let base = load_local(paths, mc)?;
    let major = base.java_version.as_ref().map_or(8, |j| j.major_version);
    let (java_exe, found) = java::find_java(paths, major, true);

    // Scratch ".minecraft" containing just the vanilla version the installer needs to see.
    let tmp = paths.root.join("tmp").join("optifine");
    let _ = std::fs::remove_dir_all(&tmp);
    let mcdir = tmp.join(".minecraft");
    let vdir = mcdir.join("versions").join(mc);
    std::fs::create_dir_all(&vdir)?;
    std::fs::copy(paths.version_json(mc), vdir.join(format!("{mc}.json")))?;
    std::fs::copy(paths.version_jar(mc), vdir.join(format!("{mc}.jar")))?;
    std::fs::write(mcdir.join("launcher_profiles.json"), br#"{"profiles":{},"version":3}"#)?;

    // OptiFine's installer defaults to %APPDATA%\.minecraft, so point APPDATA at the scratch folder.
    let (jar_buf, tmp2, je) = (jar.to_path_buf(), tmp.clone(), java_exe.clone());
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::process::Command::new(&je)
            .arg("-jar")
            .arg(&jar_buf)
            .env("APPDATA", &tmp2)
            .env("USERPROFILE", &tmp2)
            .current_dir(&tmp2)
            .status()?;
        Ok(())
    })
    .await?
    .with_context(|| {
        let hint = if found { "" } else { " (bundled Java not found, tried system Java)" };
        format!("could not run Java {major} ({}){hint}", java_exe.display())
    })?;

    // What did the installer create?
    let mut produced = None;
    if let Ok(rd) = std::fs::read_dir(mcdir.join("versions")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name != *mc
                && name.to_lowercase().contains("optifine")
                && e.path().join(format!("{name}.json")).exists()
            {
                produced = Some(name);
                break;
            }
        }
    }
    let Some(produced) = produced else {
        bail!("OptiFine was not installed. Press Install again, then click Install in the OptiFine window and close it when it says it is done.");
    };
    let json_path = mcdir.join("versions").join(&produced).join(format!("{produced}.json"));
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json_path)?)?;
    if v.get("inheritsFrom").and_then(|x| x.as_str()) != Some(mc.as_str()) {
        bail!(
            "This OptiFine build uses an unexpected profile layout, so it was not imported. \
             The installer's output was kept in {}",
            tmp.display()
        );
    }

    // Import: libraries (OptiFine + its launch wrapper) and the profile under our own id.
    let libs = mcdir.join("libraries");
    if libs.exists() {
        copy_dir(&libs, &paths.libraries())?;
    }
    let pid = profile_id(mc, Some(Loader::OptiFine));
    v["id"] = serde_json::Value::String(pid.clone());
    let dir = paths.version_dir(&pid);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(paths.version_json(&pid), serde_json::to_vec_pretty(&v)?)?;

    let meta = load_local(paths, &pid)?;
    let res = install::resolve_libraries(&meta, paths);
    download::fetch_all(client, res.jobs, on_progress).await?;

    std::fs::write(dir.join(".installed"), b"1")?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}
