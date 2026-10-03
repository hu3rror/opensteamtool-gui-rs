# OpenSteamTool Manager

## Main Page UI Reference & Design Specification

**Target:** Rust + egui / Windows Native Desktop GUI
**Scope:** 主页面（Main Page）
**Languages:** `zh` / `en`
**Theme:** Dark only（当前阶段不设计 Light Mode）

---

# 1. 产品定位

OpenSteamTool Manager 是一个 Windows 原生 Steam Patch 管理工具。

主页面的产品体验定位为：

> **Premium Game Launcher × Focused Utility**

即：

* 有明显的游戏产品感
* 高级、克制
* 稍微带有 Steam 式的深色游戏软件气质
* 以“当前最重要的动作”为核心
* 不呈现为 Dashboard
* 不呈现为开发者 / 运维工具
* 不追求电竞、Neon、Glow 等强烈视觉效果

### 核心体验原则

用户打开主页面后，应快速理解：

> **现在 Patch 是什么状态 → 现在最重要的动作是什么 → 如果不能操作，为什么 → Steam 当前是否运行 → 最近一次操作结果是什么。**

---

# 2. 设计原则

## 2.1 Action-first

主页面任何稳定状态下，都应存在：

> **一个唯一的 Primary CTA**

Primary CTA 根据当前业务状态动态变化。

---

## 2.2 Single Source of Truth

同一事实只在主页面呈现一次。

禁止重复：

* Patch 版本
* Steam 运行状态
* 同一错误
* 同一更新结论

尤其：

> **Patch Version 永不渲染。**

本地版本与线上版本仅供 `update_flow` 内部比较。

---

## 2.3 职责分离

主页面中的几个区域必须保持明确边界：

```text
Deploy Status
    → Patch 当前部署事实

Primary CTA
    → 当前最重要的动作

Secondary Actions
    → 备用路径 / 维护操作

Health Warning
    → Compatibility 风险

Patch Update Check
    → Patch 在线维护

Status Bar
    → Steam 持续状态 + Busy + 最近操作结果
```

不得让某个区域逐渐承担其他区域的职责。

---

# 3. 页面总体结构

主页面采用单列、垂直节奏：

```text
Windows Native Title Bar
        ↓
Application Header
        ↓
Hero Surface
        ↓
Health Warning（仅必要时）
        ↓
Secondary Action Group
        ↓
Status Bar
```

其中：

```text
Hero Surface
├── Patch Eyebrow
├── Deploy Status
├── Supporting Text（必要时）
├── Primary CTA
└── Patch Update Check
```

---

# 4. Windows Window Chrome

## Locked

使用：

> **Windows 原生标题栏**

不实现自定义 Window Chrome。

Windows 原生标题栏承担：

* Window identity
* Minimize
* Maximize
* Close
* Windows 原生窗口行为

不要为了视觉风格自行接管：

* 标题栏拖拽
* 双击标题栏行为
* 最小化
* 最大化
* 关闭
* 系统窗口控制

---

# 5. Application Header

Header 采用：

```text
[Logo + App Name]                         [Settings]
```

## Locked

左侧：

> 完整横向 Logo（Icon + App Name）

右侧：

> Settings Icon

Header 不负责：

* 页面标题
* Hero 状态
* Steam State
* Patch Version

### 禁止

不要额外出现：

* `Home`
* `Launcher`
* `Steam Patch`
* `OpenSteamTool Manager` 页面标题

Windows Title Bar 和 Application Header 已经足够承担应用身份。

---

# 6. Visual Direction

## 6.1 Overall Character

最终视觉方向：

> **Dark / Premium / Calm / Game-aware / Action-first**

关键词：

* Deep Blue-Gray
* Low visual density
* Moderate contrast
* Strong Primary CTA
* Subtle Surface hierarchy
* Subtle game atmosphere

---

## 6.2 Steam-like Reference

允许借鉴的是：

* 深色蓝灰氛围
* 游戏产品的整体亲和感
* 克制而实用的交互

禁止直接复制：

* Steam 导航结构
* Steam 卡片结构
* Steam 按钮样式
* Steam UI 组件
* Steam 品牌装饰

