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
属于「在线更新」流程的按钮级动作（检查更新 / 立即更新），与「操作」区分——「操作」只指补丁+Steam 组合动作；按钮文案「立即更新」对应流程术语「下载并解压」。
_Avoid_: 更新操作

**忙碌态（Busy）**:
后台操作进行中、UI 暂停交互并显示阶段文案的状态。
_Avoid_: loading、加载中

**托盘（Tray）**:
Windows 系统托盘图标，窗口隐藏到托盘后的恢复入口与退出出口。
_Avoid_: 系统栏、状态栏图标

**自动隐身（Auto-tray）**:
检测到 Steam 进程启动后自动把窗口隐藏到托盘的行为；Steam 退出后自动恢复窗口。
_Avoid_: 最小化到托盘（那是「最小化至托盘」的行为）

**最小化至托盘（Minimize-to-Tray）**:
用户手动最小化窗口时，窗口隐藏到托盘（而非任务栏）的行为；设置弹窗「常规偏好」页勾选项，持久化于 `gui_config.toml`，默认开启。
_Avoid_: 最小化隐身（旧称）、隐藏到任务栏

**Steam 进程组（Steam Process Group）**:
判定「Steam 在运行」的进程集合：exe 路径位于 Steam 目录下的进程，排除 `steamservice.exe`；「关闭 Steam」对整组生效。
_Avoid_: Steam 相关进程（模糊）

**Steam 运行状态（Steam State）**:
判定「Steam 在运行/不在运行」的共享入口：`SteamState` 模块持有进程表，提供 `alive`（steam.exe 口径）、`group_running(dir)`（进程组口径）与 `kill(dir)` 三查询，每次锁内实时刷新+判定；2s 轮询缓存与边沿事件由「Steam 进程监视器」承担。
_Avoid_: 进程状态、运行检测

**关闭完成（Close Settled）**:
「关闭 Steam」的成功态：Steam 进程组内所有进程从进程表消失（轮询预算 5s）；超时视为关闭失败。
_Avoid_: 关闭成功（易与 spawn 成功混淆）

**设置对话框（Settings Dialog）**:
顶栏「设置」按钮打开的模态对话框，按页签组织非主流程设置：含「常规偏好」「配置编辑器」「OnlineFix 启动预设」三页，页签互不共享状态，会话内记忆上次页签（默认「常规偏好」）。底部仅一条全局动态操作栏（Footer），各页签子视图内部不渲染按钮条。
_Avoid_: 设置窗口、选项框

**配置编辑器（Config Editor）**:
编辑 `<Steam>/opensteamtool.toml`（上游 OpenSteamTool 配置文件）的文本编辑器：载入现有内容、保存前 TOML 语法校验（错误带行列定位）、原子写入；编辑区提供「撤销」与「未保存的编辑」覆盖确认。操作按钮（从示例模板创建/撤销/保存）位于设置对话框底部动态 Footer。
_Avoid_: 设置编辑器

**示例模板（Example Template）**:
配置编辑器内内置的上游 `opensteamtool.example.toml` 快照，一键载入作为编辑起点（文件不存在时的引导入口）。
_Avoid_: 示例配置

**未保存的编辑（Unsaved Edits）**:
配置编辑器缓冲与磁盘内容不一致的状态（编辑后未保存）；存在时「从示例模板创建」先弹覆盖确认，避免误覆盖。
_Avoid_: 脏标记、有改动、dirty

**撤销（Undo）**:
配置编辑器回退最近一次编辑的动作（「撤销」按钮或 Ctrl+Z），重做走 Ctrl+Y；撤销历史随应用会话存在（载入/保存不清空）。
_Avoid_: 回退、revert

**OnlineFix 启动预设（OnlineFix Preset）**:
把 `-onlinefix` 写入指定游戏 AppID 的「启动选项」的持久设置（改 `localconfig.vdf`，Steam 原生读取）；上游据此把该游戏的 AppID 重写为 480（`kOnlineFixAppId`）实现在线修复。预设页展示选中账号的「当前生效游戏」，候选从本地 `appmanifest_*.acf` 解析游戏名称（缺失回退纯数字）。
_Avoid_: 联机修复、在线模式

**启动选项（Launch Options）**:
`localconfig.vdf` 中 `Apps/<appid>` 块下的 `LaunchOptions` 键值，即 Steam 游戏属性里的「启动选项」。
_Avoid_: 启动参数、命令行参数（「参数」指 `-onlinefix` 本身）

**VDF 备份（VDF Backup）**:
修改 `localconfig.vdf` 前生成的时间戳副本（`localconfig.vdf.bak-<unix秒>`），用于异常回滚。
_Avoid_: 备份文件

**兼容性体检（Compatibility Probe）**:
对 Steam 目录下核心 DLL（`steamclient64.dll` / `steamui.dll`）计算 SHA-256，并按「通道」向 `OpenSteam001/steam-monitor` 探查上游签名/IPC 规约是否适配的后台过程；产出「健康度」六态结论。
_Avoid_: 兼容检查、版本检测

**通道（Channel）**:
上游 `steam-monitor` 仓库的分支名，决定拉取何种签名：`pattern`（特征码）/ `ipc`（IPC 规约）。
_Avoid_: 分支、类型

**组件（Component）**:
上游仓库内的一级目录名，指代被探针的核心 DLL：`steamclient`（steamclient64.dll）/ `steamui`（steamui.dll）。
_Avoid_: 模块、目标

**离线缓存（Offline Cache）**:
按 `<Steam>/opensteamtool/{通道}/{组件}/<sha256>.toml` 落盘的本地签名文件；体检命中即「离线可用」。
_Avoid_: 本地缓存、签名缓存

