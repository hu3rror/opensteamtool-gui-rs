//! OpenSteamTool Manager — egui/glow 原生 GUI 入口。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

use eframe::egui;

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../app.ico");
    match image::load_from_memory_with_format(bytes, image::ImageFormat::Ico) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            }
        }
        Err(_) => egui::IconData::default(),
    }
}

fn main() -> eframe::Result {
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
            .with_icon(load_icon()),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "OpenSteamTool Manager",
        options,
        Box::new(|cc| Ok(Box::new(ui::App::new(cc, guard)))),
    )
}
