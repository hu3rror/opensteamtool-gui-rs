# 在线更新：检查结果单一事实源

「检查更新」的结果此前双存——消息 payload 一份、渲染状态一份；「本地版本 vs 线上版本」的比较散落在行文案、通知、下载按钮三处消费。决定：`update_flow` 模块持有唯一检查结果，派生出全部下游消费。

- **唯一事实源**：状态 `Idle / Checking / Checked(Result)`，结果只存这一份；`Notice::UpdateChecked` 降级为无 payload 标记（仅「显示检查结果」）。
- **单一派生**：`derived(local_version) -> UpdateDerived { notice, download }`——通知分类、下载可用性两处消费全部从它产出，「本地版本 == 线上版本」的比较只活在这一处（主页面版本行随 #32 删除后，`line` 行分类一并移除，见 ADR-0011 修订）。
- **下载后保持 Checked 靠重派生**：flow 记录「最后一次检查时的线上版本」，下载成功后不清除；本地版本更新后 `derived` 自然落 UpToDate、下载按钮消失——机制是版本相等，不是「下载过就藏」。行为与旧实现逐帧一致。
- **与忙碌门禁衔接**：检查/下载仍经 `gate.start` 放行（交互类，ADR-0007）；flow 的 Checking 是「结论未定」持久状态，gate 的 Checking 是瞬态互斥种类，二者语义不同不冲突。

## 修订（#32/#33：入口收缩到 Settings — General）

主页面精简（#32）后，「检查更新」不再有任何主页面交互入口（检查/下载按钮与
版本行全部移除），唯一操作入口收缩为 Settings — General 的「补丁更新检查」小节
（「检查补丁更新」按钮；结果文案不渲染补丁版本号，见 ADR-0014）——检查结果文案
仍可经通知栏（`render_patch_notice`，无版本号）出现在主页面。事实源/派生架构不变：
检查结果仍只存 `update_flow` 一份，`derived` 仍从「本地版本 vs 线上版本」派生下游
文案与下载可用性。注意：首次运行向导步骤 3 的「检查 → 下载」不经 `update_flow`，
由 `wizard_download` 直连 `updater` 一次性完成——向导只需要一个成败结果，不参与
UI 的检查状态（见 ADR-0013）。

验收：`cargo test --workspace` / `cargo check --workspace --all-targets` /
`cargo clippy --workspace --all-targets` / `cargo fmt --all --check` 全绿。

（原验收单测覆盖状态转移与 derived 全分支——同/异/本地缺失/空线上版本/Err/Idle/Checking/下载重派生/重检覆盖——仍适用。）
