# OpenSteamTool Manager

## Main Page UI Reference & Design Specification

**Target:** Rust + egui / Windows Native Desktop GUI
**Scope:** Main Page
**Languages:** `zh` / `en`
**Theme:** Dark only

---

# 1. 产品定位

OpenSteamTool Manager 是 Windows 原生桌面工具，用于管理 Steam Patch 的部署、卸载与在线更新。

主页面定位为：

> **Premium Game Launcher × Focused Utility**

希望用户感受到：

* 高级
* 克制
* 专业
* 有游戏产品气质
* 略带 Steam 式深色蓝灰氛围
* 操作直接
* 信息低密度

但不要呈现为：

* Steam Clone
* Dashboard
* Mod Manager
* Developer Tool
* DevOps / Admin Console
* 电竞 RGB UI
* Neon Game UI

核心目标：

> **像一个认真设计过的、面向玩家的 Windows Launcher。**

---

# 2. 核心交互原则

## 2.1 Action-first

主页面任何稳定状态都应存在一个唯一的：

> **Primary CTA**

Primary CTA 由当前业务状态派生。

---

## 2.2 Single Source of Truth

同一个事实只在首页显示一次。

特别是：

> **Patch Version 永远不渲染。**

禁止出现：

```text
v1.4.2
Patch Version
Current Version
Latest Version
Online Version
```

本地版本 / 线上版本只属于 `update_flow` 内部逻辑。

---

## 2.3 职责分离

```text
Deploy Status
    → Patch 是否已应用

Primary CTA
    → 当前最重要的操作

Secondary Actions
    → 备用路径 / 维护操作

Health Warning
    → Compatibility 风险

Patch Update Check
    → Patch 在线维护

Status Bar
    → Steam State + Busy + Recent Operation Result
```

任何组件不得越权承担其他组件的业务事实。

---

# 3. Source of Truth

本 UI Spec 的约束层级：

```text
GLOSSARY.md
    ↓
相关 ADR
    ↓
本 Main Page UI Reference（语义 / 状态 / 交互）
    ↓
原型渲染效果（视觉呈现基准：prototypes/main-page-ui-prototype.html 的 B / L1 变体）
    ↓
现有代码事实
    ↓
Implementation Tuning
```

> **视觉实施始终以原型渲染效果为准**，不以 spec 文字想象为准。spec 的文字只能
> 表达目的大概（色值表、结构、层级、间距意图），无法表达渐变、tint、hairline、
> 圆角、动效、呼吸感等实际观感；当两者不一致时，原型渲染效果优先，spec 文字
> 按原型订正（用户裁决）。

### GLOSSARY.md

负责：

> 领域术语、状态定义、事实来源和不可违反的业务语义。

### ADR

负责：

> 已经裁决的产品 / 架构行为。

### Main Page UI Reference

负责：

> Main Page 的视觉、布局和交互呈现。

### Existing Code

负责：

> 当前实际实现，需要审查，不自动视为目标设计。

---

# 4. Specification Status

本规格中的内容分两类。

## Locked

包括：

* 产品定位
* 信息架构
* 状态映射
* 术语
* Primary / Secondary hierarchy
* Status Bar 职责
* 品牌色板（§10 定稿：Dark B + Light L1 两套色值）
* Patch Version 不渲染
* Busy 行为
* 确认框规则
* Health Warning 位置
* Update Action 归属

未经 ADR / Spec 变更，不自行修改。

> **深浅色切换的状态**：已实现（ADR-0016 / #39）。Light Mode 色板（L1）已定稿（§10）
> 并接线：`Palette { dark(), light() }` 双色源、config `theme` 三态偏好、System 模式
> 跟随系统（含运行时变化与原生标题栏联动）。本节 Locked 内容为 Main Page 重写时的
> 定稿；切换实现细节以 ADR-0016 为准。

## Tunable

包括：

* 具体字号
* 具体 spacing
* 具体 radius（§38 层级固定，数值可调）
* Window recommended size（首帧参考见 §40）
* Hero max width
* 语义色的派生变体（hover / 徽章浅底 / selection 的混合比例）
* 微调 hover transition
* Icon 细节（尺寸 / 笔宽微调；构型已定稿，见 §36）
* egui implementation detail

这些可以在实际原型 / 渲染测试中调整。

色板基础槽位值（§10）属于 Locked，不在 Tunable 内。

---

# 5. 总体页面结构

```text
Windows Native Title Bar
        ↓
Application Header
        ↓
Hero Surface
    ├── Patch Eyebrow
    ├── Deploy Status
    ├── Supporting Text（必要时）
    ├── Primary CTA
    └── Patch Update Check
        ↓
Health Warning（仅特定状态）
        ↓
Secondary Action Group
        ↓
Status Bar
```

---

# 6. Window Chrome

使用：

> **Windows 原生标题栏**

不实现自定义 Window Chrome。

原生标题栏负责：

* Window identity
* Minimize
* Maximize
* Close
* Native window behavior

不得为了视觉设计接管上述行为。

---

# 7. Application Header

Header：

```text
[Logo + App Name]                         [Settings]
```

左侧：

> 完整横向 Logo

右侧：

> Settings Icon

Header 不显示：