目标是：

> **“让人感觉像游戏生态里的成熟产品”，而不是“Steam Clone”。**

---

# 7. Surface System

视觉材质采用：

> **Light Layering + Subtle Game Atmosphere**

页面需要有：

```text
Page Background
    ↓
Hero Surface
    ↓
Primary CTA
```

## Hero Surface

Hero 与 Background 有低强度亮度层次。

使用：

* 低对比 Surface
* 1px 左右的低对比 Border
* 极弱品牌氛围

不使用：

* Glassmorphism
* 强阴影
* Neon
* Glow
* 大面积 Gradient
* 发光边框

### Border

Hero 默认使用极细、中性的 Border。

Border 的职责是：

> **定义 Surface 边界**

不是：

> **作为状态动画。**

---

# 8. Color System

采用项目已有的 `Semantic Palette` 作为唯一色彩来源。

至少包括：

```text
Background
Surface
Surface Elevated
Border

Text Primary
Text Secondary
Text Muted

Accent Blue
Success Green
Warning Amber
Error Red

Warning Secondary Blue
```

## Brand Blue

采用：

> **明显但克制的 Brand Blue**

主要用于：

* Primary CTA
* Interactive
* Active / Hover 等派生状态

不用于：

* 大面积背景
* 整块 Hero 染色
* 大面积装饰

---

## 特殊警戒色

严格遵循 Glossary：

> `Warning Secondary Blue`

**唯一用于：**

> `退出 Steam 并卸载补丁`

不得扩展给其他按钮。

---

# 9. Typography

不引入额外品牌字体。

使用：

> **系统字体 / 现有 egui font fallback**

品牌感主要来自：

* Logo
* Color
* Layout
* Surface
* Typography hierarchy
* Icon system

---

## Typography Hierarchy

层级：

```text
Hero Status
    ↓
App Name / Primary Action
    ↓
Supporting Text
    ↓
Secondary Action
    ↓
Status Bar
```

### Hero Status

例如：

```text
Patch

已应用
```

`已应用 / 未应用` 是 Hero 的主要文字。

---

## 状态文字的视觉原则

正常状态：

> 大字号、中性色为主、语义色只做点缀。

例如：

```text
Patch

✓ 已应用
```

不应做成整块高饱和绿色。

异常 / Busy：

> 根据语义提高视觉权重，但不把整个 Surface 染成状态色。

---

# 10. Hero Surface

Hero 是主页面视觉核心。

它不是传统 Dashboard Card。

Hero 的职责：

> **Patch Deploy Status + Primary CTA + Patch Update Check**

---

## Hero 内部结构

固定信息顺序：

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

---

# 11. Deploy Status

严格使用项目 Glossary 的用户术语：

```text
已应用
未应用
```

禁止自行创造：

```text
Installed
Active
Ready
Enabled
Patched
```

---

## Path Invalid

Steam Path Invalid：

> **不是新的第三种 Deploy Status**

业务上仍：

```text
Deploy Status = 未应用
```

同时可以显示：

```text
Steam 路径无效
```

以及：

```text
前往设置修复
```

不得把 Path Invalid 单独定义为：

```text
Deploy Status = Path Invalid
```

---

# 12. Supporting Text

Supporting Text 默认隐藏。

只在：

> 用户需要理解当前状态，或者 Primary CTA 当前不可用

时显示。

例如：

```text
Patch

未应用

补丁未下载
请点击「检查补丁更新」下载新版本
```

Supporting Text 的职责：

> **解释**

而不是：

> **新增操作入口**

因此：

> 不在 Supporting Text 内再次创建 Download Button。

真正操作仍由：

> `检查补丁更新`

负责。

---

# 13. Primary CTA

Primary CTA 是首页最强交互元素。

视觉方案：

> **Solid Brand Blue + Very Subtle Game Atmosphere**

允许：

* 非常轻的渐变
* Hover 亮度变化
* Press slight sink
* Focus ring
* Subtle depth

禁止：

* Neon
* Glow
* 强阴影
* 发光边框
* 大范围动画

