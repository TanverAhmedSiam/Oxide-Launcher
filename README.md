<!--
  HOW TO USE THIS FILE
  1. Search for "YOUR_USERNAME" and replace it with your GitHub name.
  2. Search for "TODO" and fill those in (license, RAM number, ...).
  3. Add your screenshots to docs/screenshots/ using the file names below.
     Every spot that needs a photo is marked with  >>> PHOTO N <<<  in a comment.
     The image line right under each marker already points to the right file name.

  SHOT LIST (suggested size: about 1100 x 660, PNG)
    01-home.png          Home screen (play card + setup)
    02-versions.png      Version list / loader dropdown open
    03-mods-browse.png   Mods > Browse with a project selected (file list on the right)
    04-required-mods.png The "Required mods" window
    05-my-mods.png       Mods > My mods (with an update available if possible)
    06-settings-mc.png   Settings > Minecraft tab
    07-settings-launcher.png  Settings > Launcher tab
    08-installer.png     The Windows installer welcome page
-->

<div align="center">

<img width="704" height="286" alt="rsz_2gemini_generated_image_icj65gicj65gicj6" src="https://github.com/user-attachments/assets/0f34393a-5e64-4857-904c-cdf7f849dafb" />

# Oxide Launcher

**A fast, lightweight Minecraft: Java Edition launcher for Windows, written in Rust.**
Bundled Java, one-click mod loaders, built-in Modrinth and CurseForge mod browser.