* Home
* Launcher
* Steam Patch
* Page Title
* Patch Version
* Steam State

Header 的职责：

> **品牌识别 + Settings 入口。**

---

# 8. Visual Direction

总体：

> **Dark / Premium / Calm / Professional / Game-aware**

视觉组合：

```text
Neutral Dark Foundation
        ↓
Cool / Blue-Gray Surface
        ↓
Restrained Brand Accent
        ↓
Strong Primary CTA
```

游戏感来自：

* 比例
* 留白
* CTA
* Icon
* Surface depth
* Brand accent

而不是：

* Glow
* Neon
* 强 Gradient
* 粒子
* 巨型标题
* 电竞风发光边框

---

# 9. Color Direction

## 9.1 当前问题诊断

如果原型视觉感觉“憨”，优先检查：

1. Accent Blue 是否饱和度过高。
2. Background 与 Surface 是否都偏蓝，导致整体像“蓝色玩具 UI”。
3. Primary CTA 是否过于鲜艳。
4. 圆角、蓝色 Surface、按钮填充是否同时过强。
5. Neutral Gray 是否不足，导致整个页面缺乏专业的中性骨架。

新的颜色策略：

> **降低蓝色面积和饱和度，提高中性深灰的占比。**

页面不是：

```text
Blue UI
```

而是：

```text
Professional Dark UI
+
Blue Interaction Accent
```

---

# 10. 品牌色板（已定稿）：Palette B — Deep Navy / Ice Blue

> 经交互原型对比后定稿。候选评估与其他方向（A / C / D / E）的记录见 §12。
> 本节为基础槽位值，全部派生色（hover / 徽章浅底 / selection）由 §11 规则从
> 本表派生，不新增独立色值。

### Character

> 更接近 Steam 的蓝灰生态，但比 Steam 更干净。Neutral 深灰占主导，蓝色
> 仅作为 Interaction Accent 出現，不做大面积染色（§9.1 诊断持续生效）。

```text
Background       #0F151C
Surface          #151E28
Surface Elevated #1C2835
Border           #293746

Text Primary     #E8EDF3
Text Secondary   #A9B5C3
Text Muted       #778596

Accent Blue      #5C91C7
Accent Hover     #6A9FD1

Success Green    #6BA88F
Warning Amber    #C39A5B
Error Red        #C97979

Warning Blue     #587A9D
```

### 语义槽（与 ADR-0010 槽位对齐）

```text
Background      → 窗口页面底
Surface         → Hero 底
Surface Elevated → 卡片/悬浮底、禁用态底
Border          → 分隔线、hairline
Text Primary    → 正文/大标题
Text Secondary  → 次级文案
Text Muted      → 弱化/禁用文案、灰色状态
Accent Blue     → 交互蓝（Primary CTA / hover / selection）
Accent Hover    → 交互蓝悬停/激活
Success Green   → 成功、Steam 运行状态点
Warning Amber   → 健康风险警示、警告徽章
Error Red       → 错误文案/失败状态
Warning Blue    → 仅「退出 Steam 并卸载补丁」
```

特点：

* Steam 感最明显，但比纯 Steam 风更现代、更干净、更克制
* Neutral 深灰占主导，蓝灰统一，Accent 面积受限
* 「已应用」的 Hero 状态：大字号中性主文字 + 绿色 ✓ 点缀，不整块染绿（§37）
* Hero / Surface 不做蓝底、不大面积染色

### 参考 Design Token（数值 Tunable，随渲染测试微调）

```text
窗口首帧参考   620 × 520 lp（§40）
内容列 max-width  600 lp
圆角层级（§38）  Hero 18 / CTA 10 / Row 10 / Small 7 lp
垂直呼吸（§39）  上留白 : 下留白 ≈ φ : 1（弹性分配）
eyebrow       “PATCH”（双语一致，小号大写英文，§14）
```

### Light Mode Palette（已定稿）：L1 — Ice Mist

> **状态：已接线（ADR-0016 / #39）**——Main Page 重写当时只落实 Dark（B），
> 切换功能已另行立项实现：L1 为已启用的 Light Mode 定稿色源（原型定稿，见 §12），
> 色值以 `src/theme.rs` 的 `Palette::light()` 为准（L1 accent hover 为定稿槽值、
> 对比度敏感项向黑混、警戒组由 Warning Blue 派生、GitHub mark 保留黑 mark，见 ADR-0016）。

### Character

> 与 B 有效互补：同一品牌蓝家族、冷白冰蓝底；Neutral 冷白为主，蓝色作
> Interaction Accent 出現。B 的「冷深蓝灰」与 L1 的「冷亮白」是同一品牌
> 的亮面色相，不做暖色/奇异色偏离。

```text
Background       #F3F6F9
Surface          #FFFFFF
Surface Elevated #E8EEF5
Border           #D3DCE6

Text Primary     #1C2630
Text Secondary   #47566A
Text Muted       #8494A7

Accent Blue      #3E76AC
Accent Hover     #35689A

Success Green    #3F8E63
Warning Amber    #A67C2E
Error Red        #C2554E

Warning Blue     #3E6F9E
```

