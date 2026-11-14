# 补丁 DLL 来源调研：SteamDaddy-Dll vs OpenSteamTool（及其下游 fork）

## 0. 速览（每条都在下文有来源与原文片段）

1. **`Contrary-v2/SteamDaddy-Dll` 公开可达**（`/repos` → 200），**有 Releases，共 2 个**：`v4.0.7`（2026-11-12T03:45:02Z，仅 `SteamDaddy.Dll-v4.0.7-Release.zip`）、`v4.0.6`（2026-11-04T18:39:31Z，Debug+Release 双 zip）。最新资产下载 URL：`https://github.com/Contrary-v2/SteamDaddy-Dll/releases/download/v4.0.7/SteamDaddy.Dll-v4.0.7-Release.zip`。
2. **它是 OpenSteam001/OpenSteamTool 的 fork**（API `fork: true`、`parent` = OpenSteam001/OpenSteamTool），但**产物与本项目现在取的三个 DLL 不同**：zip 内是 `SteamDaddy.dll` + `dwmapi.dll` + `version.dll.disable` + `xinput1_4.dll` + `cloud_redirect.dll` + `ost-manager.exe` + `steamdaddy.toml`，**没有 `OpenSteamTool.dll`**；**不含 `version.txt`**（上游 zip 也不含）；zip 布局与上游不同。
3. **上游 OpenSteam001/OpenSteamTool**：最新 release `1.4.8`（2026-06-13T12:18:05Z），资产 `OpenSteamTool-1.4.8-Debug.zip` / `OpenSteamTool-1.4.8-Release.zip`；最后 commit `2026-07-06`（`2a08b0bf`）；**未归档**；开放 issue 73 个（PR 17 个），其中十余个是「不工作/签名或 IPC spec 缺失」类（`not working` 10、`not work` 12、`failed` 6、`broken` 3、`crash` 3）。
4. **仍活跃且发布同款三件套 zip 的下游 fork**（抽样 100 个 fork 中 15 个有 release，全部已解包核对；下表为 4.1 按发布时间倒序的 16 行。抽样结果，非全集）：`eisora08/OpenSteamTool`（1★，`v1.4.9.0`，`2026-11-13T21:10:45Z`）、`huanyuejue/OpenSteamTool`（357★，`1.5.5.4`，`2026-11-13T08:34:28Z`）、`matheeshapathirana/OpenSteamTool`（3★，`v1.5.0`，`2026-11-12T15:28:02Z`）、`LotusHirasawaSusumu/OpenSteamTool`（0★，`0.0.1`，`2026-11-01T20:19:30Z`）、`leonardoneos/OpenSteamTool`（0★，`v2.6.5`，`2026-10-25T22:38:45Z`）、`railzfh/OST`（0★，`v1.4.9`，`2026-10-25T16:38:20Z`）、`isxlan0/OpenSteamTool`（3★，`v1.4.8`，`2026-10-22T05:23:07Z`） 等。资产命名统一为 `OpenSteamTool-<tag>-{Debug,Release}.zip`（`isxlan0` 例外，只发 `Release.7z`）。
5. **许可证：两边都是 GPL-3.0**（API `license.spdx_id` + 两份 `LICENSE` 文件首行）。**两个仓库都没有任何关于再分发/fork 政策/DMCA 的显式说明**；唯一的立场性文字是 OpenSteamTool README 的 research/educational 免责声明（原文见 5.2）。SteamDaddy-Dll 连 Issues 功能都关闭（`has_issues: false`），不存在该仓库的讨论线索。

> 抓取方式：GitHub REST API（`curl` / `gh api`，认证令牌来自本机 `gh`）、`raw.githubusercontent.com`、以及**实际下载并解包** release 资产。
> 时间戳一律按 API 原样引用（API 返回的日期均落在 2026 年，未做任何换算）。
> 每条结论后附来源 URL + 原文片段；无法核实的一律标 **UNVERIFIED** 并写明尝试过什么。

---

## 1. Contrary-v2/SteamDaddy-Dll：可达性、Releases、资产

### 1.1 仓库公开可达（HTTP 状态逐条）

| 请求 | 状态 | 备注 |
|---|---|---|
| `GET https://api.github.com/repos/Contrary-v2/SteamDaddy-Dll` | **200** | 见下文字段 |
| `GET https://api.github.com/repos/Contrary-v2/SteamDaddy-Dll/releases` | **200** | 数组长度 2 |
| `GET https://api.github.com/repos/Contrary-v2/SteamDaddy-Dll/releases/latest` | **200** | `v4.0.7` |
| `GET https://github.com/Contrary-v2/SteamDaddy-Dll`（HTML） | **200** | |
| `GET https://github.com/Contrary-v2/SteamDaddy-Dll/releases/latest`（HTML） | **302** | → `/releases/tag/v4.0.7` |
| `GET https://raw.githubusercontent.com/Contrary-v2/SteamDaddy-Dll/main/README.md` | **200** | 16 304 bytes |
| `GET .../releases/download/v4.0.7/SteamDaddy.Dll-v4.0.7-Release.zip` | **200** | 2 983 889 bytes |

