<!--
  Paste everything below the line into the GitHub release description.
  Before publishing:
    - Replace YOUR_USERNAME.
    - Fill in the SHA-256 (run:  Get-FileHash .\OxideLauncher-Setup-1.0.0.exe -Algorithm SHA256).
    - Optionally attach 1-2 screenshots (drag them into the description box).
-->

# Oxide Launcher 1.0.0

**The first public release.** A fast, lightweight Minecraft: Java Edition launcher for Windows, written in Rust, with Java included and a built-in mod browser.

![Oxide Launcher](https://raw.githubusercontent.com/YOUR_USERNAME/oxide-launcher/main/docs/screenshots/01-home.png)

## Download

| File | Size | SHA-256 |
|---|---|---|
| `OxideLauncher-Setup-1.0.0.exe` | TODO MB | `TODO` |

Run the installer and follow the wizard. No administrator rights and no separate Java install are needed.

> **Windows SmartScreen** may show "unknown publisher" because the installer is not code-signed yet. Choose **More info, then Run anyway**.

## Highlights

- **Java included.** Eclipse Temurin 8, 17, 21 and 25 are bundled. The launcher picks the right one for every Minecraft version automatically.
- **One-click mod loaders.** Fabric, Quilt, Forge, NeoForge and OptiFine, shown only when they exist for the version you chose.
- **Built-in mod browser.** Search Modrinth and CurseForge, see only files that match your version and loader, and install with a progress bar.
- **Required mods, shown first.** Before a mod installs, you see the mods it needs (for example Fabric API) and can install them together.
- **My mods.** See what is installed, check for updates, update one or all, and remove mods.
- **Light on memory.** Native interface with no Electron and no web view.
- **Oxide look.** Pure-black AMOLED theme with a soft-yellow accent, landscape layout, icons and mod logos. A light theme is included.

## What is in 1.0.0

**Game**
- Release, snapshot, beta and alpha versions, newest to oldest
- Checksum-verified downloads from Mojang with parallel downloads and retries
- Works with old versions, including legacy assets and the Log4Shell fix for 1.7 to 1.18
- Installing never needs an account

**Mod loaders**
- Fabric and Quilt through their official APIs
- Forge (1.12.2 and newer) and NeoForge (1.20.2 and newer) through their official installers, run silently
- OptiFine through a guided install: you download the file from optifine.net, Oxide runs OptiFine's own installer and imports the result

**Mods, resource packs and shaders**
- Modrinth and CurseForge (CurseForge needs your own free API key)
- Files go to `mods`, `resourcepacks` or `shaderpacks` automatically
- Logos for every project, shrunk and cached to save memory

**Accounts**
- Password-free accounts (choose a username)

**Settings**
- Game folder, window size (presets, fullscreen, custom), Java choice, RAM (slider and exact MB, limited to your installed memory) and integrated or discrete GPU
- Dark or light theme, interface size, minimize while playing, show or hide logos

**Installer**
- Standard Windows wizard, per-user install by default, optional desktop shortcut
- Choose which Java versions to include (full or compact)
- Uninstaller asks before deleting your game data

## Requirements

- Windows 10 or 11, 64-bit
- A graphics driver with OpenGL support
- Internet connection for downloading the game and mods

## Known limitations

- **No Microsoft login yet.** Password-free accounts work for singleplayer and servers in offline mode. Servers that verify accounts, skins and Realms need an official account. Microsoft login is planned.
- **Forge needs Minecraft 1.12.2 or newer; NeoForge needs 1.20.2 or newer.**
- **OptiFine cannot be downloaded automatically.** Its website requires you to download the file yourself. OptiFine profiles do not load mods; use Forge or Fabric for mods.
- **Mod installs need a loader.** Choose Fabric, Quilt, Forge or NeoForge before installing mods.
- Data packs and modpack import are not included yet.

## Upgrading from an earlier test build

If you used an earlier test build, your data folder `%APPDATA%\RustMCLauncher` is moved to `%APPDATA%\OxideLauncher` automatically the first time you start 1.0.0. Your versions, accounts, worlds and mods come with it.

## Something wrong?

Please [open an issue](https://github.com/YOUR_USERNAME/oxide-launcher/issues/new) and attach the log from `%APPDATA%\OxideLauncher\logs` (`latest.log` for game problems, `forge-install.log` or `neoforge-install.log` for loader installs), plus your Windows version and the Minecraft version you tried.

## License

Oxide Launcher is free software under the **AGPL-3.0**. The source for this release is in the repository at the `v1.0.0` tag. It is an independent project and is not affiliated with Mojang Studios, Microsoft, OptiFine, Modrinth, CurseForge or the Forge, NeoForge, Fabric and Quilt projects. "Minecraft" is a trademark of Mojang Studios.
