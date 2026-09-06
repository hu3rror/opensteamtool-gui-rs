//! egui 界面：顶栏 + 3 卡片 + 确认弹窗。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::egui;
use egui::Frame;

use crate::compat;
use crate::config_editor;
use crate::external;
use crate::onlinefix;
use crate::dll::{self, DeployStatus};
use crate::paths;
use crate::gui_config::GuiConfig;
use crate::i18n::{Lang, LanguagePreference, Strings};
use crate::process::{self, SteamEvent, SteamMonitor};
use crate::settings::{ConfigEditError, ConfigEditorState, OfError, OfStatus, OnlineFixState};
use crate::steam;
use crate::steam_state::SteamState;
use crate::tray::{Tray, TrayAction};
use crate::updater::{self, OnlineInfo, UpdateError};
use crate::workflow::{self, Action, BusyKind};

// ---------- 效仿 Python 版外观（opensteamtool-gui-py THEME 色板） ----------
// 蓝 accent（#0f6cbd）+ 中性灰底 + hairline 卡片；无渐变/毛玻璃/发光点。
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x0F, 0x6C, 0xBD); // accent_bar / btn_primary_bg
const ACCENT_ACTIVE: egui::Color32 = egui::Color32::from_rgb(0x11, 0x5E, 0xA3); // btn_primary_hover
const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(0xF8, 0xF9, 0xFA); // bg_app
const CARD_BG: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF); // card_bg
const BORDER: egui::Color32 = egui::Color32::from_rgb(0xE2, 0xE8, 0xF0); // card_border
const ENTRY_BORDER: egui::Color32 = egui::Color32::from_rgb(0xCB, 0xD5, 0xE1); // entry_border
const FILL_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0xF8, 0xFA, 0xFC); // entry_bg / btn_secondary_bg
const TEXT_INK: egui::Color32 = egui::Color32::from_rgb(0x0F, 0x17, 0x2A); // text_main
const TEXT_SUB: egui::Color32 = egui::Color32::from_rgb(0x33, 0x41, 0x55); // text_sub
const TEXT_WEAK: egui::Color32 = egui::Color32::from_rgb(0x64, 0x74, 0x8B); // text_muted
const STATUS_INSTALLED: egui::Color32 = egui::Color32::from_rgb(0x15, 0x80, 0x3D); // status_installed 绿
const BTN_DEPLOY_BG: egui::Color32 = egui::Color32::from_rgb(0x16, 0xA3, 0x4A); // btn_deploy_b_bg 绿
const BTN_DEPLOY_HOVER: egui::Color32 = egui::Color32::from_rgb(0x15, 0x80, 0x3D);
const BTN_SECONDARY_HOVER: egui::Color32 = egui::Color32::from_rgb(0xE2, 0xE8, 0xF0);
const BTN_UNINSTALL_A_BG: egui::Color32 = egui::Color32::from_rgb(0xF0, 0xF9, 0xFF); // 退出并卸载（浅蓝描边）
const BTN_UNINSTALL_A_FG: egui::Color32 = egui::Color32::from_rgb(0x02, 0x84, 0xC7);
const BTN_UNINSTALL_A_BORDER: egui::Color32 = egui::Color32::from_rgb(0x7D, 0xD3, 0xFC);
const BTN_UNINSTALL_A_HOVER: egui::Color32 = egui::Color32::from_rgb(0xE0, 0xF2, 0xFE);
const BTN_UNINSTALL_B_BG: egui::Color32 = egui::Color32::from_rgb(0x02, 0x84, 0xC7); // 卸载并重启（蓝）
const BTN_UNINSTALL_B_HOVER: egui::Color32 = egui::Color32::from_rgb(0x03, 0x69, 0xA1);
const DOT_RUNNING: egui::Color32 = STATUS_INSTALLED; // 成功/进行中圆点
const ERR_RED: egui::Color32 = egui::Color32::from_rgb(0xDC, 0x26, 0x26); // 错误红
const STATUS_WARN: egui::Color32 = egui::Color32::from_rgb(0xB4, 0x53, 0x09); // 琥珀（上游已适配未缓存）
// 状态徽章浅底（pill badge 背景，深色文字配浅色底，效仿 #E6F7ED 一类）。
const BADGE_GREEN: egui::Color32 = egui::Color32::from_rgb(0xE6, 0xF7, 0xED);
const BADGE_AMBER: egui::Color32 = egui::Color32::from_rgb(0xFE, 0xF3, 0xC7);
const BADGE_RED: egui::Color32 = egui::Color32::from_rgb(0xFE, 0xE2, 0xE2);
const BADGE_GRAY: egui::Color32 = egui::Color32::from_rgb(0xF1, 0xF5, 0xF9);

/// 设置弹窗 Footer 预留高度（分隔线 + 按钮行 + 间距 + 边距）。
/// 滚动区高度 = 当前可用高度 − 此预留，随窗口缩放自适应（SPEC AC5：根除固定魔法常数）。
const FOOTER_RESERVE: f32 = 56.0;
fn install_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = PANEL_BG;
    visuals.window_fill = CARD_BG;
    visuals.faint_bg_color = FILL_SECONDARY;
    visuals.extreme_bg_color = FILL_SECONDARY; // TextEdit 底色（Python entry_bg #f8fafc）
    visuals.override_text_color = Some(TEXT_INK);
    let radius = egui::CornerRadius::same(8);
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        w.corner_radius = radius;
    }
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, ENTRY_BORDER);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, ENTRY_BORDER);
    visuals.widgets.inactive.bg_fill = CARD_BG;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.bg_fill = FILL_SECONDARY;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT_ACTIVE);
    visuals.selection.bg_fill = egui::Color32::from_rgb(0xD0, 0xE2, 0xFF);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.window_margin = egui::Margin::symmetric(20, 18);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
    });
}

/// 卡片容器：白底 + hairline 边框 + 小圆角。
fn card_frame() -> Frame {
    Frame::new()
        .fill(CARD_BG)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(18, 16))
}

/// Python 风格按钮的样式参数（底色 / hover 色 / 文字色 / 描边）。
struct PyStyle {
    bg: egui::Color32,
    hover: egui::Color32,
    fg: egui::Color32,
    border: Option<egui::Color32>,
}

/// 按钮样式（语义名，对齐操作语义；样式常量表集中在此，新增样式只加变体）。
#[derive(Clone, Copy)]
enum ButtonStyle {
    /// 绿色主按钮（应用补丁并启动 Steam）。
    Deploy,
    /// 白色次按钮（正常启动 Steam）。
    Launch,
    /// 浅蓝描边按钮（退出 Steam 并卸载补丁）。
    UninstallExit,
    /// 蓝色实心按钮（卸载补丁并重启 Steam）。
    UninstallRestart,
    /// 主蓝按钮（保存/下载/确认）。
    Primary,
    /// 次灰按钮（检查更新/浏览/取消）。
    Secondary,
    /// 顶栏入口按钮（GitHub 外链 / 设置 ⚙，白底蓝字，固定小尺寸）。
    Lang,
}

impl ButtonStyle {
    fn style(self) -> PyStyle {
        match self {
            ButtonStyle::Deploy => PyStyle {
                bg: BTN_DEPLOY_BG,
                hover: BTN_DEPLOY_HOVER,
                fg: egui::Color32::WHITE,
                border: None,
            },
            ButtonStyle::Launch => PyStyle {
                bg: CARD_BG,
                hover: BTN_SECONDARY_HOVER,
                fg: TEXT_SUB,
                border: Some(ENTRY_BORDER),
            },
            ButtonStyle::UninstallExit => PyStyle {
                bg: BTN_UNINSTALL_A_BG,
                hover: BTN_UNINSTALL_A_HOVER,
                fg: BTN_UNINSTALL_A_FG,
                border: Some(BTN_UNINSTALL_A_BORDER),
            },
            ButtonStyle::UninstallRestart => PyStyle {
                bg: BTN_UNINSTALL_B_BG,
                hover: BTN_UNINSTALL_B_HOVER,
                fg: egui::Color32::WHITE,
                border: None,
            },
            ButtonStyle::Primary => PyStyle {
                bg: ACCENT,
                hover: ACCENT_ACTIVE,
                fg: egui::Color32::WHITE,
                border: None,
            },
            ButtonStyle::Secondary => PyStyle {
                bg: FILL_SECONDARY,
                hover: BTN_SECONDARY_HOVER,
                fg: TEXT_SUB,
                border: Some(ENTRY_BORDER),
            },
            ButtonStyle::Lang => PyStyle {
                bg: CARD_BG,
                hover: FILL_SECONDARY,
                fg: ACCENT,
                border: Some(ENTRY_BORDER),
            },
        }
    }
}

/// Python 风格按钮：手动绘制底/描边/文字，hover 换色（效仿 tkinter <Enter>/<Leave>）。
/// `enabled=false` 时文字弱化为 muted 且不响应点击。
fn styled_button(
    ui: &mut egui::Ui,
    text: &str,
    style: ButtonStyle,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    let style = style.style();
    // 长文案（如英文 "Download & Extract New Version"）超出固定宽度时会被绘制在
    // 按钮边界外截断；按文本宽度自适应，最小仍为调用方指定的 size。
    let font_id = egui::FontId::proportional(13.0);
    let text_w = ui.painter().layout_no_wrap(text.to_owned(), font_id, style.fg).size().x;
    let size = egui::vec2(size.x.max(text_w + 28.0), size.y);
    paint_py_button(ui, text, style, size, enabled)
}

/// 固定尺寸版（不随文本宽度自适应）：T4 设置按钮 ⚙ 恒为 28×28 正方形。
fn styled_fixed_button(
    ui: &mut egui::Ui,
    text: &str,
    style: ButtonStyle,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    paint_py_button(ui, text, style.style(), size, enabled)
}

/// Python 风格按钮绘制核心：按给定尺寸画底/描边/文字（无自适应撑宽）。
fn paint_py_button(
    ui: &mut egui::Ui,
    text: &str,
    style: PyStyle,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let fill = if enabled && response.hovered() {
            style.hover
        } else {
            style.bg
        };
        let stroke = style
            .border
            .map_or(egui::Stroke::NONE, |c| egui::Stroke::new(1.0, c));
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(8),
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        let color = if enabled { style.fg } else { TEXT_WEAK };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(13.0),
            color,
        );
    }
    response
}

/// 顶栏右侧按钮组（T4 AC1）：`[ GitHub ]` 外链 + `[ ⚙ ]` 设置（28×28 固定方形，无布局抖动）。
/// 语言切换按钮已移除（入口在常规偏好页，SPEC §8.4）。
/// RTL 布局：设置位于最右；返回 (GitHub, 设置) 两个按钮响应（测试据此定位点击坐标）。
fn top_bar_buttons(ui: &mut egui::Ui, strings: &Strings) -> (egui::Response, egui::Response) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let settings = styled_fixed_button(
            ui,
            strings.btn_settings,
            ButtonStyle::Lang,
            egui::vec2(28.0, 28.0),
            true,
        );
        ui.add_space(8.0);
        let github = styled_button(
            ui,
            strings.btn_github,
            ButtonStyle::Lang,
            egui::vec2(28.0, 28.0),
            true,
        );
        (github, settings)
    })
    .inner
}

/// 顶栏（T4 AC1）：标题 + 右侧按钮组。GitHub 点击经分派器调起仓库 URL；
/// 设置点击置位 `open_settings`（由调用方执行打开逻辑）。
/// 返回两个按钮响应（测试据此定位点击坐标）。
fn render_top_bar(
    ui: &mut egui::Ui,
    strings: &Strings,
    dispatcher: &dyn external::ExternalSystemDispatcher,
    open_settings: &mut bool,
) -> (egui::Response, egui::Response) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(strings.app_title)
                .size(16.0)
                .strong()
                .color(TEXT_INK),
        );
        let (github, settings) = top_bar_buttons(ui, strings);
        if github.clicked() {
            let _ = dispatcher.open_browser_url(external::GITHUB_REPO_URL);
        }
        let settings = settings.on_hover_text(strings.settings_title);
        if settings.clicked() {
            *open_settings = true;
        }
        (github, settings)
    })
    .inner
}

/// 通用 pill 徽章：浅色底 + 圆角 + 状态图标/文字（对齐「兼容性徽章」样式先例）。
fn pill_badge(ui: &mut egui::Ui, icon: &str, text: &str, fg: egui::Color32, bg: egui::Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(format!("{icon} {text}")).size(12.5).color(fg));
        });
}

/// 部署状态徽章样式 → (图标, 文案, 前景色, 底色)。
fn deploy_badge_style(
    strings: &Strings,
    status: DeployStatus,
) -> (&'static str, &'static str, egui::Color32, egui::Color32) {
    match status {
        DeployStatus::Deployed => ("✔", strings.status_deployed, STATUS_INSTALLED, BADGE_GREEN),
        DeployStatus::NotDeployed => ("●", strings.status_not_deployed, STATUS_WARN, BADGE_AMBER),
        DeployStatus::InvalidPath => ("?", strings.status_invalid, TEXT_WEAK, BADGE_GRAY),
    }
}

/// Steam 进程状态徽章样式（复用 2s 轮询的 steam_running）。
fn steam_badge_style(
    strings: &Strings,
    running: bool,
) -> (&'static str, &'static str, egui::Color32, egui::Color32) {
    if running {
        (
            "●",
            strings.status_steam_running,
            STATUS_INSTALLED,
            BADGE_GREEN,
        )
    } else {
        ("○", strings.status_steam_not_running, TEXT_WEAK, BADGE_GRAY)
    }
}

/// 已部署态卸载按钮文案：Steam 运行中 → 「退出 Steam 并卸载补丁」；已退出 → 直接「卸载补丁」。
fn uninstall_label(strings: &Strings, steam_running: bool) -> &'static str {
    if steam_running {
        strings.btn_exit_and_uninstall
    } else {
        strings.btn_uninstall
    }
}

/// 补丁控制台按钮点击意图（T5 AC2）。
#[derive(Default)]
struct PatchActions {
    apply_and_launch: bool,
    launch: bool,
    exit_and_uninstall: bool,
    uninstall_and_restart: bool,
}