仓库元数据（API 原值）：

```json
"full_name": "Contrary-v2/SteamDaddy-Dll",
"description": "Dlls for SteamDaddy",
"private": false,
"fork": true,
"language": "C++",
"created_at": "2026-10-28T12:27:04Z",
"pushed_at": "2026-11-12T03:45:02Z",
"archived": false, "disabled": false,
"has_issues": false, "has_discussions": false,
"stargazers_count": 18, "forks_count": 1, "open_issues_count": 0,
"license": {"key": "gpl-3.0", "spdx_id": "GPL-3.0"}
```

上游关系（同一响应内 `parent` / `source`，两者相同）：

```json
"parent": {"full_name": "OpenSteam001/OpenSteamTool", "fork": false,
           "pushed_at": "2026-07-06T09:28:57Z", "stargazers_count": 2616}
```

→ **是公开可达的仓库，且是 OpenSteam001/OpenSteamTool 的 GitHub fork**（`fork: true` + `parent` 字段双重印证）。注意 `has_issues: false`：**该仓库没有 Issues 功能**。

### 1.2 Releases 全量（共 2 个，无 draft、无 prerelease）

来源：`https://api.github.com/repos/Contrary-v2/SteamDaddy-Dll/releases`

| tag | published_at | assets（名称 / 字节 / content_type / download_count） |
|---|---|---|
| `v4.0.7` | `2026-11-12T03:45:02Z` | `SteamDaddy.Dll-v4.0.7-Release.zip` / 2 983 889 / `application/x-zip-compressed` / 92 |
| `v4.0.6` | `2026-11-04T18:39:31Z` | `SteamDaddy.Dll-v4.0.6-Debug.zip` / 3 944 530 / 117；`SteamDaddy.Dll-v4.0.6-Release.zip` / 1 632 384 / 252 |

`/releases/latest` 响应要点（原文）：

```json
"tag_name": "v4.0.7",
"name": "v4.0.7",
"published_at": "2026-11-12T03:45:02Z",
"draft": false, "prerelease": false,
"body": ""
```

（**最新 release 的 `body` 是空字符串** —— 没有 release notes。）

最新 release 唯一资产的下载 URL（`browser_download_url` 原值）：

```
https://github.com/Contrary-v2/SteamDaddy-Dll/releases/download/v4.0.7/SteamDaddy.Dll-v4.0.7-Release.zip
```

资产命名规律（已验证两点）：

- 文件名形如 **`SteamDaddy.Dll-<tag>-Release.zip`**（注意 `Dll` 前是点号、tag 带 `v` 前缀，与 OpenSteamTool 的 `OpenSteamTool-1.4.8-Release.zip` 不同）。
- **最新 tag `v4.0.7` 只有 Release zip，没有 Debug zip**；Debug zip 只存在于 `v4.0.6`。
- 只有 `.zip` 资产，**没有单文件 DLL 资产**。
- tags 端点（`/tags`）同样只有 `v4.0.7`、`v4.0.6`。

---

## 2. SteamDaddy-Dll 到底装了什么

### 2.1 README 原文关键片段

来源：`https://raw.githubusercontent.com/Contrary-v2/SteamDaddy-Dll/main/README.md`（HTTP 200）

自我定位：

> "**SteamDaddy** is an advanced C++20 user-space dynamic instrumentation engine for the Valve Steam client."

> "This repository contains the standalone, low-level C++ binaries (`SteamDaddy.dll`, `version.dll`, `dwmapi.dll`)."

构建产物说明：

> "**`SteamDaddy.dll`**: The primary interception payload injected into Steam.
> **`version.dll`**: Recommended Windows Version proxy loader for modern Steam.
> **`dwmapi.dll`**: Alternative DWM sideload proxy containing `kEcosystemSignature = "SteamDaddy"`."

关于 dwmapi 的风险提示：

> "On recent Steam client builds, deploying `dwmapi.dll` into the Steam root folder can trigger a `VGUI_Setup failed` startup crash. We strongly recommend using **`version.dll`** as your sideload proxy. If you previously deployed `dwmapi.dll`, delete it from your Steam root."