---

## CTA Geometry

Primary CTA：

* 固定高度体系
* 稳定 Action Width
* 单行
* Icon + Label 整体居中

不允许因中英文变化产生剧烈几何变化。

不要：

* 自动换行
* 通过缩小字号硬塞
* 通过 ellipsis 截断核心文案

---

# 14. Primary CTA State Mapping

这是主页面的核心状态映射：

| Deploy Status            | Steam State | Primary CTA   |
| ------------------------ | ----------- | ------------- |
| 未应用                      | 未运行         | 应用补丁并启动 Steam |
| 已应用                      | 未运行         | 启动 Steam      |
| 已应用                      | 运行中         | 重启 Steam      |
| 未应用 + Steam Path Invalid | —           | 前往设置修复        |
| 任意交互操作                   | Busy        | Busy Stage    |

---

# 15. “未应用”状态

Primary：

```text
[Play] 应用补丁并启动 Steam
```

Secondary：

```text
[Play] 正常启动 Steam
```

### 视觉层级

```text
Primary
████████████████████

Secondary
        [Play] 正常启动 Steam
```

`正常启动 Steam` 是：

> **备用启动路径**

不是第二个 Primary。

---

## 补丁文件缺失

根据 Glossary：

```text
应用补丁并启动 Steam
```

必须 disabled。

同时 Hero 显示：

```text
未应用

补丁未下载
请点击「检查补丁更新」下载新版本
```

而：

```text
正常启动 Steam
```

仍保持可用。

---

# 16. “已应用 + Steam 未运行”

Primary：

```text
[Play] 启动 Steam
```

Secondary：

```text
[Uninstall] 卸载补丁
[Uninstall] 卸载补丁并重启 Steam
```

Primary 是唯一核心动作。

---

# 17. “已应用 + Steam 运行中”

Primary：

```text
[Restart] 重启 Steam
```

Secondary：

```text
[Exit + Uninstall] 退出 Steam 并卸载补丁
[Uninstall + Restart] 卸载补丁并重启 Steam
```

排序：

```text
重启 Steam
    ↓
退出 Steam 并卸载补丁
    ↓
卸载补丁并重启 Steam
```

其中：

> `退出 Steam 并卸载补丁`

使用 `Warning Secondary Blue`。

---

# 18. Secondary Action System

Secondary 采用：

> **Inline Action**

默认：

```text
[Icon]  卸载补丁
```

不是完整按钮。

---

## Hit Area

实际点击区域：

> **整行可点击**

但默认背景透明。

Hover：

```text
╭────────────────────────────╮
│ [Icon]  卸载补丁            │
╰────────────────────────────╯
```

因此：

> **Visual Weight Low / Hit Area Large**

---

# 19. Secondary Action Group

Secondary 固定：

> **纵向排列**

不横向并排。

原因：

* 中文 / English 长度不同
* 避免操作竞争
* 保持 Launcher 的单列节奏
* 降低布局复杂度

Group 使用：

> **Spacing + Very Subtle Divider**

不是 Card。

---

# 20. Action Safety

根据已确认的产品交互原则：

> **明确点击即直接执行。**

原则层面不增加确认弹窗，包括：

* 卸载补丁
* 重启 Steam
* 退出 Steam 并卸载补丁
* 卸载补丁并重启 Steam

### 20.1 裁决：关闭确认框的既有例外（按 ADR-0007 #36 修订执行）

上述原则有一个既有例外，落地时保持 ADR-0007 现状、不因本规格撤销：

> **仅「退出 Steam 并卸载补丁」/「卸载补丁并重启 Steam」在 Steam 运行中弹「关闭确认」框**；
> 「应用补丁并启动 Steam」（Steam 运行中直接放行，`kill_first` 由运行态派生）与
> 「重启 Steam」恒不经确认框（按钮语义本身即「关闭并重启」，再确认是冗余打扰）。

即「卸载补丁并重启 Steam」「退出 Steam 并卸载补丁」在 Steam 运行中仍保留关闭确认框；
「卸载补丁」（Steam 未运行时路径）与「重启 Steam」「应用补丁并启动 Steam」确认框不出现。

