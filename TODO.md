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

## 待办

- [ ] **候选 3 · 在线更新双源真相合并为 UpdateFlow**（Worth exploring，in-process）
  - 问题：同一检查结果被 `UpdateState::Checked` 与 `Notice::UpdateChecked` 各存一份；
    「本地版本 == 线上版本」比较逻辑在 render_notice / card3 线上行 / 下载按钮门控三处重复。
  - 方案：单一事实源 + 单一派生（行文案、通知文案、下载按钮可用性都从同一状态计算）。
  - 参考：评审报告卡片 #3。

- [ ] **候选 4 · 错误→文案映射统一收拢**（Worth exploring，in-process）
  - 问题：8 个错误枚举的文案映射分居三处——5 个在 `i18n::Strings`、2 个是 ui.rs 自由函数、
    `CompatError` 走 `Display` 旁路（英文串塞进本地化模板）；`VdfError::Structure` 用 magic string
    跨模块手抄（onlinefix.rs 生产 / i18n.rs 消费）。
  - 方案：全部映射收进 Strings（签名同构，仅 config_error_text 多收 lang）；VDF 结构错误码类型化；
    `CompatError` 逐分支双语（与 UpdateError 同等待遇），Display 只留给日志。
  - 参考：评审报告卡片 #4。

## 备注

- 评审中「删除测试不通过」的项不成候选，勿重复建议：compat 跨模块依赖
  （`config_editor::remote_url_template`、`updater::download_agent` 为 SPEC §7.6 钦定落位）、
  onlinefix 字节级 VDF Doc（已是深模块）、`Strings` 扁平结构（接口宽但实现不浅，增长机制机械）。
- 候选 1 的实施决策已落进 SPEC §7.5/7.6/7.7，后续候选不要与其语义冲突；
  如需重新审视（如刷新重试 UX），先与用户确认。
- 推送/发布需用户显式确认（AGENTS.md §2）；PowerShell 基线为 pwsh 7（BOM-less UTF-8）。