Credits & Upstream（承认 fork 谱系）：

> "[OpenSteam001/OpenSteamTool](https://github.com/OpenSteam001/OpenSteamTool) — Upstream Foundation"
> "[huanyuejue/OpenSteamTool](https://github.com/huanyuejue/OpenSteamTool) — Upstream Provider & Manifest Optimizations"
> "[mmxlyo/OpenSteamTool](https://github.com/mmxlyo/OpenSteamTool) — Denuvo Scheme 2 & Tooling Upstream"

安装方式（README「Manual Sideload DLL Proxy Mode」）要求把 `SteamDaddy.dll` 与 `version.dll`（或 `dwmapi.dll`）放进 Steam 根目录，目录名仍是 `<Steam Root>\config\stplug-in\`（**不是** OpenSteamTool 的 `config/lua`）。

配置文件名 `steamdaddy.toml`，撤回兼容：

> "**Backwards Compatibility**: If `steamdaddy.toml` is not present, SteamDaddy automatically checks for `opensteamtool.toml` as a seamless fallback."

### 2.2 实际 zip 内容（已下载解包，非推测）

`SteamDaddy.Dll-v4.0.7-Release.zip`（HTTP 200，2 983 889 B）条目与大小：

```
cloud_redirect.dll   1731072
dwmapi.dll            138752
ost-manager.exe      1571328
SteamDaddy.dll       1597952
steamdaddy.toml           25
version.dll.disable   135168
xinput1_4.dll         134656
```

对照 `OpenSteamTool-1.4.8-Release.zip`（HTTP 200，1 160 226 B）：

```
dwmapi.dll            109568
dwmapi.exp            11722
dwmapi.lib            20586
lua_static.lib      1191406
OpenSteamTool.dll    1446400
xinput1_4.dll         122880
xinput1_4.exp          2502
xinput1_4.lib          4344
```

结论（均由解包结果直接得到）：

| 问题 | 答案 |
|---|---|
| 是否 fork OpenSteam001/OpenSteamTool | **是**（API `fork: true` + `parent`；README「Upstream Foundation」） |
| 是否同一组三个 DLL | **否**。没有 `OpenSteamTool.dll`；主载荷是 `SteamDaddy.dll`；代理 DLL 是 `version.dll.disable`（带 `.disable` 后缀，需改名才生效）和 `dwmapi.dll`（大小也不同：138 752 vs 109 568；`xinput1_4.dll` 134 656 vs 122 880） |
| 是否含 `version.txt` | **不含**。SteamDaddy zip 无 `version.txt`；OpenSteamTool zip 同样**不含** `version.txt` |
| zip 布局是否与 OpenSteamTool 一致 | **不一致**。OpenSteamTool 是构建产物平铺（3 个可部署 DLL + `*.lib`/`*.exp` + `lua_static.lib`）；SteamDaddy 是「自定义载荷 + 代理 + 附带工具 + 配置」平铺（多出 `cloud_redirect.dll`、`ost-manager.exe`、`steamdaddy.toml`） |

补充事实：本项目自身的 `dlls/version.txt` **是本工具写出来的**，不是从任何 zip 读的——见「5. 对本项目设计决策的直接含义」。

---

## 3. 对照：OpenSteam001/OpenSteamTool

`GET https://api.github.com/repos/OpenSteam001/OpenSteamTool` → **HTTP 200**；`/releases/latest` → **HTTP 200**；HTML `/releases/latest` → **302** → `/releases/tag/1.4.8`。

### 3.1 最新 release

```json
"tag_name": "1.4.8",          // 注意：无 "v" 前缀
"name": "1.4.8",
"published_at": "2026-06-13T12:18:05Z",
"html_url": "https://github.com/OpenSteam001/OpenSteamTool/releases/tag/1.4.8",
"author": {"login": "github-actions[bot]"}
```

资产（两项，均有 sha256 digest 与 download_count）：

| name | size | digest | download_count | browser_download_url |
|---|---|---|---|---|
| `OpenSteamTool-1.4.8-Debug.zip` | 29 039 044 | `sha256:7dbe4dcf0210bdb9d14edb925a4deadcbb39e67d89772f6e886322f27d7d4aec` | 69 574 | `https://github.com/OpenSteam001/OpenSteamTool/releases/download/1.4.8/OpenSteamTool-1.4.8-Debug.zip` |
| `OpenSteamTool-1.4.8-Release.zip` | 1 160 226 | `sha256:966654604d258d5d5383e72fec616df7957bf86f760c67eeb3d0e18cd882c710` | 607 128 | `https://github.com/OpenSteam001/OpenSteamTool/releases/download/1.4.8/OpenSteamTool-1.4.8-Release.zip` |

