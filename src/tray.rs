//! Windows 系统托盘：图标、菜单与事件。
//! 事件经 `set_event_handler` 回调转发（与全局 receiver 互斥，替代轮询；#51）。

use std::sync::Arc;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
pub enum TrayAction {
    ToggleVisible,
    Show,
    Quit,
    ToggleMinimizeToTray,
    /// 菜单「重启 Steam」（Steam 未运行时也可点击：等价「直接启动」，见 `set_restart_enabled`）。
    RestartSteam,
    OpenLuaFolder,
}

pub struct Tray {
    _icon: TrayIcon,
    _menu: Menu,
    minimize_item: CheckMenuItem,
    restart_item: MenuItem,
    open_lua_folder_item: MenuItem,
}

impl Tray {
    /// 创建托盘并注册事件回调（`set_event_handler`，与全局 receiver 互斥）。
    /// `sink` 在托盘事件线程被调用（非 UI 线程），须 `Send + Sync`；收到已映射的
    /// `TrayAction` 后由调用方负责转发进消息通道并唤醒 UI 线程。
    /// 创建失败（托盘不可用）返回 `None`，此时不注册回调。
    /// 9 个参数均为互不相同的单一值，无自然归并（clippy::too_many_arguments）。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        icon: Option<Icon>,
        tooltip: &str,
        show_label: &str,
        quit_label: &str,
        minimize_label: &str,
        minimize_checked: bool,
        restart_label: &str,
        open_lua_folder_label: &str,
        sink: impl Fn(TrayAction) + Send + Sync + 'static,
    ) -> Option<Self> {
        let show_item = MenuItem::new(show_label, true, None);
        let quit_item = MenuItem::new(quit_label, true, None);
        let minimize_item = CheckMenuItem::new(minimize_label, true, minimize_checked, None);
        let restart_item = MenuItem::new(restart_label, true, None);
        let open_lua_folder_item = MenuItem::new(open_lua_folder_label, true, None);
        let menu = Menu::new();
        menu.append(&show_item).ok()?;
        menu.append(&minimize_item).ok()?;
        menu.append(&restart_item).ok()?;
        menu.append(&open_lua_folder_item).ok()?;
        menu.append(&quit_item).ok()?;

        let tray = TrayIconBuilder::new()
            .with_icon(icon?)
            .with_menu(Box::new(menu.clone()))
            .with_tooltip(tooltip)
            .with_menu_on_left_click(false) // 左键不弹菜单，自己处理 toggle
            .build()
            .ok()?;

        // 与全局 receiver 互斥：注册后 receiver 不再收到事件。
        let sink = Arc::new(sink);
        let icon_sink = sink.clone();
        TrayIconEvent::set_event_handler(Some(move |event| match event {
            TrayIconEvent::Click {
                button,
                button_state,
                ..
            } if button == tray_icon::MouseButton::Left
                && button_state == tray_icon::MouseButtonState::Up =>
            {
                icon_sink(TrayAction::ToggleVisible);
            }
            TrayIconEvent::DoubleClick { .. } => icon_sink(TrayAction::ToggleVisible),
            _ => {}
        }));
        let show_id = show_item.id().clone();
        let minimize_id = minimize_item.id().clone();
        let restart_id = restart_item.id().clone();
        let open_lua_folder_id = open_lua_folder_item.id().clone();
        let quit_id = quit_item.id().clone();
        let menu_sink = sink.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == show_id {
                menu_sink(TrayAction::Show);
            } else if event.id == minimize_id {
                menu_sink(TrayAction::ToggleMinimizeToTray);
            } else if event.id == restart_id {
                menu_sink(TrayAction::RestartSteam);
            } else if event.id == open_lua_folder_id {
                menu_sink(TrayAction::OpenLuaFolder);
            } else if event.id == quit_id {
                menu_sink(TrayAction::Quit);
            }
        }));

        Some(Self {
            _icon: tray,
            _menu: menu,
            minimize_item,
            restart_item,
            open_lua_folder_item,
        })
    }

    /// 按 Steam 路径有效性置灰/启用「重启 Steam」菜单项（路径无效置灰）。
    /// Steam 未运行时点击无影响于重启语义——关闭步骤对未运行组是 no-op，等价「直接启动」。
    pub fn set_restart_enabled(&self, enabled: bool) {
        self.restart_item.set_enabled(enabled);
    }

    pub fn set_open_lua_folder_enabled(&self, enabled: bool) {
        self.open_lua_folder_item.set_enabled(enabled);
    }

    /// 更换托盘图标（深/浅主题双态，ADR-0016）：主题切换时由 App 调用。
    pub fn set_icon(&self, icon: Option<Icon>) -> Result<(), tray_icon::Error> {
        self._icon.set_icon(icon)
    }

    pub fn is_minimize_to_tray(&self) -> bool {
        self.minimize_item.is_checked()
    }

    pub fn set_minimize_to_tray(&self, checked: bool) {
        self.minimize_item.set_checked(checked);
    }
}
