# OpenSteamTool Manager

A native Windows tool that deploys, removes, and updates OpenSteamTool patches for Steam. Built with Rust and egui/eframe (glow): a single binary, no runtime dependencies.

[简体中文](README_zh-CN.md)

## Features

- **Deploy / uninstall patches**: copy (or remove) the three target DLLs — `OpenSteamTool.dll`, `dwmapi.dll`, `xinput1_4.dll` — in your Steam directory
- **Online update**: check GitHub Releases for the latest version, download and extract into the local `dlls/` folder
- **Auto-detect Steam path**: resolved from the registry, or pick the folder manually
- **Steam-aware window**: hides to the system tray when Steam starts, restores on exit; hides automatically after launch operations
- **Tray**: left-click toggles visibility; menu has Show, "Minimize to tray automatically", Quit
- **Bilingual UI**: Chinese or English, chosen by system locale, switchable at runtime
- **Settings dialog**: edit the upstream `opensteamtool.toml` (Steam dir) with a validated TOML editor; starts from the bundled example template when the file is missing
- **OnlineFix launch preset**: toggle the `-onlinefix` launch option for a game in `localconfig.vdf` from the settings dialog (auto-backup before writing, Steam must be closed); one-click copy of the argument

## Usage

1. Download the latest ZIP from [Releases](../../releases) and extract it anywhere
2. Run `opensteamtool-manager.exe` (portable, no install)
3. On first use, populate `dlls/`: click "Check for Updates" then "Download & Extract New Version" to fetch the DLLs, or drop them in manually
4. Pick your Steam path, then "Apply Patch & Launch Steam"

Patches live in a `dlls/` folder next to the executable. The app starts without any loading screen; all operations run on background threads so the UI never freezes.

## Build

Requires Rust (edition 2024) and the MSVC toolchain.

```sh
cargo build --release
# Output: target/release/opensteamtool-manager.exe (~6.8 MB)
```

Package the portable ZIP (same script local and CI use):

```sh
powershell -File tools/build-release.ps1 -Version <version>
```

Tests:

```sh
cargo test
```

## Releases

Pushing a `v*` tag triggers GitHub Actions to build, test, package, and create a Release (see `.github/workflows/release.yml`; tags look like `v1.0.0`):

```sh
git tag v1.0.0
git push origin v1.0.0
```

You can also trigger it manually from the Actions tab.

## Terminology

Deploy, Uninstall, Local Version, Online Version, Action, Auto-tray, Minimize-to-Tray — definitions in [CONTEXT.md](CONTEXT.md).

## Source layout

```text
src/
├── main.rs       # eframe entry point
├── config_editor.rs # opensteamtool.toml read/validate/atomic-write (settings dialog)
├── onlinefix.rs # localconfig.vdf LaunchOptions edits (OnlineFix preset: VDF parser + backup)
├── ui.rs         # egui UI (3 cards + tray + auto-tray wiring)
├── workflow.rs   # action planning and step execution (plan/execute)
├── dll.rs        # target DLL deploy/uninstall, local status
├── steam.rs      # registry path detection, steam.exe launch
├── process.rs    # Steam process monitor/termination (sysinfo)
├── updater.rs    # GitHub update check, download & extract
├── tray.rs       # system tray
└── i18n.rs       # bilingual strings and copy mapping
```

Spec: [SPEC.md](SPEC.md).
