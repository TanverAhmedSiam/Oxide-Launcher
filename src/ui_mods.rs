//! The Mods screen: Browse (search Modrinth/CurseForge, install with a progress bar) and
//! My mods (what is installed, update, remove).
use crate::theme::{self, ic};
use eframe::egui;
use rmcl::engine::{
    icons::{self, Icon},
    loaders::Loader,
    trace,
    mods::{self, DepStatus, FileChoice, InstalledItem, ModsApi, PlanDep, Project, ProjectKind, SourceFilter, Target},
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use tokio::runtime::Handle;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Browse,
    Mine,
}

enum MEvent {
    Results(u64, Result<Vec<Project>, String>),
    Files(u64, Result<Vec<FileChoice>, String>),
    Progress { file: String, done: u64, total: u64 },
    Installed(Result<Vec<String>, String>),
    Updates(Vec<(String, FileChoice)>),
    Icon(String, Result<Icon, String>),
    Plan(u64, Result<Vec<PlanDep>, String>),
}

enum PendingState {
    Resolving,
    Ready(Vec<PlanDep>),
    Failed(String),
}

/// An install waiting for the user to confirm the required mods.
struct Pending {
    project: Project,
    file: FileChoice,
    pk: ProjectKind,
    state: PendingState,
}

struct InstallProgress {
    project_id: String,
    filename: String, // the main file being installed (matches a card)
    current: String,  // file being downloaded right now (may be a dependency)
    done: u64,
    total: u64,
}

pub struct ModsState {
    pub show_logos: bool,
    last_tab: Option<Tab>,
    api: ModsApi,
    target: Option<Target>,
    game_dir: PathBuf,
    cache_dir: PathBuf,
    tab: Tab,
    // browse
    query: String,
    pk: ProjectKind,
    source: SourceFilter,
    results: Vec<Project>,
    searching: bool,
    search_err: String,
    project: Option<Project>,
    files: Vec<FileChoice>,
    files_loading: bool,
    files_err: String,
    // installed
    mine_pk: ProjectKind,
    installed: Vec<InstalledItem>,
    unmanaged: [Vec<String>; 3],
    updates: HashMap<String, FileChoice>,
    checking: bool,
    checked: bool,
    // shared
    install: Option<InstallProgress>,
    pending: Option<Pending>,
    plan_ticket: u64,
    queue: VecDeque<(Project, FileChoice, ProjectKind)>,
    status: String,
    icons: HashMap<String, Option<egui::TextureHandle>>,
    icon_pending: HashSet<String>,
    search_ticket: u64,
    files_ticket: u64,
    tx: Sender<MEvent>,
    rx: Receiver<MEvent>,
}

fn fmt_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

fn day(date: &str) -> &str {
    date.get(..10).unwrap_or(date)
}

fn draw_icon(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, size: f32) {
    let sz = egui::vec2(size, size);
    let rounding = egui::Rounding::same(size * 0.2);
    let tex = tex.filter(|t| {
        let s = t.size_vec2();
        s.x >= 1.0 && s.y >= 1.0
    });
    match tex {
        Some(t) => {
            ui.add(
                egui::Image::from_texture(egui::load::SizedTexture::from_handle(t))
                    .fit_to_exact_size(sz)
                    .rounding(rounding),
            );
        }
        None => {
            let p = theme::pal(ui);
            let (rect, _) = ui.allocate_exact_size(sz, egui::Sense::hover());
            ui.painter().rect_filled(rect, rounding, p.card_hi);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                ic::CUBE,
                egui::FontId::proportional(size * 0.5),
                p.muted,
            );
        }
    }
}

impl ModsState {
    /// Drops the loaded logos (they reload quickly from the disk cache). Called while a game runs.
    pub fn free_icons(&mut self) {
        self.icons.clear();
        self.icon_pending.clear();
    }

    /// The logo texture for a project, if logos are on and it has loaded.
    fn tex(&self, url: &Option<String>) -> Option<&egui::TextureHandle> {
        if !self.show_logos {
            return None;
        }
        url.as_ref().and_then(|u| self.icons.get(u)).and_then(|o| o.as_ref())
    }

