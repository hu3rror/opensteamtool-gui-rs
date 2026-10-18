# 托盘/后台常驻应用空闲时 CPU/GPU 占用：业界态度与惯例

> 研究范围：软件开发行业对 system tray / menu bar（托盘常驻）应用空闲资源占用的态度与惯例。
> 方法：每个主张回到拥有它的源头（官方文档/源码/规范优先，权威社区共识其次），附 URL 与关键原文。
> 时间线说明：egui PR #7905 合并于 2026-03-24（0.34.0 里程碑）；本仓库（opensteamtool-gui-rs）使用 eframe 0.36，已包含该修复。

---

## 0. 结论速览

- **业界有统一但未成文的态度**：托盘/后台应用空闲时应**不持续渲染、事件循环休眠**，CPU 占用趋近 0%；任何持续空闲空转（busy-loop、隐藏后仍全帧率渲染）都被当作 **bug/缺陷** 处理，而非可接受的取舍。
- **可接受量级（社区与平台层共识）**：空闲 CPU 应在 **0%～<1%**（单个核的 1% 即 Chromium 对后台页的硬性上限；egui/eframe 官方修复目标是 "near 0%"；被用户投诉的阈值低至 0.1%～0.3%）；GPU 方面应**不持续合成/绘制**（隐藏窗口停止渲染、无动画）。
- **主流框架的实际行为**：平台层（Windows DWM 缓存位图、Chromium occlusion、Qt 最小化即停渲染）与框架层（winit `ControlFlow::Wait`、Electron `backgroundThrottling` 默认开、egui ≥0.34 空闲睡眠）**默认就趋向"零渲染+睡眠"**；对"应用确实需要后台工作"的情形，官方策略是**限流降频**（egui 100ms 心跳 ≤10Hz、Chromium 后台定时器 1 次/分钟、Electron 节流动画与定时器）。
- **理想 vs 现实**：理想是"零渲染、无事即睡、事件驱动唤醒"；现实是各框架用"限流降频"兜底，且大量真实应用（Slack/Teams/Discord 及各类 Electron/Tauri 托盘应用）仍因轮询、GIF 动画、隐藏 WebView 持续合成而闲置吃 CPU，被广泛批评。

---

## 1. egui/eframe（Rust）官方立场

### 1.1 官方立场一句话

**空闲的不可见窗口应"零渲染 + 事件循环睡眠"；若应用主动请求重绘（后台任务、托盘事件），则限流到 ≥100ms 间隔（≤10 次/秒）。** 即"空闲零渲染，有需要时限流降频"——两者都是官方行为，缺一不可。

### 1.2 缺陷背景：隐藏窗口烧 CPU 与无法恢复

**Issue #7776 "Windows: High CPU Usage when not visible"**（2025-12-14 创建，2026-03-24 由 PR #7905 关闭）
- URL: https://github.com/emilk/egui/issues/7776
- 报告者 Kyryh："When not visible (by sending the `egui::ViewportCommand::Visible(false)` command) a simple egui application will use ~16% CPU on my CPU while using 0% otherwise"
- 期望行为原文："CPU usage should stay low, similar to when it's minimized" —— 即隐藏窗口应像最小化一样≈0%。
- 根因（PR #7905 描述）：隐藏后事件循环仍以 `ControlFlow::Poll` 全速旋转，repaint 请求被立即调度，形成紧循环烧 CPU。

**Issue #5229 "[eFrame] Can't set window Visible(true) after setting the window Visible(false) on Windows"**（2024-10-07 创建，2026-03-24 关闭）
- URL: https://github.com/emilk/egui/issues/5229
- 报告者 jonatino："once you set a window Visible(false), those events [RedrawRequested] are no longer sent to the window (at least on Windows 11) therefore making it impossible to set the window Visible(true) using a viewport command."
- tray 应用开发者 Infraviored："I am building a standard tray-based utility. The app lives in the tray, and the `eframe` window is only shown when the user clicks 'Settings'... This completely blocks the standard single-process architecture for tray apps on Windows."
- 评论者 Nahor："Also, the application is taking 100% of a CPU core when the window is hidden because the event loop never blocks anymore."
- 结论：隐藏窗口空转 + 无法恢复，被托盘应用开发者视为**严重缺陷**；期待的行为就是"事件循环阻塞/睡眠"。