---

# 21. Busy System

Busy 与 `Busy Gate` 对齐。

当任何交互类 Action / Update Action 执行时：

```text
Action / Update Action
        ↓
Busy Gate
        ↓
Main UI Busy
```

同一时间只允许一个交互类后台操作。

---

## Busy Presentation

页面结构保持不变。

Hero 原位进入：

> **Stage-driven Busy State**

例如：

```text
Patch

正在应用补丁…
```

下一阶段：

```text
Patch

正在启动 Steam…
```

而不是：

```text
正在执行“应用补丁并启动 Steam”
```

Busy 文案表达：

> **当前阶段**

---

## Busy During Interaction

交互类控制：

```text
Primary       disabled
Secondary     disabled
Update        disabled
```

UI 不应出现多个并行可点击 Action。

---

# 22. Progress

只有存在可靠进度值时才显示 Progress Bar。

### 无真实进度

```text
正在启动 Steam…
```

### 有真实进度

```text
正在下载补丁…

██████████████░░░░░░ 68%
```

### 解压等无可靠进度阶段

```text
正在解压补丁…
```

禁止伪造：

```text
正在启动 Steam… 67%
```

---

# 23. Error System

错误不统一做成大红色 Error Page。

采用：

> **Error UI 权重由“下一步需要做什么”决定。**

---

## 操作失败

例如：

```text
补丁应用失败

[Retry] 重试
```

Hero 可以接管。

---

## 环境阻塞

例如：

```text
未应用

Steam 路径无效

[Settings] 前往设置修复
```

Hero 接管。

---

## Update 链路失败

错误停留在 Update Action 所属区域：

```text
[Refresh] 检查补丁更新
检查失败
```

不要把 Update Error 同时复制到：

* Hero
* Status Bar
* Update

多个地方。

---

# 24. Success System

不创建独立 Success Hero。

成功后：

> **直接进入新的稳定状态**

例如：

```text
未应用
    ↓
正在应用补丁…
    ↓
已应用
```

不是：

```text
未应用
    ↓
正在应用补丁…
    ↓
✓ 应用成功
    ↓
已应用
```

成功事实由：

```text
Deploy Status
+
Status Bar Recent Result
```

承担。

---

# 25. Patch Update Check

`Patch Update Check` 属于：

> **Hero / Deploy Status 区域内部的 Patch Maintenance**

它与 App Update Check 完全独立。

---

## Normal State

轻量 Inline Action：

```text
[Refresh] 检查补丁更新
```

---

## Checking

同一按钮位：

```text
[Progress] 检查中…
```

检查中不进入 Status Bar。

---

## No Update

恢复：

```text
[Refresh] 检查补丁更新
```

必要时在该区域附近提供简短结果提示。

**不得显示 Patch Version。**

---

## Update Available

该按钮位视觉升级：

```text
╭────────────────────────────────╮
│ [Download] 下载并解压新版本     │
╰────────────────────────────────╯
```

这是一个视觉升级事件，但：

> **仍然不能抢过 Primary CTA。**

---

## Download / Extract

```text
正在下载补丁…
```

如果有真实下载进度，则显示 Progress Bar。

之后：

```text
正在解压补丁…
```

---

## Failure

恢复 Update Action，并在其附近表达失败：

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

# 26. Patch Version Rule

这是硬性约束：

> **Patch Version 永远不渲染。**

主页面任何位置禁止出现：

* 本地 Patch Version
* Online Version
* Current Version
* Latest Version
* Version Number

因此不要出现：

```text
Patch · v1.4.2
v1.4.2 → v1.5.0
```

版本只属于：

```text
update_flow
```

内部。

---

# 27. Health Warning

主页面只显示 Glossary 定义的两类 Health Risk：

```text
上游尚未适配
未找到核心 DLL
```

其他健康度：

* 检查中
* 网络不可用
* 上游已适配（未缓存）
* 完美兼容

不在主页面显示风险警示。

---

## Position

位于：

```text
Hero
↓
Health Warning
↓
Secondary
```

正常状态不存在该区域。

---

