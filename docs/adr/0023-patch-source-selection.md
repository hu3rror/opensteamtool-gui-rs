# 补丁源可选：源 = 仓库端点 + 布局 + 文件清单

> **部分被取代（ADR-0024）**：源入口改到独立页签、列表去掉「推荐」、应用与下载合成一次显式动作，缺字段回退改为默认源；源模型、布局、文件清单与构建号决策仍有效。

补丁下载源此前硬编码为上游 `OpenSteam001/OpenSteamTool` 的 latest release（`src/updater.rs:13`），上游自 1.4.8（2026-06-13）起无新 release、最后 commit 2026-07-06，开放 issue 中「不工作／签名与 IPC spec 缺失」占十余个（研究文档 `docs/research-patch-dll-sources.md` §3.3）。决策：把「源」提升为一等概念——源 = **仓库端点 + 布局 + 文件清单**，内置三档 + 自定义，并顺带引入「客户端构建号」这一唯一可离线获得的失效信号。

## 决策

- **源模型**：`PatchSource` 四臂——`Upstream`（`OpenSteam001/OpenSteamTool`，标注「已停更，不推荐」）、`Huanyuejue`（`huanyuejue/OpenSteamTool`，357★，`1.5.5.4`）、`SteamDaddy`（`Contrary-v2/SteamDaddy-Dll`，`v4.0.7`）、`Custom { url, layout }`。不设第四个「上游（禁用）」档：与 `Upstream` 重复，仅去掉其映射（该档在决策过程中被提出后否决）。
- **布局（Layout）**：`enum Layout { OpenSteamTool, SteamDaddy }`，两臂各自给出 `root_files()`（落 Steam 根的清单 + 改名规则）与 `lua_subdir()`：
  - `OpenSteamTool`：`OpenSteamTool.dll` / `dwmapi.dll` / `xinput1_4.dll`，Lua 目录 `config/lua`；
  - `SteamDaddy`：`SteamDaddy.dll` / `version.dll`（由 zip 内 `version.dll.disable` 改名）/ `xinput1_4.dll`，Lua 目录 `config/stplug-in`；`steamdaddy.toml`、`ost-manager.exe`、`cloud_redirect.dll` 不落地。
- **部署/卸载/判据同源**：`dll::target_dlls_present`、`dll::check_status`、`dll::probe_facts`、`dll::deploy`、`dll::uninstall` 全部改读布局，`TARGET_DLLS` 常量删除（其职责并入 `Layout::root_files`）；`dll::lua_dir`/`ensure_lua_dir` 同样按布局取子目录（调用点 `src/ui.rs:1381`、`src/ui.rs:1385`）。
- **解包只取清单成员并按改名规则落 `dlls/`**：`download_and_extract` 从 zip 白名单提取，未知文件丢弃（沿用 ADR-0021 的原子写与 `first_match` 下载层）。
- **配置**：`config.toml` 新增 `patch_source`（`kind` + `custom.url` + `custom.layout`）与 `last_client_build`（构建号快照）。老配置缺 `patch_source` 与新装一致，都取默认源 `Huanyuejue`（ADR-0024 起不再回退 `Upstream`，列表也不标「推荐」）；按 `theme`/`minimize_to_tray` 先例不 bump `CONFIG_VERSION`。
- **入口**：设置 → Steam 页新增「补丁源」小节（下拉 + 自定义 URL + 布局类型下拉 + 一句来源说明/免责声明）；主页面「补丁更新检查」按钮不变，检查与下载都按当前源执行。向导不加步，源下拉与免责声明渲染在步骤 3 决策区；`Effect::Finish` 携带源，`wizard_download` 按选中的源下载。
- **自定义源校验**：保存时只做形状校验（`https://api.github.com/repos/<owner>/<repo>/releases/latest` 或可归一化为该形状的仓库/Release 页 URL），不做网络探测；真实失败（404、无 `.zip` 资产、清单成员缺失）在下载时以 `UpdateError` 呈现。自定义源的能力边界 = URL + 布局类型，不提供文件名映射表。
- **客户端构建号**：读 `<Steam>/package/steam_client_win32.manifest` 的 `version` 字段（本机实测 `"version" "1769731672"`），读不到即静默降级为「未确认」。部署成功时把值写入 `config.toml` 的 `last_client_build`；`update_flow::derived` 增加 `client_build_mismatch`，不一致时给**一条**提示（当前构建号 + 快照构建号并排 + 「更新 DLL」直达），两种不一致（DLL 比客户端新 / 客户端比 DLL 新）共用同一文案，不阻断下载或部署。
- **`steam.cfg` 只读不写**：读取后仅在诊断区显示「检测到已禁用客户端自动更新」（外部约定：`BootStrapperInhibitAll=Enable` / `BootStrapperForceSelfUpdate=False`）；写它等于替用户关掉客户端自动更新。
- **`version.txt` 语义不变**：仍只写 release `tag_name` 去 `v` 前缀；来源与构建号都进 `config.toml`，扩展 `version.txt` 的方案被否决（见下）。

