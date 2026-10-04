<div align="center">
  <img src="assets/logo.png" width="96" alt="OpenSteamTool Manager logo">

  # OpenSteamTool Manager

  Deploy, remove, and update OpenSteamTool patches for Steam — a portable native Windows GUI.

  [![CI](https://img.shields.io/github/actions/workflow/status/hu3rror/opensteamtool-gui-rs/ci.yml)](https://github.com/hu3rror/opensteamtool-gui-rs/actions)
  [![Release](https://img.shields.io/github/v/release/hu3rror/opensteamtool-gui-rs)](https://github.com/hu3rror/opensteamtool-gui-rs/releases)
  [![License: MIT](https://img.shields.io/github/license/hu3rror/opensteamtool-gui-rs)](LICENSE)

  [简体中文](README_zh-CN.md)

</div>

A single-binary Windows tool that manages the OpenSteamTool patch set (three target DLLs) in your Steam install: deploy, uninstall, and keep them up to date — with launch and shutdown flows around the patch. Built with Rust and egui/eframe (glow); no installer, no runtime dependencies.

## Features

- **One-click actions** — apply the patch and launch Steam in one action; plain launch, exit & uninstall, uninstall & restart, and restart Steam, with confirmation only where it matters
- **Deploy status at a glance** — a status card shows deployed / not deployed / invalid path, plus a one-line health warning when the compatibility probe reports "not yet supported upstream" or "core DLLs not found" (click to open Settings → Steam)
- **First-run setup wizard** — four steps (language → theme → Steam path → optional patch download), skippable at any step and re-runnable from Settings
- **Three-tab settings dialog** — General (language, theme), Steam (path editor + compatibility probe), and About (app update check, re-run wizard); every change saves instantly
- **Patch update check on the main page** — check, then download & extract a new version when one is available; patch version numbers stay out of the UI
- **Light / dark themes** — follow the system or pick manually (Settings → General)
- **Compatibility probe** — hashes Steam's core DLLs and checks upstream signatures (pattern / IPC channels), with auto- and manual precache
- **Steam-aware window** — auto-hides to the tray when Steam starts, restores on exit; "minimize to tray" preference is persisted
- **Single instance** — launching the app again never opens a second window; it brings the existing one (even hidden in the tray) back to the foreground
- **Tray controls** — left-click toggles visibility; menu has Show, Restart Steam, "Minimize to tray automatically", and Quit
- **Portable by design** — settings live in `config.toml`, patches in `dlls/`, both next to the executable; copy the folder and it just works
- **Bilingual UI** — Chinese or English, picked from the system locale and switchable at runtime
- **Compact default window** — opens at the minimum size and only grows when the content needs more room

## Quick start

1. Download the latest ZIP from [Releases](../../releases) and extract it anywhere.
2. Run `opensteamtool-manager.exe` — no installation.
3. The first run opens the setup wizard: pick a language, a theme, and your Steam path, then optionally download the patch DLLs (skip any step, do it later).
4. Click **Apply Patch & Launch Steam**. If the patch isn't downloaded yet, use the patch-update check button on the main page, then download & extract.

> [!TIP]
> Everything is portable: `config.toml` (settings) and `dlls/` (patch files) sit next to the exe — move the whole folder to another machine and it just works.

## Build

Requires Rust (edition 2024) and the MSVC toolchain.

```sh
cargo build --release
# Output: target/release/opensteamtool-manager.exe (~6.8 MB)
```

Package the portable ZIP (same script used locally and in CI; requires PowerShell 7+, `pwsh`):

```sh
pwsh -File tools/build-release.ps1 -Version <version>
```

Tests:

```sh
cargo test
```

## Release

Pushing a `v*` tag builds, tests, packages, and publishes a GitHub Release with the portable ZIP (see `.github/workflows/release.yml`):

```sh
git tag v1.0.0
git push origin v1.0.0
```

## Source layout

```text
src/
├── main.rs        # eframe entry point
├── config.rs      # portable app config (config.toml next to exe, ADR-0012)
├── wizard.rs      # first-run setup wizard state machine (ADR-0013)
├── steam.rs       # registry path detection, steam.exe launch
├── steam_state.rs # shared Steam process table (alive / group_running / kill)
├── process.rs     # Steam process monitor (2s polling cache, edge events)
├── singleton.rs   # single instance: named-mutex gate + wake event to raise existing window (ADR-0017)
├── dll.rs         # target DLL deploy/uninstall, local status
├── workflow.rs    # action planning and step execution (plan/execute)
├── busy.rs        # busy gate: exclusive interactive background ops (ADR-0007)
├── updater.rs     # patch & app update checks, download & extract
├── update_flow.rs # single source of truth for update-check results (ADR-0008)
├── compat.rs      # compatibility probe: hashing / mirror chain / precache
├── compat_flow.rs # compatibility probe orchestration state machine (ADR-0006)
├── fsutil.rs      # shared atomic file write
├── theme.rs       # palette & visual assembly (ADR-0010/0016)
├── main_page.rs   # main-page view-model derivation & icon painting
├── tray.rs        # system tray
├── i18n.rs        # bilingual strings and error→copy mapping (ADR-0009)
└── ui.rs          # egui UI (main page, settings dialog, wizard rendering)
```

Design decisions are recorded in `docs/adr/` (ADR-0001–0017); domain terms are defined in [GLOSSARY.md](GLOSSARY.md); the locked UI spec and interactive prototype live in `docs/design/`.
