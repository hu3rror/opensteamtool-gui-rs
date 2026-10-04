//! Windows 系统托盘：图标、菜单与事件。

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
pub enum TrayAction {
    ToggleVisible,
    Show,
    Quit,
    ToggleMinimizeToTray,
    /// 菜单「重启 Steam」（Steam 未运行时也可点击：等价「直接启动」，见 `set_restart_enabled`）。
    RestartSteam,
}

pub struct Tray {
    _icon: TrayIcon,
    _menu: Menu,
    show_item: MenuItem,
    quit_item: MenuItem,
    minimize_item: CheckMenuItem,
    restart_item: MenuItem,
}

impl Tray {
    pub fn new(
        icon: Option<Icon>,
        tooltip: &str,
        show_label: &str,
        quit_label: &str,
        minimize_label: &str,
        minimize_checked: bool,
        restart_label: &str,
    ) -> Option<Self> {
        let show_item = MenuItem::new(show_label, true, None);
        let quit_item = MenuItem::new(quit_label, true, None);
        let minimize_item = CheckMenuItem::new(minimize_label, true, minimize_checked, None);
        let restart_item = MenuItem::new(restart_label, true, None);
        let menu = Menu::new();
        menu.append(&show_item).ok()?;
        menu.append(&minimize_item).ok()?;
        menu.append(&restart_item).ok()?;
        menu.append(&quit_item).ok()?;

        let tray = TrayIconBuilder::new()
            .with_icon(icon?)
            .with_menu(Box::new(menu.clone()))
            .with_tooltip(tooltip)
            .with_menu_on_left_click(false) // 左键不弹菜单，自己处理 toggle
            .build()
            .ok()?;

        Some(Self {
            _icon: tray,
            _menu: menu,
            show_item,
            quit_item,
            minimize_item,
            restart_item,
        })
    }

    /// 按 Steam 路径有效性置灰/启用「重启 Steam」菜单项（路径无效置灰）。
    /// Steam 未运行时点击无影响于重启语义——关闭步骤对未运行组是 no-op，等价「直接启动」。
    pub fn set_restart_enabled(&self, enabled: bool) {
        self.restart_item.set_enabled(enabled);
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

    pub fn poll(&self) -> Option<TrayAction> {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click {
                    button,
                    button_state,
                    ..
                } if button == tray_icon::MouseButton::Left
                    && button_state == tray_icon::MouseButtonState::Up =>
                {
                    return Some(TrayAction::ToggleVisible);
                }
                TrayIconEvent::DoubleClick { .. } => return Some(TrayAction::ToggleVisible),
                _ => {}
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.show_item.id() {
                return Some(TrayAction::Show);
            }
            if event.id == self.minimize_item.id() {
                return Some(TrayAction::ToggleMinimizeToTray);
            }
            if event.id == self.restart_item.id() {
                return Some(TrayAction::RestartSteam);
            }
            if event.id == self.quit_item.id() {
                return Some(TrayAction::Quit);
            }
        }
        None
    }
}
