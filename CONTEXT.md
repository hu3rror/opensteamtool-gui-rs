# OpenSteamTool Manager Context

Windows 原生工具，管理 Steam 补丁的部署/卸载与在线更新（Rust + egui 桌面 GUI，单二进制）。

## Language

**补丁（Patch）**:
OpenSteamTool 的三个目标 DLL 的合称（`OpenSteamTool.dll`、`dwmapi.dll`、`xinput1_4.dll`），注入 Steam 目录以生效。
_Avoid_: mod、注入物

**部署（Deploy）**:
把本地 `dlls/` 中的补丁复制到 Steam 目录并使其生效的动作。
_Avoid_: 安装、应用

**卸载（Uninstall）**:
从 Steam 目录删除补丁 DLL 使其失效的动作。
_Avoid_: 移除

**本地版本（Local Version）**:
`dlls/version.txt` 记录的、当前随本工具分发的补丁版本。
_Avoid_: 本地文件版本、仓库版本

**线上版本（Online Version）**:
GitHub releases 最新版本的 `tag_name`（去前缀 `v`）。
_Avoid_: 远程版本、上游版本

**忙碌态（Busy）**:
后台操作进行中、UI 暂停交互并显示阶段文案的状态。
_Avoid_: loading、加载中

**托盘（Tray）**:
Windows 系统托盘图标，窗口隐藏到托盘后的恢复入口与退出出口。
_Avoid_: 系统栏、状态栏图标

**自动隐身（Auto-tray）**:
检测到 Steam 进程启动后自动把窗口隐藏到托盘的行为；Steam 退出后自动恢复窗口。
_Avoid_: 最小化到托盘（指用户手动，非自动）

## Rules

- 术语 `部署/卸载` 只用于补丁 DLL 相对 Steam 目录的操作，不与「启动/退出 Steam」混用。
- `本地版本` 与 `线上版本` 区分严格：前者来自本地 `dlls/`，后者来自 GitHub。
- 谈论窗口行为时区分「手动最小化」（用户操作）与「自动隐身」（Steam 联动）。