派生变体（hover / 徽章浅底 / selection）与 Dark 同规则（§11），只是
在 L1 色源上派生；对比度敏感项（Health Warning 文字、Busy 文字）在浅底
需混黑调深（原型中 `--warn-fg` / `--busy-ink` 的经验值在该模式内派生）。

---

# 11. 色彩纪律（Color Discipline）

不混用其他 Palette 方向（A / C / D / E、L2 / L3 已评估未选用，见 §12）。

Dark（B）与 Light（L1）是同一品牌的两套已定稿色源，各自独立存在；
切换功能已实现（ADR-0016），两套色源分别对应 `Palette::dark()` / `Palette::light()`。

所有颜色按单一推导链：

```text
Background
↓
Surface
↓
Text
↓
Accent
↓
Semantic
```

全部从该 Palette 派生。

尤其：

> **Accent Blue 不应该同时成为 Background、Surface 和 CTA 的主色。**

Accent 是：

> **Interaction Color**

不是：

> **UI Base Color**

---

# 12. 原型决策记录（Palette 定稿）

设计探索阶段以交互原型对比候选方向（`prototypes/main-page-ui-prototype.html`，
A / B / D 三变体），重点观察：

* Primary CTA 是否更专业
* Hero 是否仍然有游戏感
* Update Action 是否足够安静
* Warning 是否能够从 Neutral 中脱离
* 整体是否还像 Steam 周边工具，而不是独立产品

**结论：定稿 B — Deep Navy / Ice Blue（§10）。**

选择理由：Steam 蓝灰生态的亲近感 + 比 Steam 更现代的干净程度；Neutral 深灰
占主导、Accent 克制，长期使用不腻、不产生“蓝色玩具 UI”感（§9.1）。

随原型确认的呈现细节（已并入 §10 / §14 / §32 / §36 / §38 / §39 / §40）：
底部状态栏 dock、黄金比垂直呼吸、eyebrow 用大写英文 PATCH（双语一致）、
设置齿轮构型（圆环 + 8 圆齿 + 中心点）。

评估过的其他方向（未选用，禁混用）：A — Graphite / Steel（最专业）、
C — Gunmetal / Cobalt（最冷峻）、D — Charcoal / Muted Teal（品牌差异）、
E — Slate / Indigo（现代独立游戏感）。

## Light Mode 定稿（L1 — Ice Mist）

在 B 定稿基础上，原型追加三套与 B 互补的浅色候选（同一品牌蓝家族，
Neutral 色温不同）：

* **L1 — Ice Mist（冷白冰蓝）**：与 B 最直接的亮色镜像；**已定稿（§10）**
* L2 — Warm Slate（暖纸灰蓝）：冷暖撞色，未选用
* L3 — Steel Mist（中性钢蓝灰）：最克制，未选用

选择理由：与 B 同属冷蓝灰家族，亮度翻转后品牌辨识连续；主页面与设置页
默认未来同一切换，冷白底对长时间使用友好。

> **视觉基准**：本 Spec 的文字描述不完整表达观感（渐变 / tint / hairline /
> 间距 / 圆角），Main Page 实施的视觉呈现以原型渲染（B 与 L1 变体）为准（§3）。

---

# 13. Hero Surface

Hero 是主页面唯一真正意义上的核心 Surface。

职责：

```text
Patch
+
Deploy Status
+
Supporting Text
+
Primary CTA
+
Patch Update Check
```

视觉：

* 比 Background 略亮
* 低对比 Border
* 极弱层次
* 不做明显阴影
* 不做 Glass
* 不做 Glow

---

# 14. Hero Content Hierarchy

顺序：

```text
Patch
↓
Deploy Status
↓
Supporting Text（必要时）
↓
Primary CTA
↓
Patch Update Check
```

`Patch`：

> eyebrow / muted，**文案定稿为英文大写 “PATCH”**（双语一致，小号、次级色、
> 字距拉开；前缀「降权短条」（border 色 10×2 圆角横条，ADR-0018）。不在中文界面显示“补丁”二字（突兀，原型修正）。

`已应用 / 未应用`：

> Hero Status

Primary：

> 第二级视觉焦点

---

# 15. Deploy Status

严格使用：

```text
已应用
未应用
```

禁止：

```text
Installed
Active
Ready
Enabled
Patched
```

---

# 16. Patch Version

硬规则：

> **Patch Version 永远不渲染。**

主页面：

* Hero 不显示
* Update 不显示
* Status Bar 不显示
* Health Warning 不显示
* Secondary 不显示

本地版本 / 线上版本仅作为更新流程内部数据。

---

# 17. Supporting Text

默认不显示。

只有在：

> 用户需要理解当前状态 / 当前 Primary 不可用

时显示。

例如：

```text
Patch

未应用

补丁未下载
请点击「检查补丁更新」下载新版本
```

Supporting Text 只：

> 解释

不：

> 新增操作入口

---

# 18. Primary CTA

Primary CTA 是页面视觉重点。

视觉：

> **Solid Brand Accent + Very Subtle Depth**

允许：

* 轻微色阶
* Hover brightness
* Press downshift
* Focus ring

禁止：

* Neon
* Glow
* 强阴影
* 发光 Border
* 大面积 Gradient

---

## 18.1 Primary Geometry

Primary：

* 稳定 Action Width
* 固定高度体系
* 单行
* Icon + Label 整体居中

