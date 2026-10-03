# 统一语义色板（unified semantic palette）

Rust 版外观此前逐按钮/逐状态手写配色：27 个常量 + 4 处内联（3 处 `Color32::WHITE` + 1 处 selection 底 `from_rgb`），蓝系 8 个值、绿系 3 个值、重复别名 3 组（#15803D ×3、#E2E8F0 ×2、#0284C7 ×2）。颜色种类失控的根源是「每个视觉形态一个色值」——hover、徽章浅底、selection、警戒按钮各写死一套变体。决定：收敛为固定语义槽集合，所有形态变体一律由槽派生，不新增色值；唯一例外是「警戒组」第二蓝，专门保留给「退出 Steam 并卸载补丁」这一个按钮。

## 决策

- **基础槽（12 个语义槽）**：现行值为 **Dark（B）定稿**（spec §10 Deep Navy / Ice Blue，见 `docs/design/main-page-ui-reference.md` §10；`src/theme.rs` 为唯一代码来源）：
  - accent 蓝 `#5C91C7`（主按钮底 / accent bar / 蓝字 / selection stroke / hover stroke）
  - 成功绿 `#6BA88F`（成功状态文字 / 状态绿点）
  - 警告琥珀 `#C39A5B`（上游已适配未缓存）
  - 错误红 `#C97979`（错误文案）
  - 文字三档 `#E8EDF3` / `#A9B5C3` / `#778596`（正文 / 次级 / 弱化·禁用态·灰状态；Dark 下文字为浅色，语义翻转见 spec §10）
  - 面板底 `#0F151C`（Background）
  - 卡片底 `#151E28`（Surface）
  - 反色白 `#FFFFFF`（深底按钮文字；与卡片底不同槽）
  - 卡片边框 `#293746`（Border，兼灰徽章底）
  - 控件层次/描边 `#1C2835`（ENTRY = Surface Elevated 深色对偶，输入框 / 次按钮描边）
- **警戒组（唯一写死例外，仅「退出 Steam 并卸载补丁」使用）**：Dark 家族 Warning Secondary Blue——`bg #1A2A3D` / `hover #23374F` / `fg #587A9D` / `border #4A6E93`。这是对早期「单蓝全派生」草案的修订：第二蓝被保留为卸载类动作的唯一视觉入口，其余按钮一律不再使用。
- **派生变体（不占槽）**：按钮 hover = 底色暗化 ×0.92（Neutral ×0.9，`darken` 截断）；徽章浅底 = 语义色 ×15% + 页面底色 ×85%（`blend(base, panel_or_white, 0.85)`——Dark 向 PANEL、L1 向 WHITE，`Palette::badge_bg`）；selection 底 = accent ×18% + 白（`Palette::selection_bg`）。对比度敏感项按模式派生（ADR-0016）：Dark health 前景混白 0.2、busy 用 accent 纯色；L1 前景/忙碌文字向黑混（`warn_fg` / `busy_ink`）。
- **按钮样式 3 种（Deploy 废止）**：
  - Primary（accent 蓝实心 / 白字，无描边）：应用补丁并启动 Steam（原 Deploy 绿废止，见 Main Page 定稿 §18/§19 统一 Solid Brand Blue）/ 下载并解压新版本 / 保存 / 确认
  - Caution（警戒组天蓝描边）：仅「退出 Steam 并卸载补丁」
  - Neutral（卡片底描边 / 次级文字）：其余全部按钮（启动 / 正常启动（未部署时）/ 重启 / 卸载 / 卸载补丁并重启 / 检查更新 / 一键缓存签名 / 浏览 / 取消 / 设置 / 语言切换；原 Launch + Secondary + UninstallRestart + Lang 统一并入，含「卸载并重启」从蓝实心降级）
- **命名收拢**：旧 STATUS_INSTALLED / BTN_DEPLOY_BG / DOT_RUNNING → `SUCCESS`；ERR_RED → `DANGER`；STATUS_WARN → `WARN`；ACCENT 沿用。作废常量：ACCENT_ACTIVE、FILL_SECONDARY、BTN_DEPLOY_HOVER、BTN_SECONDARY_HOVER、BTN_UNINSTALL_A_*、BTN_UNINSTALL_B_*、BADGE_*。
- **结构**：`src/theme.rs` 收纳 `Palette { dark(), light() }` 结构体（12 语义槽 + 反色白 + 警戒组 4 色）、派生方法（`badge_bg` / `selection_bg` / `accent_hover` / `warn_fg` / `busy_ink` / `health_warning_bg/border`）、基础函数（`darken` / `blend`）与按钮样式解析表（`ButtonStyle::palette` → `ButtonPalette`）；ui.rs 只引用语义名/当前 Palette 实例，不出现任何内联色值字面量（有源码扫描测试守卫）。派生函数与解析表可单测（金样值），色板不变量有测试（Dark 槽数恰 12 且全异、派生色不占槽、仓库无越权色值）。
- **深浅切换状态**：切换功能**已实现**（#39 / ADR-0016）：config `theme` 三态字段、`Palette::dark()/light()` 双色源、System 模式跟随 OS（含运行时变化与原生标题栏联动）。Dark（B）与 Light（L1 — Ice Mist）均定稿于 spec §10；L1 accent_hover 为定稿槽值（非派生，spec 内部矛盾裁定定稿优先）；L1 警戒组由 Warning Blue `#3E6F9E` 派生、对比度敏感项向黑混（原型经验值）；GitHub mark 白/黑双态。详见 ADR-0016。
- **取代 ADR-0003 配色小节**：本 ADR 槽表取代 0003 的逐按钮色值清单；0003 保留的 kill-ai-slop 约束与组件形态（accent bar、独立按钮区、纯文字状态）不变。

## 相对旧版 GUI 的偏离

旧版卸载类有两个视觉：浅蓝描边「退出并卸载」、蓝实心「卸载并重启」。本版警戒组保留天蓝描边（第二蓝）但仅限「退出并卸载」；「卸载并重启」降级为 Neutral（与其余次操作统一无填充）。状态术语、按钮文案、布局不动。

验收：`cargo build` 通过；`theme.rs` 派生函数与按钮解析表单测全绿；肉眼核对主界面四按钮区 + 兼容性徽章六态 + 顶栏对齐；除 theme.rs 槽定义与警戒组外，仓库无 `Color32::from_rgb` 之类的内联色值字面量残留（源码扫描测试守卫）。
