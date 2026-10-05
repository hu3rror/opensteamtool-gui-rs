# 单实例：命名互斥体裁决 + 命名事件唤醒（重复启动唤起既有窗口）

不允许多开：重复启动 `opensteamtool-manager.exe` 时不再出现第二个窗口，而是让已经
存在的窗口恢复显示并回到前台。

## 为什么做

- 便携单二进制工具，窗口 + 托盘 + 后台监控（Steam 进程轮询、compat 体检、更新检查）
  的架构与「配置文件 exe 旁、整目录拷贝即迁移」（ADR-0012）都隐含单实例前提：
  两个实例同时写 `config.toml` / `dlls/` / `cache/verified.toml` 会互相覆盖，部署
  动作并发执行也没有意义。多开是用户很容易遇到的操作（重复双击、开始菜单 + 快捷方式、
  任务栏点两次），应当直接唤起既有窗口而不是开第二个。
- 「重复打开 → 打开已经存在的界面」是 Windows 桌面软件的普遍预期（浏览器、VS Code
  等均如此）；本工具已有托盘隐藏机制（自动隐身 / 最小化隐身，ADR-0001），第二实例
  必须能把托盘隐藏中的窗口恢复显示——只拒绝不唤起会让用户以为软件没反应。

## 决策

- **命名互斥体裁决首实例**（`CreateMutexW`，`Local\OpenSteamToolManager.Singleton`）：
  `GetLastError() == ERROR_ALREADY_EXISTS` = 已有实例，重复启动方直接退出（退出码 0）。
  `Local\` 会话级作用域：同一登录会话内唯一，多会话（远程桌面/多用户）并存互不干扰。
- **命名事件唤醒首实例**（`CreateEventW`，auto-reset，
  `Local\OpenSteamToolManager.Activate`）：重复启动方置位后退出；首实例持有一行后台
  线程 `WaitForSingleObject(INFINITE)` 阻塞等待，每次置位向 UI 发 `Msg::ActivateRequested`，
  UI 走既有 `set_window_visible(true)` 路径——与托盘「显示」同一机制，前台唤起由
  winit 的 `focus_window`（Alt 键前台夺取）保证，不受 Windows 前台锁限制。
- **已可见不再补聚焦**：`write_window_visible` 仅在窗口由隐藏转为可见时置
  `pending_focus`，窗口已可见时不发 `ViewportCommand::Focus`。winit 的
  `force_window_active` 用「模拟 Alt 键」绕过前台锁（`SendInput` 注入 Alt +
  `SetForegroundWindow`），注入的 Alt 会命中当时前台窗口的菜单栏/命令栏首项——
  实测在资源管理器窗口上表现为「新建」键被选中（仅选中、不触发点击）；重复双击
  图标等唤起场景下前台恰被资源管理器先抢占，副作用不可避免且 `SetForegroundWindow`
  本身并不可靠。托盘隐藏→恢复仍走「恢复显示 + 聚焦」（`SW_SHOW` 自行激活窗口，
  后续 Focus 因已在前台不再注入 Alt），行为不变。
- **启动竞态**：首实例建互斥体后、建事件前的窗口期极小；重复启动方此时会先建出事件，
  置位会因随后无其他句柄而随对象销毁丢失。处理：重复启动方循环重试（≤50 次 × 5ms），
  直到观察到事件已由首实例持有（`ERROR_ALREADY_EXISTS`）再置位；超时放弃（首实例
  已退出的极端情形，用户再点一次即正常启动）。
- **守卫生命周期**：`Singleton` 守卫随唤醒线程移动、线程循环至进程退出——中途 drop
  会 `CloseHandle` 互斥体，多开防护随即失效（第二次启动会误判为无实例）。
- **降级**：互斥体/事件创建失败（系统资源耗尽）不 panic——无守卫可用的极端情形下
  应用照常运行（多开防护失效但可用性优先，与 config 降级惯例一致，ADR-0012）。
- **实现**：`src/singleton.rs` 新模块，复用既有 `windows-sys` 依赖（新增
  `Win32_Foundation` / `Win32_Security` / `Win32_System_Threading` 特性），零新依赖。

## 否决的方案

- **`single-instance` crate**：只提供「是否唯一」检测，无唤醒机制（置位方与等待方
  无 IPC 通道），仍要自写命名事件；且跨平台抽象在本项目（Windows-only，CI 与运行时
  均 Windows）上无收益。零新依赖直接落在既有 `windows-sys` 上更贴合。
- **第二实例直接 `FindWindowW` + `SetForegroundWindow`**：需要知道窗口标题（本地化）
  且对托盘隐藏窗口的恢复要额外处理；首实例自唤起（winit 前台夺取）更可靠、复用
  既有托盘路径，无需在第二实例维护窗口查找逻辑。
- **`Global\` 作用域**：全局互斥会跨登录会话锁死（A 会话开着，B 会话无法启动）。
  便携工具按会话独立窗口是正确语义，`Local\` 是刻意选择。

## 验收

- 首实例运行中再次启动 exe：第二进程立即退出（退出码 0），无第二个窗口/托盘图标；
  托盘隐藏状态恢复显示并聚焦（`SW_SHOW` 激活路径）；窗口已可见时不再补聚焦，
  且不注入任何击键（低层键盘钩子断言：修复前后对比实测，可见未聚焦/最小化两场景
  在修复前均会注入 Alt，修复后全绿，回归脚本保留在 `target/debug/alt-focus-loop9.ps1`）。
- 首实例退出后可正常再次启动；崩溃（进程被杀）后不残留锁（互斥体句柄随进程关闭，
  对象随之销毁），无需清理。
- 多会话（远程桌面另开会话）各自可启动独立实例。
- `cargo test --workspace` / `cargo check --workspace --all-targets` /
  `cargo clippy --workspace --all-targets` / `cargo fmt --all --check` 全绿；
  单测覆盖：重复 `acquire` 判已有实例、释放后可重取、置位唤醒等待者、auto-reset
  复位。双实例启动已实测：第二实例退出、首实例存活；「托盘隐藏 → 唤醒带回前台」
  依赖与托盘「显示」同一 `set_window_visible(true)` 路径（生产已验证）加信号机制
  单测，未单独人工验证，待手动验证项。
