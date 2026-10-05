# Steam 路径单一写入点：工作值进编排核心、`CommitPath` 唯一切入（Path Commit）

切片 1（ADR-0019）后 `steam_path` 仍是壳字段，两处写点（设置页提交 / 向导 Finish）各自重复「写 config 镜像 → persist → 刷新事实 → 重探体检」链条——路径值无单一属主、编排决策不可测。D4（架构勘察）提案「工作路径 + 编辑缓冲分离、单一写入点」，Q5 拍板成型并折叠进 ADR-0019 切片 2。决策：工作路径迁入编排核心，`AppEvent::CommitPath` 是唯一写入缝；编辑缓冲留壳（`SteamPathEditor`）。

## 为什么做

- 路径变更的编排（何时刷新文件事实、何时重探体检）是**决策**：两处调用点各自手写同一链条，行为漂移风险高、不可测（bug 藏身处与切片 1 同类）。
- 缓冲（编辑中的文本）与工作值（已生效路径）是两种生命周期：缓冲属于设置对话框 UI 态，工作值属于域状态——值不双持，写点不双开。
- 插图上 D4 与切片 1 同源：App 级决策统一进编排核心，壳只执行效果。

## 决策

- **值归属**：工作路径是编排核心字段（`AppCore::steam_path: String`），快照暴露（`Snapshot.steam_path`，构造注入与 `CommitPath` 均归一 trim）——壳不再持有 `steam_path` 字段（删除双真相）。
- **唯一写缝**：`AppEvent::CommitPath(String)`：核心 trim 存储，返回 `[RefreshFacts, FeedCompatPath]`（顺序与现状一致：先刷文件事实再重探体检）。**不做合法性复校**——设置侧判据在 `SteamPathEditor::submit`（编辑器是缓冲的属主），向导侧终局判据在向导内；行为等价，不收紧（向导步骤 1/2 关窗无条件写携带值的现状显式保留）。
- **config 镜像写点留壳**：`Config::steam_path` + `persist_config()` 仍在两处壳调用点（设置提交 / 向导 Finish）——config 属壳（ADR-0019 边界），且向导 Finish 需要语言/主题/路径**一次原子写**；「单一写入点」指路径域状态与编排，不指 config 文件（诚实标注：config 镜像可能短暂保留未 trim 的向导携带值，读点全走核心快照，无观察点）。
- **效果承载力**：`RunPlan` / `SpawnWorkflow` 效果携带 `steam_dir` payload（决策时刻注入，效果自足）；`RefreshFacts` 保持无 payload，执行器从快照取（产出源 4 个事件分支 + 3 处壳直调，统一查快照是唯一一致解）。
- **新效果**：`AppEffect::FeedCompatPath(String)`——壳执行器喂 `compat_flow::Event::PathChanged`（体检流程仍是壳子域状态机，切片 3 前不迁移；决策进核心、执行留壳）。壳原 `feed_path_changed` 直调删除，启动首次喂归 App::new。

## 否决的方案

- **PathStore 独立类型**：单 String + 单缝不需要包装（编辑缓冲已在壳），推测性泛化。
- **config 写收进核心**（效果化 persist）：向导被迫拆两次原子写（语言/主题一次、路径一次），破坏现状单次写语义。
- **核心复校路径合法性**：`is_valid_steam_dir` 出现第三个实现点（judge 重复），且打断「向导终局无条件提交」的行为等价。
- **`RefreshFacts` 带 steam_dir payload**：4 个事件分支各带一次重复参数，与 3 处壳直调（本就查快照）分裂成两套路径源。

## 验收

- 行为等价：设置页提交有效路径 → 部署状态/操作按钮刷新；向导终局（含步骤 1/2 关窗）→ 路径生效、config 一次原子写；提交路径 → 体检重探（PathChanged）；非法路径设置页仍拒绝。
- 编排核心新增测试：`CommitPath` trim + 存储 + 效果序 `[RefreshFacts, FeedCompatPath]` + 快照暴露；其它域状态不受污染。
- `cargo test --workspace` / `cargo check --workspace --all-targets` / `cargo clippy --workspace --all-targets` / `cargo fmt --all --check` 全绿。