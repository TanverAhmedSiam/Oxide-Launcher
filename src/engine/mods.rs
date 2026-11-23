//! Modrinth + CurseForge clients: search, list compatible files, download into the right
//! folder, and keep a list of what was installed (so it can be updated or removed).
use super::download::{self, Job};
use super::loaders::Loader;
use anyhow::{bail, Context, Result};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

const MR: &str = "https://api.modrinth.com/v2";
const CF: &str = "https://api.curseforge.com/v1";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Source {
    Modrinth,
    CurseForge,
}
impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Modrinth => "Modrinth",
            Source::CurseForge => "CurseForge",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SourceFilter {
    All,
    Modrinth,
    CurseForge,
}
impl SourceFilter {
    pub fn label(self) -> &'static str {
        match self {
            SourceFilter::All => "All",
            SourceFilter::Modrinth => "Modrinth",
            SourceFilter::CurseForge => "CurseForge",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ProjectKind {
    Mod,
    ResourcePack,
    Shader,
}
impl ProjectKind {
    pub const ALL: [ProjectKind; 3] = [ProjectKind::Mod, ProjectKind::ResourcePack, ProjectKind::Shader];
    pub fn label(self) -> &'static str {
        match self {
            ProjectKind::Mod => "Mods",
            ProjectKind::ResourcePack => "Resource packs",
            ProjectKind::Shader => "Shaders",
        }
    }
    /// Folder inside the game directory.
    pub fn folder(self) -> &'static str {
        match self {
            ProjectKind::Mod => "mods",
            ProjectKind::ResourcePack => "resourcepacks",
            ProjectKind::Shader => "shaderpacks",
        }
    }
    pub fn index(self) -> usize {
        match self {
            ProjectKind::Mod => 0,
            ProjectKind::ResourcePack => 1,
            ProjectKind::Shader => 2,
        }
    }
    fn modrinth_type(self) -> &'static str {
        match self {
            ProjectKind::Mod => "mod",
            ProjectKind::ResourcePack => "resourcepack",
            ProjectKind::Shader => "shader",
        }
    }
    fn cf_class(self) -> u32 {
        match self {
            ProjectKind::Mod => 6,
            ProjectKind::ResourcePack => 12,
            ProjectKind::Shader => 6552,
        }
    }
}

/// What the user has selected in the launcher: files must match this.
#[derive(Clone)]
pub struct Target {
    pub mc: String,
    pub loader: Option<Loader>,
    pub profile_id: String,
    pub installed: bool,
}

#[derive(Clone)]
pub struct Project {
    pub source: Source,
    pub id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub downloads: u64,
    pub url: String,
    pub icon_url: Option<String>,
}

#[derive(Clone)]
pub struct Dep {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
}

/// How a required mod stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DepStatus {
    /// Already in the folder.
    Installed,
    /// Will be downloaded.
    ToInstall,
    /// Exists, but the author only allows downloads from their website.
    Manual,
    /// No file for this Minecraft version / loader.
    NotFound,
}

/// A mod that another mod needs, shown to the user before installing.
#[derive(Clone)]
pub struct PlanDep {
    pub project_id: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub url: String,
    pub file: Option<FileChoice>,
    pub status: DepStatus,
}

fn find_installed<'a>(installed: &'a [InstalledItem], src: Source, pid: &str) -> Option<&'a InstalledItem> {
    installed
        .iter()
        .find(|i| i.kind == ProjectKind::Mod && i.source == src && i.project_id == pid)
}

#[derive(Clone)]
pub struct FileChoice {
    pub project_id: Option<String>,
    pub label: String,
    pub release: String, // release | beta | alpha
    pub filename: String,
    pub url: Option<String>, // None = the author disallows third-party downloads (CurseForge)
    pub sha1: Option<String>,
    pub date: String, // full ISO timestamp (sorts and compares correctly)
    pub deps: Vec<Dep>,
}

