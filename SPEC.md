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
- kill 用 `sysinfo::kill` 终止整个 Steam 进程组（exe 路径位于 Steam 目录下、排除 `steamservice.exe`），轮询等整组从进程表消失（预算 5s、间隔 100ms，见 ADR-0004）。
- 检测结果供多个操作复用，避免重复扫描。

流程（与旧版一致）：
- 操作前若 Steam 在运行 → 弹窗询问「是否自动关闭 Steam 并继续」→ 拒绝则取消；同意则关闭，关闭失败报错。

### 2.6 Steam 启动

- 校验 `Steam 目录\steam.exe` 存在，以 Steam 目录为 cwd 启动；不存在报错。
- spawn 成功不算成功：等待 2s 确认 `steam.exe` 存活，失败重试 1 次后报错（见 ADR-0004）。

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
- [x] kill 整个 Steam 进程组（路径过滤、排除 `steamservice.exe`）后轮询等整组消失（预算 5s、间隔 100ms，原仅 steam.exe + 1s），未退出时明确报错
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
├── compat.rs        # Steam 核心版本健康度体检（哈希/远程探针/缓存，见 §7）
├── workflow.rs      # 操作判定表与执行（plan/execute）
├── tray.rs          # 系统托盘（左键显隐切换、菜单）
├── i18n.rs          # 双语文案、系统语言检测
└── ui.rs            # egui 界面（3 卡片 + 顶栏）
```

依赖候选：`eframe/egui`、`winreg`、`sysinfo`、`ureq`（轻量 HTTP）或 `reqwest`、`zip`、`image`（图标转 rgba）。
已定依赖（`Cargo.toml`）：`eframe`（glow）、`winreg`、`sysinfo`、`ureq`（json+rustls）、`zip`、`windows-sys`、`serde_json`、`rfd`（目录选择器）、`image`（ico→rgba，仅 `ico` feature）；§7 新增 `sha2`（0.10）。release 用 `opt-level="z"` + fat LTO + `panic="abort"`，`--release` 体积约 6.7MB。

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

## 7. Steam 核心版本健康度体检与 Pattern/IPC 缓存管理

> 本节为新增功能规格（2026-09-05 确立，经代码/上游仓库双向核对修正）。
> 上游事实基准：`OpenSteam001/steam-monitor` 三分支 `pattern` / `ipc` / `protobuf`（默认），
> `{channel}` 即**分支名**；`pattern` 分支含 `steamclient/*.toml`（特征码）与 `steamui/*.toml`，
> `ipc` 分支仅含 `steamclient/*.toml`（IPC 规约）。

### 7.1 概述

OpenSteamTool（上游）自重构后不再将硬编码特征码打包进 DLL，而是在每次由注入器随 Steam 启动时，计算本地核心 DLL 的 SHA-256 哈希，并按通道从 `OpenSteam001/steam-monitor` 拉取匹配的 TOML 签名文件与 IPC 规约。

本项目需在 **Card 1（Steam 安装路径卡片）底部**新增版本健康度诊断指示器：在用户指定或自动识别 Steam 路径后，后台异步计算核心文件哈希并探查远程/本地缓存适配情况，在不阻塞 UI 的前提下提供兼容性状态指示及一键离线缓存预热。

### 7.2 架构对齐（AGENTS.md 约束落地）

1. **UI 零阻塞**：禁止在 `eframe::App::update` 内做 DLL 读取、SHA-256 计算与网络请求；复用现有 `ui.rs` 的 `spawn()`（`std::thread::spawn` + mpsc）+ `handle_messages()`（`rx.try_recv` 轮询）机制回传状态（`Msg` 枚举新增变体）。
2. **零白屏 / 零等待**：启动立即渲染，体检初始为 `Checking` 骨架态，完成后就地刷新徽标。
3. **单一二进制与依赖控制**：
   - 哈希：`sha2 = "0.10"`（**需新增**，Cargo.toml 现无此依赖），缓冲区分块流式读取，避免大文件爆内存。
   - HTTP：**复用项目现有 `ureq 3.4`**（同步、rustls；spec 原拟 reqwest 已否决，不引入新依赖）。
     ureq 语义注意：`agent.head(url).call()` 对 2xx 返回 `Ok(Response)`；**4xx 返回 `Err(ureq::Error::StatusCode(u16))`**（非响应对象），404 判定为 `matches!(err, Error::StatusCode(404))`。
4. **严格双语**：所有新增状态文本、Tooltip、弹窗词条在 `src/i18n.rs` 登记 `en` 与 `zh-CN`（`Strings` struct 加字段 + `zh()`/`en()` 各赋值一行）。
5. **用户意愿优先**：优先解析 `<Steam>/opensteamtool.toml` 的 `[remote].url_template`；注意模板语义为**替代**（custom mirror replaces built-in sources），即自定义模板存在时不再回退 GitHub/jsDelivr。

### 7.3 核心文件与通道映射

| 探测目标 | 本地文件 | SHA-256 来源 | Channel（=分支） | Component | 本地缓存路径 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| SteamClient Pattern | `<Steam>/steamclient64.dll` | `steamclient64.dll` | `pattern` | `steamclient` | `<Steam>/opensteamtool/pattern/steamclient/<sha256>.toml` |
| SteamUI Pattern | `<Steam>/steamui.dll` | `steamui.dll` | `pattern` | `steamui` | `<Steam>/opensteamtool/pattern/steamui/<sha256>.toml` |
| SteamClient IPC | `<Steam>/steamclient64.dll` | `steamclient64.dll` | `ipc` | `steamclient` | `<Steam>/opensteamtool/ipc/steamclient/<sha256>.toml` |

哈希输出：标准 64 位小写十六进制（如 `3f864358fcf5...`；该前缀在线上 `pattern/steamclient/` 真实存在）。

### 7.4 远程探针链路（Mirror Chain）

URL 占位符统一为 `{channel}` / `{component}` / `{sha256}`，解析顺序：

1. **用户自定义（替代，非追加）**：`<Steam>/opensteamtool.toml` 中 `[remote].url_template` 非空则用之；键默认注释状态，文件不存在/无键/空值 → `None` → 回退。
2. **官方 GitHub Raw**：`https://raw.githubusercontent.com/OpenSteam001/steam-monitor/{channel}/{component}/{sha256}.toml`（`{channel}` 即分支名）。
3. **jsDelivr CDN 回退**：`https://fast.jsdelivr.net/gh/OpenSteam001/steam-monitor@{channel}/{component}/{sha256}.toml`。

### 7.5 状态判定决策矩阵

探针用独立 `ureq::Agent`：`timeout_connect(4s)` + `timeout_global(5s)`（`updater.rs` 现有 agent 是 10s/30s，不适合探针），`HEAD` 请求。

```text
[检查本地核心 DLL 是否存在]
    ├── 不存在 -> FileNotFound
    └── 存在 -> 计算 SHA-256
                   │
                   ├── [检查本地缓存文件] 存在 -> Cached = true / 缺失 -> Cached = false
                   │
                   └── [HEAD 探测 url_template -> GitHub -> jsDelivr]
                           ├── HTTP 2xx -> RemoteAvailable
                           ├── Err(StatusCode(404))
                           │       ├── Cached == true  -> CompatibleOffline
                           │       └── Cached == false -> IncompatiblePending
                           └── 网络超时/连接错误
                                   ├── Cached == true  -> CompatibleOffline
                                   └── Cached == false -> NetworkError
```

综合健康度：
- **Fully Compatible**：3 项探针均 `RemoteAvailable` 或 `CompatibleOffline`，且三项均有本地缓存。
- **Available Online**：上游已适配但本地缺缓存（提示一键预热）。
- **Pending Upstream**：任意探针 404 且本地无缓存。
- **Error / Missing**：Steam 路径错误或核心 DLL 缺失。


**快速体检短路（增强）**：`probe_all`（启动/路径变更入口）**完全零网络**——
`Cached == true` 直接判定 `CompatibleOffline`（有缓存的机器启动即绿）；
`Cached == false` 乐观假定 `RemoteAvailable{cached:false}`（琥珀「上游已适配 (未缓存)」+ 预热按钮可点，
无缓存启动即离开 Checking）。`Checking` 仅存续于本地哈希阶段（steamclient64.dll 哈希去重，Pattern/IPC 复用）。
网络适配状态由 `probe_all_refresh` 后台异步补齐（快速报告含短路/乐观项即触发一次全量网络探测：
短路项可升级 `RemoteAvailable{cached:true}`，乐观项 404 时降级 `IncompatiblePending`，汇总徽章随之更新）。
探针超时 4s/5s 收紧至 2.5s/2.5s，无网络时快速失败。

**验证缓存（增强）**：后台刷新确认适配（Found）的项持久化到 `<exe>/cache/verified.toml`
（`{target → sha256}` 映射，原子写）。下次快速体检：签名缓存命中 → `CompatibleOffline`（绿）；
验证缓存命中 → `RemoteAvailable{cached:true}`（绿，零网络）；均未命中 → 乐观琥珀。
缓存由 DLL 哈希自动失效：Steam 更新改哈希即重验，无时间有效期；只记 Found（404/错误不缓存，
避免上游补发签名后本地仍失真）。
### 7.6 模块设计（`src/compat.rs`）

`main.rs` 注册 `mod compat;`。核心数据结构（与既有 `UpdateError::Network` 风格一致）：

```rust
pub enum ProbeTarget { PatternSteamClient, PatternSteamUi, IpcSteamClient }
// channel() -> "pattern"/"pattern"/"ipc"; component() -> "steamclient"/"steamui"/"steamclient"
// relative_dll() -> "steamclient64.dll"/"steamui.dll"/"steamclient64.dll"

pub enum ProbeStatus {
    Checking,
    RemoteAvailable { cached: bool },
    CompatibleOffline,
    IncompatiblePending,
    NetworkError(String), // 网络超时/连接错误（含 URL 解析失败）
    FileNotFound,
}

pub struct ProbeReport { pub target: ProbeTarget, pub sha256: Option<String>, pub status: ProbeStatus, pub cache_path: PathBuf }
pub struct OverallHealthReport { pub steamclient_pattern: ProbeReport, pub steamui_pattern: ProbeReport, pub steamclient_ipc: ProbeReport, pub is_all_compatible: bool, pub has_missing_cache: bool }
```

**配置读取**：`config_editor.rs` 现无读取函数（仅 `validate()`/`write_atomic()`），需**新增** `pub fn remote_url_template(steam_dir: &Path) -> Option<String>`：`toml_edit` 解析 `<Steam>/opensteamtool.toml`，取 `[remote].url_template` 非空字符串；文件缺失/解析失败/键空 → `None`。

**功能清单**：
- `sha256_of_file(path) -> io::Result<String>`：`std::fs::File` + 64KB 缓冲分块流式 `Sha256`。
- `cache_path(steam_dir, target) -> PathBuf`：`<Steam>/opensteamtool/{channel}/{component}/` 目录存在性检查。
- `probe_all(steam_dir) -> OverallHealthReport`：三探针循环，先本地（哈希/缓存）后网络（镜像链 HEAD）。
- `precache(steam_dir, target, report) -> Result<(), CompatError>`：GET 拉取 TOML（复用 `download_agent` 超时模式），`fsutil::write_atomic` 原子写入缓存路径（先建目录）。

### 7.7 UI 布局与交互（`src/ui.rs`）

**落位**：`card1()`（路径输入 + 浏览按钮下方）新增「Steam 核心兼容性」小节。注意：DLL 部署状态在 Card 2，Card 1 内无既有状态列表。

```text
┌─────────────────────────────────────────────────────────────┐
│ Card 1: Steam 安装路径                                      │
│ [路径输入框....................] [浏览...]                   │
│ ─────────────────────────────────────────────────────────── │
│ Steam 核心兼容性: [ ● 已全面适配 ]      [ 详细信息 / 预热缓存 ]│
└─────────────────────────────────────────────────────────────┘
```

**状态视觉**：
1. `Checking`：灰色 `[ ○ 检查中... ]`。
2. `Fully Compatible`（含 `RemoteAvailable{cached:true}` 与 `CompatibleOffline` 全绿）：绿色 `[ ● 完美兼容 ]`，Tooltip「核心特征码与 IPC 规约已全部就绪并离线缓存」。
3. `RemoteAvailable{cached:false}`：黄/蓝绿 `[ ● 上游已适配 (未缓存) ]` + 小按钮 `[ 预热离线缓存 ]`。
4. `IncompatiblePending`：红色 `[ ▲ 暂未适配 ]`，Tooltip「Steam 版本已更新，上游尚未发布匹配签名」。
5. `FileNotFound`：灰色 `[ ? 未找到核心文件 ]`。
6. `NetworkError`：灰/橙 `[ ? 网络不可用 ]`，cached 时显示离线可用。

**详情与预热交互**：
- 「详细信息」展开/弹窗展示 3 行明细：`steamclient64.dll`（Pattern）、`steamui.dll`（Pattern）、`steamclient64.dll`（IPC），每行 = SHA-256 前 12 位 + 状态徽标。
- 有 `RemoteAvailable{cached:false}` 项时显示 `[ 一键缓存签名 ]`：后台线程 GET 下载 → 建 `<Steam>/opensteamtool/{channel}/{component}/` → 原子写入 `<sha256>.toml` → 重跑本地探针刷新状态。
- **自动预热（增强）**：体检落定为 Online（上游已适配未缓存）且无预热进行中时，自动触发同一下载链路——Steam 更新后首次启动零点击补缓存；自动失败静默（不弹错误、徽章保持 Online、手动入口保留），成功复用「缓存已就绪」提示，无新增文案。预热目标以签名缓存文件实际存在性为准（含验证缓存命中但未下载的情形）。
- **触发防抖**：Steam 路径输入逐字符 `resp.changed()` 触发刷新——体检线程仅当路径与上次体检路径不同才 spawn（防每字符起线程）。

### 7.8 国际化词表（`src/i18n.rs`）

| 键名 | en | zh-CN |
| :--- | :--- | :--- |
| `compat_title` | Steam Core Compatibility | Steam 核心兼容性 |
| `compat_checking` | Checking compatibility... | 正在检查兼容性... |
| `compat_status_ready` | Fully Compatible | 完美兼容 (已缓存) |
| `compat_status_online` | Supported (Not Cached) | 上游已适配 (未缓存) |
| `compat_status_offline` | Compatible (Offline Cache) | 离线可用 (使用本地缓存) |
| `compat_status_pending` | Unsupported (Pending) | 上游尚未适配此版本 |
| `compat_status_missing` | DLLs Not Found | 未找到核心 DLL |
| `compat_status_network` | Network Unavailable | 网络不可用 |
| `compat_btn_precache` | Pre-cache Signatures | 预热离线缓存 |
| `compat_btn_details` | Details | 详细信息 |
| `compat_btn_precache_all` | Pre-cache All Signatures | 一键缓存签名 |
| `compat_precaching` | Downloading... | 正在缓存... |
| `compat_precache_done` | Cache pre-warmed | 缓存已就绪 |
| `compat_precache_failed` | Pre-cache failed: {err} | 缓存预热失败：{err} |
| `compat_tip_pending` | Steam has been updated. Please wait for upstream signatures. | Steam 版本已更新，上游尚未发布适配签名，请等待更新。 |
| `compat_tip_ready` | Core signatures & IPC specs ready and cached offline. | 核心特征码与 IPC 规约已全部就绪并离线缓存。 |
| `compat_tip_online` | Supported upstream — pre-cache now for offline use. | 上游已适配，可一键预热离线缓存。 |
| `compat_tip_missing` | Core DLLs not found (steamclient64.dll / steamui.dll). | 未找到核心 DLL（steamclient64.dll / steamui.dll）。 |
| `compat_tip_network` | Network unavailable — results unknown; cached items remain usable offline. | 网络不可用，体检结果未知；已缓存项仍可离线使用。 |
| `compat_row_dll` | {dll} ({kind}) | {dll}（{kind}） |

### 7.9 实现清单与验收

1. **依赖**：`Cargo.toml` 增 `sha2 = "0.10"`（ureq 已存在，复用）。
2. **`src/compat.rs`**：哈希（流式）→ 配置读取（`config_editor::remote_url_template`）→ 单次探测（本地缓存 + HEAD 镜像链）→ `OverallHealthReport` → 预热下载持久化。
3. **集成**：`Msg` 增 `Compat(Result<OverallHealthReport, ...>)` / `CompatPrecached(...)`；`App` 持有 `compat_report` 与触发状态；`card1()` 底部渲染；路径变更防抖触发；预热按钮异步调用。
4. **i18n**：§7.8 词表完整登记 `en`/`zh-CN`。
5. **测试**：
   - `compat.rs`：临时目录伪造 DLL（写入已知字节）+ 伪造缓存目录，验证哈希、缓存命中、`FileNotFound` 判定、URL 构造（三占位符替换、自定义模板替代语义）。
   - 网络探针不依赖真实网络：HEAD 判定逻辑拆为纯函数（输入镜像链结果枚举 → 输出 `ProbeStatus`）单测覆盖决策矩阵全分支。
   - `config_editor`：`remote_url_template()` 覆盖文件缺失/无键/空值/有效值/自定义模板。
6. **构建验收**：`cargo check` / `cargo test` 无错误（既有 `cargo fmt --check` 噪音与 1 条 clippy warning 与本功能无关，勿动）。

# 8. UI 架构重构与交互规范

> 本节定义主界面拓扑、设置弹窗、OnlineFix 感知与存储双模的架构重构。决策经 2026-09-05 grill-with-docs 三轮确认（见 §8.2 决策记录），是对既有 UI（§2.9/§2.10/§7.7）的修订，冲突处本节优先。

## 8.1 Context & Motivation

1. **主界面视线断裂与职责倒挂**：原「本地应用状态卡片」仅承载单行状态文本，与下方裸露的「操作按钮区」逻辑强绑定但视觉割裂；「在线版本更新卡片」占据近 1/4 视窗，把低频辅助功能置于主启动动线之下；顶栏缺源码仓库入口，语言切换按钮常驻顶栏造成噪音。
2. **设置弹窗交互混乱**：弹窗内嵌页签专属按钮区与 Modal 通用底栏（`btn_close`）上下并存，双层操作栏分散焦点；弹窗仅含高级配置（TOML 编辑与 OnlineFix），缺乏软件自身全局偏好（语言、托盘行为）入口；编辑器滚动区高度依赖固定魔法常数（`CFG_FIXED_H = 300.0`、`OF_FIXED_H = 340.0`），缩放适应性差。
3. **OnlineFix 启动预设感知盲区**：候选仅从 Lua 脚本扫出纯数字 AppID，未结合本地清单（`appmanifest_<appid>.acf`）解析游戏名称；未反查当前生效的 OnlineFix 游戏（现状仅「选中 (账号, AppID) 逐项查询」），用户无法获知当前激活状态。
4. **存储与路径权限脆弱性**：配置、核心资产与临时验证缓存混杂于 `<exe>` 同级或 `<exe>/cache/`；在 Program Files 等 UAC 保护目录部署时写盘会 `PermissionDenied`；`gui_config.toml`（用户偏好）与 `cache/`（可清理缓存）分离后消除误伤风险。

## 8.2 决策记录（三轮 grilling 确认）

| # | 决策 | 结论 |
| :--- | :--- | :--- |
| R1 | 语言体系 | `LanguagePreference { Auto, Zh, En }` 三态，默认 `Auto`（= 跟随系统，即现状 `detect_system_lang`）；入口从顶栏移入常规偏好页 |
| R2 | GuiConfig 范围 | 本期仅 `{ language, minimize_to_tray }`，**不含** `custom_steam_path`（另立 ticket）；配置文件统一 `gui_config.toml` |
| R3 | 底部状态栏 | 与现状 `notice_bar` 合并**单行**：左 notice（busy/结果），右本地版本 + 检查更新；busy 时按钮禁用 |
| R4 | OnlineFix 启用入口 | 页签 Footer 左侧 `[复制参数][启用][停用]`，右侧 `[关闭]`（AC5 修订） |
| R5 | 术语 | UI 用口语化「最小化至托盘」（CONTEXT.md 词条同步，旧称「最小化隐身」废弃）；「自动隐身」不变 |
| R6 | effective_dll_dir | 「有效」= 更新目录三个目标 DLL 全齐；部署/卸载/版本读取统一走生效目录 |
| R7 | 生效游戏反查 | 只反查**当前选中账号**；进入页签自动同步 AppID 输入框，看板停用与 Footer 停用同指一操作 |
| R8 | 设置弹窗默认页 | 默认「常规偏好（General）」；会话内记忆上次页签保留 |
| R9 | 更新按钮文案 | 按钮文案改「立即更新」（i18n 新词条 `btn_update_now`）；流程术语「下载并解压」保留于 CONTEXT.md |
| R10 | 配置文件命名 | Portable 与 Installed 统一 `gui_config.toml`，仅目录不同（AC7 的 `config.toml` 为笔误修正） |
| R11 | 词表修订 | CONTEXT.md 全量修订：新增「补丁控制台」「当前生效游戏」「常规偏好」「便携模式/安装模式」 |
| R12 | spec 落盘 | 本节（SPEC.md §8）；分支 `feat/ui-redesign` |

## 8.3 领域模型（修订版）

```rust
use std::path::PathBuf;

/// 运行环境存储策略：便携模式 vs 系统规范模式
pub enum StorageMode {
    /// 便携模式：exe 同级目录可写，配置/缓存/DLL 资产均收拢于 exe 同级
    Portable,
    /// 安装模式：exe 同级只读（如 Program Files），配置/缓存回退至 AppData
    Installed,
}

/// 集中式路径解析器契约
pub trait PathResolver: Send + Sync {
    fn mode(&self) -> StorageMode;
    /// GUI 用户偏好配置文件（统一命名 gui_config.toml）
    fn config_path(&self) -> PathBuf;
    /// 临时验证缓存目录（verified.toml 所在目录）
    fn cache_dir(&self) -> PathBuf;
    /// 打包自带只读 DLL 目录（<exe>/dlls）
    fn bundled_dll_dir(&self) -> PathBuf;
    /// 在线更新 DLL 写入目标（Portable → <exe>/dlls；Installed → %LOCALAPPDATA%/OpenSteamTool/dlls）
    fn update_target_dll_dir(&self) -> PathBuf;
    /// 生效 DLL 目录：更新目录三个目标 DLL 全齐则优先，否则回退自带目录
    fn effective_dll_dir(&self) -> PathBuf;
}

/// GUI 全局偏好（gui_config.toml）；缺失/损坏 → 静默回退默认（Auto + 勾选）
pub struct GuiConfig {
    pub language: LanguagePreference,
    pub minimize_to_tray: bool,
}

pub enum LanguagePreference { Auto, Zh, En }

/// 解析后的游戏候选项（AppID + 本地名称映射；ACF 缺失时 name 为 None）
pub struct CandidateGame {
    pub appid: u32,
    pub name: Option<String>,
}

/// 当前生效的 OnlineFix 游戏（选中账号 localconfig.vdf 中带 -onlinefix 的 AppID）
pub struct ActiveOnlineFixGame {
    pub appid: u32,
    pub name: Option<String>,
}

/// 设置弹窗页签（默认 General，会话内记忆上次页签）
pub enum SettingsTab { General, ConfigEditor, OnlineFix }

/// 状态栏内联更新操作态
pub enum InlineUpdateStatus {
    Idle,
    Checking,
    UpToDate { version: String },
    UpdateAvailable { current: String, latest: String, zip_url: String },
    Downloading,
    Failed { reason: String },
}
```

## 8.4 系统边界（In-Scope）与非目标（Non-goals）

**In-Scope**：
- 顶栏右侧固定 `[ GitHub ]` + `[ ⚙ ]`（28×28）；移除语言切换按钮（入口移至常规偏好页）。
- 合并 Card 2 与操作区为「补丁控制台」；移除 Card 3，压缩为窗口底部固定单行状态栏（§8.6 AC3）。
- 设置弹窗三页签（常规偏好 / 配置编辑器 / OnlineFix 预设）+ 全局单行动态 Footer；根除内容区局部按钮条与滚动区魔法常数。
- OnlineFix：选中账号 `localconfig.vdf` 反查生效游戏（看板 + 快捷停用）；`appmanifest_<appid>.acf` 名称解析。
- 双模路径体系：全局 `PathResolver` 解析 `gui_config.toml`、`verified.toml`、`dlls/` 在便携/安装模式下的落点与层叠回退。

**Non-goals**（延续既有约束，均已在现行代码中落实）：
- 不引入 TOML 结构化表单控件（保持全文本编辑，兼容上游字段演进）。
- 禁止外部网络接口解析游戏元数据（ACF 缺失直接回退纯数字）。
- 禁止 UAC 提权（受保护目录无感回退 AppData）。
- 不引入重量级 VDF 解析库（保持自研行级保真处理）。
- 不扩展双语以外语种。
- 新增依赖最小化：浏览器调起用 `cmd /c start`（零依赖），不新增 crate。

## 8.5 测试接缝与隔离策略

```rust
/// 1. 环境只读探测接缝：解耦物理文件系统权限判定
pub trait EnvironmentProbe: Send + Sync {
    fn is_directory_writable(&self, path: &Path) -> bool;
    fn get_appdata_dir(&self) -> Option<PathBuf>;
    fn get_local_appdata_dir(&self) -> Option<PathBuf>;
    fn get_exe_dir(&self) -> Option<PathBuf>;
}

/// 2. 本地清单读取接缝：解耦物理磁盘 ACF 文件 IO
pub trait ManifestAccessor: Send + Sync {
    fn read_manifest_content(&self, steam_dir: &Path, appid: u32) -> Option<String>;
}

/// 3. 外部进程与命令分派接缝：默认浏览器调起（cmd /c start 实现）
pub trait ExternalSystemDispatcher: Send + Sync {
    fn open_browser_url(&self, url: &str) -> Result<(), String>;
}
```

**Mock 策略**：`MockEnvironmentProbe`（只读/可写路径）断言路径跳转；`MockManifestAccessor`（合规/畸变/缺失 ACF）断言候选解析；内存 VDF 字节串（多 AppID 混合/无标记/单标记/祖先链缺失）断言 `get_active_game`；`ctx.run_ui` 无头模式验证 Footer 按钮响应。

## 8.6 验收标准（修订版）

### AC 1: 顶栏导航与外部入口
- 顶栏右侧仅 `[ GitHub ]` + `[ ⚙ ]`（28×28 正方形，语言切换不引起布局抖动）；无 `[ EN / 中文 ]` 按钮。
- 点击 GitHub 经分派器调起默认浏览器访问 `https://github.com/hu3rror/opensteamtool-gui-rs`。

### AC 2: 补丁控制台合并卡片
- Card 2 与操作区合并为单一带框卡片；顶部为「部署状态」彩色 pill 徽章 + 「Steam 进程状态（运行中/未运行）」（复用 2s 轮询）。
- 未部署：`[ ▶ 应用补丁并启动 Steam ]`（强调）+ `[ ▶ 正常启动 Steam ]`（次要）；已部署：卸载类动作（沿用现状四态）。
- 按钮行与状态指示器处于同一卡片内边距作用域。

### AC 3: 底部状态栏与内联更新
- 窗口底部固定单行：左侧运行/执行提示（Notice，busy 或最近结果），右侧本地版本号 + 次要样式 `[ 检查更新 ]`。
- 可更新：右侧转为可更新提示 + 强调色主按钮 `[ 立即更新 ]`；点击后原地进入「正在下载...」禁用态，不弹窗、不拉起卡片。

### AC 4: 常规偏好与持久化
- 首次进入设置弹窗默认展示「常规偏好」页签（会话内记忆保留）。
- 含「界面语言」三选（自动检测/简体中文/English）与「最小化至托盘」复选框；变更即时生效并写盘 `gui_config.toml`。
- 冷启动自动还原，不被系统默认环境覆盖。

### AC 5: 消除设置弹窗双排按钮
- 各页签子视图内部不得渲染局部 `ui.horizontal` 按钮条；弹窗底部仅一条全局单行 Footer。
- General：Footer 仅右对齐 `[ 关闭 ]`。
- ConfigEditor：Footer 左 `[ 从示例模板创建 ][ 撤销 ]`，右 `[ 保存 ][ 关闭 ]`。
- OnlineFix：Footer 左 `[ 复制参数 ][ 启用 ][ 停用 ]`，右 `[ 关闭 ]`。
- 滚动区高度由可用空间动态计算，根除 `CFG_FIXED_H`/`OF_FIXED_H` 魔法常数。

### AC 6: OnlineFix 状态感知与名称映射
- 选中账号 `localconfig.vdf` 存在带 `-onlinefix` 的 AppID（如 1812150）：页签顶部以看板高亮「当前生效游戏」（AppID + 名称）并提供 `[ 停用 ]`；进入页签自动同步 AppID 输入框。
- `appmanifest_367520.acf` 存在且含名称：候选展示为 `[ 空洞骑士 (367520) ]` 胶囊；缺失安全回退 `[ 367520 ]`；点击胶囊填充输入框。

### AC 7: 存储路径双模自适应回退
- 可写目录：`mode()` 为 Portable，`config_path()` = `<exe>/gui_config.toml`，`cache_dir()` = `<exe>/cache/`。
- 只读目录：`mode()` 为 Installed，`config_path()` = `%APPDATA%\OpenSteamTool\gui_config.toml`，`cache_dir()` = `%LOCALAPPDATA%\OpenSteamTool\cache\`，更新 DLL 写 `%LOCALAPPDATA%\OpenSteamTool\dlls\` 无 `PermissionDenied`；部署/卸载/版本读取统一走 `effective_dll_dir`（更新目录三个 DLL 全齐优先）。

## 8.7 国际化词表变更（i18n.rs）

**新增**：`btn_github`、`settings_tab_general`、`lang_auto`/`lang_zh`/`lang_en`、`status_steam_running`/`status_steam_not_running`、`btn_update_now`、`of_active_title`（当前生效游戏）、`of_deactivate`（停用）、`patch_console_title`（补丁控制台，替换 `card2_title`）。
**改名**：`btn_download_and_extract` → `btn_update_now`（「立即更新」；流程术语「下载并解压」保留于 CONTEXT.md）。
**保留**：`card3_title` 相关词条随 Card 3 移除而删除（若未被他处引用）。