不允许：

* 自动换行
* 省略号截断核心文案
* 因英文过长而极端缩小字体

---

# 19. Primary CTA Mapping

| Deploy Status            | Steam State | Primary CTA   |
| ------------------------ | ----------- | ------------- |
| 未应用                      | 未运行         | 应用补丁并启动 Steam |
| 已应用                      | 未运行         | 启动 Steam      |
| 已应用                      | 运行中         | 重启 Steam      |
| 未应用 + Steam Path Invalid | —           | 前往设置修复        |
| Busy                     | 任意          | Busy Stage    |

---

# 20. Action Safety

原则：

> **用户明确点击动作后，默认直接执行。**

原则层面不增加确认弹窗。

适用：

* 卸载补丁
* 重启 Steam
* 退出 Steam 并卸载补丁
* 卸载补丁并重启 Steam

但是，存在一个已经由 **ADR-0007 / #36** 确认的例外。

---

## 20.1 Confirm Dialog Exception

以下规则优先于“默认直接执行”：

> **仅「退出 Steam 并卸载补丁」和「卸载补丁并重启 Steam」在 Steam 正在运行时弹出“关闭确认”框。**

即：

```text
Steam Running
    ↓
退出 Steam 并卸载补丁
    → Confirm Dialog

Steam Running
    ↓
卸载补丁并重启 Steam
    → Confirm Dialog
```

确认框是：

> **关闭确认**

其职责是告知用户：

> 当前 Steam 正在运行，此操作会先关闭 Steam。

---

## 20.2 Explicit No-Confirm Actions

以下操作不得出现确认框：

### 应用补丁并启动 Steam

Steam 正在运行时：

> 直接放行。

`kill_first` 由运行态派生。

### 重启 Steam

始终：

> 不弹确认框。

因为：

> `重启 Steam` 的按钮语义已经包含“关闭并重新启动”，再次确认属于冗余打扰。

### 卸载补丁

仅在：

> Steam 未运行的可用路径

直接执行。

### 其他状态

遵循具体状态矩阵。

---

## 20.3 Confirm Dialog Boundary

确认框只阻断：

> `退出 Steam 并卸载补丁`
> `卸载补丁并重启 Steam`

且仅在：

> Steam 正在运行

时出现。

确认框不得扩散到：

* Primary CTA
* Update Action
* 重启 Steam
* Steam 未运行时的卸载补丁
* 正常启动 Steam

---

# 21. Secondary Action

Secondary：

> Inline Action

例如：

```text
[Icon] 卸载补丁
```

默认无完整 Button Surface。

Hover：

```text
╭────────────────────────────╮
│ [Icon] 卸载补丁              │
╰────────────────────────────╯
```

---

# 22. Secondary Hit Area

视觉上：

> Inline

实际点击：

> **整行**

因此：

```text
Visual Weight = Low
Hit Area = Full Row
```

---

# 23. Secondary Action Group

Secondary：

> 纵向排列

不横向排列。

Group 使用：

> 间距 + 极轻分隔线

不创建完整 Card。

---

## 23.1 未应用

```text
[Play] 正常启动 Steam
```

Primary：

```text
[Play] 应用补丁并启动 Steam
```

---

## 23.2 已应用 + Steam 未运行

```text
[Uninstall] 卸载补丁
[Uninstall] 卸载补丁并重启 Steam
```

---

## 23.3 已应用 + Steam 运行中

```text
[Exit + Uninstall] 退出 Steam 并卸载补丁
[Uninstall + Restart] 卸载补丁并重启 Steam
```

第一项：

> 使用 `Warning Secondary Blue`

但不使用：

> Error Red

---

# 24. Health Warning

只在 Health Summary：

```text
上游尚未适配
未找到核心 DLL
```

时出现。

---

## 24.1 Position

```text
Hero
↓
Health Warning
↓
Secondary
```

正常状态不占空间。

---

## 24.2 Visual

统一：

> Warning Amber + Warning Icon

例如：

```text
╎ [Warning] 当前 Steam 尚未适配        设置 →
```

或：

```text
╎ [Warning] 未找到 Steam 核心文件      设置 →
```

整行点击：

> Settings → Steam

---

## 24.3 Health vs Deploy

严格区分：

```text
Deploy Status
├── 已应用
└── 未应用
```

和：

```text
Health Summary
├── 上游尚未适配
└── 未找到核心 DLL
```

Health Warning 不改变 Deploy Status 的术语。

---

# 25. Steam Path Invalid

Steam Path Invalid：

> 不作为第三种 Deploy Status。

正确：

```text
Deploy Status = 未应用
```

附加：

```text
Steam 路径无效
```

Primary：

```text
[Settings] 前往设置修复
```

原 Operation Actions：

> disabled

---

# 26. Patch Update Check

位置：

> Hero / Deploy Status 区域内部

它不是 Status Bar 功能。

---

## 26.1 Normal

```text
[Refresh] 检查补丁更新
```

---

## 26.2 Checking

同一按钮位：

```text
[Progress] 检查中…
```

检查中状态：

> 不进入 Status Bar。

---

## 26.3 No Update

恢复：

```text
[Refresh] 检查补丁更新
```

必要的检查结论可在同一区域内联表达。