/// One entry of the per-game-folder list of things the launcher installed.
#[derive(Clone, Serialize, Deserialize)]
pub struct InstalledItem {
    pub kind: ProjectKind,
    pub source: Source,
    pub project_id: String,
    pub title: String,
    pub filename: String,
    pub label: String,
    pub date: String,
    pub url: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    /// Installed automatically because another mod needs it.
    #[serde(default)]
    pub dependency: bool,
}

impl InstalledItem {
    pub fn as_project(&self) -> Project {
        Project {
            source: self.source,
            id: self.project_id.clone(),
            title: self.title.clone(),
            author: String::new(),
            description: String::new(),
            downloads: 0,
            url: self.url.clone(),
            icon_url: self.icon_url.clone(),
        }
    }
}

// ---------- list of installed items ----------

fn manifest_path(game_dir: &Path) -> PathBuf {
    game_dir.join(".rmcl").join("installed.json")
}

pub fn load_installed(game_dir: &Path) -> Vec<InstalledItem> {
    std::fs::read(manifest_path(game_dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_installed(game_dir: &Path, items: &[InstalledItem]) -> Result<()> {
    let p = manifest_path(game_dir);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_vec_pretty(items)?)?;
    Ok(())
}

/// Files in the folder that the launcher did not install (added by hand, or before this feature).
pub fn list_unmanaged(game_dir: &Path, kind: ProjectKind, items: &[InstalledItem]) -> Vec<String> {
    let tracked: HashSet<&str> =
        items.iter().filter(|i| i.kind == kind).map(|i| i.filename.as_str()).collect();
    let mut out: Vec<String> = std::fs::read_dir(game_dir.join(kind.folder()))
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| !n.starts_with('.') && !tracked.contains(n.as_str()))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

pub fn remove_item(game_dir: &Path, item: &InstalledItem) -> Result<()> {
    let name = Path::new(&item.filename).file_name().context("bad file name")?;
    match std::fs::remove_file(game_dir.join(item.kind.folder()).join(name)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut items = load_installed(game_dir);
    items.retain(|i| !(i.kind == item.kind && i.source == item.source && i.project_id == item.project_id));
    save_installed(game_dir, &items)
}

pub fn remove_unmanaged(game_dir: &Path, kind: ProjectKind, name: &str) -> Result<()> {
    let n = Path::new(name).file_name().context("bad file name")?;
    std::fs::remove_file(game_dir.join(kind.folder()).join(n))?;
    Ok(())
}

/// Replace an older entry of the same project (deleting its old file when the name changed).
fn upsert(items: &mut Vec<InstalledItem>, mut new: InstalledItem, folder: &Path) {
    if let Some(pos) = items
        .iter()
        .position(|i| i.kind == new.kind && i.source == new.source && i.project_id == new.project_id)
    {
        let old = items.remove(pos);
        new.dependency = new.dependency && old.dependency; // keep "installed by me" if it was
        if old.filename != new.filename {
            let _ = std::fs::remove_file(folder.join(&old.filename));
        }
    }
    items.push(new);
}

// ---------- Modrinth JSON ----------
#[derive(Deserialize)]
struct MrSearch {
    hits: Vec<MrHit>,
}
#[derive(Deserialize)]
struct MrHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    author: String,
    downloads: u64,
    project_type: String,
    icon_url: Option<String>,
}
#[derive(Deserialize)]
struct MrProject {
    slug: String,
    title: String,
    project_type: String,
    icon_url: Option<String>,
}
#[derive(Deserialize)]
struct MrVersion {
    #[serde(default)]
    project_id: Option<String>,
    name: String,
    version_number: String,
    version_type: String,
    date_published: String,
    files: Vec<MrFile>,
    #[serde(default)]
    dependencies: Vec<MrDep>,
}
#[derive(Deserialize)]
struct MrFile {
    url: String,
    filename: String,
    primary: bool,
    hashes: MrHashes,
}
#[derive(Deserialize)]
struct MrHashes {
    sha1: Option<String>,
}
#[derive(Deserialize)]
struct MrDep {
    project_id: Option<String>,
    version_id: Option<String>,
    dependency_type: String,
}

// ---------- CurseForge JSON ----------
#[derive(Deserialize)]
struct CfList<T> {
    data: Vec<T>,
}
#[derive(Deserialize)]
struct CfOne<T> {
    data: T,
}
#[derive(Deserialize)]
struct CfMod {
    id: u64,
    name: String,
    summary: String,
    #[serde(rename = "downloadCount", default)]
    download_count: f64,
    #[serde(default)]
    authors: Vec<CfAuthor>,
    links: Option<CfLinks>,
    logo: Option<CfLogo>,
}
#[derive(Deserialize)]
struct CfAuthor {
    name: String,
}
#[derive(Deserialize)]
struct CfLinks {
    #[serde(rename = "websiteUrl")]
    website_url: Option<String>,
}
#[derive(Deserialize)]
struct CfLogo {
    #[serde(rename = "thumbnailUrl")]
    thumbnail_url: Option<String>,
}
#[derive(Deserialize)]
struct CfFile {
    #[serde(rename = "modId", default)]
    mod_id: u64,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(rename = "releaseType")]
    release_type: u32,
    #[serde(default)]
    hashes: Vec<CfHash>,
    #[serde(default)]
    dependencies: Vec<CfDep>,
    #[serde(rename = "fileDate")]
    file_date: String,
}
#[derive(Deserialize)]
struct CfHash {
    value: String,
    algo: u32,
}
#[derive(Deserialize)]
struct CfDep {
    #[serde(rename = "modId")]
    mod_id: u64,
    #[serde(rename = "relationType")]
    relation_type: u32,
}

fn mr_to_choice(v: MrVersion) -> Option<FileChoice> {
    let f = v.files.iter().find(|f| f.primary).or(v.files.first())?;
    Some(FileChoice {
        project_id: v.project_id.clone(),
        label: if v.name.trim().is_empty() { v.version_number.clone() } else { v.name.clone() },
        release: v.version_type.clone(),
        filename: f.filename.clone(),
        url: Some(f.url.clone()),
        sha1: f.hashes.sha1.clone(),
        date: v.date_published.clone(),
        deps: v
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == "required")
            .map(|d| Dep { project_id: d.project_id.clone(), version_id: d.version_id.clone() })
            .collect(),
    })
}

