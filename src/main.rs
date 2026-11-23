#![cfg_attr(windows, windows_subsystem = "windows")]

mod theme;
mod ui_mods;

use eframe::egui;
use theme::ic;
use rmcl::engine::{
    accounts,
    config::{Account, AccountKind, Config, Gpu, JavaChoice, Theme, WindowSize},
    download, gpu, install, installed, java, launch,
    optifine, trace,
    loaders::{self, Loader},
    mods::Target,
    paths::Paths,
    versions::{self, Manifest},
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Release,
    Snapshot,
    Beta,
    Alpha,
}
impl Kind {
    fn from_api(s: &str) -> Kind {
        match s {
            "snapshot" => Kind::Snapshot,
            "old_beta" => Kind::Beta,
            "old_alpha" => Kind::Alpha,
            _ => Kind::Release,
        }
    }
    fn api(self) -> &'static str {
        match self {
            Kind::Release => "release",
            Kind::Snapshot => "snapshot",
            Kind::Beta => "old_beta",
            Kind::Alpha => "old_alpha",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Kind::Release => "Release",
            Kind::Snapshot => "Snapshot",
            Kind::Beta => "Beta",
            Kind::Alpha => "Alpha",
        }
    }
}

enum Msg {
    Manifest(Result<Manifest, String>),
    Progress(usize, usize),
    Installed(Result<(), String>),
    Exited(Result<(), String>),
    Loaders(String, Vec<Loader>),
    Status(String),
}

#[derive(PartialEq)]
enum Busy {
    Idle,
    Installing,
    Running,
}
#[derive(PartialEq)]
enum View {
    Main,
    Settings,
    Mods,
}
#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    Minecraft,
    Launcher,
}

/// Shown when installing OptiFine: it must be downloaded by the user.
struct OptifineDialog {
    mc: String,
    found: Option<PathBuf>,
    error: String,
}

struct App {
    logos: Option<theme::Logos>,
    optifine: Option<OptifineDialog>,
    optifine_jar: Option<PathBuf>,
    rt: tokio::runtime::Runtime,
    client: reqwest::Client,
    paths: Paths,
    cfg: Config,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    manifest: Option<Manifest>,
    kind: Kind,
    selected: Option<String>,
    loader: Option<Loader>, // None = vanilla
    loader_cache: HashMap<String, Vec<Loader>>,
    loader_pending: HashSet<String>,
    last_selected: Option<String>,
    busy: Busy,
    progress: (usize, usize),
    status: String,
    show_add: bool,
    new_name: String,
    add_err: String,
    // settings
    view: View,
    tab: SettingsTab,
    draft: Config,
    java_detected: Vec<(String, PathBuf)>,
    total_ram_mb: u32,
    mods: ui_mods::ModsState,
    /// What is installed, read from disk (works offline).
    inst: installed::InstalledScan,
    /// The user pressed Play with no account: create one, then start the game.
    play_after_account: bool,
}

