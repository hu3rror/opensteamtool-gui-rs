# 短条统一为降权短条（accent 短条语言退役）

主页面 Hero 顶部 eyebrow 的 accent 横杠与设置章节标题的 accent 竖条两套「accent 短条」语言并存。
Hero eyebrow 已先定稿改「降权横条」（border 色 10×2 圆角横短条）；本章把章节标题竖条一并统一，
全系统只保留一种短条形态，accent 色从短条设备上退役。

> 定稿来源：`prototypes/hero-eyebrow-prototype.html` 变体 B（eyebrow）+ `prototypes/section-bar-prototype.html`
> 变体 B（全系统降权横条）。两原型为 throwaway，定稿后按原型技能归档。

## 为什么做

- **「格格不入」诊断成立且互为孤例**：用户对 Hero 横杠的感受定位为「权重倒挂 + 形态孤例」——
  accent 短条比其引导的弱色文字更抢眼，且横条在应用内是孤例。但竖条（3×13 accent）同样是孤例：
  横条与竖条互相印证，各自身边没有同类。只修 Hero 横条，竖条孤例依旧存在，问题只解决一半。
- **accent 语义收拢**：全系统统一后，accent 色只剩交互元素（Primary CTA、激活页签、链接、选中态、
  combo 选中文字），结构标记全部退为中性 border 槽——「accent=动作、中性=结构」的系统化结果
  （原型 B 渲染自验确认此观感）。
- **零新槽零新依赖**：降权短条用既有 `border` 槽，不动「语义色板」，不新增色值，不违反
  ADR-0010「UI 层不写内联色值」。

## 决策

- **短条唯一规格**：border 色、10×2、圆角 1、与文字间距 8lp。三处渲染点统一：
  - Hero eyebrow（`hero_eyebrow`）：14×2 accent + gap 10 → 10×2 border + gap 8。
  - 章节标题（`card_title` 帮助函数，设置对话框全部 7 处标题共用）：3×13 accent + gap 8 →
    10×2 border + gap 8，圆角 0 → 1。
  - 兼容性小节标题（`compat_section` 内联副本）：同 `card_title` 规则。
- **accent 短条语言退役**：accent 不再用于任何短条；标题文字、章节行距、按钮、徽章、页签一律不动。
- **设置对话框宽度测量副本同步几何**：测量代码只分配布局不绘制，但几何须与真实渲染一致
  （宽差 3→10 = +7px），`SETTINGS_DIALOG_WIDTH`（580）下不溢出。
- **不动「语义色板」槽值**、不新增槽；主题切换（ADR-0016）行为不变，深/浅各用各自 border 槽值。
- **口径确认**：本 ADR 只改短条形态与间距，不碰 eyebrow/标题文字（字号、字重、字距、文案、大小写）。

## 否决的方案

- **C 降权竖条**（3×13 竖条保留、accent 降为 border）：只修权重、不修形态——竖条仍是横条的孤例，
  两套形态并存的问题原样保留。
- **只修 Hero 横条、章节竖条保持现状**：两套短条语言并存，accent 短条仍在章节标题上喧宾夺主。
- **保留 14×2 accent 横杠**：即用户初判「格格不入」的现状。
- **引入新语义槽承载短条色**（如 `bar` 槽）：派生自 border 无收益，破坏「派生不占槽」不变量。

## 验收

- 主页面 Hero eyebrow 与设置对话框全部章节标题（含兼容性小节）为同一形态：border 色 10×2 圆角横短条。
- accent 色在短条上零出现；交互元素 accent 不变；标题文字与行距与改动前一致。
- 设置对话框在 `SETTINGS_DIALOG_WIDTH` 580 下无横向溢出；自适应高度行为不变
  （`settings_dialog_footer_visible_at_autosized_window` 等现有无头渲染测试全绿）。
- `cargo test --workspace` / `cargo check --workspace --all-targets` /
  `cargo clippy --workspace --all-targets` / `cargo fmt --all --check` 全绿。
- 深/浅主题观感对照原型 `section-bar-prototype.html` 变体 B 人工验收。