### 1.3 PR #7905 "Improve behavior of invisible windows"（合并 2026-03-24，里程碑 0.34.0）

- URL: https://github.com/emilk/egui/pull/7905
- 官方修复方案（PR body 原文）：
  > "**For #7776:** Three layers of throttling for invisible windows:
  > - **Heartbeat scheduling:** After painting an invisible window, schedule the next repaint 100ms in the future (instead of immediately). This keeps viewport commands flowing while limiting to ~10 repaints/sec.
  > - **Event throttling:** In `user_event`, throttle `RequestRepaint` events for invisible windows to at least 100ms delay, preventing egui's repaint callback from bypassing the heartbeat.
  > - **ControlFlow fix:** Only set `ControlFlow::Poll` for visible windows. Invisible windows use `WaitUntil` instead of spinning.
  > - **Backend sleep:** Add `is_visible() == Some(false)` alongside the existing `is_minimized()` sleep check in both wgpu and glow backends (defense in depth)."
- 验收标准原文："CPU stays near 0% while window is invisible (was ~16% before fix)"
- **评审中维护者 emilk 的立场**（run.rs 评审线程）——明确偏好"无事可做就睡、不调度任何工作"：
  > "This will _always_ schedule a new 'repaint' (actually: call to `App::logic`) for invisible windows. Why? If they have something to do, they should call `.request_repaint`, and if not - can't they just sleep?"
  > 建议："with this simple change we get the throttling, but won't schedule work unless the user explicitly requests it"
  > 提交 936c120 "Address review: stop scheduling repaints for idle invisible windows" 落实了该意见。
- 结论：**egui/eframe 官方 = 空闲不可见窗口不主动调度重绘（零渲染、事件循环 `Wait` 睡眠）；若应用主动请求重绘，限流到 ≥100ms（≤10Hz）**。

### 1.4 合并后的源码（eframe main；本项目 eframe 0.36 已包含）

源码：`crates/eframe/src/native/run.rs`（https://github.com/emilk/egui/blob/main/crates/eframe/src/native/run.rs）

- 常量 `const INVISIBLE_WINDOW_REPAINT_INTERVAL: Duration = Duration::from_millis(100);`（run.rs:27）
- `check_redraw_requests` 注释（run.rs，约 254-260 行）：
  > "// Throttle any already-scheduled repaints for invisible windows to avoid busy-looping. If no repaint was requested by the app, the window will simply sleep. // See: https://github.com/emilk/egui/issues/7776"
- 事件循环控制流（run.rs，约 290-306 行）：
  > "// Always set an explicit, sleeping control flow. Previously we only set `WaitUntil` when a repaint was already scheduled, which meant that a `ControlFlow::Poll` set earlier was never undone once the last timed repaint had been consumed, leaving the loop spinning. // See https://github.com/emilk/egui/issues/8326."
  > `Some(next_repaint_time) => ControlFlow::WaitUntil(next_repaint_time), None => ControlFlow::Wait,`
- `RequestRepaint` 事件节流（run.rs，约 375-381 行）：
  > "// Throttle repaints for invisible windows to prevent high CPU usage on Windows. // See: https://github.com/emilk/egui/issues/7776" → `when.max(Instant::now() + INVISIBLE_WINDOW_REPAINT_INTERVAL)`
- 后端睡眠兜底：`winit_integration.rs` 的 `sleep_if_invisible_or_minimized()`（注释引用 #325 "On Mac, a minimized window uses up all CPU" 与 #7776），glow/wgpu 后端在不可见/最小化时 sleep 10ms（"defense in depth"）。
- 语义细节（源码核对）：`windows_next_repaint_times` 仅在应用请求了重绘（`RepaintNow/RepaintNext/RepaintAt`，run.rs:127-156）时才存在条目；不可见窗口的已调度重绘被 cap 在 ≤100ms 内处理（保证 `Visible(true)` 等 viewport 命令 ≤100ms 送达），而 `user_event` 又把请求本身限频到 ≥100ms；**应用完全不请求时无条目 → `ControlFlow::Wait` → 完全休眠**。

### 1.5 egui 文档对 repaint 模型的定位（按需重绘是设计原则）

- **egui README**（https://github.com/emilk/egui#readme，约 234 行）：
  > "`egui` only repaints when there is interaction (e.g. mouse movement) or an animation, so if your app is idle, no CPU is wasted."
