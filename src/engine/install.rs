use super::download::{self, Job};
use super::paths::Paths;
use super::versions::*;
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Resolved {
    pub jobs: Vec<Job>,
    pub classpath: Vec<PathBuf>,
    /// (native jar, extract-exclude prefixes)
    pub natives: Vec<(PathBuf, Vec<String>)>,
}

/// Work out which libraries apply to Windows. No network access here.
/// Note: libraries without `downloads` (Forge/Fabric style, Maven coordinates only)
/// are handled in the loaders milestone.
pub fn resolve_libraries(meta: &VersionMeta, paths: &Paths) -> Resolved {
    let mut r = Resolved { jobs: vec![], classpath: vec![], natives: vec![] };
    let arch = if cfg!(target_pointer_width = "64") { "64" } else { "32" };

    for lib in &meta.libraries {
        if !rules_allow(&lib.rules) || lib.clientreq == Some(false) {
            continue;
        }
        let Some(dl) = &lib.downloads else {
            // Maven-style entry (Fabric/Quilt): just a name and a repository URL.
            if let Some(path) = maven_path(&lib.name) {
                let mut base = lib
                    .url
                    .clone()
                    .unwrap_or_else(|| "https://libraries.minecraft.net/".to_string());
                if !base.ends_with('/') {
                    base.push('/');
                }
                let p = paths.libraries().join(&path);
                r.jobs.push(Job { url: format!("{base}{path}"), path: p.clone(), sha1: None });
                r.classpath.push(p);
            }
            continue;
        };

        if let Some(a) = &dl.artifact {
            let p = paths.libraries().join(&a.path);
            if !a.url.is_empty() {
                r.jobs.push(Job { url: a.url.clone(), path: p.clone(), sha1: a.sha1.clone() });
                r.classpath.push(p);
            } else if p.exists() {
                // No download link: a file the Forge installer generated itself.
                r.classpath.push(p);
            }
        }
        // Old-style natives (<= 1.18): separate classifier jar to be extracted.
        if let (Some(nmap), Some(cls)) = (&lib.natives, &dl.classifiers) {
            if let Some(key) = nmap.get("windows") {
                let key = key.replace("${arch}", arch);
                if let Some(a) = cls.get(&key) {
                    let p = paths.libraries().join(&a.path);
                    r.jobs.push(Job { url: a.url.clone(), path: p.clone(), sha1: a.sha1.clone() });
                    let excl = lib.extract.as_ref().and_then(|e| e.exclude.clone()).unwrap_or_default();
                    r.natives.push((p, excl));
                }
            }
        }
    }
    r
}

/// "group:artifact:version[:classifier][@ext]" -> "group/path/artifact/version/artifact-version.jar"
fn maven_path(name: &str) -> Option<String> {
    let (coords, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let mut parts = coords.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let file = match parts.next() {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!("{}/{artifact}/{version}/{file}", group.replace('.', "/")))
}

#[derive(Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObj>,
    #[serde(default)]
    pub r#virtual: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}
#[derive(Deserialize)]
pub struct AssetObj {
    pub hash: String,
}

pub fn asset_index(paths: &Paths, meta: &VersionMeta) -> Result<AssetIndex> {
    let p = paths.assets().join("indexes").join(format!("{}.json", meta.asset_index.id));
    Ok(serde_json::from_slice(&std::fs::read(p)?)?)
}

pub async fn install(
    client: &reqwest::Client,
    paths: &Paths,
    entry: &ManifestEntry,
    on_progress: impl Fn(usize, usize),
) -> Result<VersionMeta> {
    // 1. version JSON
    download::fetch_retry(
        client,
        &Job {
            url: entry.url.clone(),
            path: paths.version_json(&entry.id),
            sha1: entry.sha1.clone(),
        },
    )
    .await?;
    let meta = load_local(paths, &entry.id)?;

    // 2. client jar, libraries, log config
    let mut jobs = vec![Job {
        url: meta.downloads.client.url.clone(),
        path: paths.version_jar(&meta.id),
        sha1: meta.downloads.client.sha1.clone(),
    }];
    let res = resolve_libraries(&meta, paths);
    jobs.extend(res.jobs);
    if let Some(lc) = meta.logging.as_ref().and_then(|l| l.client.as_ref()) {
        jobs.push(Job {
            url: lc.file.url.clone(),
            path: paths.log_config(&lc.file.id),
            sha1: lc.file.sha1.clone(),
        });
    }

    // 3. asset index + objects
    let ai = &meta.asset_index;
    download::fetch_retry(
        client,
        &Job {
            url: ai.url.clone(),
            path: paths.assets().join("indexes").join(format!("{}.json", ai.id)),
            sha1: ai.sha1.clone(),
        },
    )
    .await?;
    let idx = asset_index(paths, &meta)?;
    for obj in idx.objects.values() {
        let pre = &obj.hash[..2];
        jobs.push(Job {
            url: format!("https://resources.download.minecraft.net/{pre}/{}", obj.hash),
            path: paths.assets().join("objects").join(pre).join(&obj.hash),
            sha1: Some(obj.hash.clone()),
        });
    }

    download::fetch_all(client, jobs, on_progress).await?;

    // 4. natives and legacy asset layouts
    extract_natives(&res.natives, &paths.natives(&meta.id))?;
    if idx.r#virtual || idx.map_to_resources {
        for (name, obj) in &idx.objects {
            let src = paths.assets().join("objects").join(&obj.hash[..2]).join(&obj.hash);
            let dest = if idx.map_to_resources {
                paths.instance(&meta.id).join("resources").join(name)
            } else {
                paths.assets().join("virtual").join("legacy").join(name)
            };
            if !dest.exists() {
                if let Some(p) = dest.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::fs::copy(&src, &dest)?;
            }
        }
    }
    // Marker written last, so a crashed/partial install is never shown as installed.
    std::fs::write(paths.version_dir(&meta.id).join(".installed"), b"1")?;
    Ok(meta)
}

fn extract_natives(list: &[(PathBuf, Vec<String>)], dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    for (jar, excl) in list {
        let mut z = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
        for i in 0..z.len() {
            let mut f = z.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let name = f.name().to_string();
            if name.starts_with("META-INF/") || excl.iter().any(|e| name.starts_with(e.as_str())) {
                continue;
            }
            let Some(rel) = f.enclosed_name() else { continue }; // blocks zip-slip
            let out = dir.join(rel);
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut o = std::fs::File::create(&out)?;
            std::io::copy(&mut f, &mut o)?;
        }
    }
    Ok(())
}