    pub fn new(client: reqwest::Client) -> Self {
        let (tx, rx) = channel();
        Self {
            show_logos: true,
            last_tab: None,
            api: ModsApi::new(client, None),
            target: None,
            game_dir: PathBuf::new(),
            cache_dir: PathBuf::new(),
            tab: Tab::Browse,
            query: String::new(),
            pk: ProjectKind::Mod,
            source: SourceFilter::All,
            results: vec![],
            searching: false,
            search_err: String::new(),
            project: None,
            files: vec![],
            files_loading: false,
            files_err: String::new(),
            mine_pk: ProjectKind::Mod,
            installed: vec![],
            unmanaged: [Vec::new(), Vec::new(), Vec::new()],
            updates: HashMap::new(),
            checking: false,
            checked: false,
            install: None,
            pending: None,
            plan_ticket: 0,
            queue: VecDeque::new(),
            status: String::new(),
            icons: HashMap::new(),
            icon_pending: HashSet::new(),
            search_ticket: 0,
            files_ticket: 0,
            tx,
            rx,
        }
    }

    /// Called when the screen opens. Shows popular projects for the selected version right away.
    pub fn open(
        &mut self,
        target: Target,
        cf_key: Option<String>,
        game_dir: PathBuf,
        cache_dir: PathBuf,
        rt: &Handle,
        ctx: &egui::Context,
    ) {
        self.api.set_key(cf_key);
        self.target = Some(target);
        self.game_dir = game_dir;
        self.cache_dir = cache_dir;
        self.project = None;
        self.status.clear();
        self.updates.clear();
        self.checked = false;
        if self.source == SourceFilter::CurseForge && !self.api.has_curseforge() {
            self.source = SourceFilter::All;
        }
        self.refresh();
        self.search(rt, ctx);
    }

    fn refresh(&mut self) {
        self.installed = mods::load_installed(&self.game_dir);
        for (i, k) in ProjectKind::ALL.iter().enumerate() {
            self.unmanaged[i] = mods::list_unmanaged(&self.game_dir, *k, &self.installed);
        }
    }

    // ---------------- background work ----------------

    fn search(&mut self, rt: &Handle, ctx: &egui::Context) {
        let Some(t) = self.target.clone() else { return };
        self.search_ticket += 1;
        let ticket = self.search_ticket;
        self.searching = true;
        self.search_err.clear();
        self.results.clear();
        self.project = None;
        let (api, q, pk, src, tx, ctx) =
            (self.api.clone(), self.query.clone(), self.pk, self.source, self.tx.clone(), ctx.clone());
        rt.spawn(async move {
            let r = api.search(src, &q, pk, &t).await.map_err(|e| format!("{e:#}"));
            let _ = tx.send(MEvent::Results(ticket, r));
            ctx.request_repaint();
        });
    }

    fn select_project(&mut self, p: Project, rt: &Handle, ctx: &egui::Context) {
        let Some(t) = self.target.clone() else { return };
        self.files_ticket += 1;
        let ticket = self.files_ticket;
        self.files.clear();
        self.files_err.clear();
        self.files_loading = true;
        self.project = Some(p.clone());
        let (api, pk, tx, ctx) = (self.api.clone(), self.pk, self.tx.clone(), ctx.clone());
        rt.spawn(async move {
            let r = api.files(&p, pk, &t).await.map_err(|e| format!("{e:#}"));
            let _ = tx.send(MEvent::Files(ticket, r));
            ctx.request_repaint();
        });
    }

    fn idle(&self) -> bool {
        self.install.is_none() && self.pending.is_none()
    }

    /// First step of an install: look up the required mods and let the user confirm them.
    fn begin_install(&mut self, p: Project, f: FileChoice, pk: ProjectKind, rt: &Handle, ctx: &egui::Context) {
        if pk != ProjectKind::Mod {
            self.start_install(p, f, pk, Some(vec![]), rt, ctx); // packs and shaders have none
            return;
        }
        let Some(t) = self.target.clone() else { return };
        self.plan_ticket += 1;
        let ticket = self.plan_ticket;
        self.pending = Some(Pending {
            project: p.clone(),
            file: f.clone(),
            pk,
            state: PendingState::Resolving,
        });
        let (api, installed, gd, tx, ctx) = (
            self.api.clone(),
            self.installed.clone(),
            self.game_dir.clone(),
            self.tx.clone(),
            ctx.clone(),
        );
        rt.spawn(async move {
            let r = api.resolve_plan(&p, &f, pk, &t, &gd, &installed).await.map_err(|e| format!("{e:#}"));
            let _ = tx.send(MEvent::Plan(ticket, r));
            ctx.request_repaint();
        });
    }