- **`Context::request_repaint_after` 文档**（crates/egui/src/context.rs）：
  > "This is primarily useful for applications who would like to save battery by avoiding wasted redraws when the app is not in focus."
- **`Context::request_repaint` 文档**："Call this if there is need to repaint the UI, i.e. if you are showing an animation. If this is called at least once in a frame, then there will be another frame right after this. Call as many times as you wish, only one repaint will be issued."
- 含义：egui 是**事件驱动重绘**模型——空闲零绘制；托盘/后台事件通过 `request_repaint` / `request_repaint_after` 主动唤醒（eframe 跨线程回调已接通）。
- 底层 winit 侧：`ControlFlow::Wait` = 阻塞等待新事件（睡眠），`Poll` = 持续轮询（空转）。见 https://docs.rs/winit/latest/winit/event_loop/enum.ControlFlow.html ；eframe 源码注释亦明确 "request_redraw is enough to wake the event loop"（run.rs 内 #8326 相关注释）。

---

## 2. Electron / Chromium 生态

### 2.1 Chromium 官方对"不可见内容"的资源策略：停止渲染 + 深度节流

- **Chromium Blog "Reducing power consumption for background tabs"**（2017-03）——官方给出可量化的"后台 CPU 上限"：
  - URL: https://blog.chromium.org/2017/03/reducing-power-consumption-for.html
  - "Chrome 57 will delay timers to limit average CPU load to **1% of a core** if an application uses too much CPU in background."
  - "In the long-term, the ideal is for background tabs to be **fully suspended** and instead rely on new APIs for service workers to do work in the background."
- **Chromium Blog "Tab throttling and more performance improvements in Chrome M87"**（2020-11）——"看不见就不渲染"：
  - URL: https://blog.chromium.org/2020/11/tab-throttling-and-more-performance.html
  - "We've made significant improvements by **preventing background tabs from waking up your CPU too often, and no longer rendering tabs that you can't see**."
  - "Beginning in M87, we're throttling JavaScript timer wake-ups in background tabs to **once per minute**. This reduces CPU usage by up to 5x..."
  - "Next, we're bringing **Occlusion Tracking**--which was previously added to Chrome OS and Mac--to Windows, which allows Chrome to know which windows and tabs are actually visible to you. With this information, Chrome can optimize resources for the tabs you are using, **not the ones you've minimized**..."
- **Chrome for Developers "Heavy throttling of chained JS timers beginning in Chrome 88"**——隐藏页定时器分级节流 + 动画原则：
  - URL: https://developer.chrome.com/blog/timer-throttling-in-chrome-88
  - "Intensive throttling happens to timers ... The page has been hidden for more than 5 minutes ... the browser will check timers in this group **once per minute**."
  - "**Animation is a visual thing, so it shouldn't use CPU time when the page is hidden.** `requestAnimationFrame` will wait for the page to be visible, so it doesn't use any CPU when the page is hidden."
  - 并建议：状态轮询应改为推送（web sockets / SSE / observers）。

### 2.2 Electron 官方文档：backgroundThrottling 默认开启 + 隐藏时暂停重活

- **Electron BrowserWindow / WebPreferences 文档**（`backgroundThrottling` 选项）：
  - URL: https://www.electronjs.org/docs/latest/api/browser-window ；https://github.com/electron/electron/blob/main/docs/api/structures/web-preferences.md
  - "`backgroundThrottling` boolean (optional) - Whether to throttle animations and timers when the page becomes background. This also affects the Page Visibility API. ... Defaults to `true`."
  - Page Visibility 一节："It is recommended that you pause expensive operations when the visibility state is `hidden` in order to minimize power consumption."
- **Electron 官方也将"隐藏窗口仍全帧率渲染"当作 bug 修**：PR #54341 "fix: stop a hidden page rendering after setBackgroundThrottling is re-enabled"（"Fixed hidden windows continuing to render at full frame rate..."，https://github.com/electron/electron/pull/54341）；issue #31016（Windows 上 `backgroundThrottling: false` 时隐藏窗口 rAF 仍被节流，被报告为行为不一致，https://github.com/electron/electron/issues/31016）。

### 2.3 托盘常驻 Electron 应用（Slack/Discord/Teams）空闲资源占用被广泛批评

