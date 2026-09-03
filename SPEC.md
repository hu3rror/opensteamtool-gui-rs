# OpenSteamTool Manager — Rust 重写规格书（SPEC）

> 本文档是 `opensteamtool-gui-rs`（原 `opensteamtool-gui-py`）Rust 重写的**唯一权威需求来源**。
> 开发前必读；功能以本文档为准，与旧 Python 仓库的差异以本文档为准。

## 0. 项目背景

原项目 `opensteamtool-gui-py` 是 Windows 下的 Steam 补丁管理工具（Python + tkinter，单文件 795 行，PyInstaller `--onefile` 打包）。用于部署/卸载 OpenSteamTool 的 DLL 到 Steam 目录，并支持在线检查更新。

**重写动机（用户原话）**：
- 启动时整个画面有「加载感」，启动太慢，希望越快越好、无加载感。
- 用户质疑 Tauri 太重（WebView2 内存 100-250MB）。
- 期望现代方式实现，更快启动与运行。

**最终技术选型（已确认）**：egui/eframe（纯 Rust 原生 GUI），`glow` 渲染后端。无 Node、无 WebView2、无 Python。

## 1. 技术栈决策

| 项 | 决策 | 理由 |
|---|---|---|
| GUI 框架 | egui / eframe | 即时模式，原生窗口，启动毫秒级首帧；适合本工具（3 卡片 UI）的轻量规模 |
| 渲染后端 | `glow`（OpenGL） | 比 wgpu 更小更轻，无需高性能渲染；无 GPU 时软件渲染兜底 |
| 语言 | Rust（工具链已验证：rustc/cargo 1.98 就绪） | 单二进制，无运行时依赖 |
| 后端逻辑 | 全部 Rust 重写（无 Python sidecar） | 避免 sidecar 启动开销，重燃加载感 |
| 二进制体积 | 目标 1-3MB 单 exe | 远小于 PyInstaller ~15MB |
| 内存占用 | 目标 ~50-90MB | 远小于 Tauri/WebView2 |
| 启动目标 | < 1s，无解压、无白屏、无加载感 | 核心诉求 |

## 2. 功能需求（1:1 保留现有功能 + 修复痛点）

### 2.1 核心配置

- 目标 DLL：`OpenSteamTool.dll`、`dwmapi.dll`、`xinput1_4.dll`
- 线上源：`https://api.github.com/repos/OpenSteam001/OpenSteamTool/releases/latest`
- 本地版本记录：`dlls/version.txt`（文本，如 `1.4.8`）
- 本地 DLL 存放：exe 同目录 `dlls/` 文件夹（便携版，解压即用）

### 2.2 Steam 安装路径检测

按顺序尝试注册表，返回第一个有效且 `os.path.exists` 的路径：

1. `HKEY_CURRENT_USER\Software\Valve\Steam` → `SteamPath`
2. `HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Valve\Steam` → `SteamPath`
3. `HKEY_LOCAL_MACHINE\SOFTWARE\Valve\Steam` → `SteamPath`

Rust 侧用 `winreg` crate 等价实现。

### 2.3 本地部署状态检测

- 若路径为空或非目录 → 状态「请先指定有效的 Steam 安装路径」
- 若 Steam 目录下三个目标 DLL 全部存在 → 状态「已应用」
- 否则 → 状态「未应用」

### 2.4 部署 / 卸载

- **部署**：从 `dlls/` 复制三个 DLL 到 Steam 目录；创建 `Steam 目录\config\lua` 目录；权限不足报错。
- **卸载**：删除 Steam 目录下的三个目标 DLL；权限不足报错。
- 两操作都需先处理「Steam 正在运行」的情况（见 2.5）。

### 2.5 Steam 进程管理（修复重点）

原实现痛点：
- `_is_steam_running()` 每次 spawn `tasklist` 子进程 → 阻塞 UI 数百 ms，且多按钮重复调用。
- `_kill_steam()` 用 `taskkill /F /IM steam.exe` + 最多轮询 10×0.5s = 5s 等待。

Rust 修复方案：
- 用 `sysinfo` crate 枚举进程检测 `steam.exe`（无子进程开销），单次结果缓存/事件驱动。
- kill 用 `taskkill` 或 `sysinfo::kill`，轮询间隔可缩短（如 10×0.1s）。
- 检测结果供多个操作复用，避免重复扫描。

流程（与旧版一致）：
- 操作前若 Steam 在运行 → 弹窗询问「是否自动关闭 Steam 并继续」→ 拒绝则取消；同意则关闭，关闭失败报错。

### 2.6 Steam 启动

- 校验 `Steam 目录\steam.exe` 存在，以 Steam 目录为 cwd 启动；不存在报错。

### 2.7 在线检查更新

