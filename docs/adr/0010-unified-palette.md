# 统一语义色板（unified semantic palette）

Rust 版外观此前逐按钮/逐状态手写配色：27 个常量 + 4 处内联（3 处 `Color32::WHITE` + 1 处 selection 底 `from_rgb`），蓝系 8 个值、绿系 3 个值、重复别名 3 组（#15803D ×3、#E2E8F0 ×2、#0284C7 ×2）。颜色种类失控的根源是「每个视觉形态一个色值」——hover、徽章浅底、selection、警戒按钮各写死一套变体。决定：收敛为固定语义槽集合，所有形态变体一律由槽派生，不新增色值；唯一例外是「警戒组」第二蓝，专门保留给「退出 Steam 并卸载补丁」这一个按钮。

## 决策

- **基础槽（12 个语义槽，11 个不同色值）**：
  - accent 蓝 `#0F6CBD`（主按钮底 / accent bar / 蓝字 / selection stroke / hover stroke）
  - 成功绿 `#16A34A`（Deploy 按钮底 / 成功状态文字）
  - 警告琥珀 `#B45309`（上游已适配未缓存）
  - 错误红 `#DC2626`（错误文案）
  - 文字三档 `#0F172A` / `#334155` / `#64748B`（正文 / 次级 / 弱化·禁用态·灰状态）
  - 面板底 `#F8F9FA`（兼并旧 FILL_SECONDARY：TextEdit 底、hover 底）
  - 卡片底 `#FFFFFF`
  - 反色白 `#FFFFFF`（深底按钮文字；与卡片底同值不同槽）
  - 卡片边框 `#E2E8F0`（兼灰徽章底）
  - 控件描边 `#CBD5E1`（输入框 / 次按钮描边）
- **警戒组（唯一写死例外，仅「退出 Steam 并卸载补丁」使用）**：`bg #F0F9FF` / `hover #E0F2FE` / `fg #0284C7` / `border #7DD3FC`。这是对早期「单蓝全派生」草案的修订：第二蓝被保留为卸载类动作的唯一视觉入口，其余按钮一律不再使用。
- **派生变体（不占槽）**：按钮 hover = 底色暗化 ×0.92（Neutral ×0.9）；徽章浅底 = 语义色 ×10% + 白 ×90%（灰徽章 = BORDER 槽）；selection 底 = accent ×18% + 白。
- **按钮样式 4 种**：
  - Deploy（绿实心 / 白字，无描边）：应用补丁并启动 Steam
  - Primary（accent 蓝实心 / 白字，无描边）：下载并解压新版本 / 保存 / 确认
  - Caution（警戒组天蓝描边）：仅「退出 Steam 并卸载补丁」
  - Neutral（白底描边 / 次级文字）：其余全部按钮（正常启动 / 重启 / 卸载 / 卸载补丁并重启 / 检查更新 / 一键缓存签名 / 浏览 / 取消 / 设置 / 语言切换；原 Launch + Secondary + UninstallRestart + Lang 统一并入，含「卸载并重启」从蓝实心降级）
- **命名收拢**：旧 STATUS_INSTALLED / BTN_DEPLOY_BG / DOT_RUNNING → `SUCCESS`；ERR_RED → `DANGER`；STATUS_WARN → `WARN`；ACCENT 沿用。作废常量：ACCENT_ACTIVE、FILL_SECONDARY、BTN_DEPLOY_HOVER、BTN_SECONDARY_HOVER、BTN_UNINSTALL_A_*、BTN_UNINSTALL_B_*、BADGE_*。
- **顶栏对齐**：标题按固定行高（28px）手绘文本（LEFT_CENTER 垂直居中锚点），与右侧设置按钮（72×28）垂直同轴；设置与语言切换按钮同尺寸、同 Neutral 样式；按钮左右次序维持现状（语言切换最右）。外层必须 `horizontal`——`with_layout(left_to_right)` 的子区域会吃满剩余空间把后续内容顶出可视区（原型已验证）。
- **结构**：新建 `src/theme.rs` 收纳色槽常量、警戒组、派生函数（`darken` / `blend` / `badge_bg` / `selection_bg`）与按钮样式解析表；ui.rs 只引用语义名，不出现任何内联色值字面量（有源码扫描测试守卫）。派生函数与解析表可单测（金样值），色板不变量有测试（槽数恰 12、派生色不占槽、仓库无越权色值）。
- **取代 ADR-0003 配色小节**：本 ADR 槽表取代 0003 的逐按钮色值清单；0003 保留的 kill-ai-slop 约束与组件形态（accent bar、独立按钮区、纯文字状态）不变。

## 偏离 Python 版的范围

Python 版卸载类有两个视觉：浅蓝描边「退出并卸载」、蓝实心「卸载并重启」。本版警戒组保留天蓝描边（第二蓝）但仅限「退出并卸载」；「卸载并重启」降级为 Neutral（与其余次操作统一无填充）。状态术语、按钮文案、布局不动。

验收：`cargo build` 通过；`theme.rs` 派生函数与按钮解析表单测全绿；肉眼核对主界面四按钮区 + 兼容性徽章六态 + 顶栏对齐；除 theme.rs 槽定义与警戒组外，仓库无 `Color32::from_rgb` 之类的内联色值字面量残留（源码扫描测试守卫）。