    fn start_install(
        &mut self,
        p: Project,
        f: FileChoice,
        pk: ProjectKind,
        deps: Option<Vec<PlanDep>>,
        rt: &Handle,
        ctx: &egui::Context,
    ) {
        let Some(t) = self.target.clone() else { return };
        self.install = Some(InstallProgress {
            project_id: p.id.clone(),
            filename: f.filename.clone(),
            current: f.filename.clone(),
            done: 0,
            total: 0,
        });
        self.status.clear();
        let (api, tx, ctx, gd) = (self.api.clone(), self.tx.clone(), ctx.clone(), self.game_dir.clone());
        rt.spawn(async move {
            let (txp, ctxp) = (tx.clone(), ctx.clone());
            let cb = move |name: &str, done: u64, total: u64| {
                let _ = txp.send(MEvent::Progress { file: name.to_string(), done, total });
                ctxp.request_repaint();
            };
            let r = api.install(&p, &f, pk, &t, &gd, deps, &cb).await.map_err(|e| format!("{e:#}"));
            let _ = tx.send(MEvent::Installed(r));
            ctx.request_repaint();
        });
    }

    fn check_updates(&mut self, rt: &Handle, ctx: &egui::Context) {
        let Some(t) = self.target.clone() else { return };
        self.checking = true;
        self.updates.clear();
        let (api, items, tx, ctx) =
            (self.api.clone(), self.installed.clone(), self.tx.clone(), ctx.clone());
        rt.spawn(async move {
            let v = api.check_updates(&items, &t).await;
            let _ = tx.send(MEvent::Updates(v));
            ctx.request_repaint();
        });
    }

    fn want_icon(&mut self, url: &str, rt: &Handle, ctx: &egui::Context) {
        if !self.show_logos {
            return;
        }
        if self.icons.contains_key(url) || self.icon_pending.contains(url) || self.icon_pending.len() >= 8 {
            return; // at most 8 at a time; the rest are picked up on the next repaint
        }
        self.icon_pending.insert(url.to_string());
        let (c, dir, tx, ctx, u) =
            (self.api.client().clone(), self.cache_dir.clone(), self.tx.clone(), ctx.clone(), url.to_string());
        rt.spawn(async move {
            let r = icons::load_icon(&c, &dir, &u).await.map_err(|e| format!("{e:#}"));
            let _ = tx.send(MEvent::Icon(u, r));
            ctx.request_repaint();
        });
    }

    fn want_icons(&mut self, rt: &Handle, ctx: &egui::Context) {
        let mut urls: Vec<String> = match self.tab {
            Tab::Browse => match &self.project {
                Some(p) => p.icon_url.clone().into_iter().collect(),
                None => self.results.iter().filter_map(|p| p.icon_url.clone()).collect(),
            },
            Tab::Mine => self
                .installed
                .iter()
                .filter(|i| i.kind == self.mine_pk)
                .filter_map(|i| i.icon_url.clone())
                .collect(),
        };
        if let Some(Pending { state: PendingState::Ready(deps), .. }) = &self.pending {
            urls.extend(deps.iter().filter_map(|d| d.icon_url.clone()));
        }
        for u in urls {
            self.want_icon(&u, rt, ctx);
        }
    }

