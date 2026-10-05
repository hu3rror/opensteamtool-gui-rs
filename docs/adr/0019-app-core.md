# 编排核心：App 级「事件 → 效果 → 快照」纯状态机（AppCore）

ADR-0006/0007/0008 承诺「App 只喂事件、执行效果并渲染展示态」，但兑现方（App 自己）未提炼——`ui.rs` 3589 行、38 字段把消息分发、动作编排、每帧快照构建、渲染与 eframe 接线混排在同一文件，App 级编排逻辑零测试（bug 藏身于消息顺序、确认框生命周期、门禁时序）。决策：提炼第四台状态机「编排核心」，与既有子域状态机同构。

## 为什么做

- 三个子域状态机（体检流程 / 更新流程 / 忙碌门禁）与向导已经各自兑现「事件 → 效果」模式并带测试；App 作为它们的宿主，自身却没沿用这套模式——编排决策（何时刷新、何时弹确认、门禁放行次序、向导迟到下载分支）全部埋在不可测的私有方法里。
- `ui.rs` 是改动最频繁的文件（近 25 次提交动 14 次）：编排不可测意味着每次改动都靠手动验证兜底，回归成本随界面膨胀。
- 提炼后测试面 = interface 面：消息乱序与迟到丢弃、确认框生命周期、门禁拒绝路径、向导终局后迟到下载全部可测。

## 决策

- **第四台状态机**：新模块 `app_core`（编排核心）对外接口为**事件输入 → `Vec<AppEffect>` 效果输出 → 只读快照**，与 compat_flow/wizard 的 `step(event) -> (快照, Vec<Effect>)` 同构。
- **边界（编排核心 only）**：编排核心持有全部域状态（gate / confirm / notice / update_flow / flow / wizard / steam_running / steam_path / status / local_version 等）与四个子域状态机（内部 seam）；egui 基础设施（ctx / tray / logo / palette 同步 / 窗口显隐）、设置对话框纯 UI 旗标（settings_open / settings_tab / settings_steam / app_update / compat_details_open 等 7 个）、strings / palette 留在外壳。
- **零 IO**：编排核心不碰文件 / 进程 / 网络 / egui / i18n；一切外部操作由外壳执行其产出的效果（spawn 线程、refresh 事实、persist、feed_path_changed…），结果以事件（Msg）喂回。文件事实（部署状态 / 本地版本 / DLL 齐全）由 DeploymentFacts 快照在既定刷新点收敛、以事件入编排核心、快照出给渲染（与 ADR-0012「只存用户选择」不冲突，内存快照非落盘）。
- **统一效果枚举**：编排核心事件输入产出单一 `AppEffect` 枚举，外壳只有一个 executor；compat_flow::Effect / wizard::Effect 作为内部枚举被包裹，不进 interface。
- **渲染只消费快照**：快照含 update_flow 派生结果（含下载 payload）；`MainEvent::Download` 去掉 `OnlineInfo` payload，编排核心自查自产下载效果——版本号不出编排核心（ADR-0014 语义收紧一层，渲染路径不再现查 `derived()`）。
- **无 App 级代数**：App 级消息互斥由忙碌门禁承担（ADR-0007 同时刻仅一个操作在途），不引入 epoch；compat_flow 的代数只服务其自身的并发探针。
- **增量迁移**：切片 1 消息决策核心 → 切片 2 动作编排（含 Steam 路径单一写入点，D4 顺手合流）→ 切片 3 效果统一 + 快照定型；每步行为等价、编译 / 测试 / clippy 全绿再走下一步。

## 否决的方案

- **注入持有式**（构造注入 `Arc<SteamState>`、dll_dir 等，编排核心可读 IO）：测试需搭假对象、interface 变宽；「事实以事件喂回」已覆盖同等能力，纯状态机的测试即 interface，零 mock。
- **全量非 egui 边界**（设置对话框状态与配置镜像也进编排核心）：纯 UI 旗标过不了 deletion test（删除后复杂度原地消失），只会撑宽 interface。
- **按域分派效果**（保留 exec_compat_effects / exec_wizard_effects 多 executor）：决策与执行各有多个入口，interface 碎裂，测试断言形状不统一。

## 验收

- 行为等价：应用 / 卸载 / 更新 / 向导 / 托盘 / 单实例全链路对照现有 UI 手动验收；既有测试全绿。
- 新增编排核心测试：消息乱序与迟到丢弃、确认框生命周期（request_action → confirm → start_action）、门禁拒绝路径、向导终局后迟到下载、快照派生。
- `cargo test --workspace` / `cargo check --workspace --all-targets` / `cargo clippy --workspace --all-targets` / `cargo fmt --all --check` 全绿。

## 修订（2026-10，切片 3 落地，issue #46）

切片 3（主盘）交付后的实现差异，逐条对上文：

- **子域状态机数量**：上文写四台；现状为**三台在核心**（更新流程 / 忙碌门禁 / 体检流程）——「首次运行向导」因内部文件系统判据（`is_valid_steam_dir` 每步回显、`target_dlls_present` 就绪判据）与语言/主题即改即存的壳副作用链，**留壳**，其效果（下载 / 终局）收编进 `AppEffect` 家族由统一执行器执行。向导状态机迁移留待后续切片，GLOSSARY 词条已按现状订正。
- **效果统一**：上文「单一 `AppEffect` 枚举、外壳只有一个 executor」已兑现——compat（探针/刷新/预热）与向导（下载/终局）效果平铺进家族（内部枚举被包裹、不进 interface）；壳侧 `exec_compat_effects` / `exec_wizard_effects` 已删除，统一执行器 `exec_app_effects` 是全壳唯一效果执行函数。
- **窗口显隐**：上文「egui 基础设施（…窗口显隐）留外壳」——执行仍留壳（唯一物理写入点 `set_window_visible`），但 **Steam 联动决策迁入核心**：`window_visible` 镜像（`AppEvent::WindowVisibleChanged`，壳物理写入后回喂）+ Steam 边沿事件（`SteamStarted`/`SteamStopped`）驱动 auto-tray 表决策，产出 `AppEffect::SetWindowVisible`；工作流后隐窗（RescanSteam）改核心内 pending 合成，删除壳执行器隐窗与「唯一发射者」注释债。托盘 / 单实例 / 最小化显隐仍壳决策，经同一写入点同步镜像。
- **P5 反馈环统一**：`RefreshFacts` 与 compat 路径回喂全部经效果队列（`queue.extend(core.step(…))`），不再嵌套调事件入口；壳侧 `refresh_facts()` 拆为「探测+同步」与「队列回喂」两步，队列深度注释由 ≤ 2 更新为 ≤ 3。
- **快照**：`Snapshot.compat`（体检域含报告/次数/预热错误，零 i18n）；窗口显隐镜像不进快照（避免第二事实源）。渲染仍直读向导视图一处在 GLOSSARY 显式标注。
