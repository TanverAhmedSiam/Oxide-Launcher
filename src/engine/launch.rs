use super::accounts::offline_uuid;
use super::config::WindowSize;
use super::install::{asset_index, resolve_libraries};
use super::paths::Paths;
use super::versions::*;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

pub struct LaunchOpts {
    pub username: String,
    pub ram_mb: u32,
    pub java: PathBuf,
    pub window: WindowSize,
}

fn sub(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

pub fn build_command(paths: &Paths, meta: &VersionMeta, o: &LaunchOpts) -> Result<Command> {
    let res = resolve_libraries(meta, paths);
    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut cp: Vec<String> = res.classpath.iter().map(|p| p.display().to_string()).collect();
    cp.push(paths.version_jar(&meta.jar_id).display().to_string());

    let idx = asset_index(paths, meta)?;
    let game_dir = paths.instance(&meta.id);
    std::fs::create_dir_all(&game_dir)?;
    let assets_root = paths.assets();
    let game_assets = if idx.r#virtual {
        assets_root.join("virtual").join("legacy")
    } else {
        assets_root.clone()
    };

    let mut v: HashMap<&str, String> = HashMap::new();
    v.insert("auth_player_name", o.username.clone());
    v.insert("auth_uuid", offline_uuid(&o.username));
    v.insert("auth_access_token", "0".into());
    v.insert("auth_session", "0".into());
    v.insert("auth_xuid", "0".into());
    v.insert("clientid", "0".into());
    v.insert("user_type", "legacy".into());
    v.insert("user_properties", "{}".into());
    // For loader profiles this must be the base game version ("1.20.1"), not "1.20.1-forge":
    // Forge's -DignoreList contains "${version_name}.jar" to keep the vanilla client jar
    // (1.20.1.jar) out of the module path, otherwise it clashes with Forge's patched client.
    v.insert("version_name", meta.jar_id.clone());
    v.insert("version_type", meta.kind.clone());
    v.insert("game_directory", game_dir.display().to_string());
    v.insert("assets_root", assets_root.display().to_string());
    v.insert("game_assets", game_assets.display().to_string());
    v.insert("assets_index_name", meta.asset_index.id.clone());
    v.insert("natives_directory", paths.natives(&meta.jar_id).display().to_string());
    v.insert("library_directory", paths.libraries().display().to_string());
    v.insert("classpath_separator", sep.into());
    v.insert("classpath", cp.join(sep));
    v.insert("launcher_name", "OxideLauncher".into());
    v.insert("launcher_version", env!("CARGO_PKG_VERSION").into());

    // JVM arguments
    let mut jvm = vec![
        format!("-Xmx{}M", o.ram_mb),
        format!("-Xms{}M", o.ram_mb.min(512)),
    ];
    match &meta.arguments {
        Some(a) => jvm.extend(collect_args(&a.jvm)),
        None => {
            // Pre-1.13 versions have no JVM args in the JSON.
            jvm.push("-Djava.library.path=${natives_directory}".into());
            jvm.push("-cp".into());
            jvm.push("${classpath}".into());
        }
    }
    if let Some(lc) = meta.logging.as_ref().and_then(|l| l.client.as_ref()) {
        jvm.push(lc.argument.replace("${path}", &paths.log_config(&lc.file.id).display().to_string()));
    }

    // Game arguments
    let mut game: Vec<String> = match (&meta.arguments, &meta.minecraft_arguments) {
        (Some(a), _) => collect_args(&a.game),
        (None, Some(s)) => s.split_whitespace().map(String::from).collect(),
        _ => vec![],
    };

    // Window size. Only added when the user chose one, so old versions are untouched.
    match &o.window {
        WindowSize::Auto => {}
        WindowSize::Fullscreen => game.push("--fullscreen".into()),
        WindowSize::Size(w, h) => {
            game.extend(["--width".to_string(), w.to_string(), "--height".to_string(), h.to_string()]);
        }
    }

    let mut cmd = Command::new(&o.java);
    for a in &jvm {
        cmd.arg(sub(a, &v));
    }
    cmd.arg(&meta.main_class);
    for a in &game {
        cmd.arg(sub(a, &v));
    }
    cmd.current_dir(&game_dir);
    Ok(cmd)
}