Release notes（`body`）自述安装方式：

> "Download `OpenSteamTool-1.4.8-Release.zip` (or the `-Debug` variant if you need logging to `<steam>/opensteamtool directory`)."
> "Extract and copy `dwmapi.dll`, `xinput1_4.dll` and `OpenSteamTool.dll` to your Steam root directory (e.g. `C:\Program Files (x86)\Steam`)."

历史 release（`/releases` 单页 14 条，均为「同版本 Debug+Release 双 zip」）：`1.4.8`(2026-06-13)、`v1.4.7`(2026-06-03)、`v1.4.6-fix`(2026-05-28)、`v1.4.6`(2026-05-28)、`v1.4.5`(2026-05-25)、`v1.4.4`(2026-05-20)、`v1.4.3`(2026-05-17)、`v1.4.2`(2026-05-11)、`v1.4.1`(2026-05-06)、`v1.4.0`(2026-05-04)、`v1.3.2`(2026-05-02)、`v1.3.1`(2026-05-02)、`v1.3.0`(2026-05-01)、`v1.2.0`(2026-04-30)。→ **tag 命名不统一**（`1.4.8` 无前缀，其余带 `v`），资产名统一为 `OpenSteamTool-<tag>-{Debug,Release}.zip`。

### 3.2 仓库活跃度

```json
"archived": false, "disabled": false,
"created_at": "2026-04-24T01:00:27Z",
"pushed_at": "2026-07-06T09:28:57Z",
"stargazers_count": 2616, "forks_count": 461,
"open_issues_count": 90,
"has_issues": true, "has_discussions": true
```

最后 3 个 commit（`/commits?per_page=3`）：

```
2a08b0bf 2026-07-06T09:28:56Z Fix Cloud RPC timing (vtable hooks) and add stats sync support (#153)
00984b41 2026-06-25T03:14:59Z Added CloudRedirect support (Cloud saves/luas/achivements/etc) (#138)
32350a8b 2026-06-23T03:20:00Z add Chinese (Simplified) README and language switcher entry (#130)
```

→ **最后一次 commit `2026-07-06`**，比最新 release（`2026-06-13`）晚；即 release 之后有未发版的代码。仓库**未归档**。

### 3.3 Issue 规模（"不工作"类）

- 开放 issue 总数：**73**（`search/issues?q=repo:OpenSteam001/OpenSteamTool is:issue is:open` → `total_count: 73`）；开放 PR：**17**；`open_issues_count: 90` = issue + PR。
- 关键词命中（同一搜索 API，`is:issue is:open`，全仓储文本检索）：

| 关键词 | 开放 issue 命中数 |
|---|---|
| `"not working"` | 10 |
| `"not work"` | 12 |
| `"broken"` | 3 |
| `"crash"` | 3 |
| `"failed"` | 6 |
| `"stuck"` | 1 |
| `"doesn't work"` | 0（分词/撇号导致，非真实 0） |
> 说明：`"not work"` 是 `"not working"` 的子串，两项计数重叠，不得相加。

→ 量级估计：**十余个（约 10–20）开放 issue 直接说"不工作/失效"**，且集中在「Steam 更新后签名/IPC spec 缺失」这一模式。已核实的典型标题（`/issues/<n>`）：

- `#189 [DOCS] steamclient64.dll updated, OST is not work. maybe need update ve…`
- `#171 [BUG] OpenSteamTools  Not working now`
- `#168 [BUG] No longer working as of today`
- `#213 [BUG] This morning the Steam client was updated and OpenSteam Tools st…`
- `#216 [BUG]OpenSteamTool: signature file not found for steamclient64.dll.`
- `#223 [BUG] OpensteamTool: IPC spec file notfound.`（body 原文："OpenSteamTool: IPC spec file not found. IPC interception is disabled for this session; pattern-based hooks are unaffected."）
- `#225 [BUG]`（body："cannot open game Aion2 from steam…"）
- `#226 [BUG] 1.• Unsupported Steam Version / IPC spec missing.`
- `#229 [BUG] IPC spec missing for Steam Build 1792618285`
- `#232 [BUG] Missing signature for steamui.dll cd9226db…toml`
- `#228 [BUG] Missing signatures for Build 1794013917`
- `#186 [FEATURE] Is it possible to build a new release`（body："I noticed there are new updates in the code area, but the release has not been rebuilt."）
- Discussions（11 个，含）：`OpenSteamTool在10月15日已失效 OpenSteamTool broke on October 15th.`（`/discussions/217`）、`doesnt work with latest steam updae`（`/discussions/207`）

---

