use anyhow::{Context, Result};
use rmcl::engine::{accounts, config, download, install, java, launch, paths::Paths, versions};
use std::path::PathBuf;

const USAGE: &str = "\
rmcl - Rust Minecraft launcher (milestone 1: CLI core)

USAGE:
  rmcl list [--type release|snapshot|old_beta|old_alpha]
  rmcl install <version>          (use 'latest' or 'latest-snapshot' as aliases)
  rmcl play <version> [--user NAME] [--ram MB] [--java PATH]
";

fn opt(args: &[String], key: &str) -> Option<String> {
    args.iter().position(|a| a == key).and_then(|i| args.get(i + 1).cloned())
}

async fn find_entry(
    client: &reqwest::Client,
    paths: &Paths,
    id: &str,
) -> Result<versions::ManifestEntry> {
    let m = versions::fetch_manifest(client, paths).await?;
    let id = match id {
        "latest" => m.latest.release.as_str(),
        "latest-snapshot" => m.latest.snapshot.as_str(),
        other => other,
    };
    m.versions
        .iter()
        .find(|v| v.id == id)
        .cloned()
        .with_context(|| format!("unknown version '{id}' (try `rmcl list`)"))
}

fn progress(done: usize, total: usize) {
    eprintln!("downloading: {done}/{total}");
}

// current_thread runtime: fewer threads, lower RAM. The launcher is I/O bound anyway.
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut paths = Paths::new()?;
    config::Config::load(&paths).apply_to(&mut paths);
    let client = download::client()?;

    match args.first().map(String::as_str) {
        Some("list") => {
            let kind = opt(&args, "--type");
            let m = versions::fetch_manifest(&client, &paths).await?;
            // Mojang's manifest is already ordered newest -> oldest.
            for v in m
                .versions
                .iter()
                .filter(|v| kind.as_deref().map_or(true, |k| v.kind == k))
            {
                println!("{:<24} {:<10} {}", v.id, v.kind, v.release_time);
            }
        }
        Some("install") => {
            let id = args.get(1).context("usage: rmcl install <version>")?;
            let entry = find_entry(&client, &paths, id).await?;
            install::install(&client, &paths, &entry, progress).await?;
            eprintln!("installed {}", entry.id);
        }
        Some("play") => {
            let id = args.get(1).context("usage: rmcl play <version>")?;
            let user = opt(&args, "--user").unwrap_or_else(|| "Player".into());
            accounts::validate_username(&user)?;
            let ram_mb: u32 = opt(&args, "--ram").and_then(|s| s.parse().ok()).unwrap_or(2048);
            let java_arg = opt(&args, "--java").map(PathBuf::from);

            let entry = find_entry(&client, &paths, id).await?;
            let meta = if paths.version_json(&entry.id).exists() {
                versions::load_local(&paths, &entry.id)?
            } else {
                install::install(&client, &paths, &entry, progress).await?
            };
            if let Some(j) = &meta.java_version {
                eprintln!("note: {} needs Java {}", meta.id, j.major_version);
            }
            let major = meta.java_version.as_ref().map_or(8, |j| j.major_version);
            let java = java_arg.unwrap_or_else(|| java::find_java(&paths, major, false).0);
            let opts = launch::LaunchOpts { username: user, ram_mb, java, window: config::WindowSize::Auto };
            let status = launch::build_command(&paths, &meta, &opts)?.status()?;
            eprintln!("game exited: {status}");
        }
        _ => print!("{USAGE}"),
    }
    Ok(())
}