- **Microsoft Teams / Slack 常驻资源占用（Hacker News 社区记录）**：
  - https://news.ycombinator.com/item?id=36448804 ——"Microsoft Teams is an Electron app, so it spawns eight processes, over 200 threads, and consumes about 1GB of memory while idle. Slack is a similar situation. Six processes, over 100 threads, ~750MB RAM while idle. For a chat app!"
  - https://news.ycombinator.com/item?id=16436815 ——"Slack uses ~ 2 GB of Ram and ~ 10% CPU when idle."
- **"空闲 13% CPU"被视为必须修的 bug**（Slack 曾在生产环境误发 React dev build，引发著名的空闲 CPU 问题讨论）：
  - https://news.ycombinator.com/item?id=14087899 ——"that particular issue only hurt idle CPU usage, and the usage was something like 13% CPU usage IIRC... These idle CPU bugs are reproduced simply by opening the application." / "An app that's sitting on high idle usage sets off klaxons for me."；同帖指出 Slack 隐藏窗口下 GIF 动画让 CPU "never drops below a whopping 22%"（对比 Chrome 切走后 GIF 几乎零 CPU）。
- **微软自己把"空闲资源占用"当作卖点修复**（New Teams 重写，Tech Community 官方博客）：
  - URL: https://techcommunity.microsoft.com/blog/microsoftteamsblog/introducing-the-new-microsoft-teams-now-in-preview/3774406
  - "2X faster, whether you're starting up the app, presenting in a meeting, or responding to messages；**50% less memory, lower CPU, disk utilization, and more battery life**"
- 学术量化（QUATIC 2024 实证论文《Electron vs. Web: A Comparative Analysis of Energy and Performance in Communication Apps》）：
  - URL: http://www.ivanomalavolta.com/files/papers/QUATIC_2024_2.pdf
  - 结论："Electron apps tend to utilize more CPU than Web apps, especially in the early 2-minute mark"（Electron 均 CPU 15.94% vs Web 14.03%）。

---

## 3. Qt

### 3.1 QSystemTrayIcon 官方文档：托盘是"长驻应用"的界面

- URL: https://doc.qt.io/qt-6/qsystemtrayicon.html
- "Modern operating systems usually provide a special area on the desktop, called the _system tray_ or _notification area_, where **long-running applications** can display icons and short messages."
- 文档本身不规定空闲 CPU 数字，但把托盘定位为"长期运行应用"的常驻入口；Qt 官方还提供 systray 示例（`qtwidgets-desktop-systray-example`），其模式即"主窗口隐藏到托盘 + 事件驱动显示"。

### 3.2 Qt 对隐藏窗口/后台渲染的处理：官方文档明说"隐藏期间不必浪费 CPU"

- **QWidget 文档（Qt 6.12）visible 属性**：
  - URL: https://doc.qt.io/qt-6/qwidget.html
  - "A widget receives show and hide events when its visibility status changes. **Between a hide and a show event, there is no need to waste CPU cycles preparing or displaying information to the user.** A video application, for example, might simply stop generating new frames."
- **QWidget::repaint 文档**："Repaints the widget directly by calling paintEvent() immediately, **unless updates are disabled or the widget is hidden**."（https://doc.qt.io/qt-6/qwidget.html#repaint）
- **Qt 平台源码：最小化即命令渲染器停止**（qwindows 平台插件）：
  - URL: https://github.com/qt/qtbase/blob/5.11/src/plugins/platforms/windows/qwindowswindow.cpp
  - `if (state & Qt::WindowMinimized) { handleHidden(); QWindowSystemInterface::flushWindowSystemEvents(...); // Tell QQuickWindow to stop rendering now. }`
- **Qt::WA_ShowWithoutActivating**："Show the widget without making it active."（https://doc.qt.io/qt-6/qt.html#WidgetAttribute-enum；亦可核对 https://doc.qt.io/archives/qtjambi-4.5.2_01/com/trolltech/qt/core/Qt.WidgetAttribute.html）——托盘弹窗/无焦点浮层惯例属性，与渲染策略无关但与"托盘 UI 不抢焦点"惯例相关。
- 社区印证（Qt Forum）："A hidden widget is not painted."（https://forum.qt.io/topic/125450/update-repaint-a-qwidget-while-it-is-hidden）

### 3.3 小结（Qt）

Qt 的模型与 egui 一致：**隐藏/最小化 = 不绘制（零渲染）+ 平台层停止合成**；应用逻辑靠事件循环（定时器/信号）驱动，空闲时事件循环阻塞。官方文档直接写了"隐藏期间没有理由浪费 CPU 周期"。

