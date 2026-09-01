# Steam 进程联动托盘（Auto-tray）

窗口在检测到 Steam 进程运行时自动隐藏到 Windows 托盘，Steam 退出后自动恢复并聚焦。决定：任何来源的 `steam.exe` 运行/退出都触发（不只是本工具按钮启动的）；托盘左键单击/双击均切换窗口显隐；托盘右键菜单含「显示」「退出」；窗口关闭（X）直接退出程序（托盘仅服务自动隐身与恢复，不做常驻）。

实现：`tray-icon` crate 提供托盘图标与菜单，事件通过全局 receiver 在 `App::ui` 中轮询；Steam 状态沿用 `process.rs` 现有 2s 轮询做边沿检测，用 `ViewportCommand::Visible` 控制显隐、`Focus` 恢复焦点。eframe 0.36 在 Windows 上即使窗口不可见也每 100ms 驱动一次 `App::ui`（`INVISIBLE_WINDOW_REPAINT_INTERVAL`），故隐藏期间联动逻辑不中断。
