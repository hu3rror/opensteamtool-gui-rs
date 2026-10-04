//! OpenSteamTool Manager — egui/glow 原生 GUI 入口。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod brand;
mod busy;
mod compat;
mod compat_flow;
mod config;
mod dll;
mod fsutil;
mod i18n;
mod main_page;
mod process;
mod singleton;
mod steam;
mod steam_state;
mod theme;
mod tray;
mod ui;
mod update_flow;
mod updater;
mod wizard;
mod workflow;

fn main() -> eframe::Result {
    use eframe::egui; // ViewportBuilder / ViewportCommand 类型引用。

    // 初始窗口图标按持久化主题选版（深浅双态，ADR-0016）：ViewportBuilder 图标是创建时
    // 同步设置的（运行中 ViewportCommand::Icon 在窗口显示前不生效，实测被吞），必须在创建前
    // 确定主题。System 模式此时无法解析系统深浅，先用深色兑底（与 egui fallback 同值），
    // App 首帧再按实际主题校正。
    fn initial_window_icon() -> egui::IconData {
        use crate::config::{self, ThemePreference};
        let dark = match config::load(&config::config_path()) {
            Ok(cfg) => match cfg.theme {
                ThemePreference::Dark => true,
                ThemePreference::Light => false,
                ThemePreference::System => true,
            },
            Err(_) => true,
        };
        brand::window_icon(dark)
    }

    let Some(guard) = singleton::acquire() else {
        // 不允许多开：唤醒既有窗口后本实例直接退出。
        singleton::signal_activate();
        return Ok(());
    };
    // 首帧参考 = 最小内尺寸（spec §40 签核：默认窗口贴着最小尺寸启动，只涨不缩）。
    let min_size = [620.0, 520.0];
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(min_size)
            .with_min_inner_size(min_size)
            .with_resizable(true)
            .with_icon(initial_window_icon()),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "OpenSteamTool Manager",
        options,
        Box::new(|cc| Ok(Box::new(ui::App::new(cc, guard)))),
    )
}
