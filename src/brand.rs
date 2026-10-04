//! 品牌资源统一入口（ADR-0016 图标双态）：深/浅主题各一版 LOGO，
//! 窗口标题栏 + 任务栏按钮 + 托盘 + 主页面左上角四处共用同一份深/浅映射，
//! 主题切换即换版（与 `load_github_mark` 的图片双态同机制，见 GLOSSARY「应用图标」）。
//!
//! 两个 PNG 是同一徽标的深浅两版（alpha 掩码同构）；`assets/logo.png` = 深色版
//! （深蓝底 + 橙红环 + 浅蓝 OST），`assets/logo-light.png` = 浅色版（浅底 + 深色元素）。

use eframe::egui;

const LOGO_DARK_PNG: &[u8] = include_bytes!("../assets/logo.png");
const LOGO_LIGHT_PNG: &[u8] = include_bytes!("../assets/logo-light.png");

/// 深/浅主题 → 对应 LOGO 的 PNG 字节（暗色兜底）。
fn logo_png(dark_mode: bool) -> &'static [u8] {
    if dark_mode {
        LOGO_DARK_PNG
    } else {
        LOGO_LIGHT_PNG
    }
}

/// 窗口标题栏 + 任务栏按钮图标（egui `ViewportBuilder::with_icon` / `ViewportCommand::Icon`）。
/// 解码失败兜底空 IconData（不改崩溃语义），与旧 `main.rs::load_icon` 行为一致。
pub fn window_icon(dark_mode: bool) -> egui::IconData {
    let Ok(img) = image::load_from_memory(logo_png(dark_mode)) else {
        return egui::IconData::default();
    };
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    }
}

/// 托盘图标（32×32，tray-icon RGBA）。
pub fn tray_icon(dark_mode: bool) -> Option<tray_icon::Icon> {
    let img = image::load_from_memory(logo_png(dark_mode)).ok()?;
    let img = img.resize_to_fill(32, 32, image::imageops::FilterType::Lanczos3);
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    tray_icon::Icon::from_rgba(rgba.into_raw(), w, h).ok()
}

/// 主页面左上角 LOGO 纹理（按当前主题选版；模式变化时由调用方重载）。
pub fn logo_texture(ctx: &egui::Context, dark_mode: bool) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(logo_png(dark_mode))
        .ok()?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img);
    let options = egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear));
    Some(ctx.load_texture("app-logo", color, options))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_icon_picks_theme_variant() {
        // 深/浅两版同一徽标（同尺寸），但像素内容不同——深浅映射不得错位（ADR-0016）。
        let dark = window_icon(true);
        let light = window_icon(false);
        assert_eq!((dark.width, dark.height), (256, 256));
        assert_eq!((light.width, light.height), (256, 256));
        assert_ne!(dark.rgba, light.rgba, "深浅两版图标像素应有差异");
    }
}