不得显示版本。

---

## 26.4 Update Available

同一按钮位视觉升级：

```text
╭────────────────────────────────╮
│ [Download] 下载并解压新版本     │
╰────────────────────────────────╯
```

这是 Update Action 的视觉升级。

仍然：

> 不超过 Primary CTA 的视觉权重。

---

## 26.5 Downloading

```text
[Progress] 正在下载补丁…
```

---

## 26.6 Extracting

```text
[Progress] 正在解压补丁…
```

---

## 26.7 Failure

在 Update Action 附近表达：

```text
[Refresh] 检查补丁更新
检查失败
```

或：

```text
[Download] 下载并解压新版本
下载失败
```

---

# 27. Busy

Busy 由：

> `Busy Gate`

控制。

页面整体结构保持稳定。

Hero 进入：

> Stage-driven Busy State

例如：

```text
Patch

正在应用补丁…
```

然后：

```text
Patch

正在启动 Steam…
```

Busy 文案描述：

> **当前阶段**

不是原始按钮名称。

---

# 28. Busy Interaction

交互类 Busy 期间：

```text
Primary       disabled
Secondary     disabled
Update        disabled
```

任何时刻只有一个：

> Action / Update Action

进入 Busy。

---

# 29. Progress

只有可靠进度时显示 Progress Bar。

### 不确定进度

```text
正在启动 Steam…
```

### 真实进度

```text
正在下载补丁…

██████████████░░░░░░ 68%
```

不得伪造百分比。

---

# 30. Error

错误按：

> **用户下一步需要做什么**

进行分层。

## Operation Failure

Hero：

```text
补丁应用失败

[Retry] 重试
```

## Environment Block

Hero：

```text
未应用

Steam 路径无效

[Settings] 前往设置修复
```

## Update Failure

Update Action 区域处理。

不同时复制到：

* Hero
* Status Bar
* Update Action

多个位置。

---

# 31. Success

不创建独立 Success Hero。

成功后：

> 直接进入新的稳定状态。

例如：

```text
未应用
    ↓
正在应用补丁…
    ↓
已应用
```

反馈由：

```text
Deploy Status
+
Recent Operation Result
```

共同完成。

---

# 32. Status Bar

Status Bar：

> **窗口底部专用 Dock（定稿形态）**

不做独立 Card。

Dock 形态：

* 位于窗口最底部、横跨全宽，顶部 1px 分隔线与 Header 底部分隔线一致
* 背景与页面 Background 同色，横向 padding 与 Header 对齐
* Dock 独立于内容列：内容列（Hero / Secondary）宽度由自身 max-width 决定，
  不受 Dock 影响（原型修正 1 确认）

> **验收微调（用户签核，覆盖原型 26lp）**：状态项横向起点采用 16lp 左缩进
> （整体更靠左），dot↔文字间距 8lp（原型 .sb-item gap），状态项之间 ≈20lp；
> 内容行垂直 dead-center：上 11 / 下 9 lp（egui 字形在行框内偏上 ~1.5lp，
> 反向补偿后文字距分割线与距窗底等距）。实现见 ui.rs `status_dock`。

可以容纳：

```text
Steam State
Busy
Recent Operation Result
```

但：

> Patch Update Check 的检查中 / 检查结果不占 Status Bar。

---

# 33. Steam State

恒显：

```text
● Steam 正在运行
```

或：

```text
● Steam 未运行
```

语义：

```text
Running
    → Success Green

Not Running
    → Neutral Gray
```

Steam State 不在 Hero 重复显示。

---

# 34. Busy in Status Bar

根据 Glossary：

> Status Bar 可以拥有 Busy Item。

但 Busy Item 的业务含义与 Hero 的 Busy Stage 不同：

```text
Hero
    → 当前详细阶段

Status Bar
    → Busy 作为持续状态条目
```

Update Check 的检查中 / 结果：

> 不通过 Busy Item 取代 Update Action 的局部结果。

---

# 35. Recent Operation Result

Status Bar 保存：

> 最近操作结果

直到新的相关结果覆盖。

例如：

```text
● Steam 正在运行       ✓ Steam 已重启
```

---

## Steam Start / Restart Success

若 Steam 后续退出：

```text
● Steam 未运行
```

不得继续显示：

```text
✓ Steam 已重启
```

避免过期事实冲突。

---

## Patch Operation Result

例如：

```text
● Steam 未运行       ✓ 补丁已应用
```

或：

```text
● Steam 未运行       ✓ 补丁已卸载
```

这类结果不因为 Steam State 改变而强制失效。

---

# 36. Icon System

使用统一：

> **Icon Component**

禁止直接把 Unicode / Emoji 当最终 UI 图标。

例如最终应使用：

```text
[PlayIcon]
[RestartIcon]
[SettingsIcon]
[WarningIcon]
[UninstallIcon]
[RefreshIcon]
[ProgressIcon]
```

而不是让各按钮自行使用：

```text
▶
↻
⚙
⚠
⏏
```

具体 Icon Family：

> **定稿：程序化几何绘制（egui painter），不引入图标库 / icon font 依赖。**

Icon 族：Play / Restart / Uninstall / Exit / Settings / Download / Refresh /
Warning / Progress（spinner）。