fn apply_style(ctx: &egui::Context, cfg: &Config) {
    theme::apply(ctx, cfg.theme == Theme::Dark);
    ctx.set_zoom_factor(cfg.ui_scale.clamp(0.85, 1.25));
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        // One worker thread is plenty for downloads and keeps memory low.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_stack_size(8 * 1024 * 1024)
            .enable_all()
            .build()
            .expect("runtime");
        let mut paths = Paths::new().expect("data directory");
        let cfg = Config::load(&paths);
        cfg.apply_to(&mut paths);
        theme::install_fonts(ctx);
        apply_style(ctx, &cfg);

        let total_ram_mb = {
            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            (sys.total_memory() / 1024 / 1024) as u32
        };
        let client = download::client().expect("http client");

        // Everything below comes from disk, so the launcher is usable at once, even offline.
        let inst = installed::scan(&paths);
        let manifest = versions::load_cached_manifest(&paths);
        let mut kind = Kind::from_api(&cfg.last_kind);
        let mut selected = if cfg.last_version.is_empty() { None } else { Some(cfg.last_version.clone()) };
        let loader = Loader::ALL.iter().copied().find(|l| l.slug() == cfg.last_loader);
        if selected.is_none() {
            // First start, or nothing played yet: newest installed version, else the latest release.
            if let Some(v) = inst.versions.first() {
                kind = Kind::from_api(&v.kind);
                selected = Some(v.id.clone());
            } else if let Some(m) = &manifest {
                kind = Kind::Release;
                selected = Some(m.latest.release.clone());
            }
        }
        let status = if manifest.is_none() && inst.versions.is_empty() {
            "Loading versions…".to_string()
        } else {
            String::new()
        };
        let (tx, rx) = channel();
        let app = Self {
            logos: theme::load_logos(ctx),
            optifine: None,
            optifine_jar: None,
            rt,
            client: client.clone(),
            mods: ui_mods::ModsState::new(client),
            draft: cfg.clone(),
            cfg,
            paths,
            tx,
            rx,
            manifest,
            kind,
            selected: selected.clone(),
            loader,
            inst,
            play_after_account: false,
            loader_cache: HashMap::new(),
            loader_pending: HashSet::new(),
            last_selected: selected, // so the remembered loader is not reset at start
            busy: Busy::Idle,
            progress: (0, 0),
            status,
            show_add: false,
            new_name: String::new(),
            add_err: String::new(),
            view: View::Main,
            tab: SettingsTab::Minecraft,
            java_detected: vec![],
            total_ram_mb,
        };
        app.load_manifest(ctx);
        app
    }

    fn load_manifest(&self, ctx: &egui::Context) {
        let (c, p, tx, ctx) = (self.client.clone(), self.paths.clone(), self.tx.clone(), ctx.clone());
        self.rt.spawn(async move {
            let r = versions::fetch_manifest(&c, &p).await.map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Manifest(r));
            ctx.request_repaint();
        });
    }

    /// Version list for the chosen type: installed versions first (newest first, read from disk,
    /// so they show even offline), then the rest of the official list. Returns (rows, installed count).
    fn version_rows(&self) -> (Vec<(String, bool)>, usize) {
        let api = self.kind.api();
        let mut rows: Vec<(String, bool)> = self
            .inst
            .versions
            .iter()
            .filter(|v| v.kind == api)
            .map(|v| (v.id.clone(), true))
            .collect();
        let n = rows.len();
        if let Some(m) = &self.manifest {
            for v in m.versions.iter().filter(|v| v.kind == api) {
                if !self.inst.ids.contains(&v.id) {
                    rows.push((v.id.clone(), false));
                }
            }
        }
        (rows, n)
    }

    fn start_install(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected.clone() else { return };
        let Some(entry) = self
            .manifest
            .as_ref()
            .and_then(|m| m.versions.iter().find(|v| v.id == id))
            .cloned()
        else {
            self.status = "Can't download right now: no internet connection, or the version list isn't loaded yet.".into();
            return;
        };
        self.busy = Busy::Installing;
        self.progress = (0, 0);
        self.status = if let Some(l) = self.loader.filter(|l| l.uses_installer()) {
            format!("Installing {id} with {}. This can take a few minutes…", l.name())
        } else if self.loader == Some(Loader::OptiFine) {
            format!(
                "Installing {id} with OptiFine. When the OptiFine installer window opens, click Install in it (keep its folder as it is), then close it."
            )
        } else {
            format!("Installing {id}…")
        };
        let (c, p, tx, ctx) = (self.client.clone(), self.paths.clone(), self.tx.clone(), ctx.clone());
        let loader = self.loader;
        let optifine_jar = self.optifine_jar.take();
        self.rt.spawn(async move {
            let (txp, ctxp) = (tx.clone(), ctx.clone());
            let progress = move |d: usize, t: usize| {
                let _ = txp.send(Msg::Progress(d, t));
                ctxp.request_repaint();
            };
            let r = match loader {
                None => install::install(&c, &p, &entry, progress).await.map(|_| ()),
                Some(Loader::OptiFine) => match optifine_jar {
                    Some(j) => optifine::install(&c, &p, &entry, &j, progress).await,
                    None => Err(anyhow::anyhow!("no OptiFine file was selected")),
                },
                Some(l) => loaders::install_loader(&c, &p, &entry, l, progress).await,
            }
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Installed(r));
            ctx.request_repaint();
        });
    }

    fn start_play(&mut self, ctx: &egui::Context) {
        let Some(id) = self.target_id() else { return };
        let Some(acc) = self.cfg.accounts.get(self.cfg.selected_account).cloned() else { return };
        if acc.kind != AccountKind::Offline {
            self.status = "Microsoft accounts are not supported yet.".into();
            return;
        }
        // Remember this choice: the launcher starts here next time.
        if let Some(sel) = self.selected.clone() {
            self.cfg.last_version = sel;
            self.cfg.last_loader = self.loader.map_or(String::new(), |l| l.slug().to_string());
            self.cfg.last_kind = self.kind.api().to_string();
            self.save_cfg();
        }
        self.busy = Busy::Running;
        self.status = format!("Playing {id}");
        self.mods.free_icons();
        if self.cfg.minimize_on_launch {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        let (paths, tx, ctx, cfg) = (self.paths.clone(), self.tx.clone(), ctx.clone(), self.cfg.clone());
        let (handle, client) = (self.rt.handle().clone(), self.client.clone());
        std::thread::spawn(move || {
            let run = || -> anyhow::Result<()> {
                let meta = versions::load_local(&paths, &id)?;
                let major = meta.java_version.as_ref().map_or(8, |j| j.major_version);
                // Automatic: the bundled Java for this version, or a one-time download of it.
                let (txs, ctxs) = (tx.clone(), ctx.clone());
                let say = move |s: &str| {
                    let _ = txs.send(Msg::Status(s.to_string()));
                    ctxs.request_repaint();
                };
                let java = handle.block_on(java::resolve_or_download(&client, &paths, &cfg.java, major, true, &say))?;
                let _ = tx.send(Msg::Status(format!("Playing {id}")));
                let _ = gpu::apply(&java, cfg.gpu);
                let opts = launch::LaunchOpts {
                    username: acc.username.clone(),
                    ram_mb: cfg.ram_mb,
                    java,
                    window: cfg.window.clone(),
                };
                let mut cmd = launch::build_command(&paths, &meta, &opts)?;

                // A GUI app has no console, so send game output to a log file.
                let log_dir = paths.root.join("logs");
                std::fs::create_dir_all(&log_dir)?;
                let log = std::fs::File::create(log_dir.join("latest.log"))?;
                cmd.stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                }
                let status = cmd.status()?;
                if status.success() {
                    Ok(())
                } else {
                    anyhow::bail!("game exited with {status}. See logs\\latest.log")
                }
            };
            let _ = tx.send(Msg::Exited(run().map_err(|e| format!("{e:#}"))));
            ctx.request_repaint();
        });
    }

    /// What will be installed/launched: the version, or version + loader profile.
    fn target_id(&self) -> Option<String> {
        self.selected.as_ref().map(|id| loaders::profile_id(id, self.loader))
    }

    /// Resets the loader when the version changes and looks up which loaders support it.
    fn ensure_loaders(&mut self, ctx: &egui::Context) {
        if self.selected != self.last_selected {
            self.last_selected = self.selected.clone();
            self.loader = None;
        }
        let Some(id) = self.selected.clone() else { return };
        if self.loader_cache.contains_key(&id) || self.loader_pending.contains(&id) {
            return;
        }
        if !matches!(self.kind, Kind::Release | Kind::Snapshot) {
            self.loader_cache.insert(id, vec![]); // no loaders for old alpha/beta
            return;
        }
        self.loader_pending.insert(id.clone());
        let (c, tx, ctx) = (self.client.clone(), self.tx.clone(), ctx.clone());
        self.rt.spawn(async move {
            let list = loaders::available(&c, &id).await;
            let _ = tx.send(Msg::Loaders(id, list));
            ctx.request_repaint();
        });
    }

    fn open_mods(&mut self, ctx: &egui::Context) {
        let Some(mc) = self.selected.clone() else { return };
        let profile_id = loaders::profile_id(&mc, self.loader);
        let target = Target {
            installed: self.paths.is_installed(&profile_id),
            loader: self.loader,
            profile_id,
            mc,
        };
        let key = self.cfg.curseforge_api_key.trim();
        let key = if key.is_empty() { None } else { Some(key.to_string()) };
        trace::log("open mods screen");
        self.mods.show_logos = self.cfg.show_logos;
        let game_dir = self.paths.instance(&target.profile_id);
        let cache_dir = self.paths.root.join("cache").join("icons");
        self.mods.open(target, key, game_dir, cache_dir, self.rt.handle(), ctx);
        self.view = View::Mods;
    }

    fn open_optifine_dialog(&mut self) {
        let Some(mc) = self.selected.clone() else { return };
        self.optifine = Some(OptifineDialog {
            found: optifine::find_in_downloads(&mc),
            mc,
            error: String::new(),
        });
    }

    /// Walks the user through downloading OptiFine, then installs from the file they pick.
    fn ui_optifine(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.optifine else { return };
        let mc = d.mc.clone();
        let mut chosen: Option<PathBuf> = None;
        let mut cancel = false;
        egui::Window::new("OptiFine")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing = egui::vec2(8.0, 8.0);
                ui.label("OptiFine can't be downloaded automatically: its website asks you to download the file yourself.");
                ui.label(egui::RichText::new(format!(
                    "1. Open the OptiFine downloads page and download the file for Minecraft {mc} (named OptiFine_{mc}_HD_U_….jar). On the ad page, wait a few seconds and click Skip."
                )));
                if ui.button("Open OptiFine downloads").clicked() {
                    ctx.open_url(egui::OpenUrl::new_tab(optifine::DOWNLOADS_URL));
                }
                ui.label("2. Choose the file you downloaded:");
                if let Some(f) = &d.found {
                    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Found in Downloads: {name}"));
                        if ui.button("Use this file").clicked() {
                            chosen = Some(f.clone());
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Choose file…").clicked() {
                        let mut dlg = rfd::FileDialog::new().add_filter("OptiFine", &["jar"]);
                        if let Some(dir) = directories::UserDirs::new().and_then(|u| u.download_dir().map(|p| p.to_path_buf())) {
                            dlg = dlg.set_directory(dir);
                        }
                        if let Some(p) = dlg.pick_file() {
                            chosen = Some(p);
                        }
                    }
                    if ui.button("Look again in Downloads").clicked() {
                        d.found = optifine::find_in_downloads(&mc);
                    }
                });
                if !d.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &d.error);
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        if cancel {
            self.optifine = None;
        } else if let Some(p) = chosen {
            match optifine::check_jar(&p, &mc) {
                Ok(()) => {
                    self.optifine = None;
                    self.optifine_jar = Some(p);
                    self.start_install(ctx);
                }
                Err(e) => {
                    if let Some(d) = &mut self.optifine {
                        d.error = e.to_string();
                    }
                }
            }
        }
    }

    fn save_cfg(&self) {
        let _ = self.cfg.save(&self.paths);
    }

    // ---------------- settings ----------------

    fn open_settings(&mut self) {
        self.draft = self.cfg.clone();
        self.java_detected = java::detect_installed(&self.paths);
        self.view = View::Settings;
    }

    fn commit_settings(&mut self, ctx: &egui::Context) {
        self.draft.game_dir = self.draft.game_dir.trim().to_string();
        self.cfg = self.draft.clone();
        self.cfg.apply_to(&mut self.paths);
        self.save_cfg();
        apply_style(ctx, &self.cfg);
    }

    /// Resets the tab you are looking at (still needs Save).
    fn restore_defaults(&mut self) {
        let d = Config::default();
        match self.tab {
            SettingsTab::Minecraft => {
                self.draft.game_dir = d.game_dir;
                self.draft.window = d.window;
                self.draft.java = d.java;
                self.draft.ram_mb = d.ram_mb;
                self.draft.gpu = d.gpu;
            }
            SettingsTab::Launcher => {
                self.draft.theme = d.theme;
                self.draft.ui_scale = d.ui_scale;
                self.draft.minimize_on_launch = d.minimize_on_launch;
            }
        }
    }

    fn ui_settings(&mut self, ctx: &egui::Context) {
        let p = theme::palette(ctx.style().visuals.dark_mode);
        egui::TopBottomPanel::bottom("settings_buttons")
            .frame(
                egui::Frame::none()
                    .fill(p.card)
                    .stroke(egui::Stroke::new(1.0_f32, p.stroke))
                    .inner_margin(egui::Margin::symmetric(18.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let save = theme::accent_btn(&p, &format!("{}  Save", ic::CHECK)).min_size(egui::vec2(96.0, 34.0));
                    if ui.add(save).clicked() {
                        self.commit_settings(ctx);
                        self.view = View::Main;
                    }
                    let restore = egui::Button::new(format!("{}  Restore defaults", ic::ARROW_CLOCKWISE))
                        .min_size(egui::vec2(0.0, 34.0));
                    if ui.add(restore).clicked() {
                        self.restore_defaults();
                    }
                    let cancel = egui::Button::new(format!("{}  Cancel", ic::X)).min_size(egui::vec2(0.0, 34.0));
                    if ui.add(cancel).clicked() {
                        self.view = View::Main;
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin::symmetric(18.0, 14.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(ic::GEAR).size(24.0).color(p.accent));
                    ui.label(egui::RichText::new("Settings").size(22.0).strong());
                });
                let items = [(SettingsTab::Minecraft, "Minecraft"), (SettingsTab::Launcher, "Launcher")];
                theme::segmented(ui, &mut self.tab, &items);
                ui.add_space(2.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| match self.tab {
                    SettingsTab::Minecraft => self.tab_minecraft(ui),
                    SettingsTab::Launcher => self.tab_launcher(ui),
                });
            });
    }

    fn tab_minecraft(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui);
        ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
        ui.columns(2, |cols| {
            // ---- left column: where and how the game window looks ----
            {
                let ui = &mut cols[0];

                theme::card(ui, |ui| {
                    theme::section(ui, ic::FOLDER_OPEN, "Game folder");
                    let default_dir = self.paths.root.join("instances").display().to_string();
                    if path_row(ui, &mut self.draft.game_dir, &default_dir) {
                        if let Some(f) = rfd::FileDialog::new().pick_folder() {
                            self.draft.game_dir = f.display().to_string();
                        }
                    }
                    theme::muted(ui, "Leave empty for the default. Worlds in the old folder are not moved.");
                });

                theme::card(ui, |ui| {
                    theme::section(ui, ic::BROWSER, "Window size");
                    const PRESETS: [(u32, u32); 4] = [(854, 480), (1280, 720), (1600, 900), (1920, 1080)];
                    let win_label = match &self.draft.window {
                        WindowSize::Auto => "Default".to_string(),
                        WindowSize::Fullscreen => "Fullscreen".to_string(),
                        WindowSize::Size(w, h) => format!("{w} x {h}"),
                    };
                    egui::ComboBox::from_id_salt("winsize")
                        .selected_text(win_label)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.draft.window == WindowSize::Auto, "Default").clicked() {
                                self.draft.window = WindowSize::Auto;
                            }
                            if ui
                                .selectable_label(self.draft.window == WindowSize::Fullscreen, "Fullscreen")
                                .clicked()
                            {
                                self.draft.window = WindowSize::Fullscreen;
                            }
                            for (pw, ph) in PRESETS {
                                if ui
                                    .selectable_label(
                                        self.draft.window == WindowSize::Size(pw, ph),
                                        format!("{pw} x {ph}"),
                                    )
                                    .clicked()
                                {
                                    self.draft.window = WindowSize::Size(pw, ph);
                                }
                            }
                            let is_custom = matches!(&self.draft.window, WindowSize::Size(a, b) if !PRESETS.contains(&(*a, *b)));
                            if ui.selectable_label(is_custom, "Custom…").clicked() && !is_custom {
                                self.draft.window = WindowSize::Size(1366, 768);
                            }
                        });
                    if let WindowSize::Size(sw, sh) = &mut self.draft.window {
                        ui.horizontal(|ui| {
                            ui.label("Width");
                            ui.add(egui::DragValue::new(sw).range(320..=7680).suffix(" px"));
                            ui.label("Height");
                            ui.add(egui::DragValue::new(sh).range(240..=4320).suffix(" px"));
                        });
                    }
                });

                theme::card(ui, |ui| {
                    theme::section(ui, ic::GAME_CONTROLLER, "Graphics card (GPU)");
                    ui.horizontal_wrapped(|ui| {
                        ui.radio_value(&mut self.draft.gpu, Gpu::Auto, "Automatic");
                        ui.radio_value(&mut self.draft.gpu, Gpu::Integrated, "Integrated");
                        ui.radio_value(&mut self.draft.gpu, Gpu::Discrete, "Discrete");
                    });
                    theme::muted(
                        ui,
                        "Integrated = power saving, Discrete = high performance. Only matters on PCs with two GPUs.",
                    );
                });
            }

            // ---- right column: Java and memory ----
            {
                let ui = &mut cols[1];

                theme::card(ui, |ui| {
                    theme::section(ui, ic::COFFEE, "Java / JRE");
                    let java_label = match &self.draft.java {
                        JavaChoice::Auto => "Automatic (matches the Minecraft version)".to_string(),
                        JavaChoice::Custom(path) => self
                            .java_detected
                            .iter()
                            .find(|(_, d)| d.to_string_lossy() == path.as_str())
                            .map(|(n, _)| n.clone())
                            .unwrap_or_else(|| "Custom path".to_string()),
                    };
                    egui::ComboBox::from_id_salt("java")
                        .selected_text(java_label)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    self.draft.java == JavaChoice::Auto,
                                    "Automatic (matches the Minecraft version)",
                                )
                                .clicked()
                            {
                                self.draft.java = JavaChoice::Auto;
                            }
                            for (name, path) in &self.java_detected {
                                let s = path.display().to_string();
                                if ui
                                    .selectable_label(self.draft.java == JavaChoice::Custom(s.clone()), name.as_str())
                                    .clicked()
                                {
                                    self.draft.java = JavaChoice::Custom(s);
                                }
                            }
                            if ui.selectable_label(false, "Custom path…").clicked()
                                && !matches!(self.draft.java, JavaChoice::Custom(_))
                            {
                                self.draft.java = JavaChoice::Custom(String::new());
                            }
                        });
                    if let JavaChoice::Custom(path) = &mut self.draft.java {
                        if path_row(ui, path, "path to java.exe or javaw.exe") {
                            if let Some(f) = rfd::FileDialog::new().add_filter("Java", &["exe"]).pick_file() {
                                *path = f.display().to_string();
                            }
                        }
                        if !path.trim().is_empty() && !Path::new(path.trim()).is_file() {
                            ui.colored_label(p.danger, "File not found");
                        }
                    }
                    theme::muted(
                        ui,
                        "Up to 1.16 needs Java 8, 1.17 needs 16, 1.18 to 1.20.4 needs 17, 1.20.5+ needs 21.",
                    );
                });

                theme::card(ui, |ui| {
                    theme::section(ui, ic::CPU, "Memory (RAM)");
                    let max = if self.total_ram_mb >= 1024 { self.total_ram_mb / 256 * 256 } else { 16384 };
                    ui.horizontal(|ui| {
                        ui.label(format!("{} MB of {} MB installed", self.draft.ram_mb, self.total_ram_mb));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.draft.ram_mb)
                                    .range(256..=max)
                                    .speed(64)
                                    .suffix(" MB"),
                            );
                        });
                    });
                    ui.spacing_mut().slider_width = (ui.available_width() - 20.0).max(100.0);
                    ui.add(
                        egui::Slider::new(&mut self.draft.ram_mb, 512..=max)
                            .step_by(256.0)
                            .show_value(false),
                    );
                    if self.total_ram_mb > 0 && self.draft.ram_mb as f32 > self.total_ram_mb as f32 * 0.75 {
                        ui.colored_label(p.warn, "That leaves little memory for Windows and may slow your PC down.");
                    }
                });
            }
        });
        ui.add_space(8.0);
    }

    fn tab_launcher(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);

        ui.columns(2, |cols| {
        {
        let ui = &mut cols[0];
        theme::card(ui, |ui| {
            theme::section(ui, ic::PALETTE, "Appearance");
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.draft.theme, Theme::Dark, "Dark");
                ui.radio_value(&mut self.draft.theme, Theme::Light, "Light");
            });
            ui.add_space(4.0);
            ui.label("Interface size");
            ui.spacing_mut().slider_width = (ui.available_width() - 90.0).max(100.0);
            ui.add(egui::Slider::new(&mut self.draft.ui_scale, 0.85..=1.25).step_by(0.05));
        });

        theme::card(ui, |ui| {
            theme::section(ui, ic::PLAY, "When the game starts");
            ui.checkbox(&mut self.draft.minimize_on_launch, "Minimize the launcher while the game is running");
            theme::muted(ui, "Also frees the launcher's logo memory while you play.");
        });

        }
        {
        let ui = &mut cols[1];
        theme::card(ui, |ui| {
            theme::section(ui, ic::IMAGE, "Mod logos");
            ui.checkbox(&mut self.draft.show_logos, "Show mod logos in the Mods screen");
            theme::muted(ui, "Turn off to save a little memory and bandwidth.");
        });

        theme::card(ui, |ui| {
            theme::section(ui, ic::KEY, "CurseForge API key (optional)");
            ui.add(
                egui::TextEdit::singleline(&mut self.draft.curseforge_api_key)
                    .password(true)
                    .hint_text("paste your key")
                    .desired_width(f32::INFINITY),
            );
            theme::muted(ui, "Free key from console.curseforge.com. Without it, only Modrinth is searched.");
        });
        }
        });
        ui.add_space(6.0);
    }

    // ---------------- main view ----------------

    fn ui_main(&mut self, ctx: &egui::Context) {
        let idle = self.busy == Busy::Idle;
        let installed = self.target_id().map_or(false, |id| self.paths.is_installed(&id));
        let has_account = self.cfg.accounts.get(self.cfg.selected_account).is_some();
        let p = theme::palette(ctx.style().visuals.dark_mode);
        let acc_name = self.cfg.accounts.get(self.cfg.selected_account).map(|a| a.username.clone());

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin::symmetric(24.0, 16.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(12.0, 14.0);

                    // ---- header ----
                    ui.horizontal(|ui| {
                        if let Some(l) = &self.logos {
                            theme::logo_image(ui, &l.small, 38.0);
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(egui::RichText::new("Oxide Launcher").size(20.0).strong());
                            theme::muted(ui, "Fast, lightweight, mod-ready");
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(name) = &acc_name {
                                theme::avatar(ui, name, 34.0);
                                ui.label(egui::RichText::new(name).strong());
                            }
                        });
                    });

                    // ---- landscape: play card on the left, setup on the right ----
                    ui.columns(2, |cols| {
                        {
                            let ui = &mut cols[0];
                            theme::card(ui, |ui| {
                                if let Some(l) = &self.logos {
                                    ui.vertical_centered(|ui| theme::logo_image(ui, &l.big, 210.0));
                                    ui.add_space(4.0);
                                }
                                let ver = self.selected.clone().unwrap_or_else(|| "Loading…".into());
                                ui.label(egui::RichText::new(&ver).size(30.0).strong());
                                ui.horizontal_wrapped(|ui| {
                                    theme::chip(ui, self.kind.label(), p.accent);
                                    match self.loader {
                                        Some(l) => theme::chip(ui, l.name(), p.info),
                                        None => theme::chip(ui, "Vanilla", p.muted),
                                    }
                                    if installed {
                                        theme::chip(ui, &format!("{}  Installed", ic::CHECK_CIRCLE), p.ok);
                                    } else {
                                        theme::chip(ui, "Not installed", p.muted);
                                    }
                                });
                                ui.add_space(4.0);

                                let (label, fill, enabled) = match (&self.busy, installed) {
                                    (Busy::Installing, _) => (
                                        format!("{}  Installing…", ic::DOWNLOAD_SIMPLE),
                                        p.accent.gamma_multiply(0.55),
                                        false,
                                    ),
                                    (Busy::Running, _) => {
                                        (format!("{}  Playing…", ic::PLAY), p.accent.gamma_multiply(0.55), false)
                                    }
                                    (_, true) => (
                                        format!("{}  Play", ic::PLAY),
                                        p.accent,
                                        idle && self.selected.is_some(),
                                    ),
                                    (_, false) => (
                                        format!("{}  Install", ic::DOWNLOAD_SIMPLE),
                                        p.accent,
                                        idle && self.selected.is_some(),
                                    ),
                                };
                                let resp = theme::primary_button(ui, &label, fill, enabled);
                                if resp.clicked() {
                                    if installed {
                                        if has_account {
                                            self.start_play(ctx);
                                        } else {
                                            // No account yet: ask for one, then start the game.
                                            self.play_after_account = true;
                                            self.show_add = true;
                                        }
                                    } else if self.loader == Some(Loader::OptiFine) {
                                        self.open_optifine_dialog();
                                    } else {
                                        self.start_install(ctx);
                                    }
                                }

                                if self.busy == Busy::Installing {
                                    let (d, t) = self.progress;
                                    let frac = if t > 0 { d as f32 / t as f32 } else { 0.0 };
                                    let text =
                                        if t > 0 { format!("{d} / {t} files") } else { "Preparing…".to_string() };
                                    theme::progress(ui, frac, text, t == 0);
                                }
                                if !self.status.is_empty() {
                                    let bad = ["failed", "Could not", "went wrong", "not found", "not installed"]
                                        .iter()
                                        .any(|s| self.status.contains(s));
                                    let col = if bad { p.danger } else { p.muted };
                                    ui.label(egui::RichText::new(&self.status).size(12.5).color(col));
                                }
                            });
                        }

                        {
                            let ui = &mut cols[1];
                            theme::card(ui, |ui| {
                                let w = ui.available_width();

                                // account
                                theme::section(ui, ic::USER, "Account");
                                let acc_text = self
                                    .cfg
                                    .accounts
                                    .get(self.cfg.selected_account)
                                    .map_or("No account yet (use Account below)".to_string(), |a| a.username.clone());
                                let mut chosen = None;
                                ui.add_enabled_ui(idle, |ui| {
                                    egui::ComboBox::from_id_salt("account")
                                        .selected_text(acc_text)
                                        .width(w)
                                        .show_ui(ui, |ui| {
                                            for (i, a) in self.cfg.accounts.iter().enumerate() {
                                                let kind = if a.kind == AccountKind::Offline {
                                                    "password-free"
                                                } else {
                                                    "Microsoft"
                                                };
                                                if ui
                                                    .selectable_label(
                                                        i == self.cfg.selected_account,
                                                        format!("{} ({kind})", a.username),
                                                    )
                                                    .clicked()
                                                {
                                                    chosen = Some(i);
                                                }
                                            }
                                        });
                                });
                                if let Some(i) = chosen {
                                    self.cfg.selected_account = i;
                                    self.save_cfg();
                                }
                                ui.add_space(2.0);

                                // version type
                                theme::section(ui, ic::CUBE, "Version");
                                let before = self.kind;
                                ui.add_enabled_ui(idle, |ui| {
                                    let items = [
                                        (Kind::Release, "Release"),
                                        (Kind::Snapshot, "Snapshot"),
                                        (Kind::Beta, "Beta"),
                                        (Kind::Alpha, "Alpha"),
                                    ];
                                    theme::segmented(ui, &mut self.kind, &items);
                                });
                                if self.kind != before {
                                    self.selected = self.version_rows().0.first().map(|r| r.0.clone());
                                }

                                // version list: installed first, with a green tick
                                let (rows, n_inst) = self.version_rows();
                                let sel_installed =
                                    self.selected.as_ref().map_or(false, |s| self.inst.ids.contains(s));
                                let sel_text = match &self.selected {
                                    Some(s) => theme::tick_job(&p, s, sel_installed, false),
                                    None => theme::tick_job(&p, "Select a version", false, false),
                                };
                                ui.add_enabled_ui(idle, |ui| {
                                    egui::ComboBox::from_id_salt("version")
                                        .selected_text(sel_text)
                                        .width(w)
                                        .show_ui(ui, |ui| {
                                            if rows.is_empty() {
                                                theme::muted(ui, "No versions yet. Connect to the internet to see them.");
                                            }
                                            if n_inst > 0 {
                                                theme::muted(ui, "Installed");
                                            }
                                            for (i, (id, inst)) in rows.iter().enumerate() {
                                                if n_inst > 0 && i == n_inst {
                                                    ui.separator();
                                                    theme::muted(ui, "All versions");
                                                }
                                                let chosen = self.selected.as_deref() == Some(id.as_str());
                                                if theme::tick_label(ui, chosen, id, *inst).clicked() {
                                                    self.selected = Some(id.clone());
                                                }
                                            }
                                        });
                                });
                                ui.add_space(2.0);

                                // type list: Vanilla + loaders that are installed or available, with ticks
                                theme::section(ui, ic::PUZZLE_PIECE, "Type");
                                let mc = self.selected.clone();
                                let avail: Option<Vec<Loader>> =
                                    mc.as_ref().and_then(|id| self.loader_cache.get(id)).cloned();
                                let has = |l: Option<Loader>| {
                                    mc.as_ref().map_or(false, |m| self.inst.ids.contains(&loaders::profile_id(m, l)))
                                };
                                let mut entries: Vec<(Option<Loader>, bool)> = vec![(None, has(None))];
                                for l in Loader::ALL {
                                    let inst = has(Some(l));
                                    if inst || avail.as_ref().map_or(false, |a| a.contains(&l)) {
                                        entries.push((Some(l), inst));
                                    }
                                }
                                let cur_text = theme::tick_job(
                                    &p,
                                    self.loader.map_or("Vanilla", |l| l.name()),
                                    has(self.loader),
                                    false,
                                );
                                let checking = mc.is_some() && avail.is_none();
                                ui.add_enabled_ui(idle, |ui| {
                                    egui::ComboBox::from_id_salt("loader")
                                        .selected_text(cur_text)
                                        .width(w)
                                        .show_ui(ui, |ui| {
                                            for (l, inst) in &entries {
                                                let name = l.map_or("Vanilla", |l| l.name());
                                                if theme::tick_label(ui, self.loader == *l, name, *inst).clicked() {
                                                    self.loader = *l;
                                                }
                                            }
                                            if checking {
                                                theme::muted(ui, "Checking for more mod loaders…");
                                            }
                                        });
                                });
                            });

                            // ---- toolbar ----
                            let inst_dir = match self.target_id() {
                                Some(id) => self.paths.instance(&id),
                                None => self.paths.instances_dir.clone(),
                            };
                            ui.horizontal(|ui| {
                                let gap = 8.0;
                                ui.spacing_mut().item_spacing.x = gap;
                                let w = ((ui.available_width() - gap * 4.0) / 5.0).floor();
                                if theme::tool_button(ui, ic::USER_PLUS, "Account", true, w).clicked() {
                                    self.show_add = true;
                                }
                                if theme::tool_button(ui, ic::PUZZLE_PIECE, "Mods", self.selected.is_some(), w)
                                    .clicked()
                                {
                                    self.open_mods(ctx);
                                }
                                if theme::tool_button(ui, ic::PACKAGE, "Mod folder", true, w).clicked() {
                                    open_folder(&inst_dir.join("mods"));
                                }
                                if theme::tool_button(ui, ic::FOLDER_OPEN, "Game folder", true, w).clicked() {
                                    open_folder(&inst_dir);
                                }
                                if theme::tool_button(ui, ic::GEAR, "Settings", true, w).clicked() {
                                    self.open_settings();
                                }
                            });
                        }
                    });
                });
            });

        // ---- add account window ----
        if self.show_add {
            let mut open = true;
            let mut done = false;
            egui::Window::new(if self.play_after_account { "Create an account to play" } else { "Add account" })
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(340.0)
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);
                    if self.play_after_account {
                        ui.label(
                            egui::RichText::new("You need an account to play. Choose a name and the game will start.")
                                .color(p.warn),
                        );
                    }
                    ui.add_enabled(
                        false,
                        egui::Button::new(format!("{}  Microsoft account (coming soon)", ic::USER))
                            .min_size(egui::vec2(ui.available_width(), 36.0)),
                    );
                    ui.separator();
                    theme::section(ui, ic::USER_PLUS, "Password-free account");
                    theme::muted(ui, "Just pick a name. 3-16 letters, digits or underscores.");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_name)
                            .hint_text("Username")
                            .desired_width(f32::INFINITY),
                    );
                    if !self.add_err.is_empty() {
                        ui.colored_label(p.danger, &self.add_err);
                    }
                    let create = theme::accent_btn(&p, &format!("{}  Create account", ic::CHECK))
                        .min_size(egui::vec2(ui.available_width(), 36.0));
                    if ui.add(create).clicked() {
                        let name = self.new_name.trim().to_string();
                        match accounts::validate_username(&name) {
                            Err(e) => self.add_err = e.to_string(),
                            Ok(()) if self.cfg.accounts.iter().any(|a| a.username == name) => {
                                self.add_err = "That account already exists".into()
                            }
                            Ok(()) => {
                                self.cfg.accounts.push(Account { username: name, kind: AccountKind::Offline });
                                self.cfg.selected_account = self.cfg.accounts.len() - 1;
                                self.save_cfg();
                                done = true;
                            }
                        }
                    }
                });
            if done || !open {
                self.show_add = false;
                self.new_name.clear();
                self.add_err.clear();
                let was_waiting = std::mem::take(&mut self.play_after_account);
                if done && was_waiting {
                    self.start_play(ctx);
                }
            }
        }
    }
}