    fn drain(&mut self, ctx: &egui::Context, rt: &Handle) {
        while let Ok(e) = self.rx.try_recv() {
            match e {
                MEvent::Results(t, r) => {
                    if t != self.search_ticket {
                        continue; // stale answer from an older search
                    }
                    self.searching = false;
                    match r {
                        Ok(v) => self.results = v,
                        Err(e) => {
                            self.results.clear();
                            self.search_err = e;
                        }
                    }
                }
                MEvent::Files(t, r) => {
                    if t != self.files_ticket {
                        continue;
                    }
                    self.files_loading = false;
                    match r {
                        Ok(v) => self.files = v,
                        Err(e) => {
                            self.files.clear();
                            self.files_err = e;
                        }
                    }
                }
                MEvent::Progress { file, done, total } => {
                    if let Some(ip) = &mut self.install {
                        ip.current = file;
                        ip.done = done;
                        ip.total = total;
                    }
                }
                MEvent::Installed(r) => {
                    self.install = None;
                    match r {
                        Ok(names) => self.status = format!("Installed: {}", names.join(", ")),
                        Err(e) => {
                            self.status = format!("Install failed: {e}");
                            self.queue.clear();
                        }
                    }
                    self.refresh();
                    // Drop update offers that are now installed.
                    let installed = &self.installed;
                    self.updates
                        .retain(|pid, f| !installed.iter().any(|i| &i.project_id == pid && i.filename == f.filename));
                    if let Some((p, f, pk)) = self.queue.pop_front() {
                        self.start_install(p, f, pk, None, rt, ctx);
                    }
                }
                MEvent::Plan(t, r) => {
                    if t != self.plan_ticket {
                        continue; // cancelled or replaced
                    }
                    let Some(pd) = self.pending.take() else { continue };
                    match r {
                        Ok(deps) => {
                            // Nothing new to install: no need to ask.
                            if deps.iter().all(|d| d.status == DepStatus::Installed) {
                                self.start_install(pd.project, pd.file, pd.pk, Some(deps), rt, ctx);
                            } else {
                                self.pending = Some(Pending { state: PendingState::Ready(deps), ..pd });
                            }
                        }
                        Err(e) => self.pending = Some(Pending { state: PendingState::Failed(e), ..pd }),
                    }
                }
                MEvent::Updates(v) => {
                    self.checking = false;
                    self.checked = true;
                    self.updates = v.into_iter().collect();
                }
                MEvent::Icon(url, r) => {
                    self.icon_pending.remove(&url);
                    match r {
                        Ok(ic) => {
                            if self.icons.len() > 80 {
                                self.icons.clear(); // they reload quickly from the disk cache
                            }
                            let img = egui::ColorImage::from_rgba_unmultiplied([ic.w, ic.h], &ic.rgba);
                            trace::log(&format!("icon: making texture {}x{}", ic.w, ic.h));
                            let tex = ctx.load_texture(&url, img, egui::TextureOptions::LINEAR);
                            self.icons.insert(url, Some(tex));
                        }
                        Err(_) => {
                            self.icons.insert(url, None);
                        }
                    }
                }
            }
        }
    }

    // ---------------- screens ----------------