Implementaion 约定：

* 统一 stroke 笔宽、线帽与 logical 尺寸；颜色取自语义色槽（§11）
* 设置齿轮定稿构型：圆环 + 8 圆齿 + 中心点（现有 `paint_gear` 同构，ADR-0015 最初版式）
* 图标绘制收敛到单一 icon 模块（`paint_*` 函数），不在各组件手写 SVG / 自行绘制

仍然禁止：Unicode / Emoji 作为最终 UI 图标（▶ ↻ ⚙ ⚠ ⏏）。

不要为了视觉需要自行绘制一套散落在组件里的 hand-rolled SVG。

---

# 37. Typography

使用系统字体。

不额外引入品牌字体。

层级：

```text
Hero Status
    ↓
App / Primary
    ↓
Supporting
    ↓
Secondary
    ↓
Status Bar
```

Hero Status：

> 页面最明显的文字层级。

正常状态不依赖高饱和色突出。

---

# 38. Corner Radius

采用：

> **Balanced hierarchy**

圆角遵循：

```text
Hero Surface
    ↓
Primary CTA
    ↓
Secondary Row
    ↓
Small UI
```

外层更柔和，内部交互控件逐渐更利落。

具体数值：

> Tunable（基线：Hero 18 / CTA 10 / Row 10 / Small 7 lp，见 §10）

不要所有 UI 使用同一个圆角。

---

# 39. Spacing

采用：

> **Adaptive Breathing**

原则：

* 使用稳定 spacing token
* 保留最小结构间距
* 窗口增加的空间优先转化为留白
* 垂直呼吸：上留白 : 下留白 ≈ φ : 1（黄金比例，弹性 spacer 分配，不写死像素）。
  效果上 Hero 组视觉重心落在内容区约 0.38–0.42 高度（略偏上）
* 不依赖固定窗口像素位置维持布局

> **验收微调（用户签核）**：
> * 内容净高估算基准 430lp（正常态观感与最坏态防裁的折中：最坏态 = supporting +
>   健康警告 + 两行 secondary ≈ 445lp，顶留白 79 + 列 445 仍在可用区内，底部留白
>   吸收溢出不伤内容；实测正常态 331lp，重心 ≈0.44 略偏下，避免头重脚轻）。基准属 Tunable，与滚动态无关。
> * Hero 内部节奏：eyebrow→status 8 / status→primary 22 lp；supporting 显示时
>   status→supporting 10、supporting→primary 22 lp；primary→patch-update 14 lp。
> * 内容列统一 gap 14 lp（hero→health→secondary）。
> * 内容列 max-width 居中，列内**左对齐**（Primary CTA / Secondary 行靠左，
>   对齐原型渲染；覆盖早期“居中 CTA”解读）。

---

# 40. Window Resize

窗口：

> 可 Resize

内容区域：

> 有舒适 max-width

Hero：

> 有舒适 max-width

窗口放大：

> 不无限拉伸 Hero / CTA。

额外空间：

> 转化为空白。

具体 Window Size / Max Width：

> Tunable（首帧参考 620 × 520 lp，Hero max-width 600 lp；随后续渲染测试微调）

注：首帧自适应（`autosize_inner_height` 下限）与 Dock 形态属既定行为，缩放窗口时
内容列宽度与垂直黄金比呼吸保持不变。

> **验收微调（用户签核）**：首帧自适应为**只涨不缩**——初始窗口按 spec 首帧参考
> 620×520（= 最小内尺寸；默认值原为 940×680，用户签核收紧），仅在内容超高时增长；
> 绝不因内容变矮而缩窗（旧行为会把窗口从 680 撑到 ~716，导致弹性留白重新分配、
> 内容整体下坠）。

---

# 41. DPI

全部 Design Token 使用：

> logical points

不要针对：

```text
100%
125%
150%
175%
200%
```

写专门的 layout branch。

DPI Scaling 由：

> egui / rendering layer

负责。

---

# 42. Localization

中英文均为正式产品能力。

原则：

> **先优化 wording，再通过宽度适配。**

---

## Primary CTA

必须：

* 单行
* 不换行
* Icon + Label 整体居中

---

## Secondary

优先：

* 单行
* 大点击区域
* 不极端缩小字体

---

## Update

允许比普通 Secondary 更宽。

但：

> 不显示版本号。

---

# 43. Motion

整体：

> Low / Subtle

允许：

### Hover

* brightness
* subtle surface

### Press

* slight downshift

### Busy

* spinner
* real progress

### State Transition

* short opacity / color transition

禁止：

* 大范围飞入
* background animation
* particle
* magnetic button
* glow pulse
* decorative choreography

Motion 只说明：

> **状态变化**

而不是证明：

> **“这是游戏软件”。**

---

# 44. Main Page State Matrix

