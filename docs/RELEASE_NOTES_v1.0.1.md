<!--
  Paste everything below into the GitHub release description for v1.0.1.
  Replace YOUR_USERNAME and fill in the SHA-256:  Get-FileHash .\OxideLauncher-Setup-1.0.1.exe -Algorithm SHA256
-->

# Oxide Launcher 1.0.1

A quality update based on feedback from the first testers. Fewer clicks, and it works without internet.

## What's new

- **Starts where you left off.** The launcher opens on the version and type you played last, so you can press Play straight away.
- **Installed versions show a green tick** in both the Version list and the Type list. Installed versions are listed first, newest on top.
- **Works offline.** Your installed versions load instantly from your PC. The launcher no longer waits for the internet before you can play, and the list of new versions refreshes quietly in the background.
- **Java is picked automatically.** The launcher uses the right Java for each game version. If that Java isn't on the PC, it downloads it once (checksum verified) and keeps it. Nobody needs to choose a Java version by hand any more. This also covers the Forge, NeoForge and OptiFine installers.
- **Play without an account now helps you.** Press Play with no account and a window asks you to create one. The game starts as soon as you do.

## Also improved

- Checking which mod loaders exist for a version is faster, and it gives up sooner when you are offline.
- Clearer messages when something needs the internet.

## Download and install

| File | Size | SHA-256 |
|---|---|---|
| `OxideLauncher-Setup-1.0.1.exe` | TODO MB | `TODO` |

Run the new installer over your old one. Your accounts, worlds, mods and versions are kept.

## Known limitations

No change from 1.0.0: there is no Microsoft login yet (password-free accounts only), Forge needs Minecraft 1.12.2 or newer, NeoForge needs 1.20.2 or newer, and OptiFine must be downloaded by you.

## Something wrong?

[Open an issue](https://github.com/YOUR_USERNAME/oxide-launcher/issues/new) and attach the log from `%APPDATA%\OxideLauncher\logs`.