[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-lightgrey)](#requirements)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange)](https://www.rust-lang.org/)
[![Latest release](https://img.shields.io/github/v/release/YOUR_USERNAME/oxide-launcher)](https://github.com/YOUR_USERNAME/oxide-launcher/releases/latest)
[![License: GPL v3](https://img.shields.io/badge/license-GPL--3.0-yellow)](LICENSE)

[Download](https://github.com/YOUR_USERNAME/oxide-launcher/releases/latest) ·
[Features](#features) ·
[Screenshots](#screenshots) ·
[Build from source](#build-from-source) ·
[FAQ](#faq)

</div>

---

<!-- >>> PHOTO 1 <<<  Home screen. Save as docs/screenshots/01-home.png -->
<p align="center">
  <img width="1372" height="887" alt="Screenshot 2026-11-19 095341" src="https://github.com/user-attachments/assets/0d01875a-c614-4a39-97c0-619222cba1f8" />
</p>

## Why Oxide?

- **Light on memory.** No Electron, no web view. The interface is drawn natively with [egui](https://github.com/emilk/egui) and only redraws when something changes.
  <!-- TODO: add your measured idle RAM here, e.g. "About XX MB when idle (Task Manager)." -->
- **Nothing to install first.** The installer includes Java (Eclipse Temurin 8, 17, 21 and 25) and the launcher picks the right one for every Minecraft version.
- **Everything in one place.** Pick a version, pick Fabric / Quilt / Forge / NeoForge / OptiFine, install, play, and add mods without leaving the window.
- **Looks good.** Pure-black AMOLED theme with a soft-yellow accent, plus a light theme.

## Features

### Game versions
- Release, snapshot, beta and alpha versions, listed newest to oldest.
- Downloads straight from Mojang's official servers with checksum verification, parallel downloads and automatic retries.
- Works for old versions too (legacy assets, native libraries, Log4Shell fix for 1.7 to 1.18).
- **Installing never needs an account.**

<!-- >>> PHOTO 2 <<<  Version type pills + version list / Type dropdown open. Save as docs/screenshots/02-versions.png -->
<p align="center">
  <img width="1373" height="820" alt="Screenshot 2026-11-19 095737" src="https://github.com/user-attachments/assets/ecfcfd60-ef14-44ff-974e-cd847724ea4c" />
</p>

### Mod loaders

| Loader | Supported versions | How it installs |
|---|---|---|
| **Fabric** | versions Fabric supports | Official Fabric meta API |
| **Quilt** | versions Quilt supports | Official Quilt meta API |
| **Forge** | 1.12.2 and newer | Official Forge installer, run silently |
| **NeoForge** | 1.20.2 and newer | Official NeoForge installer, run silently |
| **OptiFine** | versions listed on optifine.net | Guided: you download the file, the launcher runs OptiFine's own installer and imports it |

The Type dropdown only shows loaders that actually exist for the version you picked. Each loader gets its own game folder (`1.21.1-fabric`, `1.20.1-forge`, ...), so worlds and mods never mix.

### Mod browser (Modrinth)
- Search mods, resource packs and shaders from **Modrinth**.
- Only files that match your Minecraft version **and** loader are shown.
- **Required mods are shown before installing**, with logos, so nothing is missing at launch (for example Fabric API).
- Files go to the right folder automatically: `mods`, `resourcepacks` or `shaderpacks`.
- Real progress bar, then a green **Installed** state.
- Mod logos are shrunk and cached, so they cost almost no memory (and can be turned off).

<!-- >>> PHOTO 3 <<<  Mods > Browse with a project selected. Save as docs/screenshots/03-mods-browse.png -->
<p align="center">
  <img width="1368" height="826" alt="Screenshot 2026-11-19 100021" src="https://github.com/user-attachments/assets/182e0648-5375-4a65-87e7-f4f0cbaf47cb" />
</p>

<!-- >>> PHOTO 4 <<<  The "Required mods" window. Save as docs/screenshots/04-required-mods.png -->
<p align="center">
  <img width="588" height="450" alt="Screenshot 2026-11-19 100423" src="https://github.com/user-attachments/assets/c7a364a3-869a-4e1f-8a72-7aa853604bfa" />
</p>

### My mods
- See everything you installed, with logo, version and date.
- **Check for updates**, update one mod or **Update all**. Updating replaces the old file.
- Remove mods in one click. Files you added by hand are listed too.

<!-- >>> PHOTO 5 <<<  Mods > My mods. Save as docs/screenshots/05-my-mods.png -->
<p align="center">
  <img width="1374" height="823" alt="Screenshot 2026-11-19 100834" src="https://github.com/user-attachments/assets/329b77e0-daf7-482a-8ac8-c7b3b40ad02a" />
</p>

### Accounts
- **Password-free accounts:** just choose a username (works for singleplayer and offline-mode servers).
- Microsoft account login is **planned** (see [Roadmap](#roadmap)).

### Settings

**Minecraft tab:** game folder, window size (presets, fullscreen or custom), Java (automatic, any detected install, or your own path), RAM (slider and exact MB, limited to your installed memory), GPU preference (integrated or discrete).

**Launcher tab:** dark / light theme, interface size, minimize while playing, show or hide mod logos, CurseForge API key.

<!-- >>> PHOTO 6 <<<  Settings > Minecraft tab. Save as docs/screenshots/06-settings-mc.png -->
<p align="center">
  <img width="1377" height="837" alt="Screenshot 2026-11-19 101005" src="https://github.com/user-attachments/assets/1a6a081f-ae6a-4aea-9fbf-dd507f903664" />
</p>

<!-- >>> PHOTO 7 <<<  Settings > Launcher tab. Save as docs/screenshots/07-settings-launcher.png -->
<p align="center">
  <img width="1378" height="833" alt="Screenshot 2026-11-19 101119" src="https://github.com/user-attachments/assets/d13463da-b9f2-4fa8-a57c-e87d6b9316c0" />
</p>

## Screenshots

> Add a gallery here if you like. Put the files in `docs/screenshots/`.

<!-- >>> PHOTO 8 <<<  Optional gallery image, or the installer welcome page. Save as docs/screenshots/08-installer.png -->
<p align="center">
  <img width="796" height="611" alt="image" src="https://github.com/user-attachments/assets/3fd597af-37b0-4408-9ea0-8c4a7ca25ea1" />
</p>

## Requirements

- Windows 10 or 11, 64-bit
- A graphics driver with OpenGL support (any normal PC)
- Internet for downloading game files and mods
- Nothing else: Java is included in the installer

## Install

1. Go to the [**Releases**](https://github.com/YOUR_USERNAME/oxide-launcher/releases/latest) page.
2. Download `OxideLauncher-Setup-1.0.0.exe` and run it.
3. Follow the wizard. It installs for your user account (no administrator needed) and lets you choose which Java versions to include.

> **Windows SmartScreen** may say "unknown publisher" because the installer is not code-signed yet. Choose **More info → Run anyway**.

## Quick start

1. Open Oxide Launcher and click **Account** → create a password-free account.
2. Pick a **Version** (and a **Type**, such as Fabric).
3. Press **Install**, then **Play**.
4. Want mods? Press **Mods**, search, pick a file and press **Install**.

## Where is my data?

| What | Where |
|---|---|
| Launcher data, versions, libraries | `%APPDATA%\OxideLauncher` |
| Worlds, mods, resource packs (per version) | `%APPDATA%\OxideLauncher\instances\<version>` (changeable in Settings) |
| Settings and accounts | `%APPDATA%\OxideLauncher\config.json` |
| Logs | `%APPDATA%\OxideLauncher\logs` |

Older builds used `%APPDATA%\RustMCLauncher`; the folder is moved automatically on first start.

## Build from source

You need [Rust](https://rustup.rs/) and the **Visual Studio Build Tools** (the "Desktop development with C++" workload, which includes the Windows SDK).

```powershell
git clone https://github.com/YOUR_USERNAME/oxide-launcher.git
cd oxide-launcher

# Run the launcher
cargo run --release

# Command-line tool (list / install / play without the window)
cargo run --release --bin rmcl-cli -- list --type release
```

The first build takes a few minutes. The release binary is `target\release\oxide-launcher.exe`.

### Build the installer (with bundled Java)

1. Install [Inno Setup 6.4+](https://jrsoftware.org/isdl.php).
2. Run:

```powershell
powershell -ExecutionPolicy Bypass -File installer\build-installer.ps1
```

This builds the launcher, downloads Eclipse Temurin JREs (checksum verified), and creates `installer\output\OxideLauncher-Setup-<version>.exe`.
Use `-Java 17,21` to bundle fewer Java versions, or `-SkipBuild` to reuse the existing build.

## Project layout

```text
src/
  main.rs            window, screens, settings
  ui_mods.rs         mod browser screens
  theme.rs           colours, fonts, icons, small widgets
  engine/
    versions.rs      Mojang manifest, version JSON, profile merging
    install.rs       libraries, assets, natives
    launch.rs        builds the Java command line
    loaders.rs       Fabric, Quilt, Forge, NeoForge
    optifine.rs      guided OptiFine install
    mods.rs          Modrinth and CurseForge clients, installed list
    java.rs          bundled / custom Java selection
    config.rs        settings file
installer/           Inno Setup script and build script
assets/              logo and icons
```

## FAQ

**Do I need to own Minecraft?**
Minecraft: Java Edition is a paid game by Mojang. Oxide Launcher can download and run the game files, and password-free accounts work for singleplayer and servers in offline mode. Online servers, skins and Realms need an official account. Please buy the game to support its developers.

**Why can't I join online servers?**
Password-free accounts have no Microsoft login, so servers that verify accounts will refuse them. Microsoft login is on the roadmap.

**Why isn't OptiFine downloaded automatically?**
OptiFine's website requires you to download the file yourself, and it can't be redistributed by other sites. Oxide guides you through it and then installs it for you. For a similar result on Fabric, try Sodium + Iris from the mod browser.

**Where do I get a CurseForge API key?**
Free at [console.curseforge.com](https://console.curseforge.com). Paste it in **Settings → Launcher**. Without it, only Modrinth is searched.

**The game crashes at startup. What do I do?**
Open `%APPDATA%\OxideLauncher\logs\latest.log`. For Forge or NeoForge install problems, look at `forge-install.log` or `neoforge-install.log` in the same folder. When you report a bug, attach the relevant log.

**Forge or NeoForge says Java is missing.**
Use the installer build, which includes every Java version. If you run from source, install the Java version the game needs, or point to it in **Settings → Minecraft → Java**.

## Roadmap

- [ ] Microsoft account login
- [ ] Data packs and modpack import (`.mrpack`)
- [ ] Automatic launcher updates
- [ ] More languages

## Contributing

Issues and pull requests are welcome. Please keep changes small and focused, and run `cargo fmt` before opening a pull request.

## License

TODO: choose a license for this project (for example MIT or Apache-2.0) and add a `LICENSE` file.

Third-party components, including the bundled Eclipse Temurin Java runtimes, are listed in [`installer/THIRD-PARTY-NOTICES.txt`](installer/THIRD-PARTY-NOTICES.txt).

## Disclaimer

Oxide Launcher is an independent project. It is **not** affiliated with, endorsed by, or sponsored by Mojang Studios, Microsoft, OptiFine, Modrinth, CurseForge, Forge, NeoForge, Fabric or Quilt. "Minecraft" is a trademark of Mojang Studios. Mods and packs are the property of their authors and are installed from their official sources at your request.