## Visual

统一使用：

> **Warning Amber + Warning Icon**

例如：

```text
╎ [Warning] 当前 Steam 尚未适配              设置 →
```

或：

```text
╎ [Warning] 未找到 Steam 核心文件            设置 →
```

两种状态：

* 同一个组件
* 同一个视觉语义
* 不使用 Error Red
* 整行可点击

点击：

> `Settings → Steam`

---

# 28. Status Bar

Status Bar 位于主页面底部。

采用：

> **极轻量信息栏**

而不是独立 Card。

形式：

```text
────────────────────────────────────────────

● Steam 正在运行       ✓ 最近操作成功
```

---

# 29. Status Bar Data Model

根据 Glossary，Status Bar 可以包含：

```text
Steam State
Busy
Recent Operation Result
```

但是：

> **Patch Update Check 的检查中 / 结果不进入 Status Bar。**

它们仍然内联在 Update Action 附近。

---

# 30. Steam State

Steam State 恒显示。

```text
● Steam 正在运行
```

或：

```text
● Steam 未运行
```

语义：

```text
Running      → Success Green
Not Running  → Neutral Gray
```

Steam State 不在 Hero 重复显示。

---

# 31. Recent Operation Result

Recent Result 是：

> **最近一次相关操作的结果**

直到新的相关结果覆盖。

例如：

```text
● Steam 正在运行       ✓ Steam 已重启
```

---

## Steam Start / Restart Success

严格遵循 Glossary：

Steam 后续退出后：

```text
● Steam 未运行
```

不要继续显示：

```text
✓ Steam 已重启
```

避免产生过期语义。

---

## Patch Operation Success

例如：

```text
● Steam 未运行       ✓ 补丁已应用
```

或：

```text
● Steam 未运行       ✓ 补丁已卸载
```

这类结果不因 Steam 运行状态改变而强制失效。

---

# 32. Icon System

使用统一的：

> **Icon Component**

禁止直接依赖 Unicode / Emoji 作为 UI 图标。

例如不要：

```text
▶
↻
⚙
⚠
⏏
```

作为最终 UI 图标实现。

应该使用：

```text
[PlayIcon]
[RestartIcon]
[SettingsIcon]
[WarningIcon]
[UninstallIcon]
```

---

## Icon Rules

Icon：

* 使用统一 icon family
* 尺寸采用 logical points
* 保持一致的视觉 weight
* 颜色来自 Semantic Palette
* 不允许各组件自行决定图标风格

具体 icon family：

> **待结合现有项目依赖确定**

不在本策划书中强制引入某一个图标库。

---

# 33. Icon + Label

Primary CTA：

> **Icon + Label 作为一个整体居中**

例如：

```text
┌────────────────────────────────┐
│        [Play] 启动 Steam       │
└────────────────────────────────┘
```

英文：

```text
┌────────────────────────────────┐
│       [Play] Launch Steam      │
└────────────────────────────────┘
```

Icon 不固定占据按钮最左侧。

---

# 34. Localization

中英文都是正式产品需求。

原则：

> **先优化 wording，再用宽度适配。**

---

## Primary CTA

必须：

* 单行
* 不换行
* Icon + Label 整体居中

---

## Secondary

优先保持：

* 单行
* 足够宽的点击区域
* 不通过缩小字号解决英文过长

---

## Update

Update Action 比 Primary 更灵活，但仍应优先通过 wording 控制长度。

---

# 35. Layout / Resize

窗口：

> **允许 Resize**

内容区域：

> **存在合理 max-width**

Hero：

> **存在舒适 max-width**

但：

> **具体数值属于 implementation tuning，不作为当前 Locked Product Requirement。**

窗口放大时：

```text
不要无限拉宽 Hero
不要无限拉宽 Primary CTA
```

额外空间优先成为：

> **Whitespace / Breathing Room**

---

# 36. Vertical Rhythm

采用：

> **Adaptive Breathing**

固定的是：

> 各层之间的最小间距和 hierarchy

不是：

> 整个窗口的绝对像素位置

额外高度主要用于：

> Hero 周边的留白

---