| Deploy Status      | Steam State | Hero / Primary         | Secondary                      | Health Warning   | Update         | Status Bar                  |
| ------------------ | ----------- | ---------------------- | ------------------------------ | ---------------- | -------------- | --------------------------- |
| 未应用                | 未运行         | 应用补丁并启动 Steam          | 正常启动 Steam                     | 按 Health Summary | 检查补丁更新         | Steam 未运行                   |
| 已应用                | 未运行         | 启动 Steam               | 卸载补丁 / 卸载补丁并重启 Steam           | 按 Health Summary | 检查补丁更新         | Steam 未运行                   |
| 已应用                | 运行中         | 重启 Steam               | 退出 Steam 并卸载补丁 / 卸载补丁并重启 Steam | 按 Health Summary | 检查补丁更新         | Steam 正在运行                  |
| 未应用 + 补丁文件缺失       | 任意          | 应用补丁并启动 Steam disabled | 正常启动 Steam 可用                  | 按 Health Summary | 下载并解压新版本       | 当前 Steam State              |
| Steam Path Invalid | —           | 前往设置修复                 | Operation disabled             | 按 Health Summary | 依据 Update Flow | 当前 Steam State              |
| Busy               | 任意          | Busy Stage             | Disabled                       | 保持已有规则           | Disabled       | Steam State + Busy / Result |
| Operation Failure  | 任意          | Retry / Remediation    | 视情况                            | 视 Health Summary | 按业务归属          | Recent Result               |
| Update Available   | 任意          | 原 Primary              | 原 Secondary                    | 视 Health Summary | 下载并解压新版本       | Steam State + Result        |

---

# 45. Action Confirmation Matrix

确认行为必须以此矩阵为准：

| Action         | Steam State | Confirm |
| -------------- | ----------- | ------- |
| 应用补丁并启动 Steam  | 未运行         | No      |
| 应用补丁并启动 Steam  | 运行中         | **No**  |
| 正常启动 Steam     | 未运行         | No      |
| 启动 Steam       | 未运行         | No      |
| 重启 Steam       | 运行中         | **No**  |
| 卸载补丁           | Steam 未运行   | **No**  |
| 退出 Steam 并卸载补丁 | Steam 运行中   | **Yes** |
| 卸载补丁并重启 Steam  | Steam 运行中   | **Yes** |

其他确认行为不得自行增加。

---

# 46. Component Boundary

建议：

```text
MainPage
├── ApplicationHeader
│   ├── BrandLogo
│   └── SettingsButton
│
├── HeroSurface
│   ├── DeployStatus
│   ├── SupportingText
│   ├── PrimaryAction
│   └── PatchUpdateCheck
│
├── HealthWarningRow
│
├── SecondaryActionGroup
│   └── SecondaryActionRow[]
│
└── StatusBar
    ├── SteamStateItem
    ├── BusyItem
    └── RecentOperationItem
```

Confirm Dialog：

```text
ConfirmCloseSteamDialog
```

只由两个允许确认的 Action 进入：

```text
Exit Steam + Uninstall
Uninstall + Restart
```

并且：

> 只有 Steam Running 时进入。

---

# 47. Presentation Architecture

UI 不自行复制业务判断。

建议：

```text
Business State
      ↓
Presentation / View Model
      ↓
Main Page Components
```

例如：

```text
MainPageViewModel
├── deploy_status
├── primary_action
├── supporting_text
├── secondary_actions
├── health_warning
├── patch_update_action
└── status_bar
```

UI 不直接复制：

```text
if steam_running
if patch_applied
if update_available
```

到多个组件中。

---

# 48. Implementation Rules

AGENTS 修改 Main Page 时：

1. 优先阅读 `GLOSSARY.md` 与相关 ADR。
2. 阅读本文件。
3. 检查现有 Theme / Semantic Palette / Font / Icon。
4. 检查 Main Page 当前实现。
5. 检查 Busy Gate / Steam State / Update Flow / Health Summary 的真实来源。
6. 优先复用已有抽象。
7. 不修改已经确定的业务行为。
8. 不为视觉重构复制新的业务状态源。
9. 不引入无关功能（深浅色切换已单独立项实现，见 ADR-0016，不属于 Main Page 改动范围）。
10. 不为了视觉效果引入不必要的新依赖。

## 48.1 视觉以原型渲染为准（§3）

Main Page 的视觉呈现以 `prototypes/main-page-ui-prototype.html` 的 B / L1 变体
渲染效果为准，不得按 spec 文字自行脑补；观感不一致时以原型为准并订正 spec
（用户裁决）。原型是 throwaway 视觉参考，不是运行时资源。

## 48.2 深浅切换分离（本次只落 Dark）

> **状态更新（ADR-0016 / #39）**：切换功能已实现——语义槽结构体化为
> `Palette { dark(), light() }`（§10 L1 已接线）、config 新增 `theme` 三态字段、
> `install_theme` 按双色板装配 `Visuals::dark/light`、运行时每帧按 `ctx.theme()`
> 解析（System 模式跟随 OS 与原生标题栏）。下方条款为 Main Page 重写时的历史
> 约束，其中「不新增」清单与「L1 不进代码」已作废；实现细节以 ADR-0016 为准。

* `theme.rs` 的语义槽值改为 Dark（B）色值；UI 只消费语义槽/派生函数，
  不出现内联色值（既有源码扫描守卫测试继续生效）。
* 本次**不新增**：切换入口、config 字段、运行时模式、`Visuals::dark/light`
  分支、`theme.rs` 双实例结构（Palette 结构体化）。