/// A text box that fills the row, with a Browse button at the right edge.
/// (Right-to-left so the box takes exactly the space the button leaves; nothing overflows.)
/// Returns true when Browse was clicked.
fn path_row(ui: &mut egui::Ui, text: &mut String, hint: &str) -> bool {
    let mut browse = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!("{}  Browse", ic::FOLDER_OPEN)).clicked() {
                browse = true;
            }
            ui.add(
                egui::TextEdit::singleline(text)
                    .hint_text(hint)
                    .desired_width(ui.available_width()),
            );
        });
    });
    browse
}

fn open_folder(p: &Path) {
    let _ = std::fs::create_dir_all(p);
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // A bug in one screen should not kill the launcher: catch it, log it, go back home.
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.update_inner(ctx)));
        if r.is_err() {
            self.view = View::Main;
            self.status = "Something went wrong. Details were saved to logs\\crash.log".into();
        }
    }
}

impl App {
    fn update_inner(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Manifest(Ok(m)) => {
                    // Keep the remembered choice; only pick the latest release if nothing is selected.
                    if self.selected.is_none() {
                        self.selected = Some(m.latest.release.clone());
                        self.kind = Kind::Release;
                    }
                    self.manifest = Some(m);
                    if self.status.starts_with("Loading versions") || self.status.starts_with("Offline") {
                        self.status.clear();
                    }
                }
                Msg::Manifest(Err(e)) => {
                    self.status = if self.manifest.is_some() || !self.inst.versions.is_empty() {
                        "Offline: showing your installed versions.".to_string()
                    } else {
                        format!("Could not load versions: {e}")
                    };
                }
                Msg::Status(s) => self.status = s,
                Msg::Progress(d, t) => self.progress = (d, t),
                Msg::Loaders(id, list) => {
                    self.loader_pending.remove(&id);
                    self.loader_cache.insert(id, list);
                }
                Msg::Installed(Ok(())) => {
                    self.inst = installed::scan(&self.paths);
                    self.busy = Busy::Idle;
                    self.status = "Installed. Ready to play.".into();
                }
                Msg::Installed(Err(e)) => {
                    self.busy = Busy::Idle;
                    self.status = format!("Install failed: {e}");
                }
                Msg::Exited(r) => {
                    self.busy = Busy::Idle;
                    self.status = match r {
                        Ok(()) => "Game closed.".into(),
                        Err(e) => e,
                    };
                    if self.cfg.minimize_on_launch {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                }
            }
        }
        self.ensure_loaders(ctx);
        match self.view {
            View::Main => {
                self.ui_main(ctx);
                self.ui_optifine(ctx);
            }
            View::Settings => self.ui_settings(ctx),
            View::Mods => {
                if self.mods.ui(ctx, self.rt.handle()) {
                    self.view = View::Main;
                }
            }
        }
    }
}

/// Writes any panic (message, file and line) to %APPDATA%\\RustMCLauncher\\logs\\crash.log,
/// because the window app has no console to print to.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        trace::log("PANIC (see crash.log)");
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = format!("{info}\n\n{bt}\n");
        if let Ok(p) = Paths::new() {
            let dir = p.root.join("logs");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("crash.log"), &msg);
        }
        eprintln!("{msg}");
    }));
}

fn main() -> eframe::Result<()> {
    if let Ok(p) = Paths::new() {
        trace::init(p.root.join("logs").join("trace.log"));
    }
    install_panic_hook();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Oxide Launcher")
            .with_icon(theme::app_icon())
            .with_inner_size([1100.0, 660.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Oxide Launcher",
        options,
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    )
}
