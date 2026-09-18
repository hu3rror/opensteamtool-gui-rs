# 忙碌互斥收敛为 BusyGate 单一门禁

交互类后台操作（「动作」组合 / 「更新动作」）的忙碌态此前分散在多个标志位，按钮禁用与阶段文案各管各的，存在「忙碌但无种类」的失效态风险。决定：收敛为 `busy.rs` 的单一门禁，类型级不变量。

- **范围**：只互斥交互类后台操作——「动作」（Action 组合）与「更新动作」（检查更新 / 下载并解压）。compat 探针/刷新/预热是只读后台，不进门禁（互斥由 compat_flow 在途去重承担，见 ADR-0006）。
- **类型即不变量**：`BusyGate` 持有唯一 `Option<BusyKind>`，不存在「忙碌但无种类」的状态；`BusyKind`（Deploying/Uninstalling/Launching/Checking/Downloading/ClosingSteam）从 workflow 迁入 busy 模块。
- **接口**：`start(kind) -> bool`（空闲放行并置位 / 忙碌静默拒绝）、`replace(kind)`（阶段更新，仅忙碌时合法，debug 断言兜底）、`clear()`（完成消息统一出口，幂等）、`current()`、`is_busy()`。
- **确认弹窗悬挂期不算忙碌**：`request_action` 先查门禁（忙碌连确认框都不弹），真正 `start` 在确认后发生；悬挂期由 Modal 自行阻断其余交互。
- **渲染收敛**：动作区与更新按钮禁用源统一 `!gate.is_busy()`；底部忙碌文案取 `current()`；语言/设置/路径输入/浏览不禁用；compat 类按钮继续由 compat_flow 在途标志禁用。

验收：门禁单测覆盖 start 防重入、clear 幂等、replace 阶段更新/空闲断言、全变体完整性。
