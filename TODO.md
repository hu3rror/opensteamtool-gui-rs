# TODO — 架构深化候选项

来源：架构评审报告（评审时浏览器打开；报告路径见 `docs/agents/` 或当次会话临时目录
`architecture-review-<时间戳>.html`）。
流程模板（对齐候选 1 的完整管线）：**grilling**（设计树 + domain-modeling 内联更新 CONTEXT.md）
→ **to-spec**（issue + `ready-for-agent` 标签）→ **implement**（TDD，测试面 = 预先与用户确认的接缝）
→ **code-review**（standards + spec 双轴并行）→ **commit** → **关闭 issue**。

## 已完成

- [x] **候选 1 · 兼容性体检生命周期收敛为 compat_flow 状态机**（Strong）
  - issue #14（已关闭）、commit `c412819`（main，本地）
  - 文档：SPEC.md §7.5/7.6/7.7；CONTEXT.md「自动预热」「体检流程（Compat Flow）」
  - 要点：代数戳防陈旧、每代数一次网络刷新（一次性语义，无隐式重试）、刷新/预热并行独立在途、
    复检不递增代数、预热保持后台不进 busy 门。

- [x] **候选 2 · 忙碌态收敛为单一门禁**（Worth exploring，in-process）
  - issue #15（关闭于本候选收尾）、commit（main，本地）
  - 文档：SPEC.md §7.10；CONTEXT.md「忙碌门禁」词条 + 规则
  - 要点：范围=交互类专属（动作 + 更新动作），compat 探针/刷新/预热保持后台（Q5 语义不动，仅显式化边界）；
    `BusyGate` 持有唯一 `Option<BusyKind>`（类型即不变量）；接口 start/replace/clear/current/is_busy；
    `BusyKind` 从 workflow 迁入 busy 模块；确认弹窗悬挂期不算忙碌；渲染禁用源统一 `!gate.is_busy()`。

- [x] **候选 3 · 在线更新双源真相合并为 UpdateFlow**（Worth exploring，in-process）
  - issue #16（关闭于本候选收尾）、commit（main，本地）
  - 文档：SPEC.md §7.11；CONTEXT.md「更新流程」词条 + 规则
  - 要点：检查结果唯一事实源（update_flow 模块），`Notice::UpdateChecked` 降级为无 payload 标记；
    `derived` 一处派生行文案/通知/下载（`UpdateDerived { line, notice, download: Option<&OnlineInfo> }`）；
    下载后 Checked 保留靠重派生（机制 = 版本相等，非「下载过就藏」）；检查/下载仍经忙碌门禁。

- [x] **候选 4 · 错误→文案映射统一收拢**（Worth exploring，in-process）
  - issue #17（关闭于本候选收尾）、commit（main，本地）
  - 文档：SPEC.md §7.12（无 CONTEXT.md 词条变更——错误映射为通用编程概念）
  - 要点：8 枚举文案映射全部收进 Strings（新增 config_edit_error_text / of_error_text / compat_error_text，
    既有 5 方法不动；lang 仅 ConfigError 家族多收）；CompatError 类型活到渲染（Msg/compat_flow
    类型化，Display 只留日志）；`VdfStructureError { MissingRootChain }` 穷尽 match 杜绝 magic string。

## 待办

评审报告的 4 个架构候选（候选 1–4）已全部走完管线，无遗留待办。

## 备注

- 评审中「删除测试不通过」的项不成候选，勿重复建议：compat 跨模块依赖
  （`config_editor::remote_url_template`、`updater::download_agent` 为 SPEC §7.6 钦定落位）、
  onlinefix 字节级 VDF Doc（已是深模块）、`Strings` 扁平结构（接口宽但实现不浅，增长机制机械）。
- 候选 1 的实施决策已落进 SPEC §7.5/7.6/7.7，后续候选不要与其语义冲突；
  如需重新审视（如刷新重试 UX），先与用户确认。
- 推送/发布需用户显式确认（AGENTS.md §2）；PowerShell 基线为 pwsh 7（BOM-less UTF-8）。