/// 补丁控制台按钮行（T5 AC2）：未部署 → 应用补丁并启动 Steam / 正常启动 Steam；
/// 已部署 → 退出并卸载 / 卸载并重启（Steam 未运行时退出项改「卸载补丁」）；路径无效 → 全禁用。
/// 返回两个按钮响应（测试据此定位点击坐标）。
fn patch_console_buttons(
    ui: &mut egui::Ui,
    strings: &Strings,
    status: DeployStatus,
    steam_running: bool,
    enabled: bool,
    actions: &mut PatchActions,
) -> (egui::Response, egui::Response) {
    let gap = 12.0;
    let w = twin_button_width(ui.available_width(), gap, ui.spacing().item_spacing.x);
    let size = egui::vec2(w, 36.0);
    match status {
        DeployStatus::Deployed => {
            let first = styled_button(
                ui,
                uninstall_label(strings, steam_running),
                ButtonStyle::UninstallExit,
                size,
                enabled,
            );
            if first.clicked() {
                actions.exit_and_uninstall = true;
            }
            ui.add_space(gap);
            let second = styled_button(
                ui,
                strings.btn_uninstall_and_restart,
                ButtonStyle::UninstallRestart,
                size,
                enabled,
            );
            if second.clicked() {
                actions.uninstall_and_restart = true;
            }
            (first, second)
        }
        DeployStatus::NotDeployed => {
            let first = styled_button(
                ui,
                strings.btn_apply_and_launch,
                ButtonStyle::Deploy,
                size,
                enabled,
            );
            if first.clicked() {
                actions.apply_and_launch = true;
            }
            ui.add_space(gap);
            let second = styled_button(ui, strings.btn_launch_normal, ButtonStyle::Launch, size, enabled);
            if second.clicked() {
                actions.launch = true;
            }
            (first, second)
        }
        DeployStatus::InvalidPath => {
            let first = styled_button(ui, strings.btn_apply_and_launch, ButtonStyle::Deploy, size, false);
            ui.add_space(gap);
            let second = styled_button(ui, strings.btn_launch_normal, ButtonStyle::Launch, size, false);
            (first, second)
        }
    }
}

/// 底部状态栏：右侧（版本 + 按钮）预留宽度，左侧提示超长截断防溢出。
const STATUS_BAR_RIGHT_RESERVE: f32 = 340.0;
/// 底部状态栏：版本文本最大宽度（超长截断，防挤压按钮行）。
const STATUS_BAR_VERSION_MAX: f32 = 220.0;

/// 底部状态栏渲染输入（T6 AC3 视图模型，聚合渲染所需状态）。
struct StatusBarView<'a> {
    strings: &'a Strings,
    local_version: Option<&'a str>,
    all_local_exist: bool,
    update_state: &'a UpdateState,
    busy: bool,
    busy_kind: Option<BusyKind>,
    notice: Option<&'a Notice>,
}

/// 底部状态栏右侧按钮意图（T6 AC3）。
#[derive(Default)]
struct StatusBarActions {
    check: bool,
    download: Option<OnlineInfo>,
}

/// 本地版本展示文本（有版本 → v{ver}；已就绪未记录 / 缺失 DLL → 对应词条）。
fn local_version_text(
    strings: &Strings,
    local_version: Option<&str>,
    all_local_exist: bool,
) -> (String, egui::Color32) {
    match local_version {
        Some(v) => (
            format!("{}v{}", strings.local_version, v.trim_start_matches('v')),
            TEXT_INK,
        ),
        None if all_local_exist => (
            format!("{}{}", strings.local_version, strings.local_ver_ready_no_record),
            TEXT_SUB,
        ),
        None => (
            format!("{}{}", strings.local_version, strings.local_ver_missing),
            TEXT_WEAK,
        ),
    }
}

/// 底部状态栏右侧：本地版本 + 更新/检查按钮（T6 AC3）。
/// 可更新 → 强调色 [立即更新]；下载中 → 原地「正在下载...」禁用态；否则 → [检查更新]。
/// 返回 (版本 label, 按钮) 响应（测试据此定位点击坐标）。
fn status_bar_right(
    ui: &mut egui::Ui,
    view: &StatusBarView<'_>,
    actions: &mut StatusBarActions,
) -> (egui::Response, egui::Response) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let updatable = match view.update_state {
            UpdateState::Checked(Ok(info)) => {
                !info.version.is_empty() && Some(info.version.as_str()) != view.local_version
            }
            _ => false,
        };
        let downloading = view.busy && view.busy_kind == Some(BusyKind::Downloading);
        let btn = if updatable {
            let label = if downloading {
                view.strings.busy_downloading
            } else {
                view.strings.btn_update_now
            };
            styled_button(
                ui,
                label,
                ButtonStyle::Primary,
                egui::vec2(96.0, 26.0),
                !view.busy,
            )
        } else {
            styled_button(
                ui,
                view.strings.btn_check_update,
                ButtonStyle::Secondary,
                egui::vec2(96.0, 26.0),
                !view.busy,
            )
        };
        if btn.clicked() {
            match view.update_state {
                UpdateState::Checked(Ok(info)) if updatable => {
                    actions.download = Some(info.clone());
                }
                _ => actions.check = true,
            }
        }
        ui.add_space(8.0);
        let (text, color) =
            local_version_text(view.strings, view.local_version, view.all_local_exist);
        let text_w = ui
            .painter()
            .layout_no_wrap(text.clone(), egui::FontId::proportional(12.5), color)
            .size()
            .x;
        let version = ui.add_sized(
            egui::vec2(text_w.min(STATUS_BAR_VERSION_MAX), 18.0),
            egui::Label::new(egui::RichText::new(text).size(12.5).color(color)).truncate(),
        );
        (version, btn)
    })
    .inner
}

/// 底部状态栏左侧：busy 提示或最近结果（T6 AC3，超长截断防溢出）。
/// 返回渲染出的提示文本（None = 无提示）。
fn status_bar_notice(ui: &mut egui::Ui, view: &StatusBarView<'_>) -> Option<String> {
    let (text, color, dot_color) = if let Some(kind) = view.busy_kind {
        (view.strings.busy_label(kind).to_string(), TEXT_WEAK, ACCENT)
    } else if let Some((ok, text)) = view
        .notice
        .map(|n| render_notice(view.strings, view.local_version, n))
    {
        let (color, dot) = if ok {
            (TEXT_INK, DOT_RUNNING)
        } else {
            (ERR_RED, ERR_RED)
        };
        (text, color, dot)
    } else {
        return None;
    };
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 3.5, dot_color);
        ui.add_space(6.0);
        ui.add_sized(
            egui::vec2(
                (ui.available_width() - STATUS_BAR_RIGHT_RESERVE).max(60.0),
                16.0,
            ),
            egui::Label::new(egui::RichText::new(text.clone()).size(12.5).color(color)).truncate(),
        );
    });
    Some(text)
}

/// 卡片标题：3px 蓝色 accent bar + 标题（Python 版样式）。
fn card_title(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 13.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, ACCENT);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(text)
                .size(13.5)
                .strong()
                .color(TEXT_INK),
        );
    });
}

/// 状态行：纯文字 + 颜色（效仿 Python，无圆点徽章）。
fn status_line(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).size(13.0).strong().color(color));
}
/// 渲染最近一次结果提示为当前语言文案。
/// 纯函数（不依赖 App）：切换语言后无需重建 notice，重渲染即得新语言。
/// 检查更新成功时与本地版本比较：相同 → 「已是最新」，否则 → 「发现可更新版本」。
fn render_notice(s: &Strings, local_version: Option<&str>, notice: &Notice) -> (bool, String) {
    match notice {
        Notice::UpdateChecked(Ok(info)) => {
            let suffix = if local_version.unwrap_or("") == info.version {
                s.up_to_date
            } else {
                s.new_version
            };
            (true, format!("v{} {}", info.version, suffix))
        }
        Notice::UpdateChecked(Err(e)) => (false, s.update_error(e)),
        Notice::Downloaded(Ok(())) => (true, s.ok_downloaded.to_string()),
        Notice::Downloaded(Err(e)) => (false, s.update_error(e)),
        Notice::WorkflowDone(action, Ok(())) => (true, s.success_text(*action).to_string()),
        Notice::WorkflowDone(_, Err(e)) => (false, s.workflow_error_text(e)),
        Notice::Precheck(p) => (false, s.precheck_text(p)),
    }
}

/// 配置编辑器类型化错误 → 本地化文案（渲染闭包内调用，不借 App；语言切换后逐帧重新映射）。
fn config_err_text(strings: &Strings, lang: Lang, err: &ConfigEditError) -> String {
    match err {
        ConfigEditError::Load(m) => format!("{}: {m}", strings.err_config_load),
        ConfigEditError::Validation(e) => strings.config_error_text(lang, e),
        ConfigEditError::Save(m) => format!("{}: {m}", strings.err_config_save),
    }
}

/// OnlineFix 类型化错误 → 本地化文案。
fn of_error_text(strings: &Strings, e: &OfError) -> String {
    match e {
        OfError::WriteBlocked => strings.of_steam_running.to_string(),
        OfError::InvalidAppid => strings.err_of_invalid_appid.to_string(),
        OfError::Vdf(e) => strings.onlinefix_error(e),
    }
}

/// OnlineFix 展示状态 → (文案, 颜色)（状态模块存类型化错误，渲染时按当前语言映射）。
fn of_status_line(strings: &Strings, status: &OfStatus) -> (String, egui::Color32) {
    match status {
        OfStatus::Enabled => (strings.of_status_enabled.to_string(), STATUS_INSTALLED),
        OfStatus::Disabled => (strings.of_status_disabled.to_string(), TEXT_WEAK),
        OfStatus::Copied => (strings.of_copied.to_string(), STATUS_INSTALLED),
        OfStatus::Error(e) => (of_error_text(strings, e), ERR_RED),
    }
}

/// T7：生效游戏看板（SPEC AC6）——「当前生效游戏」标题 + AppID/名称 + `[ 停用 ]`。
/// 返回 (是否点击停用, 停用按钮响应)。看板停用与 Footer 停用同指一操作。
fn render_active_board(
    ui: &mut egui::Ui,
    strings: &Strings,
    active: &onlinefix::ActiveOnlineFixGame,
) -> (bool, egui::Response) {
    let mut deactivate = false;
    let mut btn = None;
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(strings.of_active_title).strong());
                ui.add_space(6.0);
                let label = match &active.name {
                    Some(name) => format!("{name} ({})", active.appid),
                    None => active.appid.to_string(),
                };
                ui.label(label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let resp = styled_button(
                        ui,
                        strings.of_deactivate,
                        ButtonStyle::Secondary,
                        egui::vec2(64.0, 24.0),
                        true,
                    );
                    if resp.clicked() {
                        deactivate = true;
                    }
                    btn = Some(resp);
                });
            });
        });
    (deactivate, btn.expect("停用按钮必渲染"))
}

/// T7：候选胶囊——「名称 (appid)」/ 纯数字回退（SPEC AC6）。
/// 返回 (点击的 AppID, 各胶囊按钮响应)。
fn render_candidate_capsules(
    ui: &mut egui::Ui,
    candidates: &[onlinefix::CandidateGame],
) -> (Option<u32>, Vec<egui::Response>) {
    let mut picked = None;
    let mut buttons = Vec::with_capacity(candidates.len());
    for c in candidates {
        let label = match &c.name {
            Some(name) => format!("{name} ({})", c.appid),
            None => c.appid.to_string(),
        };
        let resp = ui.small_button(label);
        if resp.clicked() {
            picked = Some(c.appid);
        }
        buttons.push(resp);
    }
    (picked, buttons)
}

/// 两枚等宽按钮并排时的单按钮宽度：`available` 减去手动 gap 与 egui 自动插入的
/// item_spacing 后再均分。公式漏掉 item_spacing 会导致按钮行实际占宽超过可用宽度，
/// 溢出并把下方依赖 `available_width` 撑满的卡片顶到窗口右缘（历史 bug）。
fn twin_button_width(available: f32, gap: f32, item_spacing: f32) -> f32 {
    ((available - gap - item_spacing) / 2.0).max(150.0)
}




/// 后台线程 → UI 线程的消息。
enum Msg {
    /// 后台阶段变化（如 kill Steam 完成后进入部署阶段）。
    Phase(BusyKind),
    UpdateChecked(Result<OnlineInfo, UpdateError>),
    Downloaded(Result<(), UpdateError>),
    /// 组合操作完成（成功/失败，携带动作以取成功文案）。
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
    /// Steam 核心兼容性体检完成（探测链路无失败路径，直接携带报告）。
    Compat(compat::OverallHealthReport),
    /// 后台网络刷新完成（覆盖短路态的网络适配明细）。
    CompatRefreshed(compat::OverallHealthReport),
    /// 缓存预热完成（成功/失败）。
    CompatPrecached(Result<(), String>),
}

/// 线上更新状态。
enum UpdateState {
    Idle,
    Checking,
    Checked(Result<OnlineInfo, UpdateError>),
}

/// 最近一次结果提示的结构化数据。
/// 渲染时（`notice_bar`）才按当前语言生成文案，切换语言无需重建。
enum Notice {
    /// 检查更新结果：成功携带线上版本（与本地版本比较决定最新/可更新文案）。
    UpdateChecked(Result<OnlineInfo, UpdateError>),
    /// 下载解压结果。
    Downloaded(Result<(), UpdateError>),
    /// 组合操作完成（成功/失败，携带动作以取成功文案）。
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
    /// 前置校验失败（类型化错误 → 本地化文案）。
    Precheck(workflow::Precheck),
}

/// 设置对话框页签（纯 UI 选择，状态层 cfg/of 本就独立，切换零耦合）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsTab {
    /// 常规偏好（默认；语言与托盘全局偏好）。
    General,
    /// 配置编辑器（TOML 文本编辑）。
    ConfigEditor,
    /// OnlineFix 启动预设。
    OnlineFix,
}

/// 设置弹窗全局单行动态 Footer 的按钮动作（SPEC §8.6 AC5）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FooterAction {
    LoadTemplate,
    Undo,
    Save,
    Close,
    Copy,
    Enable,
    Disable,
}

/// 单帧收集的 Footer 按钮意图（渲染后统一处理，避免借用冲突）。
#[derive(Default)]
struct SettingsActions {
    close: bool,
    save: bool,
    load_template: bool,
    undo: bool,
    copy: bool,
    enable: bool,
    disable: bool,
}

/// 每页签 Footer 布局：(左区顺序, 右区 right_to_left 顺序——第一个元素贴最右)。
/// SPEC AC5：General 仅右[关闭]；ConfigEditor 左[模板][撤销]右[保存][关闭]；
/// OnlineFix 左[复制][启用][停用]右[关闭]。
fn footer_layout(tab: SettingsTab) -> (&'static [FooterAction], &'static [FooterAction]) {
    match tab {
        SettingsTab::General => (&[], &[FooterAction::Close]),
        SettingsTab::ConfigEditor => (
            &[FooterAction::LoadTemplate, FooterAction::Undo],
            &[FooterAction::Close, FooterAction::Save],
        ),
        SettingsTab::OnlineFix => (
            &[
                FooterAction::Copy,
                FooterAction::Enable,
                FooterAction::Disable,
            ],
            &[FooterAction::Close],
        ),
    }
}

