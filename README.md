# OpenSteamTool Manager

A native Windows tool that deploys, removes, and updates OpenSteamTool patches for Steam. Built with Rust and egui/eframe (glow): a single binary, no runtime dependencies.

[简体中文](README_zh-CN.md)

## Features

- **Deploy / uninstall patches**: copy (or remove) the three target DLLs — `OpenSteamTool.dll`, `dwmapi.dll`, `xinput1_4.dll` — in your Steam directory
- **Slim main page**: deploy status plus the action button group (apply & launch / launch / exit & uninstall / uninstall & restart / restart Steam); a one-line health warning appears only when the compatibility probe reports "not yet supported upstream" or "core DLLs not found" (click to open Settings → Steam)
- **First-run setup wizard**: three steps (language → Steam path → optional patch download); skippable at any step, re-runnable from Settings
- **Two-tab settings dialog**: General (language, About with app update check, patch update check, re-run wizard) and Steam (path editor + compatibility probe)
- **Patch update maintenance**: in Settings → General → Patch Update Check — check, then download & extract the new version when one is available; results are shown without patch version numbers
- **App update check**: in Settings → General → About — check only; opens the download page when a new release exists (no self-update)
- **Compatibility probe**: hashes Steam's core DLLs and checks upstream signatures (pattern / IPC channels); the health badge, auto/manual precache, and details live in Settings → Steam
- **Auto-detect Steam path**: resolved from the registry, or pick the folder manually
- **Steam-aware window**: hides to the system tray when Steam starts, restores on exit; hides automatically after launch operations
- **Tray**: left-click toggles visibility; menu has Show, Restart Steam, "Minimize to tray automatically", Quit
- **Bilingual UI**: Chinese or English, chosen by system locale, switchable at runtime

## Usage

1. Download the latest ZIP from [Releases](../../releases) and extract it anywhere
2. Run `opensteamtool-manager.exe` (portable, no install)
3. First run opens the setup wizard: pick your language and Steam path, then optionally download the patch DLLs (skip any time — you can do it later)
4. Click "Apply Patch & Launch Steam". If the patch DLLs aren't downloaded yet, follow the hint to Settings → General → Patch Update Check, then download & extract

Settings persist in `config.toml` next to the executable, so the whole folder moves with you. Patches live in a `dlls/` folder next to the executable. The app starts without any loading screen; all operations run on background threads so the UI never freezes.

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

Patch, Deploy, Uninstall, Action, Restart, Local Version / Online Version (internal-only), Setup Wizard, App Update Check, Patch Update Check, Auto-tray, Minimize-to-Tray — definitions in [GLOSSARY.md](GLOSSARY.md).

## Source layout

```text
src/
├── main.rs        # eframe entry point
├── config.rs      # portable app config (config.toml next to exe, ADR-0012)
├── wizard.rs      # first-run setup wizard state machine (ADR-0013)
├── steam.rs       # registry path detection, steam.exe launch
├── steam_state.rs # shared Steam process table (alive / group_running / kill)
├── process.rs     # Steam process monitor (2s polling cache, edge events)
├── dll.rs         # target DLL deploy/uninstall, local status
├── workflow.rs    # action planning and step execution (plan/execute)
├── busy.rs        # busy gate: exclusive interactive background ops (ADR-0007)
├── updater.rs     # patch & app update checks, download & extract
├── update_flow.rs # single source of truth for update-check results (ADR-0008)
├── compat.rs      # compatibility probe: hashing / mirror chain / precache
├── compat_flow.rs # compatibility probe orchestration state machine (ADR-0006)
├── fsutil.rs      # shared atomic file write
├── tray.rs        # system tray
├── i18n.rs        # bilingual strings and error→copy mapping (ADR-0009)
└── ui.rs          # egui UI (main page, settings dialog, wizard rendering)
```

Spec: archived as GitHub issues [#18](https://github.com/hu3rror/opensteamtool-gui-rs/issues/18)–[#23](https://github.com/hu3rror/opensteamtool-gui-rs/issues/23) (SPEC.md removed from repo).