/// Newest file, preferring a stable release over beta/alpha. Expects newest-first input.
fn best(v: Vec<FileChoice>) -> Option<FileChoice> {
    let pick = v.iter().position(|f| f.release == "release").or(if v.is_empty() { None } else { Some(0) })?;
    v.into_iter().nth(pick)
}

#[derive(Clone)]
pub struct ModsApi {
    client: reqwest::Client,
    cf_key: Option<String>,
}

impl ModsApi {
    pub fn new(client: reqwest::Client, cf_key: Option<String>) -> Self {
        Self { client, cf_key }
    }
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
    pub fn set_key(&mut self, key: Option<String>) {
        self.cf_key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    }
    pub fn has_curseforge(&self) -> bool {
        self.cf_key.is_some()
    }

    async fn get_text(&self, req: reqwest::RequestBuilder) -> Result<String> {
        Ok(req.send().await.and_then(|r| r.error_for_status())?.text().await?)
    }

    // ---------------- search ----------------

    pub async fn search(
        &self,
        src: SourceFilter,
        q: &str,
        pk: ProjectKind,
        t: &Target,
    ) -> Result<Vec<Project>> {
        let mut errs = Vec::new();
        let (mut mr, mut cf) = (Vec::new(), Vec::new());
        if matches!(src, SourceFilter::All | SourceFilter::Modrinth) {
            match self.search_modrinth(q, pk, t).await {
                Ok(v) => mr = v,
                Err(e) => errs.push(format!("Modrinth: {e:#}")),
            }
        }
        if matches!(src, SourceFilter::All | SourceFilter::CurseForge) && self.cf_key.is_some() {
            match self.search_curseforge(q, pk, t).await {
                Ok(v) => cf = v,
                Err(e) => errs.push(format!("CurseForge: {e:#}")),
            }
        }
        if mr.is_empty() && cf.is_empty() && !errs.is_empty() {
            bail!("{}", errs.join("; "));
        }
        // Interleave the two sources; drop same-named duplicates.
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let (mut a, mut b) = (mr.into_iter(), cf.into_iter());
        loop {
            let (x, y) = (a.next(), b.next());
            if x.is_none() && y.is_none() {
                break;
            }
            for p in x.into_iter().chain(y) {
                if seen.insert(p.title.to_lowercase()) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }

    async fn search_modrinth(&self, q: &str, pk: ProjectKind, t: &Target) -> Result<Vec<Project>> {
        // Outer list = AND, inner list = OR.
        let mut facets: Vec<Vec<String>> = vec![
            vec![format!("project_type:{}", pk.modrinth_type())],
            vec![format!("versions:{}", t.mc)],
        ];
        if pk == ProjectKind::Mod {
            match t.loader {
                Some(Loader::Fabric) => facets.push(vec!["categories:fabric".into()]),
                // Quilt can load most Fabric mods too.
                Some(Loader::Quilt) => facets.push(vec!["categories:quilt".into(), "categories:fabric".into()]),
                Some(Loader::Forge) => facets.push(vec!["categories:forge".into()]),
                Some(Loader::NeoForge) => facets.push(vec!["categories:neoforge".into()]),
                // OptiFine itself loads no mods.
                Some(Loader::OptiFine) | None => {}
            }
        }
        let facets = serde_json::to_string(&facets)?;
        let index = if q.trim().is_empty() { "downloads" } else { "relevance" };
        let req = self
            .client
            .get(format!("{MR}/search"))
            .query(&[("query", q), ("facets", facets.as_str()), ("limit", "30"), ("index", index)]);
        let r: MrSearch = serde_json::from_str(&self.get_text(req).await?)?;
        Ok(r
            .hits
            .into_iter()
            .map(|h| Project {
                source: Source::Modrinth,
                url: format!("https://modrinth.com/{}/{}", h.project_type, h.slug),
                id: h.project_id,
                title: h.title,
                author: h.author,
                description: h.description,
                downloads: h.downloads,
                icon_url: h.icon_url,
            })
            .collect())
    }

    async fn search_curseforge(&self, q: &str, pk: ProjectKind, t: &Target) -> Result<Vec<Project>> {
        let key = self.cf_key.as_ref().context("add your CurseForge API key in Settings")?;
        let mut params: Vec<(&str, String)> = vec![
            ("gameId", "432".into()),
            ("classId", pk.cf_class().to_string()),
            ("searchFilter", q.to_string()),
            ("gameVersion", t.mc.clone()),
            ("pageSize", "30".into()),
            ("sortField", "2".into()), // popularity
            ("sortOrder", "desc".into()),
        ];
        if pk == ProjectKind::Mod {
            if let Some(l) = t.loader {
                params.push(("modLoaderType", cf_loader(l).to_string()));
            }
        }
        let req = self.client.get(format!("{CF}/mods/search")).header("x-api-key", key).query(&params);
        let r: CfList<CfMod> = serde_json::from_str(&self.get_text(req).await?)?;
        Ok(r.data.into_iter().map(cf_to_project).collect())
    }

    // ---------------- files for the selected version ----------------

    pub async fn files(&self, p: &Project, pk: ProjectKind, t: &Target) -> Result<Vec<FileChoice>> {
        let mut v = match p.source {
            Source::Modrinth => self.files_modrinth(&p.id, pk, t).await?,
            Source::CurseForge => self.files_curseforge(&p.id, pk, t).await?,
        };
        v.sort_by(|a, b| b.date.cmp(&a.date)); // newest first
        Ok(v)
    }

    async fn files_modrinth(&self, id: &str, pk: ProjectKind, t: &Target) -> Result<Vec<FileChoice>> {
        let gv = serde_json::to_string(&[&t.mc])?;
        let mut req = self
            .client
            .get(format!("{MR}/project/{id}/version"))
            .query(&[("game_versions", gv.as_str())]);
        if pk == ProjectKind::Mod {
            let loaders = match t.loader {
                Some(Loader::Fabric) => Some(r#"["fabric"]"#),
                Some(Loader::Quilt) => Some(r#"["quilt","fabric"]"#),
                Some(Loader::Forge) => Some(r#"["forge"]"#),
                Some(Loader::NeoForge) => Some(r#"["neoforge"]"#),
                Some(Loader::OptiFine) | None => None,
            };
            if let Some(l) = loaders {
                req = req.query(&[("loaders", l)]);
            }
        }
        let vs: Vec<MrVersion> = serde_json::from_str(&self.get_text(req).await?)?;
        Ok(vs
            .into_iter()
            .filter_map(|v| {
                let mut c = mr_to_choice(v)?;
                c.project_id.get_or_insert_with(|| id.to_string());
                Some(c)
            })
            .collect())
    }

    async fn files_curseforge(&self, id: &str, pk: ProjectKind, t: &Target) -> Result<Vec<FileChoice>> {
        let key = self.cf_key.as_ref().context("add your CurseForge API key in Settings")?;
        let mut params: Vec<(&str, String)> =
            vec![("gameVersion", t.mc.clone()), ("pageSize", "30".into())];
        if pk == ProjectKind::Mod {
            if let Some(l) = t.loader {
                params.push(("modLoaderType", cf_loader(l).to_string()));
            }
        }
        let req = self.client.get(format!("{CF}/mods/{id}/files")).header("x-api-key", key).query(&params);
        let r: CfList<CfFile> = serde_json::from_str(&self.get_text(req).await?)?;
        Ok(r
            .data
            .into_iter()
            .map(|f| FileChoice {
                project_id: Some(if f.mod_id != 0 { f.mod_id.to_string() } else { id.to_string() }),
                label: f.display_name,
                release: match f.release_type {
                    1 => "release",
                    2 => "beta",
                    _ => "alpha",
                }
                .to_string(),
                filename: f.file_name,
                url: f.download_url, // null when the author blocks third-party downloads
                sha1: f.hashes.iter().find(|h| h.algo == 1).map(|h| h.value.clone()),
                date: f.file_date,
                deps: f
                    .dependencies
                    .iter()
                    .filter(|d| d.relation_type == 3) // required dependency
                    .map(|d| Dep { project_id: Some(d.mod_id.to_string()), version_id: None })
                    .collect(),
            })
            .collect())
    }

    // ---------------- install ----------------

    /// Works out which mods the file requires (recursively) and whether each is already
    /// installed. Only mods have dependencies; resource packs and shaders return nothing.
    pub async fn resolve_plan(
        &self,
        p: &Project,
        f: &FileChoice,
        pk: ProjectKind,
        t: &Target,
        game_dir: &Path,
        installed: &[InstalledItem],
    ) -> Result<Vec<PlanDep>> {
        let mut out = Vec::new();
        if pk != ProjectKind::Mod {
            return Ok(out);
        }
        let dest = game_dir.join(pk.folder());
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(p.id.clone());
        let mut queue: VecDeque<Dep> = f.deps.iter().cloned().collect();

        while let Some(d) = queue.pop_front() {
            if seen.len() > 25 {
                break;
            }
            let key = d.project_id.clone().or_else(|| d.version_id.clone()).unwrap_or_default();
            if key.is_empty() || !seen.insert(key.clone()) {
                continue;
            }
            if let Some(pid) = &d.project_id {
                if let Some(it) = find_installed(installed, p.source, pid) {
                    out.push(PlanDep {
                        project_id: pid.clone(),
                        title: it.title.clone(),
                        icon_url: it.icon_url.clone(),
                        url: it.url.clone(),
                        file: None,
                        status: DepStatus::Installed,
                    });
                    continue;
                }
            }
            let file = self.resolve_dep(p.source, &d, t).await.ok().flatten();
            let pid = file
                .as_ref()
                .and_then(|f| f.project_id.clone())
                .or_else(|| d.project_id.clone())
                .unwrap_or(key);
            let info = self.project_info(p.source, Some(&pid)).await;
            let fallback = file
                .as_ref()
                .map(|f| f.filename.trim_end_matches(".jar").to_string())
                .unwrap_or_else(|| pid.clone());
            let (title, icon_url, url) = info.unwrap_or((fallback, None, String::new()));
            let status = match &file {
                None => DepStatus::NotFound,
                Some(fc)
                    if find_installed(installed, p.source, &pid).is_some()
                        || Path::new(&fc.filename)
                            .file_name()
                            .map_or(false, |n| dest.join(n).exists()) =>
                {
                    DepStatus::Installed
                }
                Some(fc) => {
                    queue.extend(fc.deps.iter().cloned());
                    if fc.url.is_some() {
                        DepStatus::ToInstall
                    } else {
                        DepStatus::Manual
                    }
                }
            };
            out.push(PlanDep { project_id: pid, title, icon_url, url, file, status });
        }
        Ok(out)
    }

    async fn fetch_file(
        &self,
        file: &FileChoice,
        dest: &Path,
        on: &(dyn Fn(&str, u64, u64) + Sync),
    ) -> Result<String> {
        let url = file.url.as_ref().context("no download link")?;
        // Keep only the file name: never trust a path from a remote API.
        let name = Path::new(&file.filename)
            .file_name()
            .context("bad file name")?
            .to_string_lossy()
            .to_string();
        let job = Job { url: url.clone(), path: dest.join(&name), sha1: file.sha1.clone() };
        download::fetch_retry_cb(&self.client, &job, &|d, tot| on(&name, d, tot)).await?;
        Ok(name)
    }

    /// Installs the required mods first, then the file itself, and records everything in the
    /// installed list. `deps` is the plan the user confirmed; `None` works it out automatically.
    /// `on_progress(file name, bytes so far, total bytes or 0)`.
    pub async fn install(
        &self,
        p: &Project,
        f: &FileChoice,
        pk: ProjectKind,
        t: &Target,
        game_dir: &Path,
        deps: Option<Vec<PlanDep>>,
        on_progress: &(dyn Fn(&str, u64, u64) + Sync),
    ) -> Result<Vec<String>> {
        if f.url.is_none() {
            bail!(
                "The author only allows downloads from the CurseForge website. \
                 Use \"Open on website\" to download it manually."
            );
        }
        let deps = match deps {
            Some(d) => d,
            None => {
                let inst = load_installed(game_dir);
                self.resolve_plan(p, f, pk, t, game_dir, &inst).await?
            }
        };
        let dest = game_dir.join(pk.folder());
        let mut items = load_installed(game_dir);
        let mut names = Vec::new();
        let mut notes = Vec::new();

        for d in &deps {
            match (d.status, d.file.as_ref()) {
                (DepStatus::ToInstall, Some(fc)) => {
                    let name = self.fetch_file(fc, &dest, on_progress).await?;
                    upsert(
                        &mut items,
                        InstalledItem {
                            kind: pk,
                            source: p.source,
                            project_id: d.project_id.clone(),
                            title: d.title.clone(),
                            filename: name.clone(),
                            label: fc.label.clone(),
                            date: fc.date.clone(),
                            url: d.url.clone(),
                            icon_url: d.icon_url.clone(),
                            dependency: true,
                        },
                        &dest,
                    );
                    save_installed(game_dir, &items)?;
                    names.push(name);
                }
                (DepStatus::Manual, _) => notes.push(format!("{} needs a manual download", d.title)),
                (DepStatus::NotFound, _) => {
                    notes.push(format!("{} was not found for this version", d.title))
                }
                _ => {}
            }
        }

        let name = self.fetch_file(f, &dest, on_progress).await?;
        upsert(
            &mut items,
            InstalledItem {
                kind: pk,
                source: p.source,
                project_id: p.id.clone(),
                title: p.title.clone(),
                filename: name.clone(),
                label: f.label.clone(),
                date: f.date.clone(),
                url: p.url.clone(),
                icon_url: p.icon_url.clone(),
                dependency: false,
            },
            &dest,
        );
        save_installed(game_dir, &items)?;
        names.push(name);
        names.extend(notes);
        Ok(names)
    }

    async fn resolve_dep(&self, src: Source, d: &Dep, t: &Target) -> Result<Option<FileChoice>> {
        match src {
            Source::Modrinth => {
                if let Some(vid) = &d.version_id {
                    let req = self.client.get(format!("{MR}/version/{vid}"));
                    let v: MrVersion = serde_json::from_str(&self.get_text(req).await?)?;
                    return Ok(mr_to_choice(v));
                }
                if let Some(pid) = &d.project_id {
                    let mut v = self.files_modrinth(pid, ProjectKind::Mod, t).await?;
                    v.sort_by(|a, b| b.date.cmp(&a.date));
                    return Ok(best(v));
                }
                Ok(None)
            }
            Source::CurseForge => match &d.project_id {
                Some(pid) => {
                    let mut v = self.files_curseforge(pid, ProjectKind::Mod, t).await?;
                    v.sort_by(|a, b| b.date.cmp(&a.date));
                    Ok(best(v))
                }
                None => Ok(None),
            },
        }
    }

    /// (title, icon url, page url) for a dependency, best effort.
    async fn project_info(&self, src: Source, id: Option<&str>) -> Option<(String, Option<String>, String)> {
        let id = id?;
        match src {
            Source::Modrinth => {
                let text = self.get_text(self.client.get(format!("{MR}/project/{id}"))).await.ok()?;
                let v: MrProject = serde_json::from_str(&text).ok()?;
                Some((v.title, v.icon_url, format!("https://modrinth.com/{}/{}", v.project_type, v.slug)))
            }
            Source::CurseForge => {
                let key = self.cf_key.as_ref()?;
                let req = self.client.get(format!("{CF}/mods/{id}")).header("x-api-key", key);
                let r: CfOne<CfMod> = serde_json::from_str(&self.get_text(req).await.ok()?).ok()?;
                let p = cf_to_project(r.data);
                Some((p.title, p.icon_url, p.url))
            }
        }
    }

    // ---------------- updates ----------------

    /// For each installed item, the newer file for this Minecraft version + loader (if any),
    /// keyed by project id.
    pub async fn check_updates(&self, items: &[InstalledItem], t: &Target) -> Vec<(String, FileChoice)> {
        // Collect first: a borrowing iterator adapter held across `.await` confuses the
        // compiler's lifetime inference for Send futures.
        let todo: Vec<InstalledItem> =
            items.iter().filter(|i| !i.project_id.is_empty()).cloned().collect();
        stream::iter(todo)
            .map(|it| async move {
                let files = self.files(&it.as_project(), it.kind, t).await.ok()?;
                let b = best(files)?;
                if b.date > it.date && b.filename != it.filename {
                    Some((it.project_id.clone(), b))
                } else {
                    None
                }
            })
            .buffer_unordered(4)
            .filter_map(|x| async move { x })
            .collect()
            .await
    }
}

fn cf_to_project(m: CfMod) -> Project {
    Project {
        source: Source::CurseForge,
        id: m.id.to_string(),
        title: m.name,
        author: m.authors.first().map(|a| a.name.clone()).unwrap_or_default(),
        description: m.summary,
        downloads: m.download_count as u64,
        url: m
            .links
            .and_then(|l| l.website_url)
            .unwrap_or_else(|| "https://www.curseforge.com/minecraft".to_string()),
        icon_url: m.logo.and_then(|l| l.thumbnail_url),
    }
}

/// CurseForge loader ids: Forge 1, Fabric 4, Quilt 5, NeoForge 6.
/// Quilt uses the Fabric listing because it can load Fabric mods.
fn cf_loader(l: Loader) -> u32 {
    match l {
        Loader::Fabric | Loader::Quilt => 4,
        Loader::Forge => 1,
        Loader::NeoForge => 6,
        Loader::OptiFine => 0, // 0 = any loader
    }
}