## 为什么做

- 死源 + 无选项 = 用户唯一出路是手动满世界找 fork zip。三个候选 fork 都发同名三件套 zip、契约一致（研究文档 §4.1），换源的成本主要在落地形态而非 HTTP。
- SteamDaddy 的落地形态与本工具现状**结构性不同**：它要求 `SteamDaddy.dll` 放在 Steam 根、Lua 在 `config/stplug-in/`，且 README 明确警告 `dwmapi.dll` 在新版 Steam 会触发 `VGUI_Setup failed`、推荐 `version.dll`（外部文档声称，未实测）。只加 URL 不加布局 = 下载成功但装不成。
- 客户端构建号是本项目唯一能离线拿到的「补丁可能已失效」信号；上游 issue 里的「签名/IPC spec 缺失」模式本质上就是客户端构建号变了而 DLL 没跟上。

## 否决的方案

- **只加 URL、布局固定为上游三件套**：SteamDaddy 源会「下载成功、部署半残」，是目前最容易制造的新故障。
- **通用 `SourceSpec` 结构 + 函数指针/闭包**：只有两个布局形态，抽象成数据驱动注册表会把「该源要哪些文件」从一处 enum 匹配摊到常量表，读起来更糟。
- **把来源写进 `version.txt`（如 `4.0.7@steamdaddy`）**：`version.txt` 的语义已被 ADR-0011 钉死（版本记录不作数，判断只认文件事实），改它会让「本地版本」字符串进入 UI 与派生比较，收益仅是省一个 config 字段。
- **用 `steam.exe` 的 `FileVersion` 当客户端构建号**：本机实测 `10.87.82.64`，是引导器版本，与客户端构建号不同源。
- **不一致就阻断下载/部署**：把「可能失效」当「必然失效」，误伤面过大。
- **自动写 `steam.cfg` 锁客户端**：静默停止安全更新的后果不可逆，收益与既有手工做法相同。
- **去掉上游源**：上游仍是唯一「官方」来源，删掉等于替用户决定第三方二进制优先。

## 未确认

- SteamDaddy 最小集是否真的可用：`steamdaddy.toml` 缺失时回落 `opensteamtool.toml`（README 声称），`xinput1_4.dll` 是否必需未确认。
- `huanyuejue` 的实际可用性与维护持续性（最后 release 2026-11-13）。
- 三个 fork 的二进制来源可信度：只有 GitHub 资产 `digest`，无签名校验；`Contrary-v2` 是 `Contrary7/SteamDaddy-Dlls` 的镜像，两者关系未核实。
- 全新安装 Steam（最新客户端）+ 旧 DLL 的重装系统路径未实测。

## 验收

- `cargo test --workspace`：布局两臂的文件清单/改名/卸载；`check_update` 的 URL 构造与资产筛选 `*.zip`；配置新字段的容错读取与 round-trip；`derived` 的构建号不一致派生。
- `cargo check --all-targets` / `cargo clippy --all-targets` / `cargo fmt --all --check` 全绿。
- 手动：三个内置源各做一次真实检查 + 下载（Stub 不可替代）；SteamDaddy 源部署后 Steam 能启动且不崩（验收人 = 用户）。

## 实施拆解（同一 PR 的提交序）

1. `dll`：`Layout` + 五处消费点 + 单测（纯重构，行为等价，上游布局保持现状）。
2. `updater`：`check_update(&PatchSource)`、资产筛选、清单化解包 + 改名。
3. `config`：`patch_source` / `last_client_build` 的读写与容错。
4. `update_flow` + 主页面：`client_build_mismatch` 派生与提示。
5. `wizard`：步骤 3 源下拉、`Finish` 携带源、免责声明、相关测试。
6. `ui`：设置 Steam 页「补丁源」小节、`wizard_download` 与更新按钮传源、i18n 字符串。
7. `docs`：本 ADR、`GLOSSARY.md` 术语与规则、README 一段（源选择 + 免责声明）。