---

## 4. Tauri / WPF / WinForms

### 4.1 Tauri

- **官方配置文档 `backgroundThrottling`（v2）**：
  - URL: https://v2.tauri.app/reference/config/
  - "By default, browsers use a suspend policy that will throttle timers and even unload the whole tab (view) to free resources after roughly 5 minutes when a view became minimized or hidden. This will pause all tasks until the documents visibility state changes back from hidden to visible by bringing the view back to the foreground."（平台限制：Linux/Windows/Android 不支持该配置项，仅 macOS/iOS 14+/17+ 支持）
- **官方托盘文档（System Tray）**：托盘图标 + 菜单 + 事件（click/double-click/enter/move/leave），示例即"点击托盘图标 → show + set_focus 主窗口"（https://v2.tauri.app/learn/system-tray/）。
- **真实 Tauri 托盘应用案例：隐藏窗口仍全速合成被当 bug 修**——nextai-translator PR #1887 "fix: suspend WebView2 renderers for hidden panels on Windows"（https://github.com/nextai-translator/nextai-translator/pull/1887）：
  > "idle CPU on Windows stayed at ~38% in v0.6.19... `WebviewWindow::hide()` ... only hides the tao OS window. It never sends `WebviewMessage::Hide` ... So the `ICoreWebView2Controller.IsVisible` flag stays `TRUE`, and the **WebView2 renderer keeps compositing frames at full rate for a window that is invisible on screen — burning CPU** the whole time the app sits idle in the tray."
- **另一个案例：托盘空闲时应卸载整个 WebView**——cockpit-tools PR #686 "Reduce tray idle memory by unloading the main WebView"（https://github.com/jlcodes99/cockpit-tools/pull/686）：
  > "Previously, closing the window to the tray only called `hide()`, so the React frontend and WebView stayed loaded in memory even while the app appeared closed. **This made the tray-only idle state much heavier than necessary.**"

### 4.2 WinForms

- **NotifyIcon 官方文档**：
  - URL: https://learn.microsoft.com/en-us/dotnet/desktop/winforms/controls/notifyicon-component-overview-windows-forms
  - "The Windows Forms `NotifyIcon` component is typically used to display icons for **processes that run in the background and do not show a user interface much of the time**."
- 文档定位即"后台进程的托盘入口"；WinForms 惯例（社区共识）：托盘常驻 = 无主窗口/隐藏主窗口 + 消息泵阻塞等待（见 §6 SO 2829582），空闲即 ≈0%。

### 4.3 WPF

- 未找到微软官方文档明确规定"WPF 隐藏窗口是否停止渲染"（诚实说明：此为资料空白）。社区/工程惯例与 WinForms 相同（托盘常驻隐藏窗口、事件驱动）；可引用 DisplayProfileManager（WPF 托盘应用）issue 作证"空闲 3% CPU 即被用户投诉、0~0.9% 属正常"（https://github.com/zac15987/DisplayProfileManager/issues/10，见 §6）。

---

## 5. 微软平台设计指南

### 5.1 通知区域（托盘）UX 指南：不打扰、可挂起、无长动画

- **Windows 桌面应用 UX 指南 "Notification Area"**（官方，Windows 7 时代，仍未更新但原则有效）：
  - URL: https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification
  - 总原则："The notification area provides notifications and status. **Well-designed programs use the notification area appropriately, without being annoying or distracting.**"
  - 对后台任务："**Suspend optional features.** Icons are used to display status for features without desktop presence. Such features tend to be long-running, optional background tasks, such as printing, indexing, scanning, or synchronizing. Users may want to suspend such features to **increase system performance, reduce power consumption**, or because they are offline."
  - "**All background tasks must have a Suspend/Resume command.** ... Users may want to temporarily suspend background tasks to increase system performance or reduce power consumption."
  - 对动画："**Don't use long-running animations to show continuous activities. Such animations are a distraction.** An icon's presence in the notification area sufficiently indicates continuous activity."（"Don't change status too frequently. Notification area icons shouldn't appear noisy, unstable, or demand attention."）
  - 诚实说明：该指南**没有给出任何空闲 CPU/GPU 的数字指标**——微软侧没有"托盘应用空闲必须 <x% CPU"的成文规范；要求集中在"不打扰、可挂起、状态变化克制"上。
