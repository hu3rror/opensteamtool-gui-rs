# OpenSteamTool Manager

Windows 原生工具，管理 OpenSteamTool 补丁的部署/卸载与在线更新。Rust + egui/eframe（glow），单二进制，无运行时依赖。

[English](README.md)

## 功能

- **部署/卸载补丁**：把 `OpenSteamTool.dll`、`dwmapi.dll`、`xinput1_4.dll` 三个目标 DLL 复制进（或移出）Steam 目录
- **精简主页面**：部署状态 + 操作按钮组（应用并启动 / 启动 / 退出并卸载 / 卸载并重启 / 重启 Steam）；仅当兼容性体检落定「上游尚未适配 / 未找到核心 DLL」时显示一行健康风险警示（点击跳 Settings — Steam）
- **首次运行向导**：三步引导（语言 → Steam 路径 → 可选补丁下载），任一步可跳过，Settings 中可重跑
- **双页签设置对话框**：General（语言 / 关于（含应用更新检查）/ 补丁更新检查 / 重新运行向导）与 Steam（路径编辑 + 兼容性体检）
- **补丁更新维护**：Settings → General → 补丁更新检查——检查后如有新版本即可下载并解压；结果文案不显示补丁版本号
- **应用更新检查**：Settings → General → 关于区——只检查，发现新版本时打开下载页跳转浏览器（不自更新）
- **兼容性体检**：对 Steam 核心 DLL 计算哈希并按通道（pattern / IPC）探查上游签名；健康度徽章、自动/手动预热与详情都在 Settings → Steam
- **Steam 路径自动检测**：按注册表顺序定位，找不到可手动指定
- **Steam 联动**：检测到 Steam 启动自动藏到系统托盘，退出后恢复；操作完成后自动隐藏
- **托盘**：左键切换显隐，菜单含「显示」「重启 Steam」「最小化时自动隐藏到托盘」「退出」
- **中英文切换**：按系统语言自动选择，可手动切换

## 使用

1. 到 [Releases](../../releases) 下载最新 ZIP，解压到任意目录
2. 运行 `opensteamtool-manager.exe`（便携版，无需安装）
3. 首次运行自动进入「首次运行向导」：选语言与 Steam 路径，可选下载补丁 DLL（任一步可跳过，稍后再补）
4. 点「应用补丁并启动 Steam」；若补丁尚未下载，按提示前往 设置 → 通用 → 补丁更新检查，再下载并解压

设置持久化在 exe 同目录的 `config.toml`，整个目录拷贝即迁移。补丁 DLL 存放在 exe 同目录的 `dlls/`。程序启动无加载感，全部操作在后台线程执行，界面不冻结。

## 构建

需要 Rust（edition 2024）与 MSVC 工具链。

```sh
cargo build --release
# 产物：target/release/opensteamtool-manager.exe（约 6.8 MB）
```

打包便携版 ZIP（本地与 CI 同脚本；需要 PowerShell 7+，即 `pwsh`）：

```sh
pwsh -File tools/build-release.ps1 -Version <版本>
```

测试：

```sh
cargo test
```

## 发布

推送 `v*` 标签后，GitHub Actions 自动构建、测试、打包并创建 Release（配置见 `.github/workflows/release.yml`，版本号形如 `v1.0.0`）：

```sh
git tag v1.0.0
git push origin v1.0.0
```

也可在 Actions 页面手动触发。

## 术语

补丁（Patch）、部署（Deploy）、卸载（Uninstall）、操作（Action）、重启（Restart）、本地版本 / 线上版本（Local Version / Online Version，内部概念）、设置向导（Setup Wizard）、应用更新检查（App Update Check）、补丁更新检查（Patch Update Check）、自动隐身（Auto-tray）、最小化隐身（Minimize-to-Tray）——定义见 [CONTEXT.md](CONTEXT.md)。

## 源码结构

```text
src/
├── main.rs        # eframe 入口
├── config.rs      # 便携应用配置（exe 同目录 config.toml，ADR-0012）
├── wizard.rs      # 首次运行向导状态机（ADR-0013）
├── steam.rs       # 注册表路径检测、steam.exe 启动
├── steam_state.rs # Steam 运行状态：共享进程表（alive / group_running / kill）
├── process.rs     # Steam 进程监视器（2s 轮询缓存与边沿事件）
├── dll.rs         # 目标 DLL 部署/卸载、本地状态检测
├── workflow.rs    # 「操作」判定表与顺序执行（plan/execute）
├── busy.rs        # 忙碌门禁：交互类后台操作互斥（ADR-0007）
├── updater.rs     # 补丁与应用更新检查、下载解压
├── update_flow.rs # 更新检查结果唯一事实源（ADR-0008）
├── compat.rs      # 兼容性体检算子（哈希/探针/预热下载）
├── compat_flow.rs # 体检流程编排状态机（ADR-0006）
├── fsutil.rs      # 原子写入共享小工具
├── tray.rs        # 系统托盘
├── i18n.rs        # 双语文案与错误→文案映射（ADR-0009）
└── ui.rs          # egui 界面（主页面 / 设置对话框 / 向导渲染）
```

规格说明：已归档为 GitHub issues [#18](https://github.com/hu3rror/opensteamtool-gui-rs/issues/18)–[#23](https://github.com/hu3rror/opensteamtool-gui-rs/issues/23)（SPEC.md 已从仓库移除）。
