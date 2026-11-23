use super::config::JavaChoice;
use super::download::{self, Job};
use super::paths::Paths;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn runtime_roots(paths: &Paths) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(me) = std::env::current_exe() {
        if let Some(d) = me.parent() {
            roots.push(d.join("runtimes")); // where the installer puts bundled JREs
        }
    }
    roots.push(paths.root.join("runtimes"));
    roots
}

fn which(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(exe)).find(|p| p.is_file())
}

/// Bundled runtime for a Minecraft version's required major Java (8, 16, 17, 21).
/// Returns (path, found). If not found, falls back to Java on PATH.
pub fn find_java(paths: &Paths, major: u32, gui: bool) -> (PathBuf, bool) {
    let exe = match (cfg!(windows), gui) {
        (true, true) => "javaw.exe",
        (true, false) => "java.exe",
        _ => "java",
    };
    // Bundled runtimes live in runtimes/java-<N>. Use the exact version, or for Java 9+ the
    // nearest newer one (Minecraft 1.17 asks for Java 16, which Java 17 runs fine).
    // Java 8 is never replaced by a newer one: old Minecraft/Forge versions break on it.
    let mut best: Option<(u32, PathBuf)> = None;
    for root in runtime_roots(paths) {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(n) = name.strip_prefix("java-").and_then(|v| v.parse::<u32>().ok()) else { continue };
            let usable = n == major || (major >= 9 && n > major);
            let p = e.path().join("bin").join(exe);
            if usable && p.exists() && best.as_ref().map_or(true, |b| n < b.0) {
                best = Some((n, p));
            }
        }
    }
    if let Some((_, p)) = best {
        return (p, true);
    }
    match which(exe) {
        Some(p) => (p, false),
        None => (PathBuf::from(exe), false),
    }
}

/// Honour the user's Settings choice, otherwise pick automatically.
pub fn resolve(paths: &Paths, choice: &JavaChoice, major: u32, gui: bool) -> (PathBuf, bool) {
    if let JavaChoice::Custom(p) = choice {
        let p = PathBuf::from(p.trim());
        if p.is_file() {
            return (p, true);
        }
    }
    find_java(paths, major, gui)
}

/// Java installs we can offer in Settings: bundled runtimes and common system locations.
pub fn detect_installed(paths: &Paths) -> Vec<(String, PathBuf)> {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    let mut out: Vec<(String, PathBuf)> = Vec::new();

    for root in runtime_roots(paths) {
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let p = e.path().join("bin").join(exe);
                if let Some(v) = name.strip_prefix("java-") {
                    if p.exists() {
                        out.push((format!("Java {v} (bundled)"), p));
                    }
                }
            }
        }
    }
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            let Ok(pf) = std::env::var(var) else { continue };
            for vendor in ["Java", "Eclipse Adoptium", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto", "Semeru"] {
                if let Ok(rd) = std::fs::read_dir(PathBuf::from(&pf).join(vendor)) {
                    for e in rd.flatten() {
                        let p = e.path().join("bin").join(exe);
                        if p.exists() {
                            out.push((format!("{} ({vendor})", e.file_name().to_string_lossy()), p));
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.1 == b.1);
    out
}

// ---------------- automatic Java ----------------

#[derive(Deserialize)]
struct AdoptiumRelease {
    binary: AdoptiumBinary,
}
#[derive(Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}
#[derive(Deserialize)]
struct AdoptiumPackage {
    link: String,
    checksum: String,
}

/// Minecraft's "Java 16" runs fine on 17, which is the version that is actually published.
fn download_feature(major: u32) -> u32 {
    if major == 16 {
        17
    } else {
        major
    }
}

fn extract_zip_strip_first(zip_path: &Path, dest: &Path) -> Result<()> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        let Some(rel) = f.enclosed_name() else { continue };
        let mut comps = rel.components();
        comps.next(); // drop the zip's top-level folder
        let rel: PathBuf = comps.collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut o = std::fs::File::create(&out)?;
            std::io::copy(&mut f, &mut o)?;
        }
    }
    Ok(())
}

/// Downloads the matching Eclipse Temurin JRE into <data>/runtimes/java-<N> (checksum verified).
pub async fn download_runtime(client: &reqwest::Client, paths: &Paths, major: u32) -> Result<()> {
    let feature = download_feature(major);
    let api = format!(
        "https://api.adoptium.net/v3/assets/latest/{feature}/hotspot?architecture=x64&image_type=jre&os=windows&vendor=eclipse"
    );
    let text = client
        .get(&api)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("GET {api}"))?
        .text()
        .await?;
    let rels: Vec<AdoptiumRelease> = serde_json::from_str(&text)?;
    let pkg = &rels
        .first()
        .with_context(|| format!("there is no Java {feature} runtime to download"))?
        .binary
        .package;

    let runtimes = paths.root.join("runtimes");
    std::fs::create_dir_all(&runtimes)?;
    let zip = runtimes.join(format!("temurin-{feature}.zip"));
    let _ = std::fs::remove_file(&zip);
    download::fetch_retry(client, &Job { url: pkg.link.clone(), path: zip.clone(), sha1: None }).await?;

    let want = pkg.checksum.clone();
    let dest = runtimes.join(format!("java-{feature}"));
    let (zip2, dest2) = (zip.clone(), dest.clone());
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut h = Sha256::new();
        std::io::copy(&mut std::fs::File::open(&zip2)?, &mut h)?;
        if !hex::encode(h.finalize()).eq_ignore_ascii_case(&want) {
            bail!("the downloaded Java file is damaged (checksum mismatch)");
        }
        let part = dest2.with_extension("part");
        let _ = std::fs::remove_dir_all(&part);
        extract_zip_strip_first(&zip2, &part)?;
        let _ = std::fs::remove_dir_all(&dest2);
        std::fs::rename(&part, &dest2)?;
        Ok(())
    })
    .await??;
    let _ = std::fs::remove_file(&zip);
    Ok(())
}

/// The right Java for a Minecraft version, with no manual choice needed:
/// the bundled runtime if there is one, otherwise it is downloaded once (first time only).
/// Only if that fails (for example offline) does it fall back to a Java found on PATH.
pub async fn ensure(
    client: &reqwest::Client,
    paths: &Paths,
    major: u32,
    gui: bool,
    status: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf> {
    let (p, found) = find_java(paths, major, gui);
    if found {
        return Ok(p);
    }
    status(&format!("Downloading Java {} (first time only)…", download_feature(major)));
    match download_runtime(client, paths, major).await {
        Ok(()) => {
            let (p2, found2) = find_java(paths, major, gui);
            if found2 {
                return Ok(p2);
            }
        }
        Err(e) => {
            if p.is_absolute() && p.exists() {
                return Ok(p); // best effort: a Java that is already on this PC
            }
            return Err(e.context(format!("Java {major} is needed but could not be downloaded")));
        }
    }
    Ok(p)
}

/// Same, but a Java path chosen in Settings wins.
pub async fn resolve_or_download(
    client: &reqwest::Client,
    paths: &Paths,
    choice: &JavaChoice,
    major: u32,
    gui: bool,
    status: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf> {
    if let JavaChoice::Custom(p) = choice {
        let pb = PathBuf::from(p.trim());
        if pb.is_file() {
            return Ok(pb);
        }
    }
    ensure(client, paths, major, gui, status).await
}