**预热（Precache）**:
把上游已适配但本地缺失的签名文件下载并原子写入「离线缓存」的动作；是兼容性体检流程中唯一落盘动作。
_Avoid_: 预下载、缓存更新

**健康度（Health Summary）**:
兼容性体检的六态结论：检查中 / 完美兼容（已缓存）/ 上游已适配（未缓存）/ 上游尚未适配 / 未找到核心 DLL / 网络不可用。
_Avoid_: 状态、兼容性状态

**兼容性徽章（Compatibility Badge）**:
Card 1 兼容性小节内的 pill 徽章（浅色底 + 圆角 + 状态图标/文字），以「健康度」六态概括体检结论；辅助说明行弱化于其下。
_Avoid_: 状态标签、状态点

**验证缓存（Verified Cache）**:
工具目录 `cache/verified.toml` 记录的「DLL 哈希 → 已验证上游适配」映射；命中即绿灯，Steam 更新改变哈希时自动失效重验（无时间有效期）。
_Avoid_: 验证记录、信任列表

**补丁控制台（Patch Console）**:
主界面第二区块：合并「部署状态」展示与「操作」按钮为单一卡片——顶部为「部署状态」徽章与「Steam 运行状态」，下方为按钮行。
_Avoid_: 卡片 2、操作区

**当前生效游戏（Active OnlineFix Game）**:
当前选中账号 `localconfig.vdf` 中带 `-onlinefix` 启动选项的 AppID（业务约束：同一时间仅一个 OnlineFix 游戏可运行）；OnlineFix 预设页顶部以看板形式高亮展示 AppID 与名称并提供快捷停用，进入预设页自动同步 AppID 输入框。
_Avoid_: 生效项、激活游戏


**候选项（Candidate）**:
OnlineFix 预设页候选胶囊背后的 AppID + 本地名称映射（代码 `CandidateGame`，SPEC §8.3）：名称来自本地 `appmanifest_<appid>.acf` 解析，ACF 缺失/畸变时名称为空，展示回退纯数字；点击胶囊填充 AppID 输入框。
_Avoid_: 候选列表、建议

**常规偏好（General）**:
设置对话框第一页签，管理工具自身全局偏好：界面语言（自动检测/简体中文/English）与「最小化至托盘」，持久化于 `gui_config.toml`。
_Avoid_: 通用设置、常规设置

**便携模式（Portable Mode）**:
exe 同级目录可写时采用的存储模式：配置（`gui_config.toml`）、缓存（`cache/`）与 DLL 资产（`dlls/`）全部收拢于 exe 同级。
_Avoid_: 绿色版、便携版

**安装模式（Installed Mode）**:
exe 同级目录只读（如 Program Files）时回退的存储模式：配置落 `%APPDATA%\OpenSteamTool\`，缓存与更新 DLL 落 `%LOCALAPPDATA%\OpenSteamTool\`，自带 DLL 仍读 exe 同级只读目录；无感回退，不要求管理员提权。
_Avoid_: 系统模式、管理员模式


## Rules

- 术语 `部署/卸载` 只用于补丁 DLL 相对 Steam 目录的操作，不与「启动/退出 Steam」混用。
- 「操作」是按钮级组合动作（可混含补丁操作与 Steam 启动/退出），与单种「部署/卸载」区分。
- 「操作」与「更新动作」是并列的两类按钮动作：前者是补丁+Steam 组合，后者属于「在线更新」流程（检查更新 / 下载并解压），不混用。
- 「部署状态」的「路径无效」在 UI 文案上归入「未应用」并附路径提示，不单独作为用户可见状态。
- 状态文案统一「已应用/未应用」（对齐 Python 版既有用户文案，ADR-0003）；按钮「应用补丁并启动 Steam」保留不更。
- `本地版本` 与 `线上版本` 区分严格：前者来自本地 `dlls/`，后者来自 GitHub。
- 窗口行为三类：「自动隐身」（Steam 联动）、「最小化至托盘」（用户最小化，勾选项开启时）、普通最小化到任务栏。
- UI「Steam 正在运行」判定只看 `steam.exe`（2s 轮询，快而便宜）；「关闭 Steam」用「Steam 进程组」判定（路径过滤，彻底），两套口径不同属有意为之。
- 「启动 Steam」的成功判定是 spawn 后确认 `steam.exe` 存活（2s 窗口，失败重试 1 次），不是 spawn 本身。
- 「Steam 进程组」的判定路径必须是实际 Steam 安装目录：路径前缀匹配会把该目录下所有进程计入组内，用宽目录（如系统临时目录）作判定路径会误伤无关进程。
- 「配置编辑器」编辑的是上游配置文件 `<Steam>/opensteamtool.toml`，与工具自身运行参数无关；保存即显式创建文件（与上游「无文件时不自动创建」的行为区分——那是用户未显式操作的情形）。
- 「OnlineFix 启动预设」只允许在 Steam 未运行时修改（`localconfig.vdf` 会被 Steam 退出时回写覆盖）；每次写盘前必先生成「VDF 备份」。
- 「启动选项」的读写走 `localconfig.vdf`（Steam 原生读取）；「复制参数」只是把 `-onlinefix` 文本给到剪贴板，不写任何文件。

- 存储双模判定在启动时进行（exe 同级可写 → 便携模式，否则安装模式）；「生效 DLL 目录」层叠优先：更新目录三个目标 DLL 齐全则优先，否则回退自带目录；部署/卸载/版本读取统一走生效目录。
- 「最小化至托盘」与「自动隐身」是两类行为：前者用户手动最小化触发（常规偏好勾选项，持久化），后者 Steam 启动/退出联动（固定策略，无设置项）。
- 生效游戏看板的「停用」与 Footer「停用」同指一操作（同一写入门闩与停用动作）；停用后看板即时消失，AppID 输入框保留用户输入不被覆盖。