/// 渲染单个 Footer 按钮并收集点击意图（SPEC AC5 单行动态 Footer）。
fn render_footer_button(
    ui: &mut egui::Ui,
    strings: &Strings,
    action: FooterAction,
    actions: &mut SettingsActions,
) {
    let (label, style, size) = match action {
        FooterAction::Close => (
            strings.btn_close,
            ButtonStyle::Secondary,
            egui::vec2(80.0, 30.0),
        ),
        FooterAction::Save => (
            strings.btn_save,
            ButtonStyle::Primary,
            egui::vec2(80.0, 30.0),
        ),
        FooterAction::LoadTemplate => (
            strings.btn_load_template,
            ButtonStyle::Secondary,
            egui::vec2(150.0, 30.0),
        ),
        FooterAction::Undo => (
            strings.btn_undo,
            ButtonStyle::Secondary,
            egui::vec2(64.0, 30.0),
        ),
        FooterAction::Copy => (
            strings.of_btn_copy,
            ButtonStyle::Secondary,
            egui::vec2(84.0, 30.0),
        ),
        FooterAction::Enable => (
            strings.of_btn_enable,
            ButtonStyle::Primary,
            egui::vec2(120.0, 30.0),
        ),
        FooterAction::Disable => (
            strings.of_btn_disable,
            ButtonStyle::Secondary,
            egui::vec2(120.0, 30.0),
        ),
    };
    if styled_button(ui, label, style, size, true).clicked() {
        match action {
            FooterAction::Close => actions.close = true,
            FooterAction::Save => actions.save = true,
            FooterAction::LoadTemplate => actions.load_template = true,
            FooterAction::Undo => actions.undo = true,
            FooterAction::Copy => actions.copy = true,
            FooterAction::Enable => actions.enable = true,
            FooterAction::Disable => actions.disable = true,
        }
    }
}

/// Steam 核心兼容性小节：体检状态 + 明细展示 + 预热进行中标记。
struct CompatUiState {
    /// 最近一次体检报告；`None` + `checking` = 骨架态。
    report: Option<compat::OverallHealthReport>,
    /// 体检/预热进行中（显示 Checking / 禁用按钮）。
    checking: bool,
    /// 明细展开开关。
    details_open: bool,
    /// 预热进行中（按钮显示「正在缓存...」）。
    precaching: bool,
    /// 预热失败文案（就地显示，不弹窗）。
    precache_error: Option<String>,
    /// 预热成功提示（重体检后保留至下次预热/路径变更）。
    precache_done: bool,
    /// 本次预热是否为自动触发（自动失败静默，不显示错误提示）。
    precaching_auto: bool,
}

impl CompatUiState {
    fn checking() -> Self {
        Self {
            report: None,
            checking: true,
            details_open: false,
            precaching: false,
            precache_error: None,
            precache_done: false,
            precaching_auto: false,
        }
    }

    fn ready(report: compat::OverallHealthReport) -> Self {
        Self {
            report: Some(report),
            checking: false,
            details_open: false,
            precaching: false,
            precache_error: None,
            precache_done: false,
            precaching_auto: false,
        }
    }
}

/// 汇总徽标分类（SPEC.md §7.7 状态视觉）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CompatSummary {
    Checking,
    Ready,
    Online,
    Pending,
    Missing,
    Network,
}

/// 汇总分类（纯函数，测试友好）：优先级 检查中 > 缺文件 > 上游未适配 > 网络错误 > 未缓存 > 全就绪。
fn compat_summary(checking: bool, report: Option<&compat::OverallHealthReport>) -> CompatSummary {
    if checking {
        return CompatSummary::Checking;
    }
    let Some(r) = report else {
        return CompatSummary::Checking;
    };
    let st = [
        &r.steamclient_pattern.status,
        &r.steamui_pattern.status,
        &r.steamclient_ipc.status,
    ];
    use compat::ProbeStatus::*;
    if st.iter().any(|s| matches!(s, FileNotFound)) {
        return CompatSummary::Missing;
    }
    if st.iter().any(|s| matches!(s, IncompatiblePending)) {
        return CompatSummary::Pending;
    }
    if st.iter().any(|s| matches!(s, NetworkError(_))) {
        return CompatSummary::Network;
    }
    if r.has_missing_cache {
        return CompatSummary::Online;
    }
    // Ready 需 is_all_compatible 确认（全项 RemoteAvailable{cached:true} 或 CompatibleOffline）。
    if r.is_all_compatible {
        CompatSummary::Ready
    } else {
        CompatSummary::Online
    }
}

/// 待预热目标（RemoteAvailable{cached:false} 且已有哈希），供「一键缓存签名」。
fn precache_targets(report: &compat::OverallHealthReport) -> Vec<(compat::ProbeTarget, String)> {
    [
        &report.steamclient_pattern,
        &report.steamui_pattern,
        &report.steamclient_ipc,
    ]
    .iter()
    .filter_map(|r| match &r.status {
        // 收集「已适配但签名缓存缺失」的项，可手动/自动补下载。
        // cached:false（乐观/未缓存）+ RemoteAvailable{cached:true}（验证缓存命中但无签名缓存）都收；
        // 以 cache_path 实际文件存在性为准（而非信任 cached 字段，它承载「显示绿」语义）。
        compat::ProbeStatus::RemoteAvailable { .. } if !r.cache_path.is_file() => {
            r.sha256.clone().map(|sha| (r.target, sha))
        }
        _ => None,
    })
    .collect()
}

/// 快速体检后是否需要后台网络刷新：存在短路项（`CompatibleOffline`）即需补查。
fn compat_needs_network_refresh(report: &compat::OverallHealthReport) -> bool {
    [
        &report.steamclient_pattern.status,
        &report.steamui_pattern.status,
        &report.steamclient_ipc.status,
    ]
    .iter()
    .any(|s| {
        // 快速体检零网络后的两类待确认项：缓存短路（CompatibleOffline）与乐观假定（RemoteAvailable{cached:false}）。
        matches!(
            s,
            compat::ProbeStatus::CompatibleOffline
                | compat::ProbeStatus::RemoteAvailable { cached: false }
        )
    })
}

/// 是否应自动预热：体检落定为 Online（上游已适配未缓存）且当前无预热进行中。
/// 离线/未适配/缺文件/网络错误等场景不触发（规格：只有 Online 态才自动下载）。
fn should_auto_precache(report: &compat::OverallHealthReport, precaching: bool) -> bool {
    !precaching && compat_summary(false, Some(report)) == CompatSummary::Online
}
pub struct App {
    /// 全局偏好（gui_config.toml 持久化状态）。
    gui_config: GuiConfig,
    lang: Lang,
    strings: Strings,
    steam_path: String,
    status: DeployStatus,
    steam_running: bool,
    steam_monitor: SteamMonitor,
    /// 共享 Steam 运行状态（进程表 + alive/group_running/kill 三查询）。
    steam_state: Arc<SteamState>,
    local_version: Option<String>,
    update_state: UpdateState,
    busy: bool,
    /// 忙碌时当前操作类型（用于显示进度文案）。
    busy_kind: Option<BusyKind>,
    /// 待确认「关闭 Steam」的操作。
    confirm: Option<Action>,
    /// 最近一次结果提示（成功/失败），渲染时按当前语言生成文案。
    notice: Option<Notice>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// egui 上下文（托盘/Steam 联动发窗口命令用）。
    ctx: egui::Context,
    /// Windows 系统托盘（可能创建失败）。
    tray: Option<Tray>,
    /// 外部系统分派器（T4：GitHub 外链经此调起默认浏览器；测试注入 mock）。
    dispatcher: Box<dyn external::ExternalSystemDispatcher>,
    /// 本地清单读取接缝（T7：ACF 名称解析经此读盘；测试注入 mock）。
    manifest: Box<dyn external::ManifestAccessor>,
    /// 窗口当前是否可见（托盘显隐切换用）。
    window_visible: bool,
    /// 首次帧后按内容高度自适应窗口（消除底部大留白）。
    autosized: bool,
    /// 显示窗口后待发送的 Focus（置顶）命令。
    pending_focus: bool,
    /// 最小化时是否自动隐藏到托盘（托盘菜单勾选项）。
    minimize_to_tray: bool,
    /// 上一帧是否处于最小化（检测最小化按钮被点击）。
    was_minimized: bool,

    /// 设置对话框是否打开（配置编辑器与 OnlineFix 预设状态见 settings.rs）。
    settings_open: bool,
    /// 配置编辑器状态（缓冲/载入/校验/保存提示，见 settings.rs）。
    cfg: ConfigEditorState,
    /// OnlineFix 启动预设状态（账号/AppID/展示状态/写入门闩，见 settings.rs）。
    of: OnlineFixState,
    /// 设置对话框当前页签（会话内记忆，默认「配置编辑器」）。
    settings_tab: SettingsTab,
    /// 「撤销」按钮点击后的待注入标志（下一帧合成 Ctrl+Z 事件 + 聚焦编辑器）。
    undo_pending: bool,
    /// 配置编辑器文本域的实测 widget id（每帧渲染时从 Response 捕获；撤销聚焦用）。
    editor_id: Option<egui::Id>,
    /// Steam 核心兼容性小节状态。
    compat: CompatUiState,
    /// 上次体检的 Steam 路径（防抖：路径未变不重复体检）。
    compat_path: String,
}

/// 读取系统中文字体数据（微软雅黑/黑体/宋体，首个可读的生效），无则 None。
fn read_system_cjk_font() -> Option<egui::FontData> {
    const CANDIDATES: [(&str, u32); 4] = [
        (r"C:\Windows\Fonts\msyh.ttc", 0),
        (r"C:\Windows\Fonts\msyhl.ttc", 0),
        (r"C:\Windows\Fonts\simhei.ttf", 0),
        (r"C:\Windows\Fonts\simsun.ttc", 0),
    ];
    for (path, index) in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            let mut data = egui::FontData::from_owned(bytes);
            data.index = index;
            return Some(data);
        }
    }
    None
}

/// 注册系统中文字体作为 fallback（egui 默认字体只有拉丁字形，无 CJK）。
fn install_cjk_font(ctx: &egui::Context) {
    let Some(data) = read_system_cjk_font() else {
        return;
    };
    ctx.add_font(egui::epaint::text::FontInsert::new(
        "cjk-fallback",
        data,
        vec![
            egui::epaint::text::InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Lowest,
            },
            egui::epaint::text::InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::epaint::text::FontPriority::Lowest,
            },
        ],
    ));
}