# 37. Hero Position

Hero：

> **略微偏上**

而不是严格数学中心。

原因：

下方存在：

* Health Warning
* Secondary Actions
* Update
* Status Bar

视觉重量更大。

因此 Hero 需要稍微上移，以保持整体视觉平衡。

---

# 38. Corner Radius

采用 Balanced hierarchy。

规则：

```text
Hero Surface
    ↓
Primary CTA
    ↓
Secondary Row
    ↓
Small UI
```

圆角随层级逐步减小。

不允许整个 UI：

> 所有组件全部使用同一个圆角。

具体数值：

> **属于 Design Token tuning，而非本文的硬编码要求。**

---

# 39. Interaction Motion

整体 Motion：

> **Low / Subtle**

允许：

### Hover

轻微：

* Brightness
* Surface
* Accent

### Press

轻微：

* Downshift
* Depth change

### Busy

允许：

* Spinner
* Determinate Progress

### State Transition

允许：

* Short opacity / color transition

---

## 禁止

* Page entrance choreography
* Magnetic Button
* 大范围飞入
* Background animation
* Particle effects
* Glow pulse
* 无意义装饰动画

原则：

> **Motion 只说明状态变化，不承担视觉炫技。**

---

# 40. Main Page State Matrix

| UI State           | Hero Status      | Primary             | Secondary                      | Health Warning   | Update              | Status Bar                         |
| ------------------ | ---------------- | ------------------- | ------------------------------ | ---------------- | ------------------- | ---------------------------------- |
| 未应用 + Steam 未运行    | 未应用              | 应用补丁并启动 Steam       | 正常启动 Steam                     | 按 Health Summary | 检查补丁更新              | Steam 未运行                          |
| 已应用 + Steam 未运行    | 已应用              | 启动 Steam            | 卸载补丁 / 卸载补丁并重启 Steam           | 按 Health Summary | 检查补丁更新              | Steam 未运行                          |
| 已应用 + Steam 运行中    | 已应用              | 重启 Steam            | 退出 Steam 并卸载补丁 / 卸载补丁并重启 Steam | 按 Health Summary | 检查补丁更新              | Steam 正在运行                         |
| 未应用 + 补丁文件缺失       | 未应用              | 应用按钮 disabled       | 正常启动 Steam 可用                  | 按 Health Summary | 下载并解压新版本可用          | 当前 Steam State                     |
| Steam Path Invalid | 未应用              | 前往设置修复              | Operation disabled             | 按 Health Summary | 依据 Update Flow      | 当前 Steam State                     |
| Busy               | 当前 Deploy Status | Busy Stage          | Disabled                       | 保持规则             | Disabled            | Steam State + Busy / Recent Result |
| 操作失败               | 当前 Deploy Status | Retry / Remediation | 视情况                            | 视 Health Summary | 按业务归属               | Recent Result                      |
| Update Available   | 原 Deploy Status  | 原 Primary           | 原 Secondary                    | 视 Health Summary | 升级为 Download Action | Steam State + Recent Result        |

---

# 41. Component Boundary

推荐将主页面 UI 组件拆分为：

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

---

# 42. Presentation Architecture

UI 不应自行推导业务状态。

推荐：

```text
Business State
      ↓
Presentation / View Model
      ↓
MainPage UI
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

其中：

```text
deploy_status
    ← deployment facts

primary_action
    ← deployment + Steam state + available operation

health_warning
    ← Health Summary

patch_update_action
    ← update_flow

status_bar
    ← Steam State + Busy + Recent Operation
