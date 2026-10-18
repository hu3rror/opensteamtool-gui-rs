# Steam 进程联动托盘（Auto-tray）

窗口在检测到 Steam 进程运行时自动隐藏到 Windows 托盘，Steam 退出后自动恢复并聚焦。决定：任何来源的 `steam.exe` 运行/退出都触发（不只是本工具按钮启动的）；托盘左键单击/双击均切换窗口显隐；托盘右键菜单含「显示」「最小化时自动隐藏到托盘」（勾选项，默认开）「退出」；窗口关闭（X）直接退出程序（托盘不做常驻，仅服务自动隐身、最小化隐身与手动显隐切换）。

实现：`tray-icon` crate 提供托盘图标与菜单，事件经 `set_event_handler` 回调转发（与全局 receiver 互斥，替代轮询，#51），回调经消息通道 + `ctx.request_repaint()` 唤醒 UI 线程；Steam 状态由后台线程 2s 轮询做边沿检测，仅状态变化上报并唤醒（#50），用 `ViewportCommand::Visible` 控制显隐、`Focus` 恢复焦点。注意 eframe 0.36：窗口最小化或隐藏时不调用 `App::ui`，只调用 `App::logic`（`run_ui_and_paint` 的 `!show_ui` 分支），故托盘/Steam 联动与最小化检测必须放在 `App::logic`。隐藏期间不发周期 repaint（唯一例外：60s 安全网心跳，事件路径全失败时的兜底；心跳之间事件循环睡眠零渲染）：eframe 0.36 内置 #7905，无 repaint 请求即 `ControlFlow::Wait` 睡眠；而 egui 的 `viewport.info.visible` 对应用侧隐藏无回写路径（eframe 只有 `Occluded -> info.occluded`），物理隐藏后 eframe 仍判定 `show_ui=true` 照常渲染，故任何周期 repaint 都会让隐藏窗口持续空转（#52）。