/// 自动隐身策略（ADR-0001）：Steam 边沿事件 + 当前窗口显隐 → 目标显隐。
/// Some(true)=显示、Some(false)=隐藏、None=不变。
fn auto_tray_policy(event: SteamEvent, window_visible: bool) -> Option<bool> {
    match (event, window_visible) {
        // 启动 → 隐藏。
        (SteamEvent::Started, true) => Some(false),
        // 退出 → 弹出。
        (SteamEvent::Stopped, false) => Some(true),
        // 其余：状态与显隐一致，不变。
        _ => None,
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        paths::init(Box::new(paths::RealEnvironmentProbe)); // 启动时探测一次存储模式。
        install_cjk_font(&cc.egui_ctx);
        install_theme(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let gui_config = GuiConfig::load(&paths::resolver().config_path());
        let lang = gui_config.language.resolve();
        let strings = Strings::new(lang);
        // 窗口标题随语言（zh: OpenSteamTool 一键管理工具 / en: OpenSteamTool Manager）。
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            strings.window_title.to_owned(),
        ));
        let steam_path = steam::detect_steam_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        // 首次体检用的路径快照（Self 构造后闭包 move，避免与字段借用冲突）。
        let probe_path = steam_path.clone();
        let steam_dir = Path::new(&steam_path);
        let status = dll::check_status(steam_dir);
        let local_version = dll::read_local_version(&paths::resolver().effective_dll_dir());
        let steam_state = Arc::new(SteamState::new());
        let steam_monitor = SteamMonitor::new(&steam_state);
        let steam_running = steam_monitor.is_running();
        let minimize_to_tray = gui_config.minimize_to_tray;
        let tray = Tray::new(
            crate::tray::load_icon(),
            strings.app_title,
            strings.tray_show,
            strings.tray_quit,
            strings.tray_minimize,
            gui_config.minimize_to_tray,
        );

        let app = Self {
            lang,
            gui_config,
            strings,
            steam_path,
            status,
            steam_running,
            steam_monitor,
            steam_state,
            local_version,
            update_state: UpdateState::Idle,
            busy: false,
            busy_kind: None,
            confirm: None,
            notice: None,
            tx,
            rx,
            ctx: cc.egui_ctx.clone(),
            tray,
            dispatcher: Box::new(external::CmdStartDispatcher),
            manifest: Box::new(external::FsManifestAccessor),
            window_visible: true,
            autosized: false,
            pending_focus: false,
            minimize_to_tray,
            was_minimized: false,
            settings_open: false,
            cfg: ConfigEditorState::new(),
            of: OnlineFixState::new(),
            settings_tab: SettingsTab::General,
            undo_pending: false,
            editor_id: None,
            compat: CompatUiState::checking(),
            compat_path: probe_path.clone(),
        };
        // 启动即触发首次体检（初始 checking 骨架态，零白屏）。
        app.spawn(&cc.egui_ctx, move || {
            Msg::Compat(compat::probe_all(Path::new(&probe_path)))
        });
        app
    }

    /// 后台线程执行任务，完成后发消息并请求重绘。
    fn spawn<F>(&self, ctx: &egui::Context, f: F)
    where
        F: FnOnce() -> Msg + Send + 'static,
    {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let msg = f();
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    /// 控制窗口显隐；显示时置 pending_focus，下一帧再发 Focus（刚变可见时 Focus 无效）。
    fn set_window_visible(&mut self, visible: bool) {
        self.window_visible = visible;
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if visible {
            self.pending_focus = true;
            // 立即唤醒下一帧消费 pending_focus（否则隐藏→可见后 repaint 间隔会拉长）。
            self.ctx.request_repaint();
        }
    }

    /// 后台操作完成后若 Steam 已运行（重启/启动类成功），隐藏窗口到托盘。
    /// 不依赖 2s 边沿监视：Steam 本就运行时边沿不触发。
    /// 场景区分靠 `steam_running` 本身：退出并卸载（不重启）→ Steam 未运行 → 不隐藏。
    fn hide_if_steam_running(&mut self) {
        if self.steam_running {
            self.set_window_visible(false);
        }
    }

    /// 处理托盘事件：切换显隐 / 显示 / 退出。
    fn handle_tray_events(&mut self) {
        let Some(tray) = &self.tray else { return };
        // 先收集动作再逐个处理，避免 tray 借用与 &mut self 冲突。
        let mut actions = Vec::new();
        while let Some(action) = tray.poll() {
            actions.push(action);
        }
        // 菜单勾选状态在 poll 后读取（CheckMenuItem 点击后自动翻转）。
        let minimize_checked = tray.is_minimize_to_tray();
        for action in actions {
            match action {
                TrayAction::ToggleVisible => self.set_window_visible(!self.window_visible),
                TrayAction::Show => self.set_window_visible(true),
                TrayAction::Quit => {
                    self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                TrayAction::ToggleMinimizeToTray => {
                    self.minimize_to_tray = minimize_checked;
                    self.gui_config.minimize_to_tray = minimize_checked;
                    let _ = self.gui_config.save(&paths::resolver().config_path());
                }
            }
        }
    }

    fn refresh_status(&mut self) {
        self.status = dll::check_status(Path::new(self.steam_path.trim()));
    }

    /// 处理后台消息：更新状态与提示。
    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Phase(kind) => self.busy_kind = Some(kind),
                Msg::UpdateChecked(res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    self.notice = Some(Notice::UpdateChecked(res.clone()));
                    self.update_state = UpdateState::Checked(res);
                }
                Msg::Downloaded(res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    self.notice = Some(Notice::Downloaded(res.clone()));
                    if let Ok(()) = res {
                        self.local_version =
                            dll::read_local_version(&paths::resolver().effective_dll_dir());
                    }
                }
                Msg::WorkflowDone(action, res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    self.notice = Some(Notice::WorkflowDone(action, res.clone()));
                    if let Ok(()) = res {
                        self.refresh_status();
                    }
                    self.steam_running = self.steam_monitor.rescan();
                    // 启动/重启类成功后 Steam 已运行 → 直接隐藏到托盘（不依赖边沿检测）；
                    // 仅退出并卸载（ExitAndUninstall）Steam 未运行 → 保持显示。
                    self.hide_if_steam_running();
                }
                Msg::Compat(report) => {
                    // 保留预热成功提示（ready 构造会重置，预热后重体检不应丢提示）。
                    let done = self.compat.precache_done;
                    self.compat = CompatUiState::ready(report.clone());
                    self.compat.precache_done = done;
                    // 短路项存在时后台补查网络适配状态（上游适配变化可感知）。
                    if compat_needs_network_refresh(&report) {
                        let path = self.steam_path.trim().to_string();
                        let ctx = self.ctx.clone();
                        self.spawn(&ctx, move || {
                            Msg::CompatRefreshed(compat::probe_all_refresh(Path::new(&path)))
                        });
                    }
                    // 自动预热：体检落定为 Online（上游已适配未缓存）且无手动预热进行中 →
                    // 后台自动下载新签名，用户零操作（失败静默，手动入口保留）。
                    if should_auto_precache(&report, self.compat.precaching) {
                        let ctx = self.ctx.clone();
                        self.start_precache_all(&ctx, true);
                    }
                }
                Msg::CompatRefreshed(report) => {
                    // 网络刷新结果覆盖短路态；不再次触发刷新（防止 quick→refresh 循环）。
                    let done = self.compat.precache_done;
                    self.compat = CompatUiState::ready(report);
                    self.compat.precache_done = done;
                }
                Msg::CompatPrecached(res) => {
                    self.compat.precaching = false;
                    let was_auto = self.compat.precaching_auto;
                    self.compat.precaching_auto = false;
                    match res {
                        Ok(()) => {
                            self.compat.precache_done = true;
                            // 预热成功 → 重跑体检刷新本地缓存状态。
                            self.compat.checking = true;
                            self.compat.report = None;
                            self.compat.precache_error = None;
                            let path = self.steam_path.trim().to_string();
                            let ctx = self.ctx.clone();
                            self.spawn(&ctx, move || {
                                Msg::Compat(compat::probe_all(Path::new(&path)))
                            });
                        }
                        Err(e) => {
                            // 自动预热失败静默（离线等场景不弹错误），状态保持 Online、手动入口保留；
                            // 手动预热失败照常显示错误提示。
                            if !was_auto {
                                self.compat.precache_error = Some(e);
                            }
                        }
                    }
                }
            }
        }
    }

    /// 用户点击操作按钮：Steam 在运行且操作需关闭 Steam → 弹确认框；否则直接执行。
    fn request_action(&mut self, ctx: &egui::Context, action: Action) {
        if self.busy {
            return;
        }
        if action.needs_close() && self.steam_running {
            self.confirm = Some(action);
            return;
        }
        self.start_action(ctx, action, false);
    }

    fn start_action(&mut self, ctx: &egui::Context, action: Action, kill_first: bool) {
        let dll_dir = paths::resolver().effective_dll_dir();
        let steam_dir = PathBuf::from(self.steam_path.trim());

        // 前置校验（类型化错误 → 本地化文案），失败则不进入忙碌状态。
        let ops = match workflow::plan(action, kill_first, &steam_dir, &dll_dir) {
            Ok(ops) => ops,
            Err(precheck) => {
                self.confirm = None;
                self.notice = Some(Notice::Precheck(precheck));
                return;
            }
        };

        self.busy = true;
        self.busy_kind = Some(ops.first().expect("plan never returns empty").phase()); // 同步首阶段，点击即见阶段文案
        self.confirm = None;

        let ctx2 = ctx.clone();
        let tx = self.tx.clone();
        let steam = self.steam_state.clone();
        self.spawn(ctx, move || {
            let res = workflow::execute(
                &ops,
                &workflow::WorkflowCtx { dll_dir, steam_dir, steam },
                |phase| {
                    let _ = tx.send(Msg::Phase(phase));
                    ctx2.request_repaint();
                },
            );
            Msg::WorkflowDone(action, res)
        });
    }


    fn check_update(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.busy_kind = Some(BusyKind::Checking);
        self.update_state = UpdateState::Checking;
        self.spawn(ctx, || Msg::UpdateChecked(updater::check_update()));
    }

    fn download_update(&mut self, ctx: &egui::Context, info: OnlineInfo) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.busy_kind = Some(BusyKind::Downloading);
        let dll_dir = paths::resolver().update_target_dll_dir();
        self.spawn(ctx, move || {
            Msg::Downloaded(updater::download_and_extract(&info, &dll_dir))
        });
    }

    // ---------- 设置对话框（PR-1：TOML 配置编辑器） ----------

    /// 打开设置：置位并标记缓冲待加载（首次进入「配置编辑器」页签时读盘）。
    fn open_settings(&mut self) {
        self.settings_open = true;
        // 配置编辑器：标记待载入（切到该页签首帧读盘）。
        self.cfg.mark_unloaded();
        // OnlineFix 区：按当前 Steam 路径刷新账号与 AppID 候选（进程组运行态写入门闩每次写前实时判定）。
        let steam_dir = Path::new(self.steam_path.trim());
        self.of.refresh(steam_dir, self.manifest.as_ref());
    }

    /// 设置对话框主体（模态；Steam 路径无效时仅提示 + 关闭）。
    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }

        let steam_dir = Path::new(self.steam_path.trim());
        let steam_ok = dll::check_status(steam_dir) != DeployStatus::InvalidPath;
        let target = config_editor::target_path(steam_dir);
        let file_exists = steam_ok && target.exists();
        let mut actions = SettingsActions::default();
        let mut template_confirm = false;
        let mut tab_clicked = None;
        let mut lang_changed = false;
        let mut minimize_changed = false;

        egui::Modal::new(egui::Id::new("settings_dialog")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading(self.strings.settings_title);
            ui.add_space(6.0);

            // 页签行：常规偏好 / 配置编辑器 / OnlineFix 预设（egui 0.36 无内置 TabView，selectable_label 手写）。
            ui.horizontal(|ui| {
                let selected = self.settings_tab == SettingsTab::General;
                if ui
                    .selectable_label(selected, egui::RichText::new(self.strings.settings_tab_general).strong())
                    .clicked()
                {
                    tab_clicked = Some(SettingsTab::General);
                }
                let selected = self.settings_tab == SettingsTab::ConfigEditor;
                if ui
                    .selectable_label(selected, egui::RichText::new(self.strings.settings_tab_config).strong())
                    .clicked()
                {
                    tab_clicked = Some(SettingsTab::ConfigEditor);
                }
                let selected = self.settings_tab == SettingsTab::OnlineFix;
                if ui
                    .selectable_label(selected, egui::RichText::new(self.strings.of_title).strong())
                    .clicked()
                {
                    tab_clicked = Some(SettingsTab::OnlineFix);
                }
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            if !steam_ok {
                ui.label(egui::RichText::new(self.strings.settings_no_steam_dir).color(TEXT_SUB));
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if styled_button(ui, self.strings.btn_close, ButtonStyle::Primary, egui::vec2(80.0, 30.0), true).clicked() {
                            actions.close = true;
                        }
                    });
                });
                return;
            }

            match self.settings_tab {
                SettingsTab::General => {
                    // 界面语言三选（变更即时生效并写盘，SPEC AC4）。
                    let lang_before = self.gui_config.language;
                    ui.horizontal(|ui| {
                        ui.label(self.strings.settings_lang_label);
                        ui.add_space(8.0);
                        ui.radio_value(&mut self.gui_config.language, LanguagePreference::Auto, self.strings.lang_auto);
                        ui.radio_value(&mut self.gui_config.language, LanguagePreference::Zh, self.strings.lang_zh);
                        ui.radio_value(&mut self.gui_config.language, LanguagePreference::En, self.strings.lang_en);
                    });
                    ui.add_space(10.0);
                    let minimize_before = self.gui_config.minimize_to_tray;
                    ui.checkbox(&mut self.gui_config.minimize_to_tray, self.strings.minimize_to_tray_check);
                    if self.gui_config.language != lang_before {
                        lang_changed = true;
                    }
                    if self.gui_config.minimize_to_tray != minimize_before {
                        minimize_changed = true;
                    }
                }
                SettingsTab::ConfigEditor => {
                    // 懒加载：首次进入该页签才读盘。
                    self.cfg.ensure_loaded(&target);
                    // 顶部固定行：目标文件。
                    status_line(
                        ui,
                        &format!("{}{}", self.strings.settings_target, target.display()),
                        TEXT_WEAK,
                    );
                    ui.add_space(8.0);

                    // 撤销：上一帧按钮点击 → 本帧合成 Ctrl+Z 并聚焦编辑器（触发 egui 原生撤销，单一系统）。
                    // 焦点用上一帧渲染捕获的真实 widget id（make_persistent_id 依 ui 作用域而异，不可自行拼装）。
                    if self.undo_pending {
                        if let Some(editor_id) = self.editor_id {
                            ctx.input_mut(|i| i.events.push(egui::Event::Key {
                                key: egui::Key::Z,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::COMMAND,
                            }));
                            ui.memory_mut(|mem| mem.request_focus(editor_id));
                        }
                        self.undo_pending = false;
                    }

                    // 中间滚动：仅编辑器。
                    let h_mid = (ui.available_height() - FOOTER_RESERVE).max(120.0);
                    let mut text_edit_id = None;
                    let editor = egui::ScrollArea::vertical()
                        .max_height(h_mid)
                        .show(ui, |ui| {
                            let resp = ui.add_sized(
                                egui::vec2(ui.available_width(), (h_mid - 20.0).max(100.0)),
                                egui::TextEdit::multiline(&mut self.cfg.text)
                                    .id_salt("settings_cfg_text")
                                    .code_editor()
                                    .desired_width(f32::INFINITY),
                            );
                            text_edit_id = Some(resp.id);
                            resp
                        });
                    self.editor_id = text_edit_id;
                    if editor.inner.changed() {
                        self.cfg.mark_edited(); // 编辑清除「已保存」提示、标记未保存。
                    }
                    ui.add_space(6.0);

                    // 底部固定行：状态 + 按钮。
                    if let Some(err) = &self.cfg.err {
                        status_line(ui, &config_err_text(&self.strings, self.lang, err), ERR_RED);
                    } else if self.cfg.saved {
                        status_line(ui, self.strings.ok_config_saved, STATUS_INSTALLED);
                    } else if !file_exists {
                        status_line(ui, self.strings.settings_file_missing, TEXT_WEAK);
                    }
                    ui.add_space(10.0);
                }
                SettingsTab::OnlineFix => {
                    // 写入门闩：快速判定（仅看 steam.exe，2s 缓存）；残留 webhelper 等孤儿由写时实时复查兜底。
                    let write_blocked = self.steam_running;
                    if write_blocked {
                        status_line(ui, self.strings.of_steam_running, TEXT_WEAK);
                    } else if self.of.accounts.is_empty() {
                        status_line(ui, self.strings.of_no_account, TEXT_WEAK);
                    } else {
                        // 顶部固定行：账号选择。
                        ui.horizontal(|ui| {
                            ui.label(self.strings.of_account_label);
                            let label = OnlineFixState::account_name(&self.of.accounts[self.of.account_idx]);
                            let mut selected_idx = None;
                            egui::ComboBox::from_id_salt("of_account")
                                .selected_text(label)
                                .width(150.0)
                                .show_ui(ui, |ui| {
                                    for (i, vdf) in self.of.accounts.iter().enumerate() {
                                        let selected = self.of.account_idx == i;
                                        let name = OnlineFixState::account_name(vdf);
                                        if ui.selectable_label(selected, name).clicked() {
                                            selected_idx = Some(i);
                                        }
                                    }
                                });
                            if let Some(i) = selected_idx {
                                self.of.select_account(i);
                                // 换账号 → 重查生效游戏并同步输入框（SPEC AC6）。
                                self.of.refresh_active(steam_dir, self.manifest.as_ref());
                            }
                        });
                        ui.add_space(6.0);

                        // 看板：当前生效游戏（SPEC AC6；看板停用与 Footer 停用同指一操作）。
                        if let Some(active) = &self.of.active {
                            if render_active_board(ui, &self.strings, active).0 {
                                actions.disable = true;
                            }
                            ui.add_space(6.0);
                        }

                        // 中间滚动：AppID 输入 + Lua 候选。
                        let h_mid = (ui.available_height() - FOOTER_RESERVE).max(120.0);
                        egui::ScrollArea::vertical().max_height(h_mid).show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(self.strings.of_appid_label);
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut self.of.appid)
                                        .desired_width(96.0)
                                        .hint_text("0"),
                                );
                                if resp.changed() {
                                    self.of.appid_changed();
                                }
                                if !self.of.candidates.is_empty() {
                                    ui.add_space(8.0);
                                    // 胶囊：ACF 含名称 → 「名称 (appid)」；缺失/畸变 → 纯数字（SPEC AC6）。
                                    if let Some(id) =
                                        render_candidate_capsules(ui, &self.of.candidates).0
                                    {
                                        self.of.appid = id.to_string();
                                        self.of.appid_changed();
                                    }
                                }
                            });
                        });
                        ui.add_space(6.0);

                        // 底部固定行：状态 + 按钮 + 单游戏限制提示。
                        self.of.refresh_status();
                        if let Some(status) = self.of.status() {
                            let (text, color) = of_status_line(&self.strings, status);
                            status_line(ui, &text, color);
                        }
                        ui.add_space(8.0);
                        // 上游限制提示（spec PR-2）：同一时间仅一个 onlinefix 游戏可运行。
                        status_line(ui, self.strings.of_single_limit, TEXT_WEAK);
                    }
                }
            }

            // 页脚：全局单行动态 Footer（SPEC AC5）——按钮集按页签布局，
            // 子视图内部不再渲染局部按钮条。
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            let (left, right) = footer_layout(self.settings_tab);
            ui.horizontal(|ui| {
                for action in left {
                    render_footer_button(ui, &self.strings, *action, &mut actions);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for action in right {
                        render_footer_button(ui, &self.strings, *action, &mut actions);
                    }
                });
            });
        });

        // 「从示例模板创建」覆盖确认（顶置模态；是 → 载入模板，否 → 取消）。
        // 「从示例模板创建」：先处理（可能置位覆盖确认），弹窗紧随其后同帧可见。
        if actions.load_template {
            if self.cfg.dirty {
                template_confirm = true; // 有未保存修改：先确认再覆盖。
            } else {
                self.cfg.fill_template();
            }
        }

        if template_confirm {
            let mut confirmed = false;
            egui::Modal::new(egui::Id::new("confirm_template")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading(self.strings.confirm_title);
                ui.add_space(8.0);
                ui.label(self.strings.confirm_template_overwrite);
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if styled_button(ui, self.strings.yes, ButtonStyle::Primary, egui::vec2(72.0, 30.0), true).clicked() {
                        confirmed = true;
                    }
                    ui.add_space(4.0);
                    if styled_button(ui, self.strings.no, ButtonStyle::Secondary, egui::vec2(72.0, 30.0), true).clicked() {
                        // 取消：本帧结束即消失。
                    }
                });
            });
            if confirmed {
                self.cfg.fill_template();
            }
        }

        if let Some(tab) = tab_clicked {
            self.settings_tab = tab; // 会话内记忆上次页签。
            // T7：进入 OnlineFix 页签 → 反查生效游戏并同步 AppID 输入框（SPEC AC6）。
            if tab == SettingsTab::OnlineFix {
                self.of.refresh_active(steam_dir, self.manifest.as_ref());
            }
        }
        if lang_changed {
            // 语言变更：即时生效（渲染 + 窗口标题）并写盘（SPEC AC4）。
            self.lang = self.gui_config.language.resolve();
            self.strings = Strings::new(self.lang);
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.strings.window_title.to_owned(),
            ));
            let _ = self.gui_config.save(&paths::resolver().config_path());
        }
        if minimize_changed {
            self.minimize_to_tray = self.gui_config.minimize_to_tray;
            let _ = self.gui_config.save(&paths::resolver().config_path());
        }
        if actions.undo {
            self.undo_pending = true; // 下一帧合成 Ctrl+Z。
        }
        if actions.save {
            self.cfg.save(&target);
        }
        if actions.enable {
            self.of
                .enable(steam_dir, self.steam_running, &self.steam_state);
            // 启用后反查生效游戏（看板即时出现；输入框已是该 AppID 不被覆盖）。
            self.of.refresh_active(steam_dir, self.manifest.as_ref());
        }
        if actions.disable {
            self.of
                .disable(steam_dir, self.steam_running, &self.steam_state);
            // 停用后反查生效游戏（看板即时消失/切换）。
            self.of.refresh_active(steam_dir, self.manifest.as_ref());
        }
        if actions.copy {
            ctx.copy_text(onlinefix::ONLINEFIX_ARG.to_owned());
            self.of.mark_copied();
        }
        if actions.close {
            self.settings_open = false;
        }
    }
    // ---------- UI ----------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let mut open_settings = false;
        let _ = render_top_bar(
            ui,
            &self.strings,
            self.dispatcher.as_ref(),
            &mut open_settings,
        );
        if open_settings {
            self.open_settings();
        }
        ui.add_space(6.0);
    }

    fn card1(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度，避免堆在左侧
            card_title(ui, self.strings.card1_title);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let edit_width = (ui.available_width() - 92.0).max(120.0);
                let resp = ui.add_sized(
                    egui::vec2(edit_width, 34.0),
                    egui::TextEdit::singleline(&mut self.steam_path)
                        .margin(egui::Margin::symmetric(10, 7))
                        .hint_text(self.strings.steam_path_label),
                );
                if resp.changed() {
                    self.refresh_status();
                    let ctx = self.ctx.clone();
                    self.maybe_start_compat_probe(&ctx);
                }
                if styled_button(ui, self.strings.browse, ButtonStyle::Secondary, egui::vec2(82.0, 34.0), true).clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.steam_path = dir.display().to_string();
                    self.refresh_status();
                    let ctx = self.ctx.clone();
                    self.maybe_start_compat_probe(&ctx);
                }
            });
            self.compat_section(ui);
        });
        ui.add_space(10.0);
    }

    /// 路径变化时触发体检（防抖：与上次体检路径相同则跳过，防逐字符起线程）。
    fn maybe_start_compat_probe(&mut self, ctx: &egui::Context) {
        let path = self.steam_path.trim().to_string();
        if path == self.compat_path {
            return;
        }
        self.compat_path = path.clone();
        self.compat = CompatUiState::checking();
        self.spawn(ctx, move || Msg::Compat(compat::probe_all(Path::new(&path))));
    }

    /// 预热：后台线程逐个下载未缓存签名，完成后触发体检刷新（SPEC.md §7.7）。
    /// `auto=true`（自动预热）时失败静默——离线等场景不弹错误，徽章保持 Online、手动入口保留。
    fn start_precache_all(&mut self, ctx: &egui::Context, auto: bool) {
        let Some(report) = &self.compat.report else {
            return;
        };
        let targets = precache_targets(report);
        if targets.is_empty() {
            return;
        }
        let steam_path = self.steam_path.trim().to_string();
        let ctx = ctx.clone();
        self.compat.precaching = true;
        self.compat.precaching_auto = auto;
        self.compat.precache_error = None;
        self.compat.precache_done = false;
        self.spawn(&ctx, move || {
            let res = targets.into_iter().try_for_each(|(target, sha)| {
                compat::precache(Path::new(&steam_path), target, &sha)
                    .map_err(|e| e.to_string())
            });
            Msg::CompatPrecached(res)
        });
    }

    /// Card 1 底部「Steam 核心兼容性」小节：第一行标题+状态徽章+操作靠右，第二行辅助说明弱化。
    fn compat_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        // 无分隔线：竖条标题 + 徽章自成边界，直接衔接上方路径输入区。

        let summary = compat_summary(self.compat.checking, self.compat.report.as_ref());

        // 第一行：左侧（竖条标题 + 状态徽章）| 右侧操作（预热 + 详细信息，贴右边缘）。
        ui.horizontal(|ui| {
            // 标题：与其他卡片一致的蓝色竖条指示器。
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 13.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, ACCENT);
                ui.add_space(8.0);
                ui.label(egui::RichText::new(self.strings.compat_title).size(13.5).strong());
            });
            ui.add_space(8.0);
            self.compat_badge(ui, summary);

            // 右侧操作区：right_to_left 首项（详细信息）贴最右，预热按钮在其左侧。
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.compat.checking {
                    if ui.button(self.strings.compat_btn_details).clicked() {
                        self.compat.details_open = !self.compat.details_open;
                    }
                }
                if summary == CompatSummary::Online && !self.compat.checking {
                    let label = if self.compat.precaching {
                        self.strings.compat_precaching
                    } else {
                        self.strings.compat_btn_precache
                    };
                    if styled_button(
                        ui,
                        label,
                        ButtonStyle::Secondary,
                        egui::vec2(132.0, 26.0),
                        !self.compat.precaching,
                    )
                    .clicked()
                    {
                        let ctx = self.ctx.clone();
                        self.start_precache_all(&ctx, false);
                    }
                }
            });
        });

        // 辅助说明：五态一句话（Checking 为瞬时态不显示），小字号弱灰 + 缩进对齐标题竖条。
        if summary != CompatSummary::Checking {
            let tip = match summary {
                CompatSummary::Ready => self.strings.compat_tip_ready,
                CompatSummary::Online => self.strings.compat_tip_online,
                CompatSummary::Pending => self.strings.compat_tip_pending,
                CompatSummary::Missing => self.strings.compat_tip_missing,
                CompatSummary::Network => self.strings.compat_tip_network,
                CompatSummary::Checking => "",
            };
            ui.horizontal(|ui| {
                // 缩进对齐标题竖条（竖条 3px + 8px gap = 11px）。
                ui.add_space(11.0);
                ui.label(egui::RichText::new(tip).size(11.5).color(TEXT_WEAK));
            });
        }
        if let Some(err) = &self.compat.precache_error {
            ui.label(
                egui::RichText::new(
                    self.strings.compat_precache_failed.replace("{err}", err),
                )
                .size(12.0)
                .color(ERR_RED),
            );
        }
        if self.compat.precache_done {
            ui.label(
                egui::RichText::new(self.strings.compat_precache_done)
                    .size(12.0)
                    .color(STATUS_INSTALLED),
            );
        }

        // 详情明细（展开时）。
        if self.compat.details_open {
            if let Some(report) = self.compat.report.clone() {
                self.compat_details(ui, &report);
            }
        }
    }

    /// 状态徽章（pill badge）：浅色底 + 深色文字 + 状态图标，视觉低于标题、高于辅助行。
    fn compat_badge(&self, ui: &mut egui::Ui, summary: CompatSummary) {
        let (icon, text, fg, bg) = match summary {
            CompatSummary::Checking => ("○", self.strings.compat_checking, TEXT_WEAK, BADGE_GRAY),
            CompatSummary::Ready => ("✔", self.strings.compat_status_ready, STATUS_INSTALLED, BADGE_GREEN),
            CompatSummary::Online => ("●", self.strings.compat_status_online, STATUS_WARN, BADGE_AMBER),
            CompatSummary::Pending => ("▲", self.strings.compat_status_pending, ERR_RED, BADGE_RED),
            CompatSummary::Missing => ("?", self.strings.compat_status_missing, TEXT_WEAK, BADGE_GRAY),
            CompatSummary::Network => ("?", self.strings.compat_status_network, TEXT_WEAK, BADGE_GRAY),
        };
        pill_badge(ui, icon, text, fg, bg);
    }

    /// 单项探针状态文案与颜色（详情明细行）。
    fn compat_status_of(&self, status: &compat::ProbeStatus) -> (&'static str, egui::Color32) {
        use compat::ProbeStatus::*;
        match status {
            Checking => (self.strings.compat_checking, TEXT_WEAK),
            RemoteAvailable { cached: true } => (self.strings.compat_status_ready, STATUS_INSTALLED),
            RemoteAvailable { cached: false } => (self.strings.compat_status_online, STATUS_WARN),
            CompatibleOffline => (self.strings.compat_status_offline, STATUS_INSTALLED),
            IncompatiblePending => (self.strings.compat_status_pending, ERR_RED),
            NetworkError(_) => (self.strings.compat_status_network, TEXT_WEAK),
            FileNotFound => (self.strings.compat_status_missing, TEXT_WEAK),
        }
    }

    /// 明细行：DLL + 类型 + SHA-256 前 12 位 + 状态。
    fn compat_details(&mut self, ui: &mut egui::Ui, report: &compat::OverallHealthReport) {
        ui.add_space(6.0);
        for (probe, kind) in [
            (&report.steamclient_pattern, "Pattern"),
            (&report.steamui_pattern, "Pattern"),
            (&report.steamclient_ipc, "IPC"),
        ] {
            let row = self
                .strings
                .compat_row_dll
                .replace("{dll}", probe.target.relative_dll())
                .replace("{kind}", kind);
            let sha = probe
                .sha256
                .as_deref()
                .map(|s| &s[..s.len().min(12)])
                .unwrap_or("—");
            let (stext, scolor) = self.compat_status_of(&probe.status);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(row).size(12.5).color(TEXT_SUB));
                ui.monospace(egui::RichText::new(sha).size(12.0).color(TEXT_WEAK));
                status_line(ui, stext, scolor);
            });
        }
        // 详情内「一键缓存签名」：有未缓存项时提供（SPEC.md §7.7）。
        if !precache_targets(report).is_empty() && !self.compat.precaching {
            let ctx = self.ctx.clone();
            if styled_button(
                ui,
                self.strings.compat_btn_precache_all,
                ButtonStyle::Secondary,
                egui::vec2(150.0, 26.0),
                true,
            )
            .clicked()
            {
                self.start_precache_all(&ctx, false);
            }
        }
    }

    /// 补丁控制台（T5 AC2）：部署状态 pill 徽章 + Steam 进程状态 + 按钮行，同一卡片内边距。
    fn patch_console(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度
            card_title(ui, self.strings.patch_console_title);
            ui.add_space(10.0);

            // 状态行：部署状态徽章 + Steam 进程状态徽章（复用 2s 轮询的 steam_running）。
            ui.horizontal(|ui| {
                let (icon, text, fg, bg) = deploy_badge_style(&self.strings, self.status);
                pill_badge(ui, icon, text, fg, bg);
                ui.add_space(6.0);
                let (icon, text, fg, bg) = steam_badge_style(&self.strings, self.steam_running);
                pill_badge(ui, icon, text, fg, bg);
            });
            ui.add_space(10.0);

            // 按钮行：意图先收集后执行，避免借用冲突。
            let mut actions = PatchActions::default();
            let _ = patch_console_buttons(
                ui,
                &self.strings,
                self.status,
                self.steam_running,
                !self.busy,
                &mut actions,
            );
            if actions.apply_and_launch {
                self.request_action(&ctx, Action::ApplyAndLaunch);
            }
            if actions.launch {
                self.request_action(&ctx, Action::Launch);
            }
            if actions.exit_and_uninstall {
                self.request_action(&ctx, Action::ExitAndUninstall);
            }
            if actions.uninstall_and_restart {
                self.request_action(&ctx, Action::UninstallAndRestart);
            }
        });
        ui.add_space(10.0);
    }

    /// 底部状态栏（T6 AC3）：左 notice / 右本地版本 + 更新按钮（内联更新，不弹窗不拉起卡片）。
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let dll_dir = paths::resolver().effective_dll_dir();
        let all_local_exist = dll::TARGET_DLLS.iter().all(|d| dll_dir.join(d).is_file());
        let view = StatusBarView {
            strings: &self.strings,
            local_version: self.local_version.as_deref(),
            all_local_exist,
            update_state: &self.update_state,
            busy: self.busy,
            busy_kind: self.busy_kind,
            notice: self.notice.as_ref(),
        };
        let mut actions = StatusBarActions::default();
        ui.horizontal(|ui| {
            let _ = status_bar_notice(ui, &view);
            let _ = status_bar_right(ui, &view, &mut actions);
        });
        if actions.check {
            self.check_update(&ctx);
        }
        if let Some(info) = actions.download {
            self.download_update(&ctx, info);
        }
    }
}

