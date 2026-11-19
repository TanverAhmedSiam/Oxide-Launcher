use super::paths::Paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum AccountKind {
    Offline,
    Microsoft,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub username: String,
    pub kind: AccountKind,
}

/// Game window size passed to Minecraft.
#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum WindowSize {
    #[default]
    Auto,
    Fullscreen,
    Size(u32, u32),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum JavaChoice {
    /// Pick the right bundled Java for each Minecraft version.
    #[default]
    Auto,
    /// Path to a java.exe / javaw.exe chosen by the user.
    Custom(String),
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum Gpu {
    #[default]
    Auto,
    Integrated,
    Discrete,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub accounts: Vec<Account>,
    pub selected_account: usize,
    // Minecraft tab
    pub game_dir: String, // empty = default
    pub window: WindowSize,
    pub java: JavaChoice,
    pub ram_mb: u32,
    pub gpu: Gpu,
    // Launcher tab
    pub theme: Theme,
    pub ui_scale: f32,
    pub minimize_on_launch: bool,
    /// Optional: your own CurseForge API key (Modrinth needs none).
    pub curseforge_api_key: String,
    /// Show mod logos in the Mods screen.
    pub show_logos: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            accounts: vec![],
            selected_account: 0,
            game_dir: String::new(),
            window: WindowSize::Auto,
            java: JavaChoice::Auto,
            ram_mb: 2048,
            gpu: Gpu::Auto,
            theme: Theme::Dark,
            ui_scale: 1.0,
            minimize_on_launch: false,
            curseforge_api_key: String::new(),
            show_logos: true,
        }
    }
}

impl Config {
    pub fn load(paths: &Paths) -> Self {
        std::fs::read_to_string(paths.root.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(&paths.root)?;
        std::fs::write(paths.root.join("config.json"), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
    /// Push settings that affect paths into `Paths`.
    pub fn apply_to(&self, paths: &mut Paths) {
        let d = self.game_dir.trim();
        paths.instances_dir = if d.is_empty() { paths.root.join("instances") } else { PathBuf::from(d) };
    }
}