- **Win32 通知区域 API 文档（shell/notification-area）**：
  - URL: https://learn.microsoft.com/en-us/windows/win32/shell/notification-area
  - "The goal is to provide user benefit through appropriate use of notifications, without being annoying or distracting."；"The user should have the final say on which icons they want to see in their notification area."

### 5.2 Raymond Chen（微软工程师，The Old New Thing）——托盘"礼仪"权威声音

- "Why do some people call the taskbar the 'tray'?"（命名纠正；"It is never correct to refer to the notification area as the tray."）——https://devblogs.microsoft.com/oldnewthing/20030910-00/?p=42583
- "Instead of creating something and then trying to hide it, simply don't create it in the first place"（对"托盘图标滥用"的批评，直指框架让滥用太容易）：
  - https://devblogs.microsoft.com/oldnewthing/20120222-00/?p=8253
  - "**It's sad that notification icon abuse has become so popular (and application frameworks make it so easy to create an abusive notification icon) that people forget how to create a well-behaved notification icon.**"
- 说明：Chen 的文章聚焦"图标使用礼仪"而非 CPU 数字；但他所代表的微软工程文化把"well-behaved"托盘应用定义为"只在必要时出现/工作"。

---

## 6. 社区共识：空闲时该 0% CPU 吗？可接受量级

### 6.1 Stack Overflow 代表性观点

- **"Safest way to idle delphi application to wait for timer?"**（高分回答，Mason Wheeler）：
  - URL: https://stackoverflow.com/questions/2829582/safest-way-to-idle-delphi-application-to-wait-for-timer
  - "When it's not doing anything, it should sit at a 'steady state' of memory, not allocating any more, and **using 1% or less of CPU time**."
  - 另一回答（vL）："just calling Sleep() could be a good way, as that will signal the OS that you don't need a time slice at all"；"If the application has no main form and just sitting in the tray (Or being totally invisible), it won't do much. The main message loop will handle all message it receive from the OS..."
- **"Effective way to make a system tray application"**（Windows 性能团队 dev manager Foredecker）：
  - URL: https://stackoverflow.com/questions/1030440/effective-way-to-make-a-system-tray-application
  - "Tray applications run all the time. So, **if they are poorly written then they can use enough system resources to interfere with other things.** ... we've seen lots and lots of very piggy tray applications."
- **"uwamp use 1-2% cpu while doing nothing in the system tray"**（用户把 1-2% 直接当 bug 报）：
  - URL: https://stackoverflow.com/questions/46374885/uwamp-use-1-2-cpu-while-doing-nothing-in-the-system-tray
  - "with mysql and php 7.1.x running, no traffic on the server, uwamp.exe uses 2-3% CPU ... just sitting in the system tray. **I would guess it's a bug?**"（确认：是控制台的监控轮询所致。）

### 6.2 真实项目 issue 中的"可接受量级"参照

- **DisplayCAL（calibration 托盘常驻工具）#5420**：用户投诉空闲 0.1%～0.3%（"相对较高"，期望 0%）；维护者回应"这实际上就是 0% 空闲"并关闭（wontfix）：
  - URL: https://hub.displaycal.net/issue/5420/
  - 用户："It uses between 0.1% -> 0.3% (I saw 0.5% once) of the CPU resources. In comparison all the windows OS services and background apps are 0%. **I would expect the tray to be similar at 0%**..."
  - 维护者："It is effectively 0% idle and every three seconds it checks running processes... There is no headroom for optimization here."
- **DisplayProfileManager（WPF 托盘应用）#10**：空闲 ~3% 被投诉；维护者实测正常值 0～0.9%，根因是隐藏窗口仍在刷新 + WASAPI 订阅过载：
  - URL: https://github.com/zac15987/DisplayProfileManager/issues/10
  - "I was checking Task Manager ... noticed Display Profile Manager using between ~3% average CPU while it is idling in the system tray. Unless it's constantly polling something, I would expect it to be mostly idle."
  - 结论段（"GPU is using a constant 0.3% with the Window closed, there might be some polling or constant refresh of a hidden window going on"——隐藏窗口持续刷新被指为问题）。

### 6.3 量级总结（业界常见基准）

