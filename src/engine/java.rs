use super::config::JavaChoice;
use super::paths::Paths;
use std::path::PathBuf;

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