impl eframe::App for App {
    /// 非渲染逻辑：托盘事件 / Steam 状态 / 最小化检测 / 后台消息。
    ///
    /// 关键：窗口最小化或隐藏时，eframe 0.36 **不调用 `App::ui`**，只调用本方法
    /// （见 `run_ui_and_paint` 的 `!show_ui` 分支 → `App::logic`）。因此最小化检测、
    /// 托盘事件处理与 repaint 续命必须放在这里，否则窗口一最小化逻辑就停摆。
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 帧首先消费 pending_focus（上帧 set_window_visible(true) 置位）：
        // 刚变可见时同帧 Focus 无效，须等窗口真正可见后再补发（置顶）。
        if self.pending_focus {
            self.pending_focus = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // 处理托盘事件（左键/菜单），可能改变窗口显隐。
        self.handle_tray_events();

        // 定时监视 Steam 运行状态（边沿事件 → 自动隐身策略）。
        if let Some(event) = self.steam_monitor.tick() {
            self.steam_running = event == SteamEvent::Started;
            if let Some(visible) = auto_tray_policy(event, self.window_visible) {
                self.set_window_visible(visible);
            }
        }

        // 隐藏到托盘/最小化时窗口不可见：用短间隔驱动，托盘事件与最小化检测响应快。
        let repaint_interval = if self.window_visible {
            process::STEAM_REFRESH_INTERVAL
        } else {
            std::time::Duration::from_millis(100)
        };
        ctx.request_repaint_after(repaint_interval);

        // 最小化时自动隐藏到托盘（勾选项开启时）。
        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        if minimized && !self.was_minimized && self.minimize_to_tray {
            // 先取消最小化再隐藏，避免最小化状态残留。
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.window_visible = false;
        }
        self.was_minimized = minimized;

        // 后台线程消息处理（忙碌/部署/启动等状态更新）。
        self.handle_messages();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // 非渲染逻辑（托盘 / Steam / 最小化 / 消息）已迁至 App::logic：
        // 窗口最小化或隐藏时 eframe 不调用 ui()，只调用 logic()。

        // eframe 0.36：root Ui 无背景色，须用 CentralPanel 填充整个窗口并绘制背景。
        let mut content_h = 0.0f32;
        egui::CentralPanel::default().show(ui, |ui| {
            self.top_bar(ui);
            self.card1(ui);
            self.patch_console(ui);
            self.status_bar(ui);

            // 用布局游标测内容底部（min_rect 被 CentralPanel 撑满，不可用）。
            content_h = ui.cursor().top();
        });

        // 首帧按内容高度自适应窗口（消除底部大留白），只设置一次。
        if !self.autosized && content_h > 0.0 {
            self.autosized = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                ui.available_width().max(620.0),
                content_h + 36.0,
            )));
        }

        // 「关闭 Steam 并继续」确认弹窗。
        if let Some(action) = self.confirm {
            let mut confirmed = false;
            let mut cancelled = false;
            egui::Modal::new(egui::Id::new("confirm_close_steam")).show(&ctx, |ui| {
                ui.set_width(320.0);
                ui.heading(self.strings.confirm_title);
                ui.add_space(8.0);
                ui.label(self.strings.confirm_close_steam);
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if styled_button(ui, self.strings.yes, ButtonStyle::Primary, egui::vec2(72.0, 30.0), true).clicked()
                    {
                        confirmed = true;
                    }
                    ui.add_space(4.0);
                    if styled_button(ui, self.strings.no, ButtonStyle::Secondary, egui::vec2(72.0, 30.0), true).clicked()
                    {
                        cancelled = true;
                    }
                });
            });
            if confirmed {
                self.start_action(&ctx, action, true);
            } else if cancelled {
                self.confirm = None;
            }
        }

        // 设置对话框（PR-1：TOML 配置编辑器）。
        self.settings_dialog(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认 egui 字体不含 CJK 字形；注册 cjk-fallback 后应能显示中文字符。
    #[test]
    fn cjk_fallback_enables_chinese_glyph() {
        let Some(font_data) = read_system_cjk_font() else {
            // 无系统字体的 CI 环境跳过（Windows 目标永不触发）。
            return;
        };

        let mut defs = egui::FontDefinitions::default();
        defs.font_data
            .insert("cjk-test".into(), std::sync::Arc::new(font_data));
        defs.families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("cjk-test".into());
        defs.families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("cjk-test".into());

        let mut fonts =
            egui::epaint::text::Fonts::new(egui::epaint::text::TextOptions::default(), defs);
        assert!(fonts.has_glyph(&egui::FontId::proportional(14.0), 'X')); // latin sanity
        assert!(fonts.has_glyph(&egui::FontId::proportional(14.0), '中'));
    }

    /// 未注册任何 CJK 字体时，默认字体确实无中文字形（红/绿判别用）。
    #[test]
    fn default_fonts_lack_cjk_glyph() {
        let defs = egui::FontDefinitions::default();
        let mut fonts =
            egui::epaint::text::Fonts::new(egui::epaint::text::TextOptions::default(), defs);
        assert!(!fonts.has_glyph(&egui::FontId::proportional(14.0), '中'));
    }

    /// 自动隐身策略（ADR-0001）：仅在与当前显隐相反时动作。
    #[test]
    fn auto_tray_policy_table() {
        assert_eq!(auto_tray_policy(SteamEvent::Started, true), Some(false));
        assert_eq!(auto_tray_policy(SteamEvent::Stopped, false), Some(true));
        assert_eq!(auto_tray_policy(SteamEvent::Started, false), None);
        assert_eq!(auto_tray_policy(SteamEvent::Stopped, true), None);
    }

    /// 检查更新成功且本地已是最新 → 底部提示应显示「已是最新」，而非「发现可更新版本」。
    #[test]
    fn update_checked_notice_shows_up_to_date_when_local_matches() {
        let zh = Strings::new(Lang::Zh);
        let info = OnlineInfo {
            version: "1.4.8".into(),
            zip_url: "https://x/z.zip".into(),
        };
        let notice = Notice::UpdateChecked(Ok(info));
        // 本地版本与线上一致 → up_to_date；不一致 → new_version。
        let (ok, text) = render_notice(&zh, Some("1.4.8"), &notice);
        assert!(ok);
        assert_eq!(
            text, "v1.4.8 (本地已是最新版)",
            "已最新不应显示「发现可更新版本」: {text}"
        );
        let (_, text) = render_notice(&zh, Some("1.4.7"), &notice);
        assert_eq!(text, "v1.4.8 (发现可更新版本)");
    }

    /// 切换语言后，同一 notice 重新渲染即得新语言文案（无需重建 notice）。
    #[test]
    fn update_checked_notice_follows_language_switch() {
        let info = OnlineInfo {
            version: "1.4.8".into(),
            zip_url: "https://x/z.zip".into(),
        };
        let notice = Notice::UpdateChecked(Ok(info));
        let zh = render_notice(&Strings::new(Lang::Zh), Some("1.4.8"), &notice);
        let en = render_notice(&Strings::new(Lang::En), Some("1.4.8"), &notice);
        assert_eq!(zh, (true, "v1.4.8 (本地已是最新版)".to_string()));
        assert_eq!(en, (true, "v1.4.8 (Up to date)".to_string()));
        // 英文界面不应出现中文。
        assert!(!en.1.contains('本'), "en notice 不应含中文: {}", en.1);
    }

    /// 其余 notice 分支（下载/工作流/precheck）跨语言映射一致。
    #[test]
    fn render_notice_other_branches_both_langs() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            let e = updater::UpdateError::Network("t".into());
            assert_eq!(
                render_notice(&s, None, &Notice::UpdateChecked(Err(e.clone()))),
                (false, s.update_error(&e))
            );
            let wf = workflow::WorkflowError {
                op: workflow::Op::Launch,
                message: "m".into(),
            };
            assert_eq!(
                render_notice(&s, None, &Notice::WorkflowDone(workflow::Action::Launch, Ok(()))),
                (true, s.success_text(workflow::Action::Launch).to_string())
            );
            assert_eq!(
                render_notice(&s, None, &Notice::WorkflowDone(workflow::Action::Launch, Err(wf.clone()))),
                (false, s.workflow_error_text(&wf))
            );
            assert_eq!(
                render_notice(&s, None, &Notice::Precheck(workflow::Precheck::NoSteamDir)),
                (false, s.precheck_text(&workflow::Precheck::NoSteamDir))
            );
            assert_eq!(
                render_notice(&s, None, &Notice::Downloaded(Err(e.clone()))),
                (false, s.update_error(&e))
            );
            assert_eq!(
                render_notice(&s, None, &Notice::Downloaded(Ok(()))),
                (true, s.ok_downloaded.to_string())
            );
        }
    }

    /// 布局回归：两枚等宽按钮 + 手动 gap + 自动 item_spacing 必须恰好等于可用宽度，
    /// 不得溢出（历史 bug：溢出把下方 card3 顶到窗口右缘贴边）。
    #[test]
    fn twin_button_width_exactly_fills_row() {
        let gap = 12.0;
        for available in [500.0, 580.0, 620.0, 800.0, 1000.0] {
            for item_spacing in [6.0, 8.0, 10.0, 12.0] {
                let w = twin_button_width(available, gap, item_spacing);
                let total = w * 2.0 + gap + item_spacing;
                assert!(
                    (total - available).abs() < 0.01,
                    "available={available} gap={gap} spacing={item_spacing} -> w={w}, total={total} 应等于可用宽度"
                );
            }
        }
        // 极窄窗口：最小宽度兜底（max(150)），允许溢出避免按钮被压扁。
        assert_eq!(twin_button_width(200.0, 12.0, 10.0), 150.0);
    }

    /// 回归：英文长文案按钮（"Download & Extract New Version"）不能被固定宽度 150px 裁剪，
    /// 按钮应按文本宽度自适应（历史 bug：首尾字符 "D"/"ion" 被裁出边界）。
    #[test]
    fn button_width_expands_for_long_english_text() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 520.0),
            )),
            ..Default::default()
        };
        let mut out = None;
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let short =
                    styled_button(ui, "检查更新", ButtonStyle::Secondary, egui::vec2(96.0, 32.0), true).rect.width();
                let long = styled_button(
                    ui,
                    "Download & Extract New Version",
                    ButtonStyle::Primary,
                    egui::vec2(150.0, 32.0),
                    true,
                )
                .rect
                .width();
                let short_en =
                    styled_button(ui, "Check Update", ButtonStyle::Secondary, egui::vec2(96.0, 32.0), true).rect.width();
                out = Some((short, long, short_en));
            });
        });
        full.textures_delta.clear();
        let (short, long, short_en) = out.unwrap();
        // 短中文文案：保持固定宽度。
        assert_eq!(short, 96.0, "检查更新 在 96px 内放下即不撑宽");
        // 英文文案超宽时按钮自适应撑宽，避免字符被裁（"Check Update" 亦曾吃满 96px）。
        assert!(
            short_en > 96.0,
            "Check Update 超出 96px 时应撑宽按钮，实际 {short_en}px"
        );
        assert!(
            long > short_en,
            "Download & Extract New Version 应比 Check Update 更宽，实际 {long}px"
        );
    }

    // ---- 顶栏（T4 #10）----

    /// 测试用分派器：记录调起的 URL，不真正拉起浏览器。
    #[derive(Default)]
    struct RecordingDispatcher {
        urls: std::sync::Mutex<Vec<String>>,
    }

    impl RecordingDispatcher {
        fn urls(&self) -> Vec<String> {
            self.urls.lock().unwrap().clone()
        }
    }

    impl external::ExternalSystemDispatcher for RecordingDispatcher {
        fn open_browser_url(&self, url: &str) -> Result<(), String> {
            self.urls.lock().unwrap().push(url.to_owned());
            Ok(())
        }
    }

    fn headless_raw() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 520.0),
            )),
            ..Default::default()
        }
    }

    /// 向 raw input 注入一次主键点击（按下+抬起同帧；点击命中基于上一帧 widget 矩形）。
    fn click_at(raw: &mut egui::RawInput, pos: egui::Pos2) {
        let modifiers = egui::Modifiers::default();
        raw.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        raw.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
    }

    /// 无头渲染顶栏一帧（不点击），返回 (github, settings) 按钮响应。
    fn render_top_bar_headless(
        ctx: &egui::Context,
        raw: &egui::RawInput,
        strings: &Strings,
        mock: &RecordingDispatcher,
    ) -> (egui::Response, egui::Response) {
        let mut out = None;
        let mut full = ctx.run_ui(raw.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut open_settings = false;
                out = Some(render_top_bar(ui, strings, mock, &mut open_settings));
                assert!(!open_settings, "无点击帧不应产生设置意图");
            });
        });
        full.textures_delta.clear();
        out.unwrap()
    }

    /// AC1：设置按钮恒为 28×28 正方形；中英文与重绘帧之间位置尺寸零抖动。
    #[test]
    fn settings_button_fixed_28x28_no_layout_jitter() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let mock = RecordingDispatcher::default();

        let (github_zh, settings_zh) =
            render_top_bar_headless(&ctx, &raw, &Strings::new(Lang::Zh), &mock);
        let (github_en, settings_en) =
            render_top_bar_headless(&ctx, &raw, &Strings::new(Lang::En), &mock);
        let (_, settings_re) = render_top_bar_headless(&ctx, &raw, &Strings::new(Lang::Zh), &mock);

        // 设置按钮恒为 28×28 正方形。
        assert_eq!(settings_zh.rect.width(), 28.0, "设置按钮宽度应为 28");
        assert_eq!(settings_zh.rect.height(), 28.0, "设置按钮高度应为 28");
        // 语言切换（标题长度不同）与重绘均不引起抖动。
        assert_eq!(settings_en.rect, settings_zh.rect, "语言切换不得引起设置按钮位置抖动");
        assert_eq!(settings_re.rect, settings_zh.rect, "重绘帧不得引起设置按钮位置抖动");
        // 布局：设置位于最右，GitHub 在其左侧。
        assert!(
            settings_zh.rect.min.x >= github_zh.rect.max.x,
            "设置按钮应位于 GitHub 按钮右侧"
        );
        assert!(
            github_en.rect.width() > 28.0,
            "GitHub 文本按钮应自适应宽于方形图标"
        );
    }

    /// AC1：点击 GitHub 经分派器调起仓库 URL（mock 记录；不真正拉起浏览器）。
    #[test]
    fn github_button_click_dispatches_repo_url() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let mock = RecordingDispatcher::default();

        // 帧 1：定位 GitHub 按钮（同时注册 widget 矩形，供下一帧点击命中）。
        let (github, _) = render_top_bar_headless(&ctx, &raw, &Strings::new(Lang::Zh), &mock);
        assert!(mock.urls().is_empty(), "无点击不应触发分派");
        let github_rect = github.rect;

        // 帧 2：点击 GitHub → 经分派器调起仓库 URL。
        let mut click = raw.clone();
        click_at(&mut click, github_rect.center());
        let mut open_settings = false;
        let mut full = ctx.run_ui(click, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = render_top_bar(ui, &Strings::new(Lang::Zh), &mock, &mut open_settings);
            });
        });
        full.textures_delta.clear();
        assert_eq!(mock.urls(), vec![external::GITHUB_REPO_URL.to_string()]);
        assert!(!open_settings, "GitHub 点击不应触发设置");
    }

    /// AC1：点击 ⚙ 设置按钮置位打开意图（分派器不被调用）。
    #[test]
    fn settings_button_click_sets_open_intent() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let mock = RecordingDispatcher::default();

        let (_, settings) = render_top_bar_headless(&ctx, &raw, &Strings::new(Lang::Zh), &mock);
        let settings_rect = settings.rect;

        let mut click = raw.clone();
        click_at(&mut click, settings_rect.center());
        let mut open_settings = false;
        let mut full = ctx.run_ui(click, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = render_top_bar(ui, &Strings::new(Lang::Zh), &mock, &mut open_settings);
            });
        });
        full.textures_delta.clear();
        assert!(open_settings, "点击设置应置位打开意图");
        assert!(mock.urls().is_empty(), "设置点击不应触发浏览器分派");
    }

    /// AC1：设置按钮 ⚙ 字形必须可渲染（默认字体或 CJK 回退），否则顶栏出现豆腐块。
    #[test]
    fn gear_glyph_renderable_with_top_bar_fonts() {
        // 默认字体（内置 NotoEmoji 等）应含 U+2699。
        let defaults = egui::FontDefinitions::default();
        let mut fonts =
            egui::epaint::text::Fonts::new(egui::epaint::text::TextOptions::default(), defaults);
        assert!(
            fonts.has_glyph(&egui::FontId::proportional(13.0), '⚙'),
            "egui 默认字体应含 ⚙ 字形"
        );
        // 注册 CJK 回退后仍可渲染（Windows 目标顶栏实际字体链）。
        let Some(data) = read_system_cjk_font() else {
            return; // 无系统字体的 CI 环境跳过（Windows 目标永不触发）。
        };
        let mut defs = egui::FontDefinitions::default();
        defs.font_data
            .insert("cjk-test".into(), std::sync::Arc::new(data));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            defs.families.entry(family).or_default().push("cjk-test".into());
        }
        let mut fonts =
            egui::epaint::text::Fonts::new(egui::epaint::text::TextOptions::default(), defs);
        assert!(fonts.has_glyph(&egui::FontId::proportional(13.0), '⚙'));
    }

    /// AC1：顶栏词条——GitHub 双语同文案；设置按钮为 ⚙ 图标（固定方形前提）。
    #[test]
    fn top_bar_labels_github_and_gear_icon() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            assert_eq!(s.btn_github, "GitHub");
            assert_eq!(s.btn_settings, "⚙");
        }
    }

    // ---- 补丁控制台（T5 #12）----

    /// 无头渲染补丁控制台按钮行一帧（不点击），返回 (首按钮, 次按钮) 响应。
    fn render_console_buttons(
        ctx: &egui::Context,
        raw: &egui::RawInput,
        status: DeployStatus,
        steam_running: bool,
        enabled: bool,
    ) -> (egui::Response, egui::Response) {
        let mut out = None;
        let mut full = ctx.run_ui(raw.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut actions = PatchActions::default();
                out = Some(patch_console_buttons(
                    ui,
                    &Strings::new(Lang::Zh),
                    status,
                    steam_running,
                    enabled,
                    &mut actions,
                ));
                assert!(!actions.apply_and_launch
                    && !actions.launch
                    && !actions.exit_and_uninstall
                    && !actions.uninstall_and_restart,
                    "无点击帧不应产生操作意图"
                );
            });
        });
        full.textures_delta.clear();
        out.unwrap()
    }

    /// 点击给定按钮（帧 2），返回产生的操作意图。
    fn click_console_button(
        ctx: &egui::Context,
        raw: &egui::RawInput,
        status: DeployStatus,
        steam_running: bool,
        enabled: bool,
        btn: egui::Response,
    ) -> PatchActions {
        let mut actions = PatchActions::default();
        let mut click = raw.clone();
        click_at(&mut click, btn.rect.center());
        let mut full = ctx.run_ui(click, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = patch_console_buttons(
                    ui,
                    &Strings::new(Lang::Zh),
                    status,
                    steam_running,
                    enabled,
                    &mut actions,
                );
            });
        });
        full.textures_delta.clear();
        actions
    }

    /// AC2：三态按钮行为——未部署（应用+启动 / 正常启动）、已部署（退出卸载 / 卸载重启）、
    /// 路径无效（全禁用，点击无意图）。
    #[test]
    fn patch_console_three_states_button_actions() {
        let ctx = egui::Context::default();
        let raw = headless_raw();

        // 未部署：首按钮 → 应用补丁并启动；次按钮 → 正常启动。
        let (first, second) =
            render_console_buttons(&ctx, &raw, DeployStatus::NotDeployed, false, true);
        let a = click_console_button(&ctx, &raw, DeployStatus::NotDeployed, false, true, first);
        assert!(
            a.apply_and_launch && !a.launch,
            "未部署首按钮应触发应用补丁并启动"
        );
        let b = click_console_button(&ctx, &raw, DeployStatus::NotDeployed, false, true, second);
        assert!(
            b.launch && !b.apply_and_launch,
            "未部署次按钮应触发正常启动"
        );

        // 已部署：首按钮 → 退出并卸载；次按钮 → 卸载并重启。
        let (first, second) =
            render_console_buttons(&ctx, &raw, DeployStatus::Deployed, true, true);
        let a = click_console_button(&ctx, &raw, DeployStatus::Deployed, true, true, first);
        assert!(a.exit_and_uninstall && !a.uninstall_and_restart);
        let b = click_console_button(&ctx, &raw, DeployStatus::Deployed, true, true, second);
        assert!(b.uninstall_and_restart && !b.exit_and_uninstall);

        // 路径无效：全禁用，点击无意图。
        let (first, second) =
            render_console_buttons(&ctx, &raw, DeployStatus::InvalidPath, false, true);
        let a = click_console_button(&ctx, &raw, DeployStatus::InvalidPath, false, true, first);
        assert!(
            !a.apply_and_launch && !a.launch && !a.exit_and_uninstall && !a.uninstall_and_restart
        );
        let b = click_console_button(&ctx, &raw, DeployStatus::InvalidPath, false, true, second);
        assert!(
            !b.apply_and_launch && !b.launch && !b.exit_and_uninstall && !b.uninstall_and_restart
        );
    }

    /// AC2：部署状态徽章三态样式映射（图标/文案/前景/底色）。
    #[test]
    fn deploy_badge_styles_map_three_states() {
        let zh = Strings::new(Lang::Zh);
        let (icon, text, fg, bg) = deploy_badge_style(&zh, DeployStatus::Deployed);
        assert_eq!(icon, "✔");
        assert_eq!(text, zh.status_deployed);
        assert_eq!(fg, STATUS_INSTALLED);
        assert_eq!(bg, BADGE_GREEN);

        let (icon, text, fg, bg) = deploy_badge_style(&zh, DeployStatus::NotDeployed);
        assert_eq!(icon, "●");
        assert_eq!(text, zh.status_not_deployed);
        assert_eq!(fg, STATUS_WARN);
        assert_eq!(bg, BADGE_AMBER);

        let (icon, text, fg, bg) = deploy_badge_style(&zh, DeployStatus::InvalidPath);
        assert_eq!(icon, "?");
        assert_eq!(text, zh.status_invalid);
        assert_eq!(fg, TEXT_WEAK);
        assert_eq!(bg, BADGE_GRAY);
    }

    /// AC2：Steam 进程状态徽章（运行中 / 未运行）样式映射。
    #[test]
    fn steam_badge_styles_map_running() {
        let zh = Strings::new(Lang::Zh);
        let (icon, text, _, bg) = steam_badge_style(&zh, true);
        assert_eq!(icon, "●");
        assert_eq!(text, zh.status_steam_running);
        assert_eq!(bg, BADGE_GREEN);

        let (icon, text, _, bg) = steam_badge_style(&zh, false);
        assert_eq!(icon, "○");
        assert_eq!(text, zh.status_steam_not_running);
        assert_eq!(bg, BADGE_GRAY);
    }

    /// AC2：已部署态卸载按钮文案随 Steam 进程切换（运行中 → 退出并卸载；已退出 → 直接卸载）。
    #[test]
    fn uninstall_label_varies_with_steam_running() {
        let zh = Strings::new(Lang::Zh);
        assert_eq!(uninstall_label(&zh, true), zh.btn_exit_and_uninstall);
        assert_eq!(uninstall_label(&zh, false), zh.btn_uninstall);
    }

    /// AC2：三态徽章均可无头渲染（同一 pill 容器，无 panic、有图形产出）。
    #[test]
    fn patch_console_badges_render_all_states() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let zh = Strings::new(Lang::Zh);
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for status in [
                    DeployStatus::Deployed,
                    DeployStatus::NotDeployed,
                    DeployStatus::InvalidPath,
                ] {
                    let (icon, text, fg, bg) = deploy_badge_style(&zh, status);
                    pill_badge(ui, icon, text, fg, bg);
                    let (icon, text, fg, bg) = steam_badge_style(&zh, false);
                    pill_badge(ui, icon, text, fg, bg);
                }
            });
        });
        full.textures_delta.clear();
        assert!(!full.shapes.is_empty(), "三态徽章渲染无图形产出");
    }

    // ---- 底部状态栏（T6 #13）----

    fn update_info(version: &str) -> OnlineInfo {
        OnlineInfo {
            version: version.to_string(),
            zip_url: "https://x/z.zip".into(),
        }
    }

    /// 构造状态栏渲染视图（测试样板）。
    fn status_view<'a>(
        strings: &'a Strings,
        local_version: Option<&'a str>,
        all_local_exist: bool,
        update_state: &'a UpdateState,
        busy: bool,
        busy_kind: Option<BusyKind>,
        notice: Option<&'a Notice>,
    ) -> StatusBarView<'a> {
        StatusBarView {
            strings,
            local_version,
            all_local_exist,
            update_state,
            busy,
            busy_kind,
            notice,
        }
    }

    /// 无头渲染状态栏右侧一帧（不点击），返回 (版本 label, 按钮) 响应。
    fn render_status_right(
        ctx: &egui::Context,
        raw: &egui::RawInput,
        view: &StatusBarView<'_>,
    ) -> (egui::Response, egui::Response) {
        let mut out = None;
        let mut full = ctx.run_ui(raw.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut actions = StatusBarActions::default();
                out = Some(status_bar_right(ui, view, &mut actions));
                assert!(
                    !actions.check && actions.download.is_none(),
                    "无点击帧不应产生更新意图"
                );
            });
        });
        full.textures_delta.clear();
        out.unwrap()
    }

    /// 点击状态栏按钮（帧 2），返回产生的更新意图。
    fn click_status_button(
        ctx: &egui::Context,
        raw: &egui::RawInput,
        view: &StatusBarView<'_>,
        btn: egui::Response,
    ) -> StatusBarActions {
        let mut actions = StatusBarActions::default();
        let mut click = raw.clone();
        click_at(&mut click, btn.rect.center());
        let mut full = ctx.run_ui(click, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = status_bar_right(ui, view, &mut actions);
            });
        });
        full.textures_delta.clear();
        actions
    }

    /// AC3：各更新态按钮行为——检查更新 / 立即更新 / busy 禁用 / 下载中禁用。
    #[test]
    fn status_bar_update_state_buttons() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let zh = Strings::new(Lang::Zh);

        // Idle：检查更新。
        let idle = UpdateState::Idle;
        let view = status_view(&zh, Some("1.4.8"), true, &idle, false, None, None);
        let (_, btn) = render_status_right(&ctx, &raw, &view);
        let a = click_status_button(&ctx, &raw, &view, btn);
        assert!(a.check && a.download.is_none());

        // 已检查且本地最新（同版本）：仍为检查更新。
        let state = UpdateState::Checked(Ok(update_info("1.4.8")));
        let view = status_view(&zh, Some("1.4.8"), true, &state, false, None, None);
        let (_, btn) = render_status_right(&ctx, &raw, &view);
        let a = click_status_button(&ctx, &raw, &view, btn);
        assert!(a.check && a.download.is_none(), "同版本应保持检查更新");

        // 已检查且可更新（线上更新）：立即更新。
        let state = UpdateState::Checked(Ok(update_info("1.4.9")));
        let view = status_view(&zh, Some("1.4.8"), true, &state, false, None, None);
        let (_, btn) = render_status_right(&ctx, &raw, &view);
        let a = click_status_button(&ctx, &raw, &view, btn);
        assert!(!a.check && a.download.is_some(), "可更新应触发立即更新");
        assert_eq!(a.download.unwrap().version, "1.4.9");

        // 检查失败：仍为检查更新。
        let state = UpdateState::Checked(Err(UpdateError::Network("t".into())));
        let view = status_view(&zh, Some("1.4.8"), true, &state, false, None, None);
        let (_, btn) = render_status_right(&ctx, &raw, &view);
        let a = click_status_button(&ctx, &raw, &view, btn);
        assert!(a.check && a.download.is_none());

        // busy（如检查中）：按钮禁用，点击无意图。
        let checking = UpdateState::Checking;
        let view = status_view(
            &zh,
            Some("1.4.8"),
            true,
            &checking,
            true,
            Some(BusyKind::Checking),
            None,
        );
        let (_, btn) = render_status_right(&ctx, &raw, &view);
        let a = click_status_button(&ctx, &raw, &view, btn);
        assert!(!a.check && a.download.is_none(), "busy 时按钮应禁用");

        // 下载中：立即更新按钮原地进入禁用态，点击无意图。
        let state = UpdateState::Checked(Ok(update_info("1.4.9")));
        let view = status_view(
            &zh,
            Some("1.4.8"),
            true,
            &state,
            true,
            Some(BusyKind::Downloading),
            None,
        );
        let (_, btn) = render_status_right(&ctx, &raw, &view);
        let a = click_status_button(&ctx, &raw, &view, btn);
        assert!(!a.check && a.download.is_none(), "下载中按钮应禁用");
    }

    /// AC3：本地版本展示文本三态（有版本 / 已就绪未记录 / 缺失 DLL）。
    #[test]
    fn local_version_text_variants() {
        let zh = Strings::new(Lang::Zh);
        let (text, _) = local_version_text(&zh, Some("1.4.8"), true);
        assert_eq!(text, format!("{}v1.4.8", zh.local_version));
        let (text, _) = local_version_text(&zh, None, true);
        assert_eq!(
            text,
            format!("{}{}", zh.local_version, zh.local_ver_ready_no_record)
        );
        let (text, _) = local_version_text(&zh, None, false);
        assert_eq!(
            text,
            format!("{}{}", zh.local_version, zh.local_ver_missing)
        );
    }

    /// AC3：左侧提示——busy 显示忙碌文案；结果显示最近结果；均无 → 不渲染。
    #[test]
    fn status_bar_notice_renders_busy_or_result() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let zh = Strings::new(Lang::Zh);

        // busy：忙碌文案。
        let downloading = BusyKind::Downloading;
        let idle = UpdateState::Idle;
        let view = status_view(
            &zh,
            Some("1.4.8"),
            true,
            &idle,
            true,
            Some(downloading),
            None,
        );
        let mut text = None;
        let mut full = ctx.run_ui(raw.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                text = status_bar_notice(ui, &view);
            });
        });
        full.textures_delta.clear();
        assert_eq!(text.as_deref(), Some(zh.busy_downloading));

        // 结果：最近一次结果。
        let notice = Notice::WorkflowDone(Action::Launch, Ok(()));
        let idle = UpdateState::Idle;
        let view = status_view(&zh, Some("1.4.8"), true, &idle, false, None, Some(&notice));
        let mut text = None;
        let mut full = ctx.run_ui(raw.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                text = status_bar_notice(ui, &view);
            });
        });
        full.textures_delta.clear();
        assert_eq!(text.as_deref(), Some(zh.ok_launched));

        // 均无 → None。
        let idle = UpdateState::Idle;
        let view = status_view(&zh, Some("1.4.8"), true, &idle, false, None, None);
        let mut text = Some("x".to_string());
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                text = status_bar_notice(ui, &view);
            });
        });
        full.textures_delta.clear();
        assert!(text.is_none());
    }
    // ---- Steam 核心兼容性（T5）----

    /// 构造探针报告样本（cache_path 占位，summary 判定不依赖它）。
    fn probe_report(target: compat::ProbeTarget, status: compat::ProbeStatus, sha: Option<&str>) -> compat::ProbeReport {
        compat::ProbeReport {
            target,
            sha256: sha.map(String::from),
            status,
            cache_path: PathBuf::from("F:/Steam/opensteamtool"),
        }
    }

    fn report_with(
        statuses: [compat::ProbeStatus; 3],
        has_missing_cache: bool,
    ) -> compat::OverallHealthReport {
        compat::OverallHealthReport {
            steamclient_pattern: probe_report(
                compat::ProbeTarget::PatternSteamClient,
                statuses[0].clone(),
                Some("abc"),
            ),
            steamui_pattern: probe_report(
                compat::ProbeTarget::PatternSteamUi,
                statuses[1].clone(),
                Some("abc"),
            ),
            steamclient_ipc: probe_report(
                compat::ProbeTarget::IpcSteamClient,
                statuses[2].clone(),
                Some("abc"),
            ),
            is_all_compatible: !has_missing_cache,
            has_missing_cache,
        }
    }

    use compat::ProbeStatus as S;

    /// 检查中 / 无报告 → Checking（骨架态）。
    #[test]
    fn compat_summary_checking_when_in_progress() {
        assert_eq!(compat_summary(true, None), CompatSummary::Checking);
        assert_eq!(compat_summary(false, None), CompatSummary::Checking);
    }

    /// 任一 DLL 缺失 → Missing（最高优先级）。
    #[test]
    fn compat_summary_missing_when_dll_absent() {
        let r = report_with(
            [
                S::FileNotFound,
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
            ],
            false,
        );
        assert_eq!(compat_summary(false, Some(&r)), CompatSummary::Missing);
    }

    /// 上游未适配 → Pending（优先于 Network/Online）。
    #[test]
    fn compat_summary_pending_beats_network_and_online() {
        let r = report_with(
            [
                S::IncompatiblePending,
                S::NetworkError("x".into()),
                S::RemoteAvailable { cached: false },
            ],
            true,
        );
        assert_eq!(compat_summary(false, Some(&r)), CompatSummary::Pending);
    }

    /// 网络错误（无 Pending）→ Network。
    #[test]
    fn compat_summary_network_when_unreachable() {
        let r = report_with(
            [
                S::NetworkError("timeout".into()),
                S::CompatibleOffline,
                S::RemoteAvailable { cached: true },
            ],
            false,
        );
        assert_eq!(compat_summary(false, Some(&r)), CompatSummary::Network);
    }

    /// 存在未缓存项 → Online（提示预热）。
    #[test]
    fn compat_summary_online_when_missing_cache() {
        let r = report_with(
            [
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
            ],
            true,
        );
        assert_eq!(compat_summary(false, Some(&r)), CompatSummary::Online);
    }

    /// 全缓存就绪（在线或离线）→ Ready。
    #[test]
    fn compat_summary_ready_when_all_cached() {
        let online = report_with(
            [
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
            ],
            false,
        );
        assert_eq!(compat_summary(false, Some(&online)), CompatSummary::Ready);
        let offline = report_with(
            [
                S::CompatibleOffline,
                S::CompatibleOffline,
                S::CompatibleOffline,
            ],
            false,
        );
        assert_eq!(compat_summary(false, Some(&offline)), CompatSummary::Ready);
    }

    /// 待预热目标：收集「已适配但签名缓存缺失」的项（以 cache_path 存在性为准）——
    /// cached:false（未缓存）+ RemoteAvailable{cached:true}（验证缓存命中但无签名缓存）都收。
    #[test]
    fn precache_targets_picks_available_without_signature() {
        let r = report_with(
            [
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: true },
                S::IncompatiblePending,
            ],
            true,
        );
        let targets = precache_targets(&r);
        // report_with 的 cache_path 均为不存在的假路径 → cached:false 与验证命中的 cached:true 都被收。
        assert_eq!(targets.len(), 2);
        let ts: Vec<_> = targets.iter().map(|(t, _)| *t).collect();
        assert!(ts.contains(&compat::ProbeTarget::PatternSteamClient));
        assert!(ts.contains(&compat::ProbeTarget::PatternSteamUi));
    }

    /// 已存在签名缓存文件的项（cache_path.is_file()）不被收集。
    #[test]
    fn precache_targets_excludes_existing_signature() {
        let dir = std::env::temp_dir().join(format!("ost_ui_sig_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut r = report_with(
            [
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
                S::IncompatiblePending,
            ],
            true,
        );
        // 给 PatternSteamClient 补一个真实存在的缓存文件 → 排除；其余 cache_path 假路径仍收。
        let path = crate::compat::cache_path(&dir, compat::ProbeTarget::PatternSteamClient, "abc");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"[x]").unwrap();
        r.steamclient_pattern.cache_path = path;
        let targets = precache_targets(&r);
        // PatternSteamClient 已补真实缓存文件 → 唯一排除；steamui（假路径）仍收；ipc 非 RemoteAvailable 不收。
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, compat::ProbeTarget::PatternSteamUi);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 快速体检后：存在短路项（CompatibleOffline）需后台刷新；其余情况不需要。
    #[test]
    fn compat_needs_network_refresh_detects_shortcut() {
        let with_offline = report_with(
            [
                S::CompatibleOffline,
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
            ],
            false,
        );
        assert!(compat_needs_network_refresh(&with_offline));
        let optimistic = report_with(
            [
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: false },
            ],
            true,
        );
        assert!(compat_needs_network_refresh(&optimistic));
        let all_confirmed = report_with(
            [
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
            ],
            false,
        );
        assert!(!compat_needs_network_refresh(&all_confirmed));
        let mixed = report_with(
            [
                S::FileNotFound,
                S::IncompatiblePending,
                S::NetworkError("x".into()),
            ],
            false,
        );
        assert!(!compat_needs_network_refresh(&mixed));
    }

    /// 自动预热判定：仅 Online 态（上游已适配未缓存）且无预热进行中才触发；
    /// Ready/Pending/Missing/Network 与进行中均不触发。
    #[test]
    fn should_auto_precache_only_fires_on_online() {
        let online = report_with(
            [
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: false },
            ],
            true,
        );
        assert!(should_auto_precache(&online, false));
        // 预热进行中不再触发（防重复，手动按钮 disabled 已覆盖）。
        assert!(!should_auto_precache(&online, true));
        let ready = report_with(
            [
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
            ],
            false,
        );
        assert!(!should_auto_precache(&ready, false));
        let pending = report_with(
            [
                S::IncompatiblePending,
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: false },
            ],
            true,
        );
        assert!(!should_auto_precache(&pending, false));
        let missing = report_with(
            [
                S::FileNotFound,
                S::FileNotFound,
                S::FileNotFound,
            ],
            false,
        );
        assert!(!should_auto_precache(&missing, false));
        let network_err = report_with(
            [
                S::NetworkError("x".into()),
                S::NetworkError("x".into()),
                S::NetworkError("x".into()),
            ],
            false,
        );
        assert!(!should_auto_precache(&network_err, false));
    }

    #[test]
    fn footer_layout_matches_spec_ac5() {
        // General：仅右[关闭]。
        let (l, r) = footer_layout(SettingsTab::General);
        assert!(l.is_empty());
        assert_eq!(r, &[FooterAction::Close]);
        // ConfigEditor：左[模板][撤销]，右区 right_to_left 数组 [Close, Save]（显示：保存 关闭）。
        let (l, r) = footer_layout(SettingsTab::ConfigEditor);
        assert_eq!(l, &[FooterAction::LoadTemplate, FooterAction::Undo]);
        assert_eq!(r, &[FooterAction::Close, FooterAction::Save]);
        // OnlineFix：左[复制][启用][停用]，右[关闭]。
        let (l, r) = footer_layout(SettingsTab::OnlineFix);
        assert_eq!(l, &[FooterAction::Copy, FooterAction::Enable, FooterAction::Disable]);
        assert_eq!(r, &[FooterAction::Close]);
    }

    #[test]
    fn footer_renders_all_tabs_without_spurious_actions() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 520.0),
            )),
            ..Default::default()
        };
        let mut rendered = 0u32;
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for tab in [
                    SettingsTab::General,
                    SettingsTab::ConfigEditor,
                    SettingsTab::OnlineFix,
                ] {
                    let (left, right) = footer_layout(tab);
                    let mut actions = SettingsActions::default();
                    ui.horizontal(|ui| {
                        for action in left {
                            render_footer_button(ui, &Strings::new(Lang::Zh), *action, &mut actions);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for action in right {
                                render_footer_button(ui, &Strings::new(Lang::Zh), *action, &mut actions);
                            }
                        });
                    });
                    ui.add_space(4.0);
                    // 无点击输入：所有意图必须保持 false。
                    assert!(
                        !actions.close
                            && !actions.save
                            && !actions.undo
                            && !actions.load_template
                            && !actions.copy
                            && !actions.enable
                            && !actions.disable,
                        "tab {tab:?} footer 无点击却产生意图"
                    );
                    rendered += 1;
                }
            });
        });
        full.textures_delta.clear();
        // 三个页签均完成渲染且有图形产出（按钮已绘制）。
        assert_eq!(rendered, 3);
        assert!(!full.shapes.is_empty(), "footer 渲染无图形产出");
    }

    // ---- T7：生效游戏看板 + 候选胶囊（#11） ----

    /// 无头渲染看板一帧（不点击），返回 (是否停用, 停用按钮)。
    fn render_active_board_headless(
        ctx: &egui::Context,
        raw: &egui::RawInput,
        strings: &Strings,
        active: &onlinefix::ActiveOnlineFixGame,
    ) -> (bool, egui::Response) {
        let mut out = None;
        let mut full = ctx.run_ui(raw.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                out = Some(render_active_board(ui, strings, active));
            });
        });
        full.textures_delta.clear();
        let (deact, btn) = out.unwrap();
        assert!(!deact, "无点击帧不应产生停用意图");
        (deact, btn)
    }

    /// 点击看板停用按钮（帧 2），返回是否产生停用意图。
    fn click_active_board_deactivate(
        ctx: &egui::Context,
        raw: &egui::RawInput,
        strings: &Strings,
        active: &onlinefix::ActiveOnlineFixGame,
        btn: egui::Response,
    ) -> bool {
        let mut click = raw.clone();
        click_at(&mut click, btn.rect.center());
        let mut out = None;
        let mut full = ctx.run_ui(click, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                out = Some(render_active_board(ui, strings, active).0);
            });
        });
        full.textures_delta.clear();
        out.unwrap()
    }

    #[test]
    fn active_board_deactivate_click_sets_disable_intent() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let zh = Strings::new(Lang::Zh);
        let active = onlinefix::ActiveOnlineFixGame {
            appid: 1361510,
            name: Some("双人成行".to_owned()),
        };
        // 帧 1：渲染定位按钮（无点击无意图）。
        let (deact, btn) = render_active_board_headless(&ctx, &raw, &zh, &active);
        assert!(!deact);
        assert!(btn.rect.width() > 0.0, "停用按钮应可点");
        // 帧 2：点击停用 → 意图置位（与 Footer 停用同指一操作）。
        assert!(
            click_active_board_deactivate(&ctx, &raw, &zh, &active, btn),
            "点击停用应产生停用意图"
        );
    }

    #[test]
    fn active_board_falls_back_to_plain_appid_without_name() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let zh = Strings::new(Lang::Zh);
        let active = onlinefix::ActiveOnlineFixGame {
            appid: 1812150,
            name: None,
        };
        // ACF 缺失 → 看板回退纯数字（无 panic、有图形产出）。
        let (_, btn) = render_active_board_headless(&ctx, &raw, &zh, &active);
        assert!(btn.rect.width() > 0.0);
    }

    #[test]
    fn candidate_capsules_show_name_and_click_fills_appid() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let candidates = vec![
            onlinefix::CandidateGame {
                appid: 367520,
                name: Some("空洞骑士".to_owned()),
            },
            onlinefix::CandidateGame {
                appid: 480,
                name: None,
            },
        ];
        // 帧 1：渲染（无点击无选择）。
        let mut out = None;
        let mut full = ctx.run_ui(raw.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                out = Some(render_candidate_capsules(ui, &candidates));
            });
        });
        full.textures_delta.clear();
        let (picked, buttons) = out.unwrap();
        assert!(picked.is_none());
        assert_eq!(buttons.len(), 2);
        // 帧 2：点击「名称 (appid)」胶囊 → 返回对应 AppID（填充输入框用）。
        let mut click = raw.clone();
        click_at(&mut click, buttons[0].rect.center());
        let mut picked = None;
        let mut full = ctx.run_ui(click, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                picked = render_candidate_capsules(ui, &candidates).0;
            });
        });
        full.textures_delta.clear();
        assert_eq!(picked, Some(367520));
    }

    #[test]
    fn candidate_capsule_plain_appid_click_fills_numeric() {
        let ctx = egui::Context::default();
        let raw = headless_raw();
        let candidates = vec![onlinefix::CandidateGame {
            appid: 480,
            name: None,
        }];
        // 帧 1 定位纯数字胶囊。
        let mut out = None;
        let mut full = ctx.run_ui(raw.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                out = Some(render_candidate_capsules(ui, &candidates));
            });
        });
        full.textures_delta.clear();
        let (_, buttons) = out.unwrap();
        // 帧 2：点击纯数字胶囊 → 返回对应 AppID。
        let mut click = raw.clone();
        click_at(&mut click, buttons[0].rect.center());
        let mut picked = None;
        let mut full = ctx.run_ui(click, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                picked = render_candidate_capsules(ui, &candidates).0;
            });
        });
        full.textures_delta.clear();
        assert_eq!(picked, Some(480));
    }
}
