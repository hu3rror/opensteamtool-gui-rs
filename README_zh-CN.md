<div align="center">
  <img src="assets/logo.png" width="96" alt="OpenSteamTool Manager 图标">

  # OpenSteamTool Manager

  为 Steam 部署、卸载与在线更新 OpenSteamTool 补丁的便携原生 Windows 图形工具。

  [![CI](https://img.shields.io/github/actions/workflow/status/hu3rror/opensteamtool-gui-rs/ci.yml)](https://github.com/hu3rror/opensteamtool-gui-rs/actions)
  [![Release](https://img.shields.io/github/v/release/hu3rror/opensteamtool-gui-rs)](https://github.com/hu3rror/opensteamtool-gui-rs/releases)
  [![License: MIT](https://img.shields.io/github/license/hu3rror/opensteamtool-gui-rs)](LICENSE)

  [English](README.md)

</div>

单二进制 Windows 工具，管理 Steam 安装中的 OpenSteamTool 补丁集（三个目标 DLL）：部署、卸载与保持更新——并围绕补丁提供 Steam 的启动/关闭流程。基于 Rust + egui/eframe（glow）构建；免安装、无运行时依赖。

## 功能

- **一键操作**：应用补丁并启动 Steam 一步完成；纯启动、退出并卸载、卸载并重启、重启 Steam，仅在必要时弹确认框
- **部署状态一目了然**：状态卡展示 已应用 / 未应用 / 路径无效；当兼容性体检报告「上游尚未适配 / 未找到核心 DLL」时显示一行健康风险警示（点击跳 Settings → Steam）
- **首次运行向导**：三步引导（语言 → Steam 路径 → 可选补丁下载），任一步可跳过，Settings 中可重跑
- **三页签设置对话框**：General（语言 / 主题）、Steam（路径编辑 + 兼容性体检）、About（应用更新检查 / 重新运行向导）；改动即改即存
- **主页面补丁更新检查**：检查后如有新版本即可下载并解压；补丁版本号不进入界面
- **深浅主题**：跟随系统或手动切换（Settings → General）
- **兼容性体检**：对 Steam 核心 DLL 计算哈希并按通道（pattern / IPC）探查上游签名，支持自动/手动预热
- **Steam 联动窗口**：Steam 启动自动隐藏到托盘、退出后恢复；「最小化时自动隐藏」偏好持久化
- **单实例**：重复启动不会新开第二个窗口，而是把已经存在的界面（含托盘隐藏中）唤起并带到前台
- **托盘控制**：左键切换显隐；菜单含「显示」「重启 Steam」「最小化时自动隐藏到托盘」「退出」
- **便携设计**：设置存 exe 同目录 `config.toml`，补丁存 `dlls/`；整目录拷贝即迁移
- **中英文界面**：按系统语言自动选择，运行时可切换
- **紧凑默认窗口**：以最小尺寸打开，内容需要时才会长高

## 快速开始

1. 到 [Releases](../../releases) 下载最新 ZIP，解压到任意目录。
2. 运行 `opensteamtool-manager.exe`——无需安装。
3. 首次运行自动进入设置向导：选语言与 Steam 路径，可选下载补丁 DLL（任一步可跳过，稍后再补）。
4. 点「应用补丁并启动 Steam」；若补丁尚未下载，用主页面上的「检查补丁更新」按钮先检查再下载并解压。

> [!TIP]
> 一切便携：`config.toml`（设置）与 `dlls/`（补丁文件）都在 exe 旁边——整个目录拷到另一台机器即可用。

## 构建

需要 Rust（edition 2024）与 MSVC 工具链。

```sh
cargo build --release
# 产物：target/release/opensteamtool-manager.exe（约 6.8 MB）
```

打包便携版 ZIP（本地与 CI 同一脚本；需要 PowerShell 7+，即 `pwsh`）：

```sh
pwsh -File tools/build-release.ps1 -Version <版本>
```

测试：

```sh
cargo test
```

## 发布

推送 `v*` 标签后自动构建、测试、打包并创建带便携 ZIP 的 GitHub Release（见 `.github/workflows/release.yml`）：

```sh
git tag v1.0.0
git push origin v1.0.0
```

## 源码结构

```text
src/
├── main.rs        # eframe 入口
├── config.rs      # 便携应用配置（exe 旁 config.toml，ADR-0012）
├── wizard.rs      # 首次运行向导状态机（ADR-0013）
├── steam.rs       # 注册表路径检测、steam.exe 启动
├── steam_state.rs # 共享 Steam 进程表（alive / group_running / kill）
├── process.rs     # Steam 进程监控（2s 轮询缓存、边沿事件）
├── singleton.rs   # 单实例：互斥体裁决 + 唤醒事件唤起既有窗口（ADR-0017）
├── dll.rs         # 目标 DLL 部署/卸载、本地状态
├── workflow.rs    # 动作规划与逐步执行（plan/execute）
├── busy.rs        # 忙碌门禁：互斥的交互式后台操作（ADR-0007）
├── updater.rs     # 补丁与应用更新检查、下载并解压
├── update_flow.rs # 更新检查结果的唯一事实源（ADR-0008）
├── compat.rs      # 兼容性体检：哈希 / 镜像链 / 预热
├── compat_flow.rs # 兼容性体检编排状态机（ADR-0006）
├── fsutil.rs      # 共享原子写文件
├── theme.rs       # 色板与视觉装配（ADR-0010/0016）
├── main_page.rs   # 主页面视图模型派生与图标绘制
├── tray.rs        # 系统托盘
├── i18n.rs        # 双语字符串与错误文案映射（ADR-0009）
└── ui.rs          # egui UI（主页面、设置对话框、向导渲染）
```

设计决策记录在 `docs/adr/`（ADR-0001–0017）；领域术语定义见 [GLOSSARY.md](GLOSSARY.md)；锁定版 UI 规格与交互原型在 `docs/design/`。