```

---

# 43. Terminology Compliance

UI 文案必须服从 `GLOSSARY.md`。

使用：

```text
应用
部署
已应用
未应用
卸载
重启
检查补丁更新
下载并解压新版本
```

严格区分：

```text
Operation
≠
Update Action
```

以及：

```text
Steam State
≠
Deploy Status
≠
Health Summary
```

禁止混用：

```text
安装
移除
升级
Mod
注入物
Active
Installed
Ready
```

---

# 44. Explicit Non-Goals

主页面当前不承担：

* Patch Version 展示
* Release Notes
* Steam Path 明细
* Compatibility Details
* DLL SHA-256
* Verified Cache 信息
* App Version
* App Update Check
* Logs / Console
* Statistics
* Dashboard
* 高级监控

这些功能存在于：

> Settings / About / Steam / 其他既有模块

而不是 Main Page。

---

# 45. Implementation Priority

AGENTS 实现时应按以下顺序完成：

## Phase 1 — Structure

先实现：

```text
Header
Hero
Health Warning
Secondary
Update
Status Bar
```

确保各区域边界正确。

---

## Phase 2 — State Mapping

实现所有主状态：

```text
未应用
已应用 + 未运行
已应用 + 运行中
Path Invalid
Busy
Error
Update Available
```

确保 UI 只消费 Presentation State。

---

## Phase 3 — Visual System

实现：

* Semantic Palette
* Typography hierarchy
* Surface hierarchy
* Radius hierarchy
* Icon system
* Primary / Secondary hierarchy

---

## Phase 4 — Localization

检查：

```text
zh
en
```

确保：

* Primary 单行
* Secondary 可用
* Update 可用
* 状态切换不导致结构崩坏

---

## Phase 5 — Interaction / Motion

最后实现：

* Hover
* Press
* Busy
* Progress
* Error
* Success transition

不要先做动画。

---

# 46. Acceptance Criteria

主页面完成后，至少满足：

### Information

* [ ] Patch Version 在首页完全不存在
* [ ] Steam State 只由 Status Bar 表达
* [ ] Deploy Status 只使用「已应用 / 未应用」
* [ ] Health Summary 未与 Deploy Status 混合
* [ ] Update Action 不进入 Status Bar

### Hierarchy

* [ ] 页面只有一个 Primary CTA
* [ ] Primary 明显强于 Secondary
* [ ] Secondary 明显强于 Status Bar
* [ ] Hero 是主视觉 Surface
* [ ] Update 默认低权重

### State

* [ ] 未应用 → 应用补丁并启动 Steam
* [ ] 已应用 + 未运行 → 启动 Steam
* [ ] 已应用 + 运行中 → 重启 Steam
* [ ] Path Invalid → Hero 提供前往设置修复
* [ ] Busy → 页面结构稳定
* [ ] Real progress → 才显示 Progress Bar
* [ ] Error → 按错误性质决定展示位置
* [ ] Success → 回到新的稳定状态

### Interaction

* [ ] Secondary 点击区域覆盖整行
* [ ] 卸载不弹确认
* [ ] 重启不弹确认
* [ ] 关闭并卸载不弹确认
* [ ] Busy Gate 正确限制交互类操作
* [ ] Update Check / Download 共用按钮位

### Visual

* [ ] Deep Blue-Gray
* [ ] Brand Blue
* [ ] Low-density
* [ ] Lightweight Surface hierarchy
* [ ] No Neon
* [ ] No Glow
* [ ] No Glassmorphism
* [ ] No Dashboard Card stacking
* [ ] No arbitrary colors

### Localization

* [ ] 中文可用
* [ ] 英文可用
* [ ] Primary 不换行
* [ ] Icon + Label 整体居中
* [ ] 英文不会通过极端缩小字号解决空间问题

---

# 47. Final Design Principle

整个主页面最终应遵循：

```text
Brand
  ↓
Patch Deploy State
  ↓
Current Primary Action
  ↓
Health / Remediation when needed
  ↓
Maintenance / Secondary
  ↓
Patch Update
  ↓
Persistent Steam / Recent Feedback
```

从用户心理路径来看：

```text
现在是什么状态？
        ↓
我现在该做什么？
        ↓
为什么不能做？
        ↓
如果我要维护怎么办？
        ↓
刚才发生了什么？
```

产品最终应该让用户感觉：

> **这是一个成熟、安静、有游戏产品质感的 Windows Launcher。**

而不是：

> 一个把 Steam、Patch、Compatibility、Update 等内部系统全部堆在首页上的管理面板。

**最终判断标准不是“首页信息多不多”，而是“用户是否能在最短路径内知道当前状态和下一步动作”。**
