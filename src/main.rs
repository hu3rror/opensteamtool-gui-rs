//! OpenSteamTool Manager — egui/glow 原生 GUI 入口。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dll;
mod i18n;
mod process;
mod steam;
mod updater;
mod ui;

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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 470.0])
            .with_resizable(false)
            .with_icon(load_icon()),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "OpenSteamTool Manager",
        options,
        Box::new(|cc| Ok(Box::new(ui::App::new(cc)))),
    )
}