| 量级 | 语境 | 来源 |
|---|---|---|
| 0%（≈0） | 空闲托盘应用应达到的目标；egui 修复验收 "near 0%"；Qt "隐藏期间不必浪费 CPU" | §1.3、§3.2 |
| <0.1%～0.5% | 用户仍可能抱怨（DisplayCAL 案例），但维护者认为已达标 | §6.2 |
| ≤1% 单核 | Chromium 对后台标签页定时器的硬性节流上限（Chrome 57 起） | §2.1 |
| 1%～3% 空闲 | 社区普遍视为"可疑/偏 bug"（uwamp、DisplayProfileManager） | §6.1、§6.2 |
| ≥10% 空闲 | 公认严重缺陷（Slack 13%、egui 修复前 16%、WebView2 38%） | §1.2、§2.3、§4.1 |

GPU：主流共识是"隐藏窗口不应有任何持续渲染/合成"（Chromium "no longer rendering tabs that you can't see"、Qt "stop rendering now"、nextai-translator 修 WebView2 持续合成、egui 后端睡眠）。托盘图标动画被微软指南明确劝阻。

---

## 7. 结论：业界是否有统一态度？主流框架实际行为 vs 理想

### 7.1 是否有统一态度

**有，且高度一致，但分散在平台/框架/社区三层、没有单一成文标准**：

1. **平台层**（Windows DWM 位图缓存、Chromium occlusion "不渲染看不见的标签页"、Qt 最小化停渲染）默认让隐藏窗口零渲染；
2. **框架层**（winit `Wait`、Electron `backgroundThrottling=true`、egui ≥0.34 空闲睡眠）默认事件循环睡眠、按需唤醒；对"应用确有后台工作"的情形用**限流降频**兜底（egui 100ms/10Hz、Chromium 定时器 1 次/分钟、Electron 节流动画与定时器）；
3. **社区/用户层**把"空闲托盘应用持续吃 CPU/GPU"当作 bug 而非特性（Slack 13%、Teams 内存/CPU 被公开批评并迫使微软重写、WebView2 隐藏持续合成被专项修复），并普遍期待空闲 ≈0%。

唯一接近"成文规范"的量化口径是 Chromium 的"后台定时器 ≤1% 单核"与微软指南的"后台任务必须可挂起、不做长动画"；微软托盘 UX 指南本身不写 CPU 数字（§5.1 已注明）。

### 7.2 主流框架实际行为 vs 理想

- **理想**：隐藏 = 零渲染 + 事件循环睡眠 + 事件驱动唤醒 + 应用逻辑同样睡眠。
- **实际**：平台与框架默认行为**接近理想**（这正是各框架文档/源码所示），但存在三类系统性偏差：(a) 框架自身历史缺陷——隐藏窗口空转（egui #5229/#7776 修复前、Electron #54341 修复前）；(b) 应用层违规——轮询、GIF/动画、隐藏窗口仍驱动渲染（Slack GIF 22%、Tauri WebView2 38%、DisplayProfileManager 3%）；(c) 平台差异——macOS occlusion 语义与 Windows 不同，隐藏≠收到 hidden 事件，导致节流不生效（Tauri 配置仅 macOS/iOS 可用、Electron #31016）。结论：**"零渲染+睡眠"是各方默认与目标，"限流降频"是兜底，持续空转则是普遍承认的缺陷**。

---

## 8. 对本项目（egui/eframe 托盘应用）最相关的启示

本项目（opensteamtool-gui-rs，eframe 0.36 + glow + `tray_icon` crate；见 `docs/adr/0005-tech-stack-egui.md` 与 `src/tray.rs`）正是 §1 所述"隐藏窗口到托盘 + 托盘事件唤醒"的标准 eframe 托盘形态，与 #5229/#7776/#7905 完全同型：

