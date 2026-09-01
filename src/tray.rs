//! Windows 系统托盘：图标、菜单与事件（左键单击/双击切换显隐，右键菜单显示/退出）。

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// 托盘动作，UI 线程在 `App::ui` 中消费。
pub enum TrayAction {
    /// 左键单击/双击：切换窗口显隐。
    ToggleVisible,
    /// 菜单「显示」。
    Show,
    /// 菜单「退出」。
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    _menu: Menu,
    show_item: MenuItem,
    quit_item: MenuItem,
}

impl Tray {
    pub fn new(
        icon: Option<Icon>,
        tooltip: &str,
        show_label: &str,
        quit_label: &str,
    ) -> Option<Self> {
        let show_item = MenuItem::new(show_label, true, None);
        let quit_item = MenuItem::new(quit_label, true, None);
        let menu = Menu::new();
        menu.append(&show_item).ok()?;
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
        })
    }

    /// 拉取一个待处理的托盘动作（非阻塞）。无事件返回 None。
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
            if event.id == self.quit_item.id() {
                return Some(TrayAction::Quit);
            }
        }
        None
    }
}

/// 从 `app.ico` 加载托盘图标（缩放到 32×32，Windows 托盘标准尺寸）。
pub fn load_icon() -> Option<Icon> {
    let bytes = include_bytes!("../app.ico");
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico).ok()?;
    let img = img.resize_to_fill(32, 32, image::imageops::FilterType::Lanczos3);
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Icon::from_rgba(rgba.into_raw(), w, h).ok()
}