## 4. 其他活跃的、发布同款 DLL zip 的下游 fork

方法：`GET /repos/OpenSteam001/OpenSteamTool/forks?sort=newest&per_page=100`（HTTP 200，100 条）→ 对每个 fork 调 `GET /repos/<fork>/releases/latest` → 对 `*-Release.zip` 逐个下载解包，核对文件集。**下表由脚本按 API 原始字节生成，不是手抄。**

### 4.1 有 release 且已解包核对的下游 fork（按 published_at 倒序）

表内 **16 行** = 抽样 100 个 fork 中有 release 的 **15** 个（含 `Contrary-v2/SteamDaddy-Dll`）+ 额外单独验证的 `huanyuejue/OpenSteamTool`（不在 newest-100 抽样内）。

| repo | stars | 最新 tag | published_at | Release 资产名 | zip 内 `OpenSteamTool.dll`/`dwmapi.dll`/`xinput1_4.dll` | 其他 DLL | 含 `version.txt` | 含 version 代理 DLL |
|---|---|---|---|---|---|---|---|---|
| `eisora08/OpenSteamTool` | 1 | `v1.4.9.0` | `2026-11-13T21:10:45Z` | `OpenSteamTool-v1.4.9.0-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `huanyuejue/OpenSteamTool` | 357 | `1.5.5.4` | `2026-11-13T08:34:28Z` | `OpenSteamTool-1.5.5.4-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 是 |
| `matheeshapathirana/OpenSteamTool` | 3 | `v1.5.0` | `2026-11-12T15:28:02Z` | `OpenSteamTool-v1.5.0-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `Contrary-v2/SteamDaddy-Dll` | 18 | `v4.0.7` | `2026-11-12T03:45:02Z` | `SteamDaddy.Dll-v4.0.7-Release.zip` | 2/3 (dwmapi.dll, xinput1_4.dll) | cloud_redirect.dll, SteamDaddy.dll | 否 | 是 |
| `LotusHirasawaSusumu/OpenSteamTool` | 0 | `0.0.1` | `2026-11-01T20:19:30Z` | `OpenSteamTool-0.0.1-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `leonardoneos/OpenSteamTool` | 0 | `v2.6.5` | `2026-10-25T22:38:45Z` | `OpenSteamTool-v2.6.5-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `railzfh/OST` | 0 | `v1.4.9` | `2026-10-25T16:38:20Z` | `OpenSteamTool-v1.4.9-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `isxlan0/OpenSteamTool` | 3 | `v1.4.8` | `2026-10-22T05:23:07Z` | `Release.7z` | 0/3 (—) | — | 否 | 否 |
| `Arielfoever/OpenSteamTool` | 0 | `v1.4.9-my` | `2026-10-20T11:48:05Z` | `OpenSteamTool-v1.4.9-my-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `PandaQuQ/OpenSteamTool` | 7 | `v1.4.8-fixtoml` | `2026-10-16T19:20:12Z` | `OpenSteamTool-v1.4.8-fixtoml-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `fyisdy/Traditional_Hook-mmxlyo-s-OpenSteamTool-` | 0 | `v1.0.0` | `2026-10-16T04:24:11Z` | `config.ini, ost-Injector.exe` | 0/3 (—) | — | 否 | 否 |
| `moxuna/OpenSteamTool` | 0 | `v1.4.8-fix` | `2026-10-15T12:47:10Z` | `OpenSteamTool-v1.4.8-fix-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `MEYGag/OpenSteamTool` | 0 | `v1.0.1` | `2026-10-15T04:27:45Z` | `OpenSteamTool-v1.0.1-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `ssyangchn/OpenSteamTool` | 0 | `v1.4.8.1` | `2026-09-20T09:09:51Z` | `OpenSteamTool-v1.4.8.1-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `qwqqvq123456/OpenSteamTool` | 0 | `1.1` | `2026-09-13T15:34:27Z` | `OpenSteamTool-1.1-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |
| `hazre/OpenSteamTool` | 2 | `1.4.8-hazre.1` | `2026-09-13T02:48:14Z` | `OpenSteamTool-1.4.8-hazre.1-Release.zip` | 3/3 (OpenSteamTool.dll, dwmapi.dll, xinput1_4.dll) | — | 否 | 否 |

### 4.2 采样中「无 release」的 fork

同一批 100 个抽样 fork 中，有 **85** 个的 `/releases/latest` 返回 **HTTP 404**（响应体 `Not Found`）。
下面的子表只列「stars ≥ 1 或 pushed_at 晚于上游最后提交 `2026-07-06T09:28:57Z`」的 23 个（其余 404 多为纯 fork，无自定义提交）：

