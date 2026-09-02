# Steam 进程联动托盘（Auto-tray）

窗口在检测到 Steam 进程运行时自动隐藏到 Windows 托盘，Steam 退出后自动恢复并聚焦。决定：任何来源的 `steam.exe` 运行/退出都触发（不只是本工具按钮启动的）；托盘左键单击/双击均切换窗口显隐；托盘右键菜单含「显示」「退出」；窗口关闭（X）直接退出程序（托盘仅服务自动隐身与恢复，不做常驻）。

实现：`tray-icon` crate 提供托盘图标与菜单，事件通过全局 receiver 在 `App::logic` 中轮询；Steam 状态沿用 `process.rs` 现有 2s 轮询做边沿检测，用 `ViewportCommand::Visible` 控制显隐、`Focus` 恢复焦点。注意 eframe 0.36：窗口最小化或隐藏时不调用 `App::ui`，只调用 `App::logic`（`run_ui_and_paint` 的 `!show_ui` 分支），故托盘/Steam 联动与最小化检测必须放在 `App::logic`，隐藏期间其按 `INVISIBLE_WINDOW_REPAINT_INTERVAL`（100ms）被驱动。
