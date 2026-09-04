# OpenSteamTool Manager

Windows 原生工具，管理 OpenSteamTool 补丁的部署/卸载与在线更新。Rust + egui/eframe（glow），单二进制，无运行时依赖。

[English](README.md)

## 功能

- **部署/卸载补丁**：把 `OpenSteamTool.dll`、`dwmapi.dll`、`xinput1_4.dll` 三个目标 DLL 复制进（或移出）Steam 目录
- **在线更新**：检查 GitHub Releases 最新版本，下载并解压到本地 `dlls/`
- **Steam 路径自动检测**：按注册表顺序定位，找不到可手动指定
- **Steam 联动**：检测到 Steam 启动自动藏到系统托盘，退出后恢复；操作完成后自动隐藏
- **托盘**：左键切换显隐，菜单含「显示」「最小化时自动隐藏到托盘」「退出」
- **中英文切换**：按系统语言自动选择，可手动切换
- **设置对话框**：编辑 Steam 目录下的上游配置文件 `opensteamtool.toml`（保存前 TOML 语法校验）；文件缺失时可从内置示例模板一键开始
- **OnlineFix 启动预设**：设置对话框内直接为指定游戏写入/移除 `-onlinefix` 启动选项（写 `localconfig.vdf`，自动备份、需先关 Steam）；支持一键复制参数

## 使用

1. 到 [Releases](../../releases) 下载最新 ZIP，解压到任意目录
2. 运行 `opensteamtool-manager.exe`（便携版，无需安装）
3. 首次使用先补 `dlls/` 目录：点「检查更新」→「下载并解压新版本」自动拉取，或手动放入目标 DLL
4. 选好 Steam 路径后，点「应用补丁并启动 Steam」

补丁 DLL 存放在 exe 同目录的 `dlls/`。程序启动无加载感，全部操作在后台线程执行，界面不冻结。

## 构建

需要 Rust（edition 2024）与 MSVC 工具链。

```sh
cargo build --release
# 产物：target/release/opensteamtool-manager.exe（约 6.8 MB）
```

打包便携版 ZIP（本地与 CI 同脚本）：

```sh
powershell -File tools/build-release.ps1 -Version <版本>
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

补丁（Patch）、部署（Deploy）、卸载（Uninstall）、本地版本（Local Version）、线上版本（Online Version）、操作（Action）、自动隐身（Auto-tray）、最小化隐身（Minimize-to-Tray）——定义见 [CONTEXT.md](CONTEXT.md)。

## 源码结构

```text
src/
├── main.rs       # eframe 入口
├── config_editor.rs # opensteamtool.toml 读取/校验/原子写入（设置对话框）
├── onlinefix.rs # localconfig.vdf 启动选项读写（OnlineFix 预设：VDF 解析/备份）
├── ui.rs         # egui 界面（3 卡片 + 托盘 + 自动隐身接线）
├── workflow.rs   # 「操作」判定表与顺序执行（plan/execute）
├── dll.rs        # 目标 DLL 部署/卸载、本地状态检测
├── steam.rs      # 注册表路径检测、steam.exe 启动
├── process.rs    # Steam 进程监视/关闭（sysinfo）
├── updater.rs    # GitHub 检查更新、下载解压
├── tray.rs       # 系统托盘
└── i18n.rs       # 双语文案与文案映射
```

规格说明见 [SPEC.md](SPEC.md)。