    /// Returns true when the user pressed Back.
    /// Landscape layout: header on top, search + results on the left, the selected project's
    /// files on the right (so the file list has the whole window height).
    pub fn ui(&mut self, ctx: &egui::Context, rt: &Handle) -> bool {
        self.drain(ctx, rt);
        self.want_icons(rt, ctx);
        if self.last_tab != Some(self.tab) {
            self.last_tab = Some(self.tab);
            trace::log(match self.tab {
                Tab::Browse => "tab: browse",
                Tab::Mine => "tab: my mods",
            });
        }
        let p = theme::palette(ctx.style().visuals.dark_mode);
        let mut back = false;
        let t = self.target.clone();

        egui::TopBottomPanel::top("mods_top")
            .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin::symmetric(18.0, 12.0)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("{}  Back", ic::ARROW_LEFT)).clicked() {
                        back = true;
                    }
                    ui.label(egui::RichText::new("Mods").size(22.0).strong());
                    if let Some(t) = &t {
                        theme::chip(ui, &format!("{}  {}", ic::CUBE, t.profile_id), p.accent);
                        if !t.installed {
                            theme::chip(ui, &format!("{}  Not installed yet. Go back and press Install first.", ic::WARNING), p.warn);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let n = self.installed.len();
                        let browse = format!("{}  Browse", ic::MAGNIFYING_GLASS);
                        let mine = format!("{}  My mods ({n})", ic::PUZZLE_PIECE);
                        ui.allocate_ui(egui::vec2(360.0, 34.0), |ui| {
                            theme::segmented(
                                ui,
                                &mut self.tab,
                                &[(Tab::Browse, browse.as_str()), (Tab::Mine, mine.as_str())],
                            );
                        });
                    });
                });
                self.ui_install_banner(ui);
                if !self.status.is_empty() {
                    let bad = ["failed", "Could not", "not found"].iter().any(|s| self.status.contains(s));
                    ui.label(
                        egui::RichText::new(&self.status)
                            .size(12.5)
                            .color(if bad { p.danger } else { p.muted }),
                    );
                }
            });

        if self.tab == Tab::Browse {
            let w = (ctx.screen_rect().width() * 0.42).clamp(380.0, 560.0);
            egui::SidePanel::left("mods_left")
                .exact_width(w)
                .resizable(false)
                .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin {
                    left: 18.0,
                    right: 8.0,
                    top: 4.0,
                    bottom: 14.0,
                }))
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);
                    if let Some(t) = &t {
                        self.ui_browse_left(ui, ctx, rt, t);
                    }
                });
        }

        let left = if self.tab == Tab::Browse { 8.0 } else { 18.0 };
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin {
                left,
                right: 18.0,
                top: 4.0,
                bottom: 14.0,
            }))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);
                match self.tab {
                    Tab::Browse => self.ui_detail(ui, ctx, rt),
                    Tab::Mine => self.ui_mine(ui, ctx, rt),
                }
            });
        self.ui_pending(ctx, rt);
        back
    }

    /// The "required mods" dialog shown before installing a mod that needs other mods.
    fn ui_pending(&mut self, ctx: &egui::Context, rt: &Handle) {
        enum Act {
            None,
            Confirm,
            Cancel,
        }
        let mut act = Act::None;
        let Some(pd) = &self.pending else { return };
        let p = theme::palette(ctx.style().visuals.dark_mode);

        egui::Window::new("Required mods")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(390.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);
                match &pd.state {
                    PendingState::Resolving => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!("Checking what {} needs…", pd.project.title));
                        });
                        if ui.button(format!("{}  Cancel", ic::X)).clicked() {
                            act = Act::Cancel;
                        }
                    }
                    PendingState::Failed(e) => {
                        ui.label("Couldn't check the required mods:");
                        ui.colored_label(p.danger, e);
                        ui.horizontal(|ui| {
                            if ui.add(theme::accent_btn(&p, "Install anyway")).clicked() {
                                act = Act::Confirm;
                            }
                            if ui.button(format!("{}  Cancel", ic::X)).clicked() {
                                act = Act::Cancel;
                            }
                        });
                    }
                    PendingState::Ready(deps) => {
                        theme::section(ui, ic::PUZZLE_PIECE, &format!("{} needs these mods to run", pd.project.title));
                        let to_install = deps.iter().filter(|d| d.status == DepStatus::ToInstall).count();
                        let problem = deps
                            .iter()
                            .any(|d| matches!(d.status, DepStatus::NotFound | DepStatus::Manual));
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            for d in deps {
                                ui.horizontal(|ui| {
                                    let tex = self.tex(&d.icon_url);
                                    draw_icon(ui, tex, 36.0);
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new(&d.title).strong());
                                        match d.status {
                                            DepStatus::Installed => {
                                                theme::chip(ui, &format!("{}  Already installed", ic::CHECK_CIRCLE), p.ok);
                                            }
                                            DepStatus::ToInstall => {
                                                let v = d
                                                    .file
                                                    .as_ref()
                                                    .map(|f| format!(" ({})", truncate(&f.label, 26)))
                                                    .unwrap_or_default();
                                                theme::chip(ui, &format!("{}  Will be installed{v}", ic::DOWNLOAD_SIMPLE), p.info);
                                            }
                                            DepStatus::Manual => {
                                                theme::chip(ui, &format!("{}  Download manually", ic::WARNING), p.warn);
                                                if !d.url.is_empty()
                                                    && ui
                                                        .button(format!("{}  Open on website", ic::ARROW_SQUARE_OUT))
                                                        .clicked()
                                                {
                                                    ctx.open_url(egui::OpenUrl::new_tab(&d.url));
                                                }
                                            }
                                            DepStatus::NotFound => {
                                                theme::chip(
                                                    ui,
                                                    &format!("{}  Not available for this version", ic::WARNING),
                                                    p.warn,
                                                );
                                            }
                                        }
                                    });
                                });
                            }
                        });
                        if problem {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} may not start without the mods marked with a warning.",
                                    pd.project.title
                                ))
                                .size(12.5)
                                .color(p.warn),
                            );
                        }
                        let label = if to_install > 0 {
                            format!(
                                "{}  Install {} + {} required mod{}",
                                ic::DOWNLOAD_SIMPLE,
                                truncate(&pd.project.title, 22),
                                to_install,
                                if to_install == 1 { "" } else { "s" }
                            )
                        } else {
                            "Install anyway".to_string()
                        };
                        if ui
                            .add(theme::accent_btn(&p, &label).min_size(egui::vec2(ui.available_width(), 36.0)))
                            .clicked()
                        {
                            act = Act::Confirm;
                        }
                        if ui.button(format!("{}  Cancel", ic::X)).clicked() {
                            act = Act::Cancel;
                        }
                    }
                }
            });

        match act {
            Act::None => {}
            Act::Cancel => {
                self.pending = None;
                self.plan_ticket += 1; // ignore a late answer
            }
            Act::Confirm => {
                if let Some(pd) = self.pending.take() {
                    let deps = match pd.state {
                        PendingState::Ready(d) => d,
                        _ => vec![],
                    };
                    self.start_install(pd.project, pd.file, pd.pk, Some(deps), rt, ctx);
                }
            }
        }
    }

    /// Shown on both tabs while something is downloading.
    fn ui_install_banner(&self, ui: &mut egui::Ui) {
        if let Some(ip) = &self.install {
            let frac = if ip.total > 0 { ip.done as f32 / ip.total as f32 } else { 0.0 };
            theme::progress(
                ui,
                frac,
                format!("{}  Downloading {}", ic::DOWNLOAD_SIMPLE, ip.current),
                ip.total == 0,
            );
        }
    }

    /// Left pane: warnings, search box, filters and the result list.
    fn ui_browse_left(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rt: &Handle, t: &Target) {
        let p = theme::pal(ui);
        if self.pk == ProjectKind::Mod && t.loader.is_none() {
            ui.label(
                egui::RichText::new(
                    "Mods need a mod loader (Fabric, Quilt, Forge or NeoForge). Go back and choose one in the Type dropdown.",
                )
                .size(12.5)
                .color(p.warn),
            );
        }
        if self.pk == ProjectKind::Mod && t.loader == Some(Loader::OptiFine) {
            ui.label(
                egui::RichText::new(
                    "OptiFine can't load mods. Use Fabric, Quilt, Forge or NeoForge for mods. Resource packs and shaders work here.",
                )
                .size(12.5)
                .color(p.warn),
            );
        }
        let mut do_search = false;
        ui.horizontal(|ui| {
            let tw = (ui.available_width() - 100.0).max(100.0);
            let te = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text(format!("{}  Search…", ic::MAGNIFYING_GLASS))
                    .desired_width(tw),
            );
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let btn = theme::accent_btn(&p, "Search").min_size(egui::vec2(0.0, 32.0));
            if ui.add(btn).clicked() || enter {
                do_search = true;
            }
        });
        let before = self.pk;
        let kinds: Vec<(ProjectKind, &str)> = ProjectKind::ALL.iter().map(|k| (*k, k.label())).collect();
        theme::segmented(ui, &mut self.pk, &kinds);
        if self.pk != before {
            do_search = true;
        }
        let has_cf = self.api.has_curseforge();
        ui.horizontal(|ui| {
            theme::muted(ui, "Source");
            let before = self.source;
            egui::ComboBox::from_id_salt("mods_source")
                .selected_text(self.source.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.source, SourceFilter::All, "All");
                    ui.selectable_value(&mut self.source, SourceFilter::Modrinth, "Modrinth");
                    ui.add_enabled_ui(has_cf, |ui| {
                        ui.selectable_value(&mut self.source, SourceFilter::CurseForge, "CurseForge");
                    });
                });
            if self.source != before {
                do_search = true;
            }
            if !has_cf {
                theme::muted(ui, "Add a CurseForge key in Settings to include it.");
            }
        });
        if do_search {
            self.search(rt, ctx);
        }
        self.ui_results(ui, rt, ctx);
    }

    fn ui_results(&mut self, ui: &mut egui::Ui, rt: &Handle, ctx: &egui::Context) {
        let p = theme::pal(ui);
        if self.searching {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Searching…");
            });
        } else if !self.search_err.is_empty() {
            ui.colored_label(p.danger, &self.search_err);
        } else if self.results.is_empty() {
            theme::muted(ui, "No results for this Minecraft version.");
        }
        let current = self.project.as_ref().map(|c| (c.source, c.id.clone()));
        let mut picked = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, pr) in self.results.iter().enumerate() {
                let selected = current.as_ref().map_or(false, |c| c.0 == pr.source && c.1 == pr.id);
                theme::card_with(ui, selected, |ui| {
                    ui.horizontal(|ui| {
                        let tex = self.tex(&pr.icon_url);
                        draw_icon(ui, tex, 52.0);
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new(&pr.title).size(15.5).strong());
                                let col = match pr.source {
                                    mods::Source::Modrinth => p.ok,
                                    mods::Source::CurseForge => p.warn,
                                };
                                theme::chip(ui, pr.source.name(), col);
                            });
                            theme::muted(ui, format!("by {}  ·  {} downloads", pr.author, fmt_count(pr.downloads)));
                            ui.label(truncate(&pr.description, 100));
                            if ui.button(format!("{}  View versions", ic::PACKAGE)).clicked() {
                                picked = Some(i);
                            }
                        });
                    });
                });
            }
        });
        if let Some(i) = picked {
            let pr = self.results[i].clone();
            self.select_project(pr, rt, ctx);
        }
    }

    /// Right pane: the selected project and its files for this Minecraft version.
    fn ui_detail(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rt: &Handle) {
        let pl = theme::pal(ui);
        let Some(pr) = self.project.clone() else {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.30);
                ui.label(egui::RichText::new(ic::PUZZLE_PIECE).size(44.0).color(pl.muted));
                ui.label(egui::RichText::new("Pick a project on the left").size(16.0).color(pl.muted));
                theme::muted(ui, "Its files for your Minecraft version will show up here.");
            });
            return;
        };
        let t = self.target.clone();
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                let tex = self.tex(&pr.icon_url);
                draw_icon(ui, tex, 60.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&pr.title).size(19.0).strong());
                    ui.horizontal_wrapped(|ui| {
                        let col = match pr.source {
                            mods::Source::Modrinth => pl.ok,
                            mods::Source::CurseForge => pl.warn,
                        };
                        theme::chip(ui, pr.source.name(), col);
                        if !pr.author.is_empty() {
                            theme::muted(ui, format!("by {}", pr.author));
                        }
                    });
                });
            });
            if !pr.description.is_empty() {
                ui.label(truncate(&pr.description, 240));
            }
        });
        if let Some(t) = &t {
            let loader = t.loader.map_or(String::new(), |l| format!("  ·  {}", l.name()));
            theme::section(ui, ic::PACKAGE, &format!("Files for Minecraft {}{loader}", t.mc));
        }

        if self.files_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading versions…");
            });
        } else if !self.files_err.is_empty() {
            ui.colored_label(pl.danger, &self.files_err);
        } else if self.files.is_empty() {
            theme::muted(ui, "No files for this Minecraft version and loader.");
        }

        let mut install_idx = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, f) in self.files.iter().enumerate() {
                theme::card(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&f.label).strong());
                        let col = match f.release.as_str() {
                            "release" => pl.ok,
                            "beta" => pl.warn,
                            _ => pl.danger,
                        };
                        theme::chip(ui, &f.release, col);
                    });
                    theme::muted(ui, format!("{}  ·  {}", day(&f.date), f.filename));

                    let same_project = |it: &&InstalledItem| {
                        it.source == pr.source && it.project_id == pr.id && it.kind == self.pk
                    };
                    let is_installed = self.installed.iter().filter(same_project).any(|it| it.filename == f.filename);
                    let other_version = self.installed.iter().filter(same_project).any(|it| it.filename != f.filename);
                    let this_install = self
                        .install
                        .as_ref()
                        .filter(|ip| ip.project_id == pr.id && ip.filename == f.filename);

                    if f.url.is_none() {
                        if ui.button(format!("{}  Open on website", ic::ARROW_SQUARE_OUT)).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(&pr.url));
                        }
                        theme::muted(ui, "The author only allows downloads from the website.");
                    } else if let Some(ip) = this_install {
                        let frac = if ip.total > 0 { ip.done as f32 / ip.total as f32 } else { 0.0 };
                        let text = if ip.total > 0 { format!("{:.0}%", frac * 100.0) } else { "Starting…".to_string() };
                        theme::progress(ui, frac, text, ip.total == 0);
                    } else if is_installed {
                        ui.label(
                            egui::RichText::new(format!("{}  Installed", ic::CHECK_CIRCLE))
                                .color(pl.ok)
                                .strong(),
                        );
                    } else {
                        let label = if other_version { "Switch to this version" } else { "Install" };
                        let btn = theme::accent_btn(&pl, &format!("{}  {label}", ic::DOWNLOAD_SIMPLE));
                        if ui.add_enabled(self.idle(), btn).clicked() {
                            install_idx = Some(i);
                        }
                    }
                });
            }
        });
        if let Some(i) = install_idx {
            let f = self.files[i].clone();
            self.begin_install(pr.clone(), f, self.pk, rt, ctx);
        }
    }

    fn ui_mine(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rt: &Handle) {
        let p = theme::pal(ui);
        let kinds: Vec<(ProjectKind, &str)> = ProjectKind::ALL.iter().map(|k| (*k, k.label())).collect();
        ui.allocate_ui(egui::vec2(440.0, 34.0), |ui| {
            theme::segmented(ui, &mut self.mine_pk, &kinds);
        });
        let busy = !self.idle() || self.checking;
        ui.horizontal(|ui| {
            let check = egui::Button::new(format!("{}  Check for updates", ic::ARROW_CLOCKWISE));
            if ui.add_enabled(!busy && !self.installed.is_empty(), check).clicked() {
                self.check_updates(rt, ctx);
            }
            let n = self.updates.len();
            if n > 0 {
                let all = theme::accent_btn(&p, &format!("{}  Update all ({n})", ic::ARROW_CIRCLE_UP));
                if ui.add_enabled(!busy, all).clicked() {
                    let mut all: Vec<(Project, FileChoice, ProjectKind)> = self
                        .installed
                        .iter()
                        .filter_map(|i| self.updates.get(&i.project_id).map(|f| (i.as_project(), f.clone(), i.kind)))
                        .collect();
                    if !all.is_empty() {
                        let (pr, f, k) = all.remove(0);
                        self.queue.extend(all);
                        self.start_install(pr, f, k, None, rt, ctx);
                    }
                }
            }
            if self.checking {
                ui.spinner();
                ui.label("Checking…");
            } else if self.checked && n == 0 {
                theme::chip(ui, &format!("{}  Everything is up to date", ic::CHECK_CIRCLE), p.ok);
            }
        });

        let items: Vec<InstalledItem> =
            self.installed.iter().filter(|i| i.kind == self.mine_pk).cloned().collect();
        let other = self.unmanaged[self.mine_pk.index()].clone();
        if items.is_empty() && other.is_empty() {
            theme::muted(ui, "Nothing here yet. Use Browse to add some.");
        }

        let mut update: Option<(InstalledItem, FileChoice)> = None;
        let mut remove: Option<InstalledItem> = None;
        let mut remove_other: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Two columns of cards: uses the wide window.
            ui.columns(2, |cols| {
                for (i, it) in items.iter().enumerate() {
                    let ui = &mut cols[i % 2];
                    theme::card(ui, |ui| {
                        ui.horizontal(|ui| {
                            let tex = self.tex(&it.icon_url);
                            draw_icon(ui, tex, 46.0);
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(egui::RichText::new(&it.title).size(15.0).strong());
                                    if it.dependency {
                                        theme::chip(ui, "Needed by another mod", p.muted);
                                    }
                                });
                                theme::muted(ui, format!("{}  ·  {}  ·  {}", it.label, day(&it.date), it.filename));
                                ui.horizontal(|ui| {
                                    if let Some(f) = self.updates.get(&it.project_id) {
                                        let txt =
                                            format!("{}  Update to {}", ic::ARROW_CIRCLE_UP, truncate(&f.label, 22));
                                        if ui.add_enabled(self.idle(), theme::accent_btn(&p, &txt)).clicked() {
                                            update = Some((it.clone(), f.clone()));
                                        }
                                    }
                                    let rm = theme::danger_btn(&p, &format!("{}  Remove", ic::TRASH));
                                    if ui.add_enabled(self.idle(), rm).clicked() {
                                        remove = Some(it.clone());
                                    }
                                });
                            });
                        });
                    });
                }
            });
            if !other.is_empty() {
                ui.add_space(6.0);
                theme::section(ui, ic::FOLDER_OPEN, "Other files (not installed by this launcher)");
                for name in &other {
                    theme::card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let rm = theme::danger_btn(&p, &format!("{}  Remove", ic::TRASH));
                                if ui.add(rm).clicked() {
                                    remove_other = Some(name.clone());
                                }
                            });
                        });
                    });
                }
            }
        });

        if let Some((it, f)) = update {
            self.begin_install(it.as_project(), f, it.kind, rt, ctx);
        }
        if let Some(it) = remove {
            self.status = match mods::remove_item(&self.game_dir, &it) {
                Ok(()) => format!("Removed {}", it.title),
                Err(e) => format!("Could not remove {}: {e:#}", it.title),
            };
            self.updates.remove(&it.project_id);
            self.refresh();
        }
        if let Some(name) = remove_other {
            self.status = match mods::remove_unmanaged(&self.game_dir, self.mine_pk, &name) {
                Ok(()) => format!("Removed {name}"),
                Err(e) => format!("Could not remove {name}: {e:#}"),
            };
            self.refresh();
        }
    }
}
