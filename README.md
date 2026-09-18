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

Package the portable ZIP (same script local and CI use; requires PowerShell 7+, `pwsh`):

```sh
pwsh -File tools/build-release.ps1 -Version <version>
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
├── steam.rs      # registry path detection, steam.exe launch
├── steam_state.rs # shared Steam process table (alive / group_running / kill)
├── process.rs    # Steam process monitor (2s polling cache, edge events)
├── dll.rs        # target DLL deploy/uninstall, local status
├── workflow.rs   # action planning and step execution (plan/execute)
├── busy.rs       # busy gate: exclusive interactive background ops (ADR-0007)
├── updater.rs    # GitHub update check, download & extract
├── update_flow.rs # single source of truth for update-check results (ADR-0008)
├── compat.rs     # compatibility probe: hashing / mirror chain / precache
├── compat_flow.rs # compatibility probe orchestration state machine (ADR-0006)
├── config_editor.rs # opensteamtool.toml read/validate/atomic-write (settings dialog)
├── onlinefix.rs  # localconfig.vdf LaunchOptions edits (OnlineFix preset: VDF parser + backup)
├── settings.rs   # settings dialog state (config editor + OnlineFix preset)
├── fsutil.rs     # shared atomic file write
├── tray.rs       # system tray
├── i18n.rs       # bilingual strings and error→copy mapping (ADR-0009)
└── ui.rs         # egui UI (3 cards + tray + auto-tray wiring)
```

Spec: [SPEC.md](SPEC.md).