1. **版本条件已满足**：本项目 eframe 0.36 ≥ 0.34，已内置 #7905 的修复——隐藏窗口时事件循环用 `ControlFlow::Wait` 睡眠、空闲不调度重绘（"If no repaint was requested by the app, the window will simply sleep"），因此**只要应用不自发 `request_repaint`，托盘空闲就是零渲染 + 睡眠**，CPU 应 ≈0%、无 GPU 绘制（glow 后端还有 `sleep_if_invisible_or_minimized` 兜底）。`docs/adr/0001-steam-tray-linkage.md` 所记"隐藏期间按 `INVISIBLE_WINDOW_REPAINT_INTERVAL`（100ms）被驱动"应理解为：**只有在有重绘请求（托盘事件、Steam 轮询）时才按 ≤10Hz 限流驱动 `App::logic`；无请求则完全休眠**——不要在隐藏状态下做任何周期性的、不依赖事件的 `request_repaint`（那会把"睡眠"退化为 10Hz 心跳，仍属可避免的空转）。
2. **保持"事件驱动 + 低频轮询"**：`process.rs` 的 2s Steam 轮询符合 Chromium 文档对状态轮询的劝诫方向（"state polling … look for a push equivalent；低频检查"）与 egui 官方 `request_repaint_after` 的省电语义；托盘菜单事件经全局 receiver 在 `App::logic` 轮询——确保这些唤醒点**全部走 `request_repaint_after`/事件，而不是每帧无条件重绘**，且隐藏在托盘期间不要运行 UI 动画/忙碌指示动画（微软指南："Don't use long-running animations"；隐藏后任何动画都是纯浪费）。
3. **托盘图标行为**：状态变化时才更新图标/tooltip，不做连续动画；菜单提供"最小化时自动隐藏到托盘"开关（项目已有，对应微软指南的 "Display icon in notification area" / Suspend 语义）。
4. **验收口径**：本项目可把"托盘空闲 CPU ≈0%（<0.1%）、GPU 0%、内存稳态"作为性能验收项写入测试/发布说明——这与 ADR-0005 的轻量目标（拒绝 WebView2 的 100-250MB 内存）一脉相承，也是业界对常驻应用的一致期待。

---

### 附：主要来源清单

- egui：#7905 https://github.com/emilk/egui/pull/7905 ；#7776 https://github.com/emilk/egui/issues/7776 ；#5229 https://github.com/emilk/egui/issues/5229 ；README https://github.com/emilk/egui#readme ；run.rs https://github.com/emilk/egui/blob/main/crates/eframe/src/native/run.rs ；context.rs https://github.com/emilk/egui/blob/main/crates/egui/src/context.rs
- Chromium：https://blog.chromium.org/2017/03/reducing-power-consumption-for.html ；https://blog.chromium.org/2020/11/tab-throttling-and-more-performance.html ；https://developer.chrome.com/blog/timer-throttling-in-chrome-88
- Electron：https://www.electronjs.org/docs/latest/api/browser-window ；https://github.com/electron/electron/blob/main/docs/api/structures/web-preferences.md ；https://github.com/electron/electron/pull/54341 ；https://github.com/electron/electron/issues/31016
- Qt：https://doc.qt.io/qt-6/qsystemtrayicon.html ；https://doc.qt.io/qt-6/qwidget.html ；https://github.com/qt/qtbase/blob/5.11/src/plugins/platforms/windows/qwindowswindow.cpp
- Tauri：https://v2.tauri.app/reference/config/ ；https://v2.tauri.app/learn/system-tray/ ；https://github.com/nextai-translator/nextai-translator/pull/1887 ；https://github.com/jlcodes99/cockpit-tools/pull/686
- 微软：https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification ；https://learn.microsoft.com/en-us/windows/win32/shell/notification-area ；https://learn.microsoft.com/en-us/dotnet/desktop/winforms/controls/notifyicon-component-overview-windows-forms ；https://devblogs.microsoft.com/oldnewthing/20030910-00/?p=42583 ；https://devblogs.microsoft.com/oldnewthing/20120222-00/?p=8253 ；https://techcommunity.microsoft.com/blog/microsoftteamsblog/introducing-the-new-microsoft-teams-now-in-preview/3774406
- 社区：https://stackoverflow.com/questions/2829582/safest-way-to-idle-delphi-application-to-wait-for-timer ；https://stackoverflow.com/questions/1030440/effective-way-to-make-a-system-tray-application ；https://stackoverflow.com/questions/46374885/uwamp-use-1-2-cpu-while-doing-nothing-in-the-system-tray ；https://hub.displaycal.net/issue/5420/ ；https://github.com/zac15987/DisplayProfileManager/issues/10 ；https://news.ycombinator.com/item?id=16436815 ；https://news.ycombinator.com/item?id=14087899 ；https://news.ycombinator.com/item?id=36448804 ；http://www.ivanomalavolta.com/files/papers/QUATIC_2024_2.pdf ；https://forum.qt.io/topic/125450/update-repaint-a-qwidget-while-it-is-hidden
