# 错误→文案映射统一收拢进 Strings

8 个错误枚举的文案映射此前分居三处（ui.rs 自由函数持有文案知识），新增枚举时容易漏映射或手抄 magic string。决定：全部收进 `Strings` 单一入口，每个错误枚举一个映射方法、签名同构。

- **新增三方法**：`config_edit_error_text(lang, e)` / `of_error_text(e)` / `compat_error_text(e)`；既有 5 方法（update_error / workflow_error_text / precheck_text / config_error_text / onlinefix_error）不动。lang 仅 ConfigError 家族多收（行列定位措辞因语言而异，Validation 分支穿透 lang）。
- **ui.rs 不再持有文案知识**：自由函数删除，`of_status_line` 只留颜色映射。
- **CompatError 类型化**：`Msg::CompatPrecached` 携带 `Result<(), CompatError>` 活到渲染，`compat_flow.precache_error` 为 `Option<CompatError>`，渲染处逐分支双语映射（Network 复用 err_network、Io 用 err_compat_io）；Display 只留日志。
- **VdfStructureError 类型化**：`VdfError::Structure(&'static str)` → `Structure(VdfStructureError)`（MissingRootChain），i18n 映射穷尽 match 杜绝 magic string 手抄；Display 输出 `structure: missing_root_chain` 不变。

验收：i18n.rs 新方法单测（compat_error_text / of_error_text / config_edit_error_text 双语 × 变体）；compat.rs `compat_error_display` 保留；compat_flow `precache_error` 断言改分支匹配；cargo test 全绿。

## 修订（#27：配置编辑器与 OnlineFix 预设移除，3008a06）

设置重设计移除「配置编辑器」与「OnlineFix 启动预设」两个功能（3008a06，Refs #27；
CONTEXT.md 词条与 README 同步删除）。随它们消亡的错误家族同步退出：
`config_edit_error_text` / `of_error_text` / `onlinefix_error` / `VdfStructureError` /
`of_status_line` 及对应单测全部删除，`Strings` 中不再有这些方法。仍存活的映射：
`update_error` / `workflow_error_text` / `precheck_text` /
`compat_error_text`（兼容性体检仍在，「错误→文案映射收进 Strings」的决策本身不变）。