- GET GitHub API，`User-Agent` 需带浏览器标识。
- 解析 `tag_name`（去前缀 `v`）；在 `assets` 中找第一个 `.zip` 的 `browser_download_url`。
- 无 zip 资产报错。
- 网络请求必须放后台线程，UI 不冻结（egui 用 `spawn`/channel 回传 UI 线程）。

### 2.8 下载并解压新版本

- 下载 zip 到内存，仅提取文件名属于目标 DLL 集合的成员写入 `dlls/`；成功则写 `version.txt`。
- 下载超时 30s、请求超时 10s 可保留或略调。
- 后台执行，进度/完成回传 UI。

### 2.9 双语切换

- 自动检测系统语言：`GetUserDefaultUILanguage` 返回 `0x0804`/`0x1004`（简体中文）→ 默认中文，否则英文；备选 `locale`。
- 手动切换按钮：中 ↔ 英，切换后更新全部文案。

### 2.10 UI 布局（3 卡片）

外观对齐 Python 版（配色/卡片 accent bar/独立操作区，见 ADR-0003）；窗口可缩放并设最小尺寸，首帧按内容自适应高度（不再固定 560×470）：

1. **卡片 1 — STEAM 安装路径**：输入框（可编辑，变更即刷新状态）+ 「浏览...」按钮（目录选择器）。
2. **卡片 2 — 本地应用状态**：状态纯文本（【已应用】/【未应用】/路径无效，效仿 Python 版）。
3. **独立操作区**（卡片 2 与卡片 3 之间，两个等宽主按钮，随状态切换）：
   - 未部署：左（绿）「▶ 应用补丁并启动 Steam」/ 右（白）「▶ 正常启动 Steam」
   - 已部署：左（浅蓝描边）「◀ 退出 Steam 并卸载补丁」/ 右（蓝）「◀ 卸载补丁并重启 Steam」
4. **卡片 3 — 在线版本更新**：本地版本显示 + 线上版本状态 + 「检查更新」按钮 + 「下载并解压新版本」按钮（有可更新版本时出现）。

顶栏：标题 + 语言切换按钮。窗口图标用现有 `app.ico`。

### 2.11 已确认的修复清单（对比旧版）

- [x] 消除 onefile 解压（换技术栈后天然解决）
- [x] 进程检测改用 sysinfo（`src/process.rs`），无 tasklist 子进程开销，结果 2s 缓存 + 事件驱动复用
- [x] kill 后轮询 10×100ms（原 5s），未退出时明确报错
- [x] 输入框变更即时刷新状态；Steam 进程状态 2s 定时刷新（事件驱动增量，非全量重建）
- [x] UI 无加载感、毫秒级首帧（egui 原生窗口，无预热/白屏）

## 3. 发布形态

- **便携版**：`cargo build --release` 单 exe + 同目录 `dlls/`，ZIP 分发，解压即用。
- exe 与 `dlls/` 的相对位置即运行时 DLL 源目录。
- 图标 `app.ico` 嵌入窗口与任务栏。

## 4. 模块划分建议

```
src/
├── main.rs          # eframe 入口，App 装配
├── steam.rs         # 注册表路径检测、steam.exe 启动
├── process.rs       # 进程检测/监视/关闭（sysinfo）
├── dll.rs           # 部署/卸载、本地状态检测
├── updater.rs       # GitHub API 检查、下载、解压、version.txt
├── workflow.rs      # 操作判定表与执行（plan/execute）
├── tray.rs          # 系统托盘（左键显隐切换、菜单）
├── i18n.rs          # 双语文案、系统语言检测
└── ui.rs            # egui 界面（3 卡片 + 顶栏）
```

依赖候选：`eframe/egui`、`winreg`、`sysinfo`、`ureq`（轻量 HTTP）或 `reqwest`、`zip`、`image`（图标转 rgba）。
已定依赖（`Cargo.toml`）：`eframe`（glow）、`winreg`、`sysinfo`、`ureq`（json+rustls）、`zip`、`windows-sys`、`serde_json`、`rfd`（目录选择器）、`image`（ico→rgba，仅 `ico` feature）。release 用 `opt-level="z"` + fat LTO + `panic="abort"`，`--release` 体积约 6.7MB。

## 5. 验证标准

- 启动 < 1s、无加载感（首帧即见完整 UI 骨架）。
- 三 DLL 部署/卸载到 Steam 目录成功。
- Steam 运行中操作弹窗询问、自动关闭成功。
- 在线检查更新/下载解压/写 version.txt 成功。
- 中英文切换即时生效。

## 6. 备注

- Git 历史：新仓库 `git init` 全新开始（用户已确认不保留旧仓库历史）。
- 旧仓库 `opensteamtool-gui-py` 冻结，不再改动。
- 本 spec 若与实际开发冲突，以开发中最新决策为准并回更本文档。