* Light（L1）值以 §10 为准，不进代码；未来切换功能立项后，把语义槽
  结构体化为 `Palette { dark(), light() }`，UI 引用点因此集中改写一次，
  不在本次重写中先行铺路（避免撞车）。
* 若实现中发现 UI 将语义槽硬编码为「当前只可能是一个深色值」的假设，
  优先改为消费槽名（低成本、无切换功能）即可，不扩大到切换机制。

---

# 49. Visual Review Checklist

## Information

* [ ] Patch Version 不存在于 Main Page
* [ ] Steam State 只由 Status Bar 表达
* [ ] Deploy Status 只使用「已应用 / 未应用」
* [ ] Health Summary 与 Deploy Status 分离
* [ ] Update Action 属于 Hero / Deploy Status

## Hierarchy

* [ ] 只有一个 Primary CTA
* [ ] Primary 明显高于 Secondary
* [ ] Secondary 明显高于 Status Bar
* [ ] Hero 是核心 Surface
* [ ] Update 默认低权重

## Color

* [ ] Neutral Dark Foundation 占主体
* [ ] Accent 仅服务交互
* [ ] Accent 不过饱和
* [ ] Warning 使用 Amber
* [ ] Error 使用 Red
* [ ] Warning Secondary Blue 仅用于指定动作
* [ ] 没有 Neon
* [ ] 没有 Glow
* [ ] 没有纯黑 Background
* [ ] 没有随机临时色值

## Interaction

* [ ] Secondary 整行可点击
* [ ] 普通卸载在 Steam 未运行时不确认
* [ ] 重启 Steam 不确认
* [ ] 应用补丁并启动 Steam 不确认
* [ ] 退出 Steam 并卸载在 Steam 运行时确认
* [ ] 卸载补丁并重启在 Steam 运行时确认
* [ ] Busy 正确锁定交互类操作

## Localization

* [ ] zh 可用
* [ ] en 可用
* [ ] Primary 不换行
* [ ] Icon + Label 整体居中
* [ ] English 不通过极端缩小字号解决
* [ ] Layout 在两种语言下保持稳定

---

# 50. Final Design Read

OpenSteamTool Manager Main Page 的最终设计方向：

> **Professional Dark Desktop UI with Premium Game Launcher Character**

核心关系：

```text
Neutral Dark Foundation
        ↓
Lightweight Hero Surface
        ↓
Strong Brand CTA
        ↓
Low-Weight Maintenance Actions
        ↓
Quiet Update Control
        ↓
Persistent Status Feedback
```

核心心理路径：

```text
当前 Patch 怎么样？
        ↓
现在应该做什么？
        ↓
为什么不能做？
        ↓
需要维护时怎么办？
        ↓
刚才发生了什么？
```

设计不应追求：

> “第一眼很炫。”

而应该追求：

> **“第一眼很成熟，而且用户马上知道下一步怎么做。”**

品牌感来自：

* Logo
* Color discipline
* Surface hierarchy
* Typography
* Spacing
* Icon consistency
* CTA treatment

而不是：

* Neon
* Glow
* 大面积蓝色
* 装饰动画
* 复杂卡片
* 假 Dashboard

---

# 51. Implementation Priority

## Phase 1 — Structure

实现：

```text
Header
Hero
Health Warning
Secondary
Status Bar
```

## Phase 2 — State Mapping

验证：

```text
未应用
已应用 + Steam 未运行
已应用 + Steam 运行中
Path Invalid
Busy
Error
Update Available
```

## Phase 3 — Interaction

完成：

```text
Primary
Secondary
Confirm Dialog Exception
Update Action
Busy
Progress
Error
Success
```

## Phase 4 — Visual

完成：

```text
Palette
Typography
Surface
Border
Radius
Icon
Motion
```

> 已完成：原型对比 A / B / D 后品牌色定稿 **Palette B（§10）**；Typography /
> Surface / Border / Radius / Icon 基线见 §37 / §13 / §38 / §36。
>
> 后续实现直接以 Palette B 为唯一色源，不再重复 Palette 选型。

## Phase 5 — Localization / DPI

验证：

```text
zh
en
Windows DPI scaling
Resize
```

---

# 52. Final Acceptance Criteria

主页面只有在以下条件全部满足后，才视为完成：

* [ ] 产品视觉不再表现为 Dashboard
* [ ] Hero 是视觉中心
* [ ] Primary CTA 清晰
* [ ] Secondary 不抢 Primary
* [ ] Patch Version 完全不渲染
* [ ] Steam State 只位于 Status Bar
* [ ] Health Warning 与 Deploy Status 分离
* [ ] Update Action 位于 Hero / Deploy Status
* [ ] Busy 保持结构稳定
* [ ] 有真实进度才显示 Progress
* [ ] Success 不制造额外 Hero
* [ ] Operation Error 按用户下一步分级
* [ ] Confirm Dialog 只出现在 ADR-0007 规定的两个 Steam-running 场景
* [ ] 中文 / English 都保持稳定布局
* [ ] Accent Blue 没有过度饱和
* [ ] 页面整体以 Neutral Dark 为主
* [ ] 无 Neon / Glow / Glassmorphism
* [ ] Icon 使用统一系统
* [ ] 不存在重复信息
* [ ] 不存在与 `GLOSSARY.md` 冲突的 UI 术语