| repo | stars | pushed_at | `/releases/latest` |
|---|---|---|---|
| `cookie42073-cmd/OpenSteamTool` | 0 | `2026-11-14T01:40:25Z` | 404 |
| `ANDonekey/OpenSteamTool` | 1 | `2026-11-12T20:04:33Z` | 404 |
| `herknight123/OpenSteamTool` | 0 | `2026-11-03T15:01:44Z` | 404 |
| `BillyCurtis/OpenSteamTool` | 0 | `2026-11-03T01:38:01Z` | 404 |
| `TRTSC/mmxlyo-OpenSteamTool` | 0 | `2026-10-28T00:29:17Z` | 404 |
| `youdianwuliao/OpenSteamTool` | 0 | `2026-10-26T05:41:01Z` | 404 |
| `Darksoan/OpenSteamTool` | 0 | `2026-10-25T15:41:22Z` | 404 |
| `Contrary7/SteamDaddy-Dll` | 21 | `2026-10-23T12:19:14Z` | 404 |
| `jelinet/OpenSteamTool` | 0 | `2026-10-16T08:39:15Z` | 404 |
| `CSSZYF/OpenSteamTool` | 0 | `2026-10-15T15:29:26Z` | 404 |
| `AllRange1337/OpenSteamTool` | 10 | `2026-10-15T09:52:59Z` | 404 |
| `HisenWeb/OpenSteamTool` | 0 | `2026-10-07T04:23:58Z` | 404 |
| `Gruddieu/OpenSteamTool` | 0 | `2026-09-26T14:00:24Z` | 404 |
| `Acker777/OpenSteamTool` | 0 | `2026-09-26T06:41:52Z` | 404 |
| `yogiant333/OpenSteamTool` | 0 | `2026-09-24T09:15:03Z` | 404 |
| `1ceshadow/OpenSteamTool` | 0 | `2026-09-23T08:15:59Z` | 404 |
| `YS-CODE-GO/OpenSteamTool` | 0 | `2026-09-14T11:24:12Z` | 404 |
| `sigmazone/OpenSteamToolA` | 0 | `2026-09-13T12:47:49Z` | 404 |
| `Berkecann/OpenSteamTool` | 2 | `2026-09-13T09:41:44Z` | 404 |
| `targink/OpenSteamTool` | 0 | `2026-09-13T03:54:07Z` | 404 |
| `kovacevicanita16-afk/OpenSteamTool` | 1 | `2026-07-06T09:28:57Z` | 404 |
| `laobamac/OpenSteamTool` | 1 | `2026-07-06T09:28:57Z` | 404 |
| `lucienlmy/OpenSteamTool` | 1 | `2026-07-06T09:28:57Z` | 404 |

### 4.3 与设计决策相关的补充事实（均由上述解包结果直接得到）

