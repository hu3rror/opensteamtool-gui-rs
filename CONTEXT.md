# OpenSteamTool Manager Context

Windows 原生工具，管理 Steam 补丁的部署/卸载与在线更新（Rust + egui 桌面 GUI，单二进制）。

## Language

**补丁（Patch）**:
OpenSteamTool 的三个目标 DLL 的合称（`OpenSteamTool.dll`、`dwmapi.dll`、`xinput1_4.dll`），注入 Steam 目录以生效。
_Avoid_: mod、注入物

**部署（Deploy）**:
把本地 `dlls/` 中的补丁复制到 Steam 目录并使其生效的动作。
_Avoid_: 安装、应用

**部署状态（Deploy Status）**:
补丁相对 Steam 目录的部署情况三态：已应用（三个目标 DLL 齐全）/ 未应用（缺失）/ 路径无效（Steam 路径为空或不是目录；UI 文案归入「未应用」并附提示）。
_Avoid_: 应用状态、补丁状态

**卸载（Uninstall）**:
从 Steam 目录删除补丁 DLL 使其失效的动作。
_Avoid_: 移除

**操作（Action）**:
用户从按钮触发的组合动作，可混含补丁操作与 Steam 启动/退出（应用补丁并启动 / 正常启动 / 退出并卸载 / 卸载并重启）。
_Avoid_: 步骤、任务

**本地版本（Local Version）**:
`dlls/version.txt` 记录的、当前随本工具分发的补丁版本。
_Avoid_: 本地文件版本、仓库版本

**线上版本（Online Version）**:
GitHub releases 最新版本的 `tag_name`（去前缀 `v`）。
_Avoid_: 远程版本、上游版本

**在线更新（Online Update）**:
检查线上版本并拉取替换本地补丁的整体流程：先「检查更新」对比版本，再「下载并解压」落地新补丁。
_Avoid_: 自动更新、升级

**检查更新（Check Update）**:
查询线上版本并与本地版本对比的动作；不下载，仅产生是否可更新的结论。
_Avoid_: 版本检查、更新检查

**下载并解压（Download & Extract）**:
拉取线上版本的 .zip 资产，提取补丁 DLL 写入 `dlls/` 并更新本地版本记录的动作。
_Avoid_: 更新安装、应用更新

**更新动作（Update Action）**:
属于「在线更新」流程的按钮级动作（检查更新 / 下载并解压），与「操作」区分——「操作」只指补丁+Steam 组合动作。
_Avoid_: 更新操作

**忙碌态（Busy）**:
后台操作进行中、UI 暂停交互并显示阶段文案的状态。
_Avoid_: loading、加载中

**托盘（Tray）**:
Windows 系统托盘图标，窗口隐藏到托盘后的恢复入口与退出出口。
_Avoid_: 系统栏、状态栏图标

**自动隐身（Auto-tray）**:
检测到 Steam 进程启动后自动把窗口隐藏到托盘的行为；Steam 退出后自动恢复窗口。
_Avoid_: 最小化到托盘（那是「最小化隐身」的行为）

**最小化隐身（Minimize-to-Tray）**:
用户手动最小化窗口时，窗口隐藏到托盘（而非任务栏）的行为；托盘菜单勾选项，默认开启。
_Avoid_: 自动隐身（那是 Steam 联动）

## Rules

- 术语 `部署/卸载` 只用于补丁 DLL 相对 Steam 目录的操作，不与「启动/退出 Steam」混用。
- 「操作」是按钮级组合动作（可混含补丁操作与 Steam 启动/退出），与单种「部署/卸载」区分。
- 「操作」与「更新动作」是并列的两类按钮动作：前者是补丁+Steam 组合，后者属于「在线更新」流程（检查更新 / 下载并解压），不混用。
- 「部署状态」的「路径无效」在 UI 文案上归入「未应用」并附路径提示，不单独作为用户可见状态。
- 状态文案统一「已应用/未应用」（对齐 Python 版既有用户文案，ADR-0003）；按钮「应用补丁并启动 Steam」保留不更。
- `本地版本` 与 `线上版本` 区分严格：前者来自本地 `dlls/`，后者来自 GitHub。
- 窗口行为三类：「自动隐身」（Steam 联动）、「最小化隐身」（用户最小化，勾选项开启时）、普通最小化到任务栏。