- `huanyuejue/OpenSteamTool` 是 stars 最高的 fork（357），README 首段明确声明 fork 关系（原文）：「**Fork 说明：** 本项目是 [OpenSteam001/OpenSteamTool](https://github.com/OpenSteam001/OpenSteamTool) 的优化分支，为适配 Steam 入库工具 [huanyuejue/Fluent-Steam-Lua](https://github.com/huanyuejue/Fluent-Steam-Lua) 而开发。」其 Release zip 内除三件套外**多一个 `version.dll.disabled`**（注意后缀是 `.disabled`，与 SteamDaddy 的 `version.dll.disable` 不同），仍**无 `version.txt`**。
- 上表中只有 `Contrary-v2/SteamDaddy-Dll` 的 zip **不含 `OpenSteamTool.dll`**（2/3：只有 `dwmapi.dll`、`xinput1_4.dll` 同名，且体积不同）。
- `huanyuejue` 的两个资产都带 `sha256` digest（`50e81416…`、`e954c67b…`）；`Contrary-v2` 的 `v4.0.7` 资产同样带 `sha256:5f614cd0…`。
- `isxlan0/OpenSteamTool` 的唯一资产是 **`Release.7z`**（无 zip、无版本号）；`7z l` 实测内含 `dwmapi.dll`、`OpenSteamTool.dll`、`xinput1_4.dll` + `*.lib/*.exp` + `lua_static.lib`。
- `railzfh/OST` 的 Release zip 额外打包了签名/IPC 元数据目录（解包实测）：`opensteamtool/`、`opensteamtool/ipc/`、`opensteamtool/ipc/steamclient/`、`opensteamtool/ipc/steamclient/253e0a11a4415b38c78a42f7bdd6ecc426455f06c1ba6453a48090df3142dbbc.toml`、`opensteamtool/pattern/`、`opensteamtool/pattern/steamclient/`、`opensteamtool/pattern/steamclient/253e0a11a4415b38c78a42f7bdd6ecc426455f06c1ba6453a48090df3142dbbc.toml`、`opensteamtool/pattern/steamui/`、`opensteamtool/pattern/steamui/cd9226db4e6c28942df70c22c0475d91fdd62cba5f100baae2d65af57cabb610.toml`。
- `fyisdy/Traditional_Hook-mmxlyo-s-OpenSteamTool-` 的 release **不是 zip**，只有 `config.ini` 与 `ost-Injector.exe` 两个资产。
- `Contrary7/SteamDaddy-Dll`（21 stars、fork 列表里 pushed `2026-10-23T12:19:14Z`）仓库本体 `GET /repos/Contrary7/SteamDaddy-Dll` 返回 **404**（已删除/改名/转私有）；SteamDaddy README 顶部徽章仍指向它。同理 `mmxlyo/OpenSteamTool`（SteamDaddy README 列为上游）也返回 **404**，只剩 `TRTSC/mmxlyo-OpenSteamTool` 这个 fork（其 `/releases/latest` 404）。

### 4.4 未核实项

- **UNVERIFIED**：issue/discussion 中用户主动贴 fork 下载链接的条目。已做：`search/issues` 关键词 `fork`（20 命中，标题全为 bug/feature）、`SteamDaddy`（7 命中，标题全为 bug）、`huanyuejue`（1 命中 = `#228`，正文是 bug 模板）；读取 `discussions/217`、`discussions/207` 正文与评论（无评论）。**未**逐条读完全部 73 个开放 issue 的 100+ 条评论，故只能说「本次抽查未发现」，不能断言「不存在」。
- **UNVERIFIED（穷举性）**：fork 共 461 个，本次只抽样 `sort=newest&per_page=100` 的前 100 条（即最新 fork），未继续翻页。因此 4.1 的 15 个「有 release 的 fork」是**已抽样验证的子集**，不是全集。
- **UNVERIFIED**：4.1 表中各 fork 的 zip **签名/来源可信度**（是否由 GitHub Actions 构建、是否有 digest）。已核三家资产都带 `digest` 字段（上游见 3.1；`Contrary-v2` 与 `huanyuejue` 见本轮抽查）；其余 fork 未逐一核对。

---

## 5. 许可证、再分发 / 下架风险表述

### 5.1 License

| 仓库 | GitHub 识别 | LICENSE 文件 |
|---|---|---|
| `OpenSteam001/OpenSteamTool` | `"license": {"key": "gpl-3.0", "name": "GNU General Public License v3.0", "spdx_id": "GPL-3.0"}` | `LICENSE`（35 149 B）首行：`GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007` |
| `Contrary-v2/SteamDaddy-Dll` | `"license": {"key": "gpl-3.0", "spdx_id": "GPL-3.0"}` | `LICENSE`（35 149 B，与上游同尺寸）首行同为 `GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007` |
| 4.1 表全部 16 个仓库 | 抽查 3 个（`OpenSteam001`、`Contrary-v2`、`huanyuejue`）为 `GPL-3.0`；其余 13 个未逐一核对 | — |

### 5.2 明确的再分发 / fork / 下架表述

**没有任何一条关于再分发、fork 政策或 DMCA/下架风险的显式声明。** 逐项核实：

- OpenSteamTool `README.md` 中 `grep -i "licen|fork|redistribut|DMCA|takedown"` → 唯一命中是 Disclaimer 段：

  > "## Disclaimer
  > This project is provided for research and educational purposes only. You are responsible for complying with local laws, platform terms of service, and software licenses."

  （README 无 License 小节，也未链接 `LICENSE`。）
- SteamDaddy `README.md` 中同样 `grep -i "fork|dmca|redistribut|version.txt"` → **0 命中**（README 只谈上游/许可徽章与安装方式）。
- Issue 搜索（全仓储，含评论）：`"DMCA"` → 0；`"takedown"` → 0；`"redistribut"` → 0；`"GPL"` → 1；`"fork and release"` → 0。
- SteamDaddy-Dll 没有 Issues 功能（`has_issues: false`），因此**不存在**该仓库的 issue 讨论可供引用。
- 两个仓库的 `.github/` 只含 `ISSUE_TEMPLATE` / `workflows`（SteamDaddy：`dir ISSUE_TEMPLATE`、`dir workflows`），未见贡献者政策类文件。

→ 事实层面可说的只有：两份代码库同为 GPL-3.0；README 里的唯一"立场"性文字是 OpenSteamTool 的 research/educational 免责声明。**再分发是否被允许/是否触发下架，属于许可证与法律解读，本调研不做结论（本文件不含法律意见）。**

---

## 6. 对本项目（opensteamtool-gui-rs）设计决策的直接含义

（以下为仓库内已有事实，供决策引用；非推测）

- 当前下载源硬编码为上游 latest：
  `src/updater.rs:14` → `"https://api.github.com/repos/OpenSteam001/OpenSteamTool/releases/latest"`（另一常量 `:16-17` 指向本项目自身 releases）。
- 当前提取逻辑「只取目标 DLL 集合成员，成功后写 `version.txt`」：
  `src/updater.rs:191` → `/// 从内存 zip 中仅提取目标 DLL 集合成员写入 dll_dir，成功后写 version.txt。`；`src/updater.rs:226-227` → `.map_err(|e| UpdateError::Io(format!("write version.txt: {e}")))?`
- `version.txt` 是本工具自己的版本标记文件：`src/dll.rs:9` → `pub const VERSION_FILE: &str = "version.txt";`；ADR `docs/adr/0011-patch-missing-state-and-guidance.md:11` 明确「`version.txt` 记录不作数」，判据以 `dlls/` 三个目标 DLL 齐全为准。
- 由此与本次事实直接冲突的两点：
  1. 上游/下游 release 包 **都不含 `version.txt`**：抽核的 3 个 `*-Release.zip`（上游、`Contrary-v2`、`huanyuejue`）均不含 `version.txt`，其余 13 个未逐一核对；另行单独解包的上游 `OpenSteamTool-1.4.8-Release.zip` 与 `isxlan0` 的 `Release.7z` 同样不含。所以 `version.txt` 只能由本工具按 release `tag_name` 写（上游 1.4.8 zip 实测条目见 2.2）；tag 前缀不统一（OST `1.4.8` 无前缀，SteamDaddy `v4.0.7` 有；huanyuejue `1.5.5.4` 无；isxlan0 `v1.4.8` 有）。
  2. SteamDaddy zip 里**没有 `OpenSteamTool.dll`**，且代理 DLL 名为 `version.dll.disable`；若把下载源切到 SteamDaddy-Dll，现有「提取三个 DLL 名」的路径会拿不到 `OpenSteamTool.dll`（`dwmapi.dll`、`xinput1_4.dll` 存在但字节不同）。
- 资产命名不统一，无法用固定模板拼接：`OpenSteamTool-1.4.8-Release.zip`（OST）、`SteamDaddy.Dll-v4.0.7-Release.zip`（SteamDaddy，只此一个资产）、`Release.7z`（isxlan0，无版本号、且非 zip）。→ 需按 API 返回的 `assets[].name` 匹配，而非按 tag 拼名。

---

## 附：本轮所有已发起的 HTTP 请求与状态汇总

| URL | 状态 |
|---|---|
| `api.github.com/repos/Contrary-v2/SteamDaddy-Dll` | 200 |
| `api.github.com/repos/Contrary-v2/SteamDaddy-Dll/releases` | 200 |
| `api.github.com/repos/Contrary-v2/SteamDaddy-Dll/releases/latest` | 200 |
| `raw.githubusercontent.com/Contrary-v2/SteamDaddy-Dll/main/README.md` | 200 |
| `raw.githubusercontent.com/Contrary-v2/SteamDaddy-Dll/main/LICENSE` | 200 |
| `github.com/Contrary-v2/SteamDaddy-Dll/releases/download/v4.0.7/SteamDaddy.Dll-v4.0.7-Release.zip` | 200 |
| `api.github.com/repos/OpenSteam001/OpenSteamTool` / `/releases/latest` / `/commits` | 200 / 200 / 200 |
| `github.com/OpenSteam001/OpenSteamTool/releases/download/1.4.8/OpenSteamTool-1.4.8-Release.zip` | 200 |
| `raw.githubusercontent.com/OpenSteam001/OpenSteamTool/main/README.md`、`/LICENSE` | 200 / 200 |
| `api.github.com/repos/huanyuejue/OpenSteamTool`（及 releases / README） | 200 |
| `api.github.com/repos/Contrary7/SteamDaddy-Dll`、`api.github.com/repos/mmxlyo/OpenSteamTool` | 404 |
| `api.github.com/repos/<每个抽样 fork>/releases/latest`（对 100 个 fork 逐个请求） | **15 × 200**（有 release）、**85 × 404** |
| `api.github.com/search/issues`（多关键词） | 200（未认证时曾遇 403 rate limit，改用认证 `gh api` 后全部 200） |
| `api.github.com/graphql`（discussions 217/207） | 200 |
