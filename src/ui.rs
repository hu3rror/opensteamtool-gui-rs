//! egui 界面：主页面（部署状态 + 操作按钮组 + 健康风险警示）+ 设置对话框 + 向导 + 确认弹窗。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use egui::Frame;

use crate::brand;
use crate::busy::{BusyGate, BusyKind};
use crate::compat;
use crate::compat_flow::{self, CompatFlow, CompatSummary};
use crate::config::{self, Config, Language, ThemePreference};
use crate::dll::{self, DeployStatus};
use crate::i18n::{Lang, Strings};
use crate::main_page::{
    self, IconKind, MainPageVm, PrimaryAction, PrimaryKind, SecondaryAction, UpdateConclusion,
    UpdateKind,
};
use crate::process::{self, SteamEvent, SteamMonitor};
use crate::singleton::Singleton;
use crate::steam;
use crate::steam_state::SteamState;
use crate::theme::{self, ButtonPalette, ButtonStyle, Palette};
use crate::tray::{Tray, TrayAction};
use crate::update_flow::UpdateFlow;
use crate::updater::{self, OnlineInfo, UpdateError};
use crate::wizard::{self, Step as WizardStep};
use crate::workflow::{self, Action};

// 颜色一律取自 theme.rs 语义色板（ADR-0010）：仓库唯一色值来源，勿在此处写内联色值。

const WIZARD_CARD_WIDTH: f32 = 430.0;

const SETTINGS_DIALOG_WIDTH: f32 = 580.0;

/// 设置对话框固定骨架高（heading + 页签 + 分割线 + 页脚；含 24/20 frame margin，
/// 旧 6/6 menu_margin 时代为 208，settings_dialog_frame 内边距上调后 +28）。
const SETTINGS_DIALOG_SKELETON_H: f32 = 236.0;
const SETTINGS_SCROLL_MIN_H: f32 = 200.0;

/// 主页面内容列最大宽度（spec §40 Tunable 基线：Hero max-width 600 lp）。
const MAIN_COLUMN_WIDTH: f32 = 600.0;

/// 圆角层级（spec §38 基线：Hero 18 / CTA 10 / Row 10 / Small 7 lp）。
const R_HERO: u8 = 18;
const R_CTA: u8 = 10;
const R_ROW: u8 = 10;
const R_SMALL: u8 = 7;

fn settings_scroll_height(window_inner_h: f32) -> f32 {
    (window_inner_h - SETTINGS_DIALOG_SKELETON_H).clamp(SETTINGS_SCROLL_MIN_H, 420.0)
}

fn autosize_inner_height(content_h: f32) -> f32 {
    (content_h + 36.0).max(SETTINGS_DIALOG_SKELETON_H + SETTINGS_SCROLL_MIN_H)
}

/// 装配深浅两套 egui Style（spec §48.2 / ADR-0016）：dark/light 各一套语义槽覆盖的 visuals，
/// 主题切换与弹出层恒用装配值、不落回 egui 默认；spacing 与主题无关，统一设置。
fn install_theme(ctx: &egui::Context) {
    let dark = build_visuals(Palette::dark());
    let light = build_visuals(Palette::light());
    ctx.options_mut(|o| {
        use std::sync::Arc;
        let mut s = (*o.dark_style).clone();
        s.visuals = dark;
        o.dark_style = Arc::new(s);
        let mut s = (*o.light_style).clone();
        s.visuals = light;
        o.light_style = Arc::new(s);
    });
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.window_margin = egui::Margin::symmetric(16, 18);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
    });
}

/// 由 palette 装配 egui 控件层 visuals（基线 Visuals::dark/light + 语义槽覆盖）。
fn build_visuals(p: Palette) -> egui::Visuals {
    let mut visuals = if p.dark_mode {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = p.panel;
    visuals.window_fill = p.panel;
    visuals.faint_bg_color = p.panel;
    visuals.extreme_bg_color = p.entry; // TextEdit 底（控件层次槽）
    visuals.widgets.inactive.weak_bg_fill = p.entry;
    visuals.widgets.open.weak_bg_fill = p.entry;
    visuals.override_text_color = Some(p.ink);
    let radius = egui::CornerRadius::same(8);
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        w.corner_radius = radius;
    }
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.entry);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, p.entry);
    visuals.widgets.inactive.bg_fill = p.card;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, p.accent);
    visuals.widgets.hovered.bg_fill = p.entry;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, p.accent_hover);
    visuals.selection.bg_fill = p.selection_bg();
    visuals.selection.stroke = egui::Stroke::new(1.0, p.accent);
    visuals
}

/// 设置对话框 Modal frame（生产与测试共用：内边距 24/20 + 1px border；测试布局须与生产同源）。
fn settings_dialog_frame(palette: Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.panel)
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(24, 20))
}

fn card_frame(palette: Palette) -> Frame {
    Frame::new()
        .fill(palette.card)
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(18, 16))
}

fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId, color: egui::Color32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font.clone(), color)
        .size()
        .x
}

fn styled_button(
    ui: &mut egui::Ui,
    text: &str,
    style: ButtonStyle,
    size: egui::Vec2,
    enabled: bool,
    palette: Palette,
) -> egui::Response {
    let font_size = if size.y >= 40.0 { 14.0 } else { 13.0 };
    let button = style.palette(palette);
    let font_id = egui::FontId::proportional(font_size);
    let text_w = text_width(ui, text, &font_id, button.fg);
    let size = egui::vec2(size.x.max(text_w + 28.0), size.y);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        paint_button_chrome(ui, rect, enabled && response.hovered(), button);
        let color = if enabled { button.fg } else { palette.weak };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(font_size),
            color,
        );
    }
    response
}

fn paint_button_chrome(ui: &mut egui::Ui, rect: egui::Rect, hovered: bool, palette: ButtonPalette) {
    let fill = if hovered { palette.hover } else { palette.bg };
    let stroke = palette
        .border
        .map_or(egui::Stroke::NONE, |c| egui::Stroke::new(1.0, c));
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(8),
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );
}

fn gear_button(ui: &mut egui::Ui, tooltip: &str, palette: Palette) -> egui::Response {
    let button = ButtonStyle::Neutral.palette(palette);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let response = response.on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        paint_button_chrome(ui, rect, hovered, button);
        let color = if hovered { palette.accent } else { button.fg };
        main_page::paint_icon(
            ui.painter(),
            IconKind::Settings,
            rect.center(),
            color,
            15.0,
            0.0,
        );
    }
    response
}

fn github_link_button(
    ui: &mut egui::Ui,
    mark: Option<&egui::TextureHandle>,
    label: &str,
    tooltip: &str,
    palette: Palette,
) -> egui::Response {
    let button = ButtonStyle::Neutral.palette(palette);
    let font_id = egui::FontId::proportional(13.0);
    let text_w = text_width(ui, label, &font_id, button.fg);
    let icon_w = if mark.is_some() { 16.0 + 8.0 } else { 0.0 };
    let size = egui::vec2((text_w + 28.0 + icon_w).max(200.0), 32.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        paint_button_chrome(ui, rect, hovered, button);
        let group_w = icon_w + text_w;
        let left = rect.left() + (rect.width() - group_w) / 2.0;
        if let Some(tex) = mark {
            let icon_rect = egui::Rect::from_min_size(
                egui::pos2(left, rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            ui.painter().image(
                tex.id(),
                icon_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                palette.white,
            );
        }
        ui.painter().text(
            egui::pos2(left + icon_w, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font_id,
            button.fg,
        );
    }
    response
}

const GITHUB_MARK_PNG: &[u8] = include_bytes!("../assets/github-mark.png");

/// 加载 GitHub mark：白底→透明；深色像素按主题模式处理——Dark 烤白、L1 保留黑 mark（ADR-0016 图片双态）。
fn load_github_mark(ctx: &egui::Context, palette: Palette) -> Option<egui::TextureHandle> {
    let mut img = image::load_from_memory(GITHUB_MARK_PNG).ok()?.to_rgba8();
    for p in img.pixels_mut() {
        if p[0] > 240 && p[1] > 240 && p[2] > 240 {
            p[3] = 0;
        } else if palette.dark_mode && p[3] > 0 {
            p[0] = 255;
            p[1] = 255;
            p[2] = 255;
        }
    }
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img);
    Some(ctx.load_texture("github-mark", color, egui::TextureOptions::LINEAR))
}

fn card_title(ui: &mut egui::Ui, text: &str, palette: Palette) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 2.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, palette.border);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(text)
                .size(13.5)
                .strong()
                .color(palette.ink),
        );
    });
}

fn status_item(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
    // 行内 spacing 收窄：dot→文字总 8lp（原型 .sb-item gap: 8；默认 item_spacing.x 10lp 会撑到 16）。
    let orig_x = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 2.0;
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).size(12.5).color(color));
    ui.spacing_mut().item_spacing.x = orig_x;
    ui.add_space(10.0);
}

fn status_line(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).size(13.0).strong().color(color));
}
fn log_warn(msg: impl std::fmt::Display) {
    eprintln!("[opensteamtool-manager] {msg}");
}

fn status_bar_items(
    steam_running: bool,
    busy: Option<BusyKind>,
    notice: Option<&Notice>,
    strings: &Strings,
    palette: Palette,
) -> Vec<(String, egui::Color32)> {
    let mut items = Vec::new();
    if steam_running {
        items.push((strings.status_steam_running.to_string(), palette.success));
    } else {
        items.push((strings.status_steam_stopped.to_string(), palette.weak));
    }
    if let Some(kind) = busy
        && !kind.is_update_flow()
    {
        items.push((strings.busy_label(kind).to_string(), palette.busy_ink()));
    }
    if let Some(n) = notice
        && !matches!(n, Notice::UpdateChecked)
    {
        let launch_success_needs_running = matches!(
            n,
            Notice::WorkflowDone(action, Ok(()))
                if matches!(action, Action::Launch | Action::Restart)
        );
        if !launch_success_needs_running || steam_running {
            let (ok, text) = render_notice(strings, n);
            items.push((text, if ok { palette.success } else { palette.danger }));
        }
    }
    items
}

fn render_notice(s: &Strings, notice: &Notice) -> (bool, String) {
    match notice {
        Notice::Downloaded(Ok(())) => (true, s.ok_downloaded.to_string()),
        Notice::Downloaded(Err(e)) => (false, s.update_error(e)),
        Notice::WorkflowDone(action, Ok(())) => (true, s.success_text(*action).to_string()),
        Notice::WorkflowDone(_, Err(e)) => (false, s.workflow_error_text(e)),
        Notice::Precheck(p) => (false, s.precheck_text(p)),
        Notice::UpdateChecked => {
            log_warn("render_notice 收到 Notice::UpdateChecked（应经 status_bar 分流）");
            (true, String::new())
        }
    }
}

fn health_warning(s: &Strings, summary: CompatSummary) -> Option<&'static str> {
    match summary {
        CompatSummary::Pending => Some(s.main_warning_pending),
        CompatSummary::Missing => Some(s.main_warning_missing),
        _ => None,
    }
}

enum Msg {
    Phase(BusyKind),
    /// 重复启动（另一实例已置位唤醒事件）：把窗口带回前台。
    ActivateRequested,
    UpdateChecked(Result<OnlineInfo, UpdateError>),
    Downloaded(Result<(), UpdateError>),
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
    /// Steam 核心兼容性体检完成（携带发起时代数，陈旧结果由流程丢弃）。
    Compat {
        epoch: compat_flow::Epoch,
        report: compat::OverallHealthReport,
    },
    CompatRefreshed {
        epoch: compat_flow::Epoch,
        report: compat::OverallHealthReport,
    },
    CompatPrecached {
        epoch: compat_flow::Epoch,
        result: Result<(), compat::CompatError>,
    },
    WizardDownload(Result<(), UpdateError>),
    /// 应用更新检查完成（Settings — 关于页签；只读查询，不进忙碌门禁）。
    AppUpdateChecked(Result<updater::AppUpdateCheckResult, UpdateError>),
}

enum Notice {
    /// 检查更新完成标记（无 payload）：结果文案由「更新流程」派生（单一事实源）。
    UpdateChecked,
    Downloaded(Result<(), UpdateError>),
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
    Precheck(workflow::Precheck),
}

/// 主页面交互意图（Hero / Secondary / Update 按钮位收集后统一处理）。
enum MainEvent {
    Action(Action),
    FixPath,
    Check,
    Download(OnlineInfo),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsTab {
    General,
    About,
    Steam,
}

/// Settings — Steam 页签的路径编辑状态（纯逻辑：编辑缓冲 + 提交判定，可单测）。
/// 与工作路径 `steam_path` 分离（ADR-0012 精神）：提交时才判定，非法输入不产出
/// 提交值、永不落盘（#31 验收）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SteamPathEditor {
    buffer: String,
    invalid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SteamPathCommit {
    Changed(String),
    Unchanged,
    /// 非空且非目录：非法（置内联错误，不产出提交值，调用方不落盘）。
    Invalid,
}

impl SteamPathEditor {
    fn new(seed: &str) -> Self {
        Self {
            buffer: seed.to_string(),
            invalid: false,
        }
    }

    /// 提交（失焦/回车）：判据与主页面路径提交同口径（ADR-0012「空 = 未设置」是合法
    /// 终态，`config.toml` 允许空 `steam_path`、启动回退注册表检测）——空串视为未设置
    fn submit(&mut self, current: &str) -> SteamPathCommit {
        let p = self.buffer.trim();
        if p.is_empty() || dll::is_valid_steam_dir(p) {
            self.invalid = false;
            if p == current {
                SteamPathCommit::Unchanged
            } else {
                SteamPathCommit::Changed(p.to_string())
            }
        } else {
            self.invalid = true;
            SteamPathCommit::Invalid
        }
    }
}

struct CompatView {
    summary: CompatSummary,
    checking: bool,
    precaching: bool,
    precache_done: bool,
    precache_error: Option<String>,
    detail_report: Option<compat::OverallHealthReport>,
}

impl CompatView {
    fn snapshot(app: &App) -> Self {
        let d = app.flow.display();
        Self {
            summary: d.summary,
            checking: d.checking,
            precaching: d.precaching,
            precache_done: d.precache_done,
            precache_error: d.precache_error.map(|err| {
                app.strings
                    .compat_precache_failed
                    .replace("{err}", &app.strings.compat_error_text(err))
            }),
            detail_report: app.compat_details_open.then(|| d.report.cloned()).flatten(),
        }
    }
}

pub struct App {
    config: Config,
    lang_pref: Language,
    lang: Lang,
    strings: Strings,
    steam_path: String,
    status: DeployStatus,
    steam_running: bool,
    steam_monitor: SteamMonitor,
    steam_state: Arc<SteamState>,
    local_version: Option<String>,
    /// 在线更新「检查更新」流程（唯一事实源 + 派生，见 GLOSSARY.md「更新流程」）。
    update_flow: UpdateFlow,
    /// 交互类后台操作互斥门禁（同时刻仅一个操作在途；见 GLOSSARY.md「忙碌门禁」）。
    gate: BusyGate,
    confirm: Option<Action>,
    notice: Option<Notice>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    tray: Option<Tray>,
    window_visible: bool,
    autosized: bool,
    pending_focus: bool,
    minimize_to_tray: bool,
    /// 深浅主题偏好（config.theme 镜像；唯一写入点 `set_theme`，ADR-0016）。
    theme_pref: ThemePreference,
    /// 当前生效色板（每帧从 `ctx.theme()` 同步；System 模式 OS 切深浅即跟随）。
    palette: Palette,
    /// GitHub mark 的加载模式（dark/light）；palette 模式变化时重载（ADR-0016 图片双态）。
    mark_dark: bool,
    /// 主页面左上角品牌 LOGO 纹理（深/浅双态，GLOSSARY「应用图标」；palette 模式变化时重载）。
    logo: Option<egui::TextureHandle>,
    /// 首帧窗口图标同步是否已执行（ViewportCommand::Icon 在窗口显示前不生效，见 ui() 首帧块）。
    icon_synced: bool,
    was_minimized: bool,

    settings_open: bool,
    settings_tab: SettingsTab,
    settings_steam: SteamPathEditor,
    /// 应用更新检查结果（Settings — 关于页签；None = 尚未检查）。检查中由 `app_update_checking` 表达（发起时覆盖旧结果）。
    app_update: Option<Result<updater::AppUpdateCheckResult, UpdateError>>,
    /// 应用更新检查是否在途（自管忙碌：只读查询不进 Busy Gate，按钮在途自禁用）。
    app_update_checking: bool,
    wizard: Option<wizard::Wizard>,
    flow: CompatFlow,
    /// 兼容性明细展开开关（纯 UI 状态，不属于流程）。
    compat_details_open: bool,
    compat_scroll_pending: bool,
    github_mark: Option<egui::TextureHandle>,
}

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

#[derive(Clone, Copy, Debug, Default)]
struct PathEditRow {
    changed: bool,
    commit: bool,
}

fn path_edit_row(
    ui: &mut egui::Ui,
    strings: Strings,
    buffer: &mut String,
    palette: Palette,
) -> PathEditRow {
    let mut row = PathEditRow::default();
    ui.horizontal(|ui| {
        let edit_width = (ui.available_width() - 92.0).max(120.0);
        let resp = ui.add_sized(
            egui::vec2(edit_width, 34.0),
            egui::TextEdit::singleline(buffer)
                .margin(egui::Margin::symmetric(10, 7))
                .hint_text(strings.steam_path_label),
        );
        if resp.changed() {
            row.changed = true;
        }
        if resp.lost_focus() {
            row.commit = true;
        }
        if styled_button(
            ui,
            strings.browse,
            ButtonStyle::Neutral,
            egui::vec2(82.0, 34.0),
            true,
            palette,
        )
        .clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            *buffer = dir.display().to_string();
            row.changed = true;
            row.commit = true;
        }
    });
    row
}

/// 三态下拉共用实现（语言 / 主题；设置页与向导同一形态，ADR-0016）。
/// 选中项文字显式用 palette.accent——egui 选中态底色取自 selection.bg_fill（浅蓝），
/// 而全局 override_text_color 会把无显式色文字强制成 ink（深色主题近白），白字浅底不可读。
fn tri_state_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    options: [(T, &'static str); 3],
    selected: T,
    width: f32,
    salt: &'static str,
    palette: Palette,
) -> Option<T> {
    let current = options
        .iter()
        .find(|(v, _)| *v == selected)
        .map(|(_, label)| *label)
        .unwrap_or_default();
    let mut chosen = None;
    egui::ComboBox::from_id_salt(salt)
        .selected_text(current)
        .width(width)
        .show_ui(ui, |ui| {
            for (v, label) in options {
                let text = if selected == v {
                    egui::RichText::new(label).color(palette.accent)
                } else {
                    egui::RichText::new(label)
                };
                if ui.selectable_label(selected == v, text).clicked() {
                    chosen = Some(v);
                }
            }
        });
    chosen
}

fn language_combo(
    ui: &mut egui::Ui,
    options: [(Language, &'static str); 3],
    selected: Language,
    width: f32,
    salt: &'static str,
    palette: Palette,
) -> Option<Language> {
    tri_state_combo(ui, options, selected, width, salt, palette)
}

/// 主题偏好 → egui 偏好（System/Dark/Light 三态一一对应，ADR-0016）。
fn to_egui_theme_pref(pref: ThemePreference) -> egui::ThemePreference {
    match pref {
        ThemePreference::System => egui::ThemePreference::System,
        ThemePreference::Dark => egui::ThemePreference::Dark,
        ThemePreference::Light => egui::ThemePreference::Light,
    }
}

/// 主题三态下拉（ADR-0016）：选项表来自 `Strings::theme_options`。
fn theme_combo(
    ui: &mut egui::Ui,
    options: [(ThemePreference, &'static str); 3],
    selected: ThemePreference,
    width: f32,
    salt: &'static str,
    palette: Palette,
) -> Option<ThemePreference> {
    tri_state_combo(ui, options, selected, width, salt, palette)
}

/// ComboBox 内部自建 horizontal 行（子内容左对齐），父层 `Align::Center` 管不到它；
/// 用外层 horizontal + 偏移把下拉推到卡片中线上（向导语言/主题步骤的居中对齐）。
fn centered_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    options: [(T, &'static str); 3],
    selected: T,
    width: f32,
    salt: &'static str,
    palette: Palette,
) -> Option<T> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.add_space((ui.available_width() - width) / 2.0);
        if let Some(v) = tri_state_combo(ui, options, selected, width, salt, palette) {
            chosen = Some(v);
        }
    });
    chosen
}

fn wizard_download() -> Result<(), UpdateError> {
    let info = updater::check_update()?;
    updater::download_and_extract(&info, &dll::dll_dir())
}

/// 步骤 3 展示决策（纯函数）：提示文案、是否错误色、主按钮、是否显示跳过；陈述永远派生自状态（文件本位判据见 ADR-0011）。
struct WizardStep3Content {
    prompt: String,
    danger: bool,
    primary: Option<(String, wizard::Event)>,
    show_skip: bool,
}

fn wizard_step3_content(strings: &Strings, dl: &wizard::DownloadState) -> WizardStep3Content {
    match dl {
        wizard::DownloadState::Idle => WizardStep3Content {
            prompt: strings.wizard_download_prompt.to_string(),
            danger: false,
            primary: Some((
                strings.wizard_btn_download.to_string(),
                wizard::Event::DownloadRequested,
            )),
            show_skip: true,
        },
        wizard::DownloadState::Ready => WizardStep3Content {
            prompt: strings.wizard_download_ready.to_string(),
            danger: false,
            primary: Some((
                strings.wizard_btn_done.to_string(),
                wizard::Event::SkipDownload,
            )),
            show_skip: false,
        },
        wizard::DownloadState::Running => WizardStep3Content {
            prompt: strings.wizard_download_running.to_string(),
            danger: false,
            primary: None,
            show_skip: true,
        },
        wizard::DownloadState::Failed(e) => WizardStep3Content {
            prompt: strings
                .wizard_download_failed
                .replace("{err}", &strings.update_error(e)),
            danger: true,
            primary: Some((
                strings.wizard_btn_retry.to_string(),
                wizard::Event::DownloadRequested,
            )),
            show_skip: true,
        },
    }
}

fn wizard_step_number(step: wizard::Step) -> u32 {
    match step {
        WizardStep::Language => 1,
        WizardStep::Theme => 2,
        WizardStep::SteamPath => 3,
        WizardStep::Download => 4,
    }
}

/// 向导步骤渲染（纯函数：视图 + 文案 → 用户意图 + 卡片矩形；不触碰 App 状态，可单测）。
fn wizard_steps_ui(
    ui: &mut egui::Ui,
    strings: Strings,
    view: &wizard::View,
    palette: Palette,
) -> (Option<wizard::Event>, egui::Rect) {
    let mut event: Option<wizard::Event> = None;
    let mut card_rect = egui::Rect::NOTHING;

    ui.add_space(24.0);
    let card_w = WIZARD_CARD_WIDTH + card_frame(palette).total_margin().sum().x;
    ui.vertical_centered(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(card_w, ui.available_height()),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                let resp = card_frame(palette).show(ui, |ui| {
                    ui.set_width(WIZARD_CARD_WIDTH);
                    ui.label(
                        egui::RichText::new(strings.wizard_title)
                            .size(16.0)
                            .strong()
                            .color(palette.ink),
                    );
                    ui.add_space(4.0);
                    let n = wizard_step_number(view.step);
                    ui.label(
                        egui::RichText::new(strings.wizard_step_of.replace("{n}", &n.to_string()))
                            .size(12.0)
                            .color(palette.weak),
                    );
                    ui.add_space(18.0);

                    match view.step {
                        WizardStep::Language => {
                            ui.label(
                                egui::RichText::new(strings.wizard_language_prompt).size(13.0),
                            );
                            ui.add_space(12.0);
                            if let Some(lang) = centered_combo(
                                ui,
                                strings.language_options(),
                                view.language,
                                240.0,
                                "wizard_language",
                                palette,
                            ) {
                                event = Some(wizard::Event::LanguageChosen(lang));
                            }
                            ui.add_space(14.0);
                            if styled_button(
                                ui,
                                strings.wizard_btn_next,
                                ButtonStyle::Primary,
                                egui::vec2(140.0, 34.0),
                                true,
                                palette,
                            )
                            .clicked()
                            {
                                event = Some(wizard::Event::LanguageSubmitted);
                            }
                        }
                        WizardStep::Theme => {
                            ui.label(egui::RichText::new(strings.wizard_theme_prompt).size(13.0));
                            ui.add_space(12.0);
                            if let Some(theme) = centered_combo(
                                ui,
                                strings.theme_options(),
                                view.theme,
                                240.0,
                                "wizard_theme",
                                palette,
                            ) {
                                event = Some(wizard::Event::ThemeChosen(theme));
                            }
                            ui.add_space(14.0);
                            if styled_button(
                                ui,
                                strings.wizard_btn_next,
                                ButtonStyle::Primary,
                                egui::vec2(140.0, 34.0),
                                true,
                                palette,
                            )
                            .clicked()
                            {
                                event = Some(wizard::Event::ThemeSubmitted);
                            }
                        }
                        WizardStep::SteamPath => {
                            ui.label(egui::RichText::new(strings.wizard_path_prompt).size(13.0));
                            ui.add_space(12.0);
                            let mut buf = view.steam_path.clone();
                            let row = path_edit_row(ui, strings, &mut buf, palette);
                            if row.changed {
                                event = Some(wizard::Event::PathEdited(buf.clone()));
                            }
                            if !view.path_valid && !view.steam_path.trim().is_empty() {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new(strings.wizard_path_invalid)
                                        .size(12.0)
                                        .color(palette.danger),
                                );
                            }
                            ui.add_space(14.0);
                            if styled_button(
                                ui,
                                strings.wizard_btn_next,
                                ButtonStyle::Primary,
                                egui::vec2(140.0, 34.0),
                                view.path_valid,
                                palette,
                            )
                            .clicked()
                            {
                                event = Some(wizard::Event::PathSubmitted);
                            }
                        }
                        WizardStep::Download => {
                            let content = wizard_step3_content(&strings, &view.download);
                            let prompt = egui::RichText::new(content.prompt.as_str()).size(13.0);
                            let prompt = if content.danger {
                                prompt.color(palette.danger)
                            } else {
                                prompt
                            };
                            ui.label(prompt);
                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                let item_gap = ui.spacing().item_spacing.x;
                                let mut row_w = 0.0;
                                let mut count: i32 = 0;
                                if content.primary.is_some() {
                                    count += 1;
                                    row_w += 140.0;
                                }
                                if content.show_skip {
                                    count += 1;
                                    row_w += 120.0;
                                }
                                row_w += (count.saturating_sub(1)) as f32 * item_gap;
                                ui.add_space(((ui.available_width() - row_w) / 2.0).max(0.0));

                                if let Some((label, ev)) = &content.primary
                                    && styled_button(
                                        ui,
                                        label.as_str(),
                                        ButtonStyle::Primary,
                                        egui::vec2(140.0, 34.0),
                                        true,
                                        palette,
                                    )
                                    .clicked()
                                {
                                    event = Some(ev.clone());
                                }
                                if content.show_skip
                                    && styled_button(
                                        ui,
                                        strings.wizard_btn_skip,
                                        ButtonStyle::Neutral,
                                        egui::vec2(120.0, 34.0),
                                        true,
                                        palette,
                                    )
                                    .clicked()
                                {
                                    event = Some(wizard::Event::SkipDownload);
                                }
                            });
                        }
                    }
                });
                card_rect = resp.response.rect;
            },
        );
    });
    (event, card_rect)
}

/// 自动隐身策略（ADR-0001）：Steam 边沿事件 + 当前窗口显隐 → 目标显隐。
fn auto_tray_policy(event: SteamEvent, window_visible: bool) -> Option<bool> {
    match (event, window_visible) {
        (SteamEvent::Started, true) => Some(false),
        (SteamEvent::Stopped, false) => Some(true),
        _ => None,
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, singleton: Singleton) -> Self {
        install_cjk_font(&cc.egui_ctx);
        install_theme(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        // 单实例唤醒线程：守卫随线程存活到进程退出——中途 drop 会放开互斥体，多开防护随之失效。
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                loop {
                    singleton.wait_activate();
                    let _ = tx.send(Msg::ActivateRequested);
                    ctx.request_repaint();
                }
            });
        }
        // 启动即恢复应用配置（语言偏好 + Steam 路径，见 ADR-0012）：缺失文件 = 默认值；损坏/版本不符 = 类型化错误降级（不 panic）。
        let config = match config::load(&config::config_path()) {
            Ok(cfg) => cfg,
            Err(e) => {
                log_warn(format!("load config.toml: {e}；使用默认值"));
                config::Config::defaults()
            }
        };
        let lang_pref = config.language;
        let lang = lang_pref.effective();
        let strings = Strings::new(lang);
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            strings.window_title.to_owned(),
        ));
        // 配置优先恢复 Steam 路径；失效或未设置时回退注册表检测（同一 is_dir 判据），检测结果不写回配置。
        let steam_path = if !dll::is_valid_steam_dir(&config.steam_path) {
            steam::detect_steam_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        } else {
            config.steam_path.clone()
        };
        let wizard = wizard::should_show(&config::config_path()).then(|| {
            wizard::Wizard::new(lang_pref, config.theme, steam_path.clone(), dll::dll_dir())
        });
        let steam_dir = Path::new(&steam_path);
        let status = dll::check_status(steam_dir);
        let local_version = dll::read_local_version(&dll::dll_dir());
        let steam_state = Arc::new(SteamState::new());
        let steam_monitor = SteamMonitor::new(&steam_state);
        let steam_running = steam_monitor.is_running();
        let minimize_to_tray = config.minimize_to_tray;
        let theme_pref = config.theme;
        // 主题在托盘创建前确定（托盘图标按当前主题选版，GLOSSARY「应用图标」深/浅双态）。
        let palette = match theme_pref {
            ThemePreference::Dark => Palette::dark(),
            ThemePreference::Light => Palette::light(),
            ThemePreference::System => match cc.egui_ctx.system_theme() {
                Some(egui::Theme::Light) => Palette::light(),
                Some(egui::Theme::Dark) | None => Palette::dark(), // 检测缺失回退 Dark（与 egui fallback 同值）
            },
        };
        let mark_dark = palette.dark_mode;
        let tray = Tray::new(
            crate::brand::tray_icon(palette.dark_mode),
            strings.app_title,
            strings.tray_show,
            strings.tray_quit,
            strings.tray_minimize,
            config.minimize_to_tray,
            strings.tray_restart,
        );

        let flow = CompatFlow::new();

        // 启动即应用持久化主题偏好（ADR-0016）：egui 默认 ThemePreference::System，
        // 不显式设置则固定 Dark/Light 会在首帧被 OS 主题覆盖，重启后恢复失效。
        cc.egui_ctx.set_theme(to_egui_theme_pref(theme_pref));
        let mut app = Self {
            config,
            lang_pref,
            lang,
            strings,
            steam_path,
            status,
            steam_running,
            steam_monitor,
            steam_state,
            local_version,
            update_flow: UpdateFlow::new(),
            gate: BusyGate::new(),
            confirm: None,
            notice: None,
            tx,
            rx,
            ctx: cc.egui_ctx.clone(),
            tray,
            window_visible: true,
            autosized: false,
            pending_focus: false,
            minimize_to_tray,
            theme_pref,
            palette,
            mark_dark,
            was_minimized: false,
            settings_open: false,
            settings_tab: SettingsTab::General,
            settings_steam: SteamPathEditor::new(""),
            app_update: None,
            app_update_checking: false,
            wizard,
            flow,
            compat_details_open: false,
            compat_scroll_pending: false,
            github_mark: load_github_mark(&cc.egui_ctx, palette),
            logo: brand::logo_texture(&cc.egui_ctx, palette.dark_mode),
            icon_synced: false,
        };
        // 窗口图标不在 App::new 发送：ViewportCommand::Icon 在窗口显示前不生效（实测被吞，
        // 标题栏沿用 ViewportBuilder 图标）。初始图标已按 config 主题选版（见 main.rs），
        // System 模式校正与后续主题切换走 ui() 的 palette 钩子/首帧同步。
        app.sync_tray_restart_enabled();
        // 启动即喂首次路径：产出首次快速体检效果（初始 checking 骨架态，零白屏）。
        app.on_compat_event(
            &cc.egui_ctx,
            compat_flow::Event::PathChanged(app.steam_path.clone()),
        );
        app
    }

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

    fn set_window_visible(&mut self, visible: bool) {
        self.window_visible = visible;
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if visible {
            self.pending_focus = true;
            self.ctx.request_repaint();
        }
    }

    fn hide_if_steam_running(&mut self) {
        if self.steam_running {
            self.set_window_visible(false);
        }
    }

    /// 同步托盘「重启 Steam」可用性（路径无效置灰；未运行点击等价「直接启动」，见 workflow::plan）。
    fn sync_tray_restart_enabled(&mut self) {
        if let Some(tray) = &self.tray {
            tray.set_restart_enabled(self.status != DeployStatus::InvalidPath);
        }
    }

    /// 最小化隐身偏好的唯一写入点（#37）：设置对话框勾选与托盘菜单勾选共用
    /// `config.minimize_to_tray` 同一事实源，任一入口变更即同步并持久化。
    fn set_minimize_to_tray(&mut self, checked: bool) {
        self.minimize_to_tray = checked;
        self.config.minimize_to_tray = checked;
        self.persist_config();
        if let Some(tray) = &self.tray {
            tray.set_minimize_to_tray(checked);
        }
    }

    /// 深浅主题偏好的唯一写入点（ADR-0016）：镜像 + egui 偏好（含原生标题栏联动）+ 持久化。
    fn set_theme(&mut self, pref: ThemePreference) {
        self.theme_pref = pref;
        self.config.theme = pref;
        self.ctx.set_theme(to_egui_theme_pref(pref));
        self.persist_config();
    }
    fn handle_tray_events(&mut self) {
        let Some(tray) = &self.tray else { return };
        let ctx = self.ctx.clone(); // 避免 `&self.ctx` 与 `&mut self` 借用冲突。
        let mut actions = Vec::new();
        while let Some(action) = tray.poll() {
            actions.push(action);
        }
        let minimize_checked = tray.is_minimize_to_tray();
        for action in actions {
            match action {
                TrayAction::ToggleVisible => self.set_window_visible(!self.window_visible),
                TrayAction::Show => self.set_window_visible(true),
                TrayAction::Quit => {
                    self.tray = None;
                    std::process::exit(0);
                }
                TrayAction::ToggleMinimizeToTray => self.set_minimize_to_tray(minimize_checked),
                TrayAction::RestartSteam => self.request_action(&ctx, Action::Restart),
            }
        }
    }

    fn refresh_status(&mut self) {
        self.status = dll::check_status(Path::new(self.steam_path.trim()));
        self.sync_tray_restart_enabled();
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Phase(kind) => self.gate.replace(kind),
                Msg::ActivateRequested => {
                    // 重复启动（含托盘隐藏中的窗口）：恢复显示并聚焦到前台。
                    self.set_window_visible(true);
                }
                Msg::UpdateChecked(res) => {
                    self.gate.clear();
                    self.update_flow.check_done(res); // 结果只存这一份（单一事实源）
                    self.notice = Some(Notice::UpdateChecked);
                }
                Msg::Downloaded(res) => {
                    self.gate.clear();
                    self.notice = Some(Notice::Downloaded(res.clone()));
                    if let Ok(()) = res {
                        self.local_version = dll::read_local_version(&dll::dll_dir());
                    }
                }
                Msg::WorkflowDone(action, res) => {
                    self.gate.clear();
                    self.notice = Some(Notice::WorkflowDone(action, res.clone()));
                    if let Ok(()) = res {
                        self.refresh_status();
                    }
                    self.steam_running = self.steam_monitor.rescan();
                    self.hide_if_steam_running();
                }
                Msg::Compat { epoch, report } => {
                    let ctx = self.ctx.clone();
                    self.on_compat_event(&ctx, compat_flow::Event::ProbeDone { epoch, report });
                }
                Msg::CompatRefreshed { epoch, report } => {
                    let ctx = self.ctx.clone();
                    self.on_compat_event(&ctx, compat_flow::Event::RefreshDone { epoch, report });
                }
                Msg::CompatPrecached { epoch, result } => {
                    let ctx = self.ctx.clone();
                    self.on_compat_event(&ctx, compat_flow::Event::PrecacheDone { epoch, result });
                }
                Msg::WizardDownload(res) => {
                    if self.wizard.is_some() {
                        let ctx = self.ctx.clone();
                        self.wizard_event(&ctx, wizard::Event::DownloadDone(res));
                    } else if res.is_ok() {
                        // 向导已跳过/关窗但在途下载仍完成：刷新本地版本，避免主界面继续显示陈旧的「补丁缺失」（跳过不等于取消网络请求，以磁盘为准）。
                        self.local_version = dll::read_local_version(&dll::dll_dir());
                        self.refresh_status();
                    }
                }
                Msg::AppUpdateChecked(res) => {
                    self.app_update_checking = false;
                    self.app_update = Some(res);
                }
            }
        }
    }

    /// #36：Steam 运行中「应用补丁并启动」不再弹确认框，直接放行优雅退出 → 部署 → 拉起；两个卸载类动作保留确认框，「重启 Steam」恒不弹。
    fn request_action(&mut self, ctx: &egui::Context, action: Action) {
        if self.gate.is_busy() {
            return;
        }
        if action.asks_to_close_steam() && self.steam_running {
            self.confirm = Some(action);
            return;
        }
        // #36：Steam 运行中「应用补丁并启动」免除确认框，但部署前仍需先关 Steam（DLL 被占用；plan 在 kill_first 时首插 CloseSteam）。
        let kill_first = self.steam_running && action == Action::ApplyAndLaunch;
        self.start_action(ctx, action, kill_first);
    }

    /// 免确认执行：仅「卸载补丁」行使用（spec §45 卸载补丁确认 No；
    /// §20.3 确认框不得扩散到 Steam 未运行时的卸载——该行渲染即未运行态，点击瞬间也不因竞态误弹）。
    fn request_action_quiet(&mut self, ctx: &egui::Context, action: Action) {
        if self.gate.is_busy() {
            return;
        }
        self.start_action(ctx, action, false);
    }

    fn start_action(&mut self, ctx: &egui::Context, action: Action, kill_first: bool) {
        let dll_dir = dll::dll_dir();
        let steam_dir = PathBuf::from(self.steam_path.trim());

        let ops = match workflow::plan(action, kill_first, &steam_dir, &dll_dir) {
            Ok(ops) => ops,
            Err(precheck) => {
                self.confirm = None;
                self.notice = Some(Notice::Precheck(precheck));
                return;
            }
        };

        // 门禁此刻应空闲（request_action 已查过、确认弹窗悬挂期 Modal 阻断交互）；Release 下仍被占用则放弃并记日志。
        let first_phase = ops.first().expect("plan never returns empty").phase();
        self.confirm = None; // 无论门禁是否放行都收掉确认弹窗，避免悬挂。
        if !self.gate.start(first_phase) {
            log_warn(format!(
                "start_action 被忙碌门禁拒绝（phase={first_phase:?}）"
            ));
            return;
        }

        let ctx2 = ctx.clone();
        let tx = self.tx.clone();
        let steam = self.steam_state.clone();
        self.spawn(ctx, move || {
            let res = workflow::execute(
                &ops,
                &workflow::WorkflowCtx {
                    dll_dir,
                    steam_dir,
                    steam,
                },
                |phase| {
                    let _ = tx.send(Msg::Phase(phase));
                    ctx2.request_repaint();
                },
            );
            Msg::WorkflowDone(action, res)
        });
    }

    fn set_language(&mut self, pref: Language) {
        self.lang_pref = pref;
        self.lang = pref.effective();
        self.strings = Strings::new(self.lang);
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            self.strings.window_title.to_owned(),
        ));
    }

    /// 把已持久化的配置镜像原子落盘到 `config.toml`；失败仅记日志不中断操作
    fn persist_config(&self) {
        if let Err(e) = config::save(&config::config_path(), &self.config) {
            log_warn(format!("persist config: {e}"));
        }
    }

    fn check_update(&mut self, ctx: &egui::Context) {
        if !self.gate.start(BusyKind::Checking) {
            return;
        }
        self.update_flow.check_started();
        self.spawn(ctx, || Msg::UpdateChecked(updater::check_update()));
    }

    fn download_update(&mut self, ctx: &egui::Context, info: OnlineInfo) {
        if !self.gate.start(BusyKind::Downloading) {
            return;
        }
        let dll_dir = dll::dll_dir();
        self.spawn(ctx, move || {
            Msg::Downloaded(updater::download_and_extract(&info, &dll_dir))
        });
    }

    /// 应用更新检查：查询本仓库最新发布并与当前程序版本比较。只读查询，不进忙碌门禁
    /// （不互斥补丁/操作类后台任务）；检查中按钮自禁用防重复发起，结果仅呈现在
    /// Settings — 关于页签。不下载、不自替换（明确非目标）。
    fn check_app_update(&mut self, ctx: &egui::Context) {
        if self.app_update_checking {
            return;
        }
        self.app_update_checking = true;
        self.app_update = None;
        self.spawn(ctx, || Msg::AppUpdateChecked(updater::check_app_update()));
    }

    fn wizard_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(view) = self.wizard.as_ref().map(|w| w.view()) else {
            return;
        };
        let strings = self.strings; // Copy：渲染期自由借用 self。
        let (event, _) = wizard_steps_ui(ui, strings, &view, self.palette);
        if let Some(event) = event {
            self.wizard_event(ctx, event);
        }
    }

    fn wizard_event(&mut self, ctx: &egui::Context, event: wizard::Event) {
        let (v, effects) = self.wizard.as_mut().unwrap().step(event);
        self.set_language(v.language);
        // 主题仅在变化时写入（路径编辑每键触发的事件不落盘）；set_theme 即改即存（ADR-0016 唯一写入点）。
        if v.theme != self.theme_pref {
            self.set_theme(v.theme);
        }
        self.exec_wizard_effects(ctx, effects);
    }

    fn exec_wizard_effects(&mut self, ctx: &egui::Context, effects: Vec<wizard::Effect>) {
        for effect in effects {
            match effect {
                wizard::Effect::Download => {
                    let ctx2 = ctx.clone();
                    self.spawn(&ctx2, || Msg::WizardDownload(wizard_download()));
                }
                wizard::Effect::Finish {
                    language,
                    theme,
                    steam_path,
                } => {
                    debug_assert!(
                        self.wizard.as_ref().is_some_and(|w| w.finished()),
                        "Finish 效果只能由已结束的向导产出"
                    );
                    self.config.language = language;
                    self.config.theme = theme;
                    self.config.steam_path = steam_path.clone();
                    self.persist_config();
                    self.steam_path = steam_path;
                    self.refresh_status();
                    self.wizard = None;
                    self.feed_path_changed(ctx);
                }
            }
        }
    }

    fn open_settings(&mut self, tab: SettingsTab) {
        self.settings_open = true;
        self.settings_tab = tab;
        self.settings_steam = SteamPathEditor::new(&self.steam_path);
    }

    /// 无 OK/Cancel（即改即存）；补丁更新检查 / 下载并解压走忙碌门禁互斥。
    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut close_clicked = false;
        // 显式 frame：Modal 默认 menu_margin(6lp) 太贴边，加边缘呼吸（对称 24/20）。
        // 测试须复用同一 frame（settings_dialog_frame），否则布局测量与生产不一致。
        egui::Modal::new(egui::Id::new("settings_dialog"))
            .frame(settings_dialog_frame(self.palette))
            .show(ctx, |ui| {
                ui.set_width(SETTINGS_DIALOG_WIDTH);
                ui.heading(self.strings.settings_title);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (SettingsTab::General, self.strings.settings_tab_general),
                        (SettingsTab::Steam, self.strings.settings_tab_steam),
                        (SettingsTab::About, self.strings.settings_tab_about),
                    ] {
                        let style = if self.settings_tab == tab {
                            ButtonStyle::Primary
                        } else {
                            ButtonStyle::Neutral
                        };
                        if styled_button(
                            ui,
                            label,
                            style,
                            egui::vec2(88.0, 28.0),
                            true,
                            self.palette,
                        )
                        .clicked()
                            && self.settings_tab != tab
                        {
                            if self.settings_tab == SettingsTab::Steam {
                                self.commit_settings_steam_path(ctx);
                            }
                            self.settings_tab = tab;
                        }
                        ui.add_space(4.0);
                    }
                });
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);
                let max_scroll_h = settings_scroll_height(ctx.content_rect().height());
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .max_height(max_scroll_h)
                    .show(ui, |ui| {
                        // 设置内小项间呼吸感：行距略大于全局（10 → 13lp），各页签统一。
                        ui.spacing_mut().item_spacing.y = 13.0;
                        match self.settings_tab {
                            SettingsTab::General => self.settings_general(ui),
                            SettingsTab::About => self.settings_about(ui, ctx),
                            SettingsTab::Steam => self.settings_steam(ui, ctx),
                        }
                    });
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if styled_button(
                            ui,
                            self.strings.btn_close,
                            ButtonStyle::Neutral,
                            egui::vec2(80.0, 30.0),
                            true,
                            self.palette,
                        )
                        .clicked()
                        {
                            close_clicked = true;
                        }
                    });
                });
            });
        if close_clicked {
            // 关闭前提交未落地编辑（兜底：无论焦点时序，有效提交都不丢失）；未变更的判定让重复提交是 no-op。
            self.commit_settings_steam_path(ctx);
            self.settings_open = false;
        }
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        card_title(ui, self.strings.settings_language_title, self.palette);
        ui.add_space(10.0);
        let options = self.strings.language_options();
        if let Some(lang) = language_combo(
            ui,
            options,
            self.lang_pref,
            220.0,
            "settings_language",
            self.palette,
        ) && lang != self.lang_pref
        {
            self.set_language(lang);
            self.config.language = lang;
            self.persist_config();
        }

        ui.add_space(16.0);
        card_title(ui, self.strings.settings_theme_title, self.palette);
        ui.add_space(10.0);
        let options = self.strings.theme_options();
        if let Some(theme) = theme_combo(
            ui,
            options,
            self.theme_pref,
            220.0,
            "settings_theme",
            self.palette,
        ) && theme != self.theme_pref
        {
            self.set_theme(theme);
        }

        ui.add_space(16.0);
        card_title(ui, self.strings.settings_tray_title, self.palette);
        ui.add_space(10.0);
        let mut minimize_to_tray = self.minimize_to_tray;
        if ui
            .checkbox(&mut minimize_to_tray, self.strings.settings_tray_minimize)
            .changed()
        {
            self.set_minimize_to_tray(minimize_to_tray);
        }
    }

    fn settings_about(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card_title(ui, self.strings.settings_app_update_title, self.palette);
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(format!(
                "{} v{}",
                self.strings.settings_version_label,
                env!("CARGO_PKG_VERSION")
            ))
            .size(13.0)
            .color(self.palette.sub),
        );
        ui.add_space(8.0);
        let mut do_app_check = false;
        ui.horizontal(|ui| {
            if styled_button(
                ui,
                self.strings.settings_btn_app_update_check,
                ButtonStyle::Neutral,
                egui::vec2(150.0, 28.0),
                !self.app_update_checking,
                self.palette,
            )
            .clicked()
            {
                do_app_check = true;
            }
            match (&self.app_update, self.app_update_checking) {
                (_, true) => {
                    ui.label(
                        egui::RichText::new(self.strings.settings_app_update_checking)
                            .size(12.5)
                            .color(self.palette.weak),
                    );
                }
                (Some(Ok(r)), false) if r.newer => {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}{}",
                            self.strings.settings_app_update_new_version, r.latest_version
                        ))
                        .size(12.5)
                        .color(self.palette.ink),
                    );
                    if styled_button(
                        ui,
                        self.strings.settings_btn_open_download_page,
                        ButtonStyle::Primary,
                        egui::vec2(120.0, 28.0),
                        true,
                        self.palette,
                    )
                    .clicked()
                    {
                        updater::open_in_browser(updater::APP_RELEASES_PAGE);
                    }
                }
                (Some(Ok(_)), false) => {
                    ui.label(
                        egui::RichText::new(self.strings.settings_app_update_up_to_date)
                            .size(12.5)
                            .color(self.palette.ink),
                    );
                }
                (Some(Err(e)), false) => {
                    ui.label(
                        egui::RichText::new(self.strings.update_error(e))
                            .size(12.5)
                            .color(self.palette.danger),
                    );
                }
                (None, false) => {}
            }
        });
        if do_app_check {
            self.check_app_update(ctx);
        }
        ui.add_space(12.0);

        card_title(ui, self.strings.settings_github_title, self.palette);
        ui.add_space(8.0);
        if github_link_button(
            ui,
            self.github_mark.as_ref(),
            self.strings.settings_github_label,
            updater::APP_REPO_PAGE,
            self.palette,
        )
        .clicked()
        {
            updater::open_in_browser(updater::APP_REPO_PAGE);
        }
        ui.add_space(12.0);

        card_title(ui, self.strings.settings_wizard_title, self.palette);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(self.strings.settings_rerun_wizard_hint)
                .size(12.0)
                .color(self.palette.weak),
        );
        ui.add_space(8.0);
        if styled_button(
            ui,
            self.strings.settings_btn_rerun_wizard,
            ButtonStyle::Neutral,
            egui::vec2(160.0, 32.0),
            !self.gate.is_busy(),
            self.palette,
        )
        .clicked()
        {
            self.rerun_wizard();
        }
    }

    fn settings_steam(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card_title(ui, self.strings.settings_steam_title, self.palette);
        ui.add_space(10.0);
        let mut do_commit = false;
        let row = path_edit_row(
            ui,
            self.strings,
            &mut self.settings_steam.buffer,
            self.palette,
        );
        if row.changed {
            self.settings_steam.invalid = false;
        }
        if row.commit {
            do_commit = true;
        }
        if do_commit {
            self.commit_settings_steam_path(ctx);
        }
        if self.settings_steam.invalid {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(self.strings.settings_steam_path_invalid)
                    .size(12.0)
                    .color(self.palette.danger),
            );
        }
        self.compat_section(ui);
    }

    fn commit_settings_steam_path(&mut self, ctx: &egui::Context) {
        match self.settings_steam.submit(&self.steam_path) {
            SteamPathCommit::Invalid | SteamPathCommit::Unchanged => {}
            SteamPathCommit::Changed(p) => {
                self.steam_path = p.clone();
                self.config.steam_path = p;
                self.persist_config();
                self.refresh_status();
                self.feed_path_changed(ctx);
            }
        }
    }

    fn rerun_wizard(&mut self) {
        self.settings_open = false;
        self.wizard = Some(wizard::Wizard::new(
            self.lang_pref,
            self.theme_pref,
            self.steam_path.clone(),
            dll::dll_dir(),
        ));
    }
    /// 顶部应用头：品牌 LOGO + 名称 + 设置齿轮（spec §7；LOGO 深/浅双态，GLOSSARY「应用图标」）。
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                if let Some(tex) = &self.logo {
                    // 20px 品牌 LOGO 居中于 30px 分配格：header 行高与高度测量测试保持不变。
                    let logo_rect =
                        egui::Rect::from_center_size(rect.center(), egui::vec2(20.0, 20.0));
                    ui.painter().image(
                        tex.id(),
                        logo_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        self.palette.white,
                    );
                } else {
                    // 纹理缺失兜底：accent 占位块（正常路径不触发）。
                    let painter = ui.painter();
                    painter.rect(
                        rect,
                        egui::CornerRadius::same(R_SMALL),
                        self.palette.accent,
                        egui::Stroke::NONE,
                        egui::StrokeKind::Inside,
                    );
                }
            }
            // logo 右缘与标题间距 ≈10px（格内居中右侧留白 5 + 此处 5）。
            ui.add_space(5.0);
            ui.label(
                egui::RichText::new(self.strings.app_title)
                    .size(15.0)
                    .strong()
                    .color(self.palette.ink),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if gear_button(ui, self.strings.settings_gear_tooltip, self.palette).clicked() {
                    self.open_settings(self.settings_tab);
                }
            });
        });
        // header 底部 hairline（与状态栏 dock 顶部分隔线一致，spec §32）。
        let y = ui.cursor().top();
        ui.painter().line_segment(
            [
                egui::pos2(ui.min_rect().left(), y),
                egui::pos2(ui.min_rect().left() + ui.available_width(), y),
            ],
            egui::Stroke::new(1.0, self.palette.border),
        );
        ui.add_space(12.0);
    }

    /// Health Warning（spec §24）：仅「上游尚未适配 / 未找到核心 DLL」两态；Amber + 整行可点跳 Settings → Steam。
    fn health_warning_line(&mut self, ui: &mut egui::Ui) {
        let summary = self.flow.display().summary;
        let Some(text) = health_warning(&self.strings, summary) else {
            return;
        };
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let bg = self.palette.health_warning_bg();
            let border_color = self.palette.health_warning_border();
            let fg = self.palette.warn_fg();
            painter.rect(
                rect,
                egui::CornerRadius::same(R_ROW),
                bg,
                egui::Stroke::new(1.0, border_color),
                egui::StrokeKind::Inside,
            );
            main_page::paint_icon(
                painter,
                IconKind::Warning,
                egui::pos2(rect.left() + 16.0, rect.center().y),
                fg,
                15.0,
                0.0,
            );
            painter.text(
                egui::pos2(rect.left() + 38.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                text,
                egui::FontId::proportional(13.0),
                fg,
            );
            painter.text(
                egui::pos2(rect.right() - 18.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                self.strings.health_go,
                egui::FontId::proportional(12.5),
                self.palette.weak,
            );
        }
        if response.clicked() {
            self.open_settings(SettingsTab::Steam);
        }
        // 与 content 统一 gap（原型 14lp）；无警告时整行不占空间（前面已 return）。
        ui.add_space(14.0);
    }

    fn on_compat_event(&mut self, ctx: &egui::Context, event: compat_flow::Event) {
        let (_, effects) = self.flow.step(event);
        self.exec_compat_effects(ctx, effects);
    }

    /// 路径输入变化时喂给体检流程（防抖与代数推进在流程模块内）。
    fn feed_path_changed(&mut self, ctx: &egui::Context) {
        let path = self.steam_path.trim().to_string();
        self.on_compat_event(ctx, compat_flow::Event::PathChanged(path));
    }

    /// 手动「一键缓存签名」：喂给体检流程（目标选择与在途去重由流程负责）。
    fn request_precache(&mut self, ctx: &egui::Context) {
        self.on_compat_event(ctx, compat_flow::Event::PrecacheRequested);
    }

    fn exec_compat_effects(&self, ctx: &egui::Context, effects: Vec<compat_flow::Effect>) {
        for effect in effects {
            match effect {
                compat_flow::Effect::Probe { epoch, path } => {
                    let ctx = ctx.clone();
                    self.spawn(&ctx, move || Msg::Compat {
                        epoch,
                        report: compat::probe_all(Path::new(&path)),
                    });
                }
                compat_flow::Effect::Refresh { epoch, path } => {
                    let ctx = ctx.clone();
                    self.spawn(&ctx, move || Msg::CompatRefreshed {
                        epoch,
                        report: compat::probe_all_refresh(Path::new(&path)),
                    });
                }
                compat_flow::Effect::Precache {
                    epoch,
                    path,
                    targets,
                } => {
                    let ctx = ctx.clone();
                    self.spawn(&ctx, move || {
                        let result = targets.into_iter().try_for_each(|(target, sha)| {
                            compat::precache(Path::new(&path), target, &sha)
                        });
                        Msg::CompatPrecached { epoch, result }
                    });
                }
            }
        }
    }

    fn compat_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);

        let v = CompatView::snapshot(self);

        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 2.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 1.0, self.palette.border);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(self.strings.compat_title)
                    .size(13.5)
                    .strong(),
            );
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            self.compat_badge(ui, v.summary);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !v.checking
                    && styled_button(
                        ui,
                        self.strings.compat_btn_details,
                        ButtonStyle::Neutral,
                        egui::vec2(88.0, 26.0),
                        true,
                        self.palette,
                    )
                    .clicked()
                {
                    self.compat_details_open = !self.compat_details_open;
                    // 展开时置位滚动请求（关闭时清掉，防残留）。
                    self.compat_scroll_pending = self.compat_details_open;
                }
                if v.summary == CompatSummary::Online && !v.checking {
                    let label = if v.precaching {
                        self.strings.compat_precaching
                    } else {
                        self.strings.compat_btn_precache
                    };
                    if styled_button(
                        ui,
                        label,
                        ButtonStyle::Neutral,
                        egui::vec2(132.0, 26.0),
                        !v.precaching,
                        self.palette,
                    )
                    .clicked()
                    {
                        let ctx = self.ctx.clone();
                        self.request_precache(&ctx);
                    }
                }
            });
        });
        if let Some(report) = &v.detail_report {
            self.compat_details(ui, report, v.summary);
        }
        if let Some(text) = &v.precache_error {
            ui.label(
                egui::RichText::new(text)
                    .size(12.0)
                    .color(self.palette.danger),
            );
        }
        if v.precache_done {
            ui.label(
                egui::RichText::new(self.strings.compat_precache_done)
                    .size(12.0)
                    .color(self.palette.success),
            );
        }
    }

    fn compat_badge(&self, ui: &mut egui::Ui, summary: CompatSummary) {
        let (icon, text, fg, bg) = match summary {
            CompatSummary::Checking => (
                "○",
                self.strings.compat_checking,
                self.palette.weak,
                self.palette.border,
            ),
            CompatSummary::Ready => (
                "✔",
                self.strings.compat_status_ready,
                self.palette.success,
                self.palette.badge_bg(self.palette.success),
            ),
            CompatSummary::Online => (
                "●",
                self.strings.compat_status_online,
                self.palette.warn,
                self.palette.badge_bg(self.palette.warn),
            ),
            CompatSummary::Pending => (
                "▲",
                self.strings.compat_status_pending,
                self.palette.danger,
                self.palette.badge_bg(self.palette.danger),
            ),
            CompatSummary::Missing => (
                "?",
                self.strings.compat_status_missing,
                self.palette.weak,
                self.palette.border,
            ),
            CompatSummary::Network => (
                "?",
                self.strings.compat_status_network,
                self.palette.weak,
                self.palette.border,
            ),
        };
        egui::Frame::new()
            .fill(bg)
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(10, 3))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("{icon} {text}"))
                        .size(12.5)
                        .color(fg),
                );
            });
    }

    fn compat_status_of(&self, status: &compat::ProbeStatus) -> (&'static str, egui::Color32) {
        use compat::ProbeStatus::*;
        match status {
            Checking => (self.strings.compat_checking, self.palette.weak),
            RemoteAvailable { cached: true } => {
                (self.strings.compat_status_ready, self.palette.success)
            }
            RemoteAvailable { cached: false } => {
                (self.strings.compat_status_online, self.palette.warn)
            }
            CompatibleOffline => (self.strings.compat_status_offline, self.palette.success),
            IncompatiblePending => (self.strings.compat_status_pending, self.palette.danger),
            NetworkError(_) => (self.strings.compat_status_network, self.palette.weak),
            FileNotFound => (self.strings.compat_status_missing, self.palette.weak),
        }
    }

    fn compat_details(
        &mut self,
        ui: &mut egui::Ui,
        report: &compat::OverallHealthReport,
        summary: CompatSummary,
    ) {
        // 展开当帧滚动到明细**起点**贴顶（#34 复看 + 评审修复）：明细高于视口时滚起点保证首行可见；仅一帧，随后交还滚动控制。
        if self.compat_scroll_pending {
            self.compat_scroll_pending = false;
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        ui.add_space(6.0);
        if summary != CompatSummary::Checking {
            let tip = match summary {
                CompatSummary::Ready => self.strings.compat_tip_ready,
                CompatSummary::Online => self.strings.compat_tip_online,
                CompatSummary::Pending => self.strings.compat_tip_pending,
                CompatSummary::Missing => self.strings.compat_tip_missing,
                CompatSummary::Network => self.strings.compat_tip_network,
                CompatSummary::Checking => "",
            };
            ui.label(egui::RichText::new(tip).size(11.5).color(self.palette.weak));
            ui.add_space(6.0);
        }
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
                ui.label(egui::RichText::new(row).size(12.5).color(self.palette.sub));
                ui.monospace(egui::RichText::new(sha).size(12.0).color(self.palette.weak));
                status_line(ui, stext, scolor);
            });
        }
        // 详情内「一键缓存签名」：有未缓存项时提供（issue #23 §7.7）。
        let precaching = self.flow.display().precaching;
        if !compat_flow::precache_targets(report).is_empty() && !precaching {
            let ctx = self.ctx.clone();
            if styled_button(
                ui,
                self.strings.compat_btn_precache_all,
                ButtonStyle::Neutral,
                egui::vec2(150.0, 26.0),
                true,
                self.palette,
            )
            .clicked()
            {
                self.request_precache(&ctx);
            }
        }
    }

    /// Hero Surface：eyebrow → 状态 → supporting → Primary CTA → Patch Update Check（spec §13/§14 顺序）。
    fn hero_surface(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, vm: &MainPageVm) {
        let mut event: Option<MainEvent> = None;
        egui::Frame::new()
            .fill(self.palette.card)
            .stroke(egui::Stroke::new(1.0, self.palette.border))
            .corner_radius(egui::CornerRadius::same(R_HERO))
            .inner_margin(egui::Margin::symmetric(30, 24))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.hero_eyebrow(ui);
                ui.add_space(8.0);
                self.hero_status(ui, vm);
                // 状态 → Supporting / Primary（原型节奏：status→primary 22，有 supporting 时 10 + 22）。
                let (gap_to_sup, gap_after_sup) = if vm.supporting.is_some() {
                    (10.0, 22.0)
                } else {
                    (22.0, 0.0)
                };
                ui.add_space(gap_to_sup);
                self.hero_supporting(ui, vm);
                ui.add_space(gap_after_sup);
                if let Some(e) = self.hero_primary(ui, ctx, vm) {
                    event = Some(e);
                }
                ui.add_space(14.0);
                if let Some(e) = self.patch_update_check(ui, ctx, vm) {
                    event = Some(e);
                }
            });
        match event {
            Some(MainEvent::Action(action)) => self.request_action(ctx, action),
            Some(MainEvent::FixPath) => self.open_settings(SettingsTab::Steam),
            Some(MainEvent::Check) => self.check_update(ctx),
            Some(MainEvent::Download(info)) => self.download_update(ctx, info),
            None => {}
        }
    }

    /// eyebrow：降权短条 + 大写 PATCH（spec §14 定稿，双语一致；短条形态与色槽见 ADR-0018）。
    fn hero_eyebrow(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 2.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 1.0, self.palette.border);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(main_page::EYEBROW)
                    .size(12.0)
                    .strong()
                    .color(self.palette.weak),
            );
        });
    }

    /// Hero 状态大字（spec §15/§27/§30/§37：正常态中性主色，语义色只做点缀）。
    fn hero_status(&self, ui: &mut egui::Ui, vm: &MainPageVm) {
        use main_page::HeroStatus::*;
        match vm.hero {
            Applied => {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(24.0, 38.0), egui::Sense::hover());
                    main_page::paint_icon(
                        ui.painter(),
                        IconKind::Check,
                        rect.center(),
                        self.palette.success,
                        17.0,
                        0.0,
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(self.strings.status_deployed)
                            .size(36.0)
                            .strong()
                            .color(self.palette.ink),
                    );
                });
            }
            NotApplied => {
                ui.label(
                    egui::RichText::new(self.strings.status_not_deployed)
                        .size(36.0)
                        .strong()
                        .color(self.palette.ink),
                );
            }
            Busy => {
                ui.label(
                    egui::RichText::new(self.busy_stage_text())
                        .size(36.0)
                        .strong()
                        .color(self.palette.sub),
                );
            }
            Failed => {
                ui.label(
                    egui::RichText::new(self.strings.op_failed)
                        .size(36.0)
                        .strong()
                        .color(self.palette.danger),
                );
            }
        }
    }

    /// 当前 busy 阶段文案（Hero 与 Primary busy 共享；描述阶段而非按钮名，spec §27）。
    fn busy_stage_text(&self) -> String {
        let kind = self.gate.current().expect("busy hero 必有忙碌种类");
        self.strings.busy_label(kind).to_string()
    }

    /// Supporting Text（spec §17）：默认不显，仅需解释状态或 Primary 不可用时。
    fn hero_supporting(&self, ui: &mut egui::Ui, vm: &MainPageVm) {
        use main_page::Supporting::*;
        let Some(sup) = vm.supporting else {
            return;
        };
        let (title, body) = match sup {
            FilesMissing => (
                self.strings.sup_files_title,
                self.strings.hint_download_patch,
            ),
            PathInvalid => (self.strings.sup_path_title, self.strings.sup_path_body),
        };
        ui.label(
            egui::RichText::new(title)
                .size(13.5)
                .strong()
                .color(self.palette.ink),
        );
        ui.label(
            egui::RichText::new(body)
                .size(13.5)
                .color(self.palette.weak),
        );
    }

    /// Primary CTA（spec §18/§19）：Solid Brand Blue，单行 Icon+Label 整体居中；busy 原位阶段化。
    fn hero_primary(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        vm: &MainPageVm,
    ) -> Option<MainEvent> {
        let (label, icon, enabled, phase) = match vm.primary.kind {
            PrimaryKind::BusyStage => {
                ctx.request_repaint(); // spinner 动画
                (
                    self.busy_stage_text(),
                    IconKind::Spinner,
                    false,
                    ctx.input(|i| i.time) as f32,
                )
            }
            PrimaryKind::Action(a) => {
                let (label, icon) = match a {
                    PrimaryAction::ApplyAndLaunch => (
                        self.strings.btn_apply_and_launch.to_string(),
                        IconKind::Play,
                    ),
                    PrimaryAction::Launch => (self.strings.btn_launch.to_string(), IconKind::Play),
                    PrimaryAction::Restart => (
                        self.strings.btn_restart_steam.to_string(),
                        IconKind::Restart,
                    ),
                    PrimaryAction::FixPath => {
                        (self.strings.pri_fix_path.to_string(), IconKind::Settings)
                    }
                };
                (label, icon, vm.primary.enabled, 0.0)
            }
        };
        let width = 340.0_f32.min(ui.available_width());
        let size = egui::vec2(width, 44.0);
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(size, sense);
        if ui.is_rect_visible(rect) {
            let hovered = enabled && response.hovered();
            let fill = if !enabled {
                self.palette.entry
            } else if hovered {
                self.palette.accent_hover
            } else {
                self.palette.accent
            };
            let fg = if enabled {
                self.palette.white
            } else {
                self.palette.weak
            };
            let painter = ui.painter();
            painter.rect(
                rect,
                egui::CornerRadius::same(R_CTA),
                fill,
                egui::Stroke::NONE,
                egui::StrokeKind::Inside,
            );
            // Icon + Label 整体居中（spec §18.1：不换行，不为英文缩字）。
            let font = egui::FontId::proportional(15.0);
            let tw = text_width(ui, &label, &font, fg);
            let icon_w = 19.0;
            let gap = 8.0;
            let group_w = tw + icon_w + gap;
            let left = rect.center().x - group_w / 2.0;
            main_page::paint_icon(
                painter,
                icon,
                egui::pos2(left + icon_w / 2.0, rect.center().y),
                fg,
                15.0,
                phase,
            );
            painter.text(
                egui::pos2(left + icon_w + gap, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                font,
                fg,
            );
        }
        if response.clicked() && enabled {
            match vm.primary.kind {
                PrimaryKind::Action(PrimaryAction::ApplyAndLaunch) => {
                    return Some(MainEvent::Action(Action::ApplyAndLaunch));
                }
                PrimaryKind::Action(PrimaryAction::Launch) => {
                    return Some(MainEvent::Action(Action::Launch));
                }
                PrimaryKind::Action(PrimaryAction::Restart) => {
                    return Some(MainEvent::Action(Action::Restart));
                }
                PrimaryKind::Action(PrimaryAction::FixPath) => return Some(MainEvent::FixPath),
                PrimaryKind::BusyStage => unreachable!("disabled busy 按钮不可点击"),
            }
        }
        None
    }

    /// Patch Update Check（spec §26）：同一按钮位 检查 → 下载；检查中/下载中 spinner 原位；
    /// 结论行内表达（上游结果由 update_flow 派生，版本永不渲染，ADR-0014）。
    fn patch_update_check(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        vm: &MainPageVm,
    ) -> Option<MainEvent> {
        let spin = matches!(
            vm.update.kind,
            UpdateKind::Checking | UpdateKind::Downloading
        );
        let (label, icon) = match vm.update.kind {
            UpdateKind::Checking => (self.strings.up_checking, IconKind::Spinner),
            UpdateKind::Downloading => (self.strings.busy_downloading, IconKind::Spinner),
            UpdateKind::Check => (
                self.strings.settings_btn_patch_update_check,
                IconKind::Refresh,
            ),
            UpdateKind::Download => (self.strings.btn_download_and_extract, IconKind::Download),
        };
        let mut event: Option<MainEvent> = None;
        ui.horizontal(|ui| {
            if spin {
                ctx.request_repaint();
            }
            let font = egui::FontId::proportional(13.0);
            let enabled = vm.update.enabled;
            let icon_w = 16.0;
            let tw = text_width(ui, label, &font, self.palette.sub);
            let width = (tw + icon_w + 26.0).max(130.0);
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(width, 30.0),
                if enabled {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            if ui.is_rect_visible(rect) {
                let hovered = enabled && response.hovered();
                if hovered {
                    ui.painter().rect(
                        rect,
                        egui::CornerRadius::same(R_SMALL),
                        self.palette.entry,
                        egui::Stroke::new(1.0, self.palette.border),
                        egui::StrokeKind::Inside,
                    );
                }
                let fg = if !enabled {
                    self.palette.weak
                } else if hovered {
                    self.palette.ink
                } else {
                    self.palette.sub
                };
                let phase = if spin {
                    ctx.input(|i| i.time) as f32
                } else {
                    0.0
                };
                main_page::paint_icon(
                    ui.painter(),
                    icon,
                    egui::pos2(rect.left() + 13.0, rect.center().y),
                    fg,
                    14.0,
                    phase,
                );
                ui.painter().text(
                    egui::pos2(rect.left() + 28.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    font,
                    fg,
                );
            }
            if response.clicked() && enabled {
                match vm.update.kind {
                    UpdateKind::Check => event = Some(MainEvent::Check),
                    UpdateKind::Download => {
                        if let Some(info) = self
                            .update_flow
                            .derived(self.local_known_version())
                            .download
                        {
                            event = Some(MainEvent::Download(info.clone()));
                        }
                    }
                    _ => {}
                }
            }
            if let Some(note) = vm.update.note {
                ui.add_space(6.0);
                match note {
                    UpdateConclusion::UpToDate => {
                        ui.label(
                            egui::RichText::new(self.strings.settings_patch_up_to_date)
                                .size(12.5)
                                .color(self.palette.success),
                        );
                    }
                    UpdateConclusion::NewVersion => {
                        ui.label(
                            egui::RichText::new(self.strings.settings_patch_new_version)
                                .size(12.5)
                                .color(self.palette.success),
                        );
                    }
                    UpdateConclusion::CheckFailed => {
                        // 行内只显示词条；失败详情向 update_flow 现查做 hover（不占布局，§26.7）。
                        let detail = self
                            .update_flow
                            .derived(self.local_known_version())
                            .notice
                            .and_then(|n| match n {
                                crate::update_flow::UpdateNotice::CheckFailed(e) => {
                                    Some(self.strings.update_error(e))
                                }
                                _ => None,
                            })
                            .unwrap_or_default();
                        ui.label(
                            egui::RichText::new(self.strings.up_check_failed)
                                .size(12.5)
                                .color(self.palette.danger),
                        )
                        .on_hover_text(detail);
                    }
                }
            }
        });
        event
    }

    /// Secondary Action Group（spec §21-§23）：纵向 Inline 行，Visual Weight 低、整行可点；
    /// 警示行（退出 Steam 并卸载）专用 Warning Secondary Blue（ADR-0010 警戒组）。
    fn secondary_group(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, vm: &MainPageVm) {
        if vm.secondary.rows.is_empty() {
            return;
        }
        for (i, row) in vm.secondary.rows.iter().enumerate() {
            let label = match row.action {
                SecondaryAction::LaunchNormal => self.strings.btn_launch_normal,
                SecondaryAction::Uninstall => self.strings.btn_uninstall,
                SecondaryAction::ExitAndUninstall => self.strings.btn_exit_and_uninstall,
                SecondaryAction::UninstallAndRestart => self.strings.btn_uninstall_and_restart,
            };
            let icon = match row.action {
                SecondaryAction::LaunchNormal => IconKind::Play,
                SecondaryAction::Uninstall | SecondaryAction::UninstallAndRestart => {
                    IconKind::Uninstall
                }
                SecondaryAction::ExitAndUninstall => IconKind::Exit,
            };
            let base_fg = if row.warn_blue {
                self.palette.caution.fg
            } else {
                self.palette.sub
            };
            let size = egui::vec2(ui.available_width().max(0.0), 40.0);
            let (rect, response) = ui.allocate_exact_size(
                size,
                if row.enabled {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                // 行间极轻分隔线（spec §23 不建完整 Card）。
                if i > 0 {
                    let line_color = theme::blend(self.palette.border, self.palette.panel, 0.55);
                    painter.line_segment(
                        [
                            egui::pos2(rect.left() + 12.0, rect.top()),
                            egui::pos2(rect.right() - 12.0, rect.top()),
                        ],
                        egui::Stroke::new(1.0, line_color),
                    );
                }
                let hovered = row.enabled && response.hovered();
                if hovered {
                    painter.rect(
                        rect,
                        egui::CornerRadius::same(R_ROW),
                        self.palette.entry,
                        egui::Stroke::NONE,
                        egui::StrokeKind::Inside,
                    );
                }
                let fg = if !row.enabled {
                    self.palette.weak
                } else if hovered {
                    self.palette.ink
                } else {
                    base_fg
                };
                main_page::paint_icon(
                    painter,
                    icon,
                    egui::pos2(rect.left() + 14.0, rect.center().y),
                    fg,
                    15.0,
                    0.0,
                );
                painter.text(
                    egui::pos2(rect.left() + 38.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(13.5),
                    fg,
                );
            }
            if response.clicked() && row.enabled {
                match row.action {
                    // 未运行时「卸载补丁」：退出无进程可退 → no-op，等效纯卸载（workflow::plan 语义）；
                    // 走免确认路径（§45），不被点击瞬间 Steam 已运行的状态误引入确认框（§20.3）。
                    SecondaryAction::Uninstall => {
                        self.request_action_quiet(ctx, Action::ExitAndUninstall);
                    }
                    SecondaryAction::LaunchNormal => {
                        self.request_action(ctx, Action::Launch);
                    }
                    SecondaryAction::ExitAndUninstall => {
                        self.request_action(ctx, Action::ExitAndUninstall);
                    }
                    SecondaryAction::UninstallAndRestart => {
                        self.request_action(ctx, Action::UninstallAndRestart);
                    }
                }
            }
        }
    }

    fn local_known_version(&self) -> Option<&str> {
        dll::dlls_present()
            .then_some(self.local_version.as_deref())
            .flatten()
    }

    /// 状态栏 dock 内容高（上 11 + 行 18 + 下 9 = 38lp；分隔线手绘在面板顶不占布局高）。
    const STATUS_DOCK_H: f32 = 38.0;

    /// 返回内容净高（header + 内容列，不含 dock 与弹性留白），供首帧窗口自适应。
    fn main_content(&mut self, ui: &mut egui::Ui) -> f32 {
        let ctx = ui.ctx().clone();
        let items = status_bar_items(
            self.steam_running,
            self.gate.current(),
            self.notice.as_ref(),
            &self.strings,
            self.palette,
        );
        // 状态栏：窗口底部专用 Dock，横跨全宽、独立于内容列（spec §32）。
        egui::Panel::bottom("status_dock")
            .frame(egui::Frame::new().fill(self.palette.panel))
            // 首帧即用稳定高度：Panel 首帧 default_outer_size 为 None 时会回退到
            // interact_size（≈20lp），导致中央面板首帧多 18lp、卡片先偏下一帧再上移（跳动）。
            .default_size(Self::STATUS_DOCK_H)
            .show_separator_line(false)
            .show(ui, |ui| {
                // 顶部分隔线横跨全宽（原型 status-dock border-top，不受内容 padding 影响）。
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.left_top(), rect.right_top()],
                    egui::Stroke::new(1.0, self.palette.border),
                );
                // 本布局内禁用行间距（否则每次子块 allocate 附加 item_spacing.y，
                // 内容行会被推偏、上下不对称，实测 +10lp）。
                ui.spacing_mut().item_spacing.y = 0.0;
                // 内容行固定高、垂直居中：字形视觉中心在行框内偏上 ~1.5lp
                // （egui 行框含 descent 空白），故上 11 / 下 9 补偿；左缩进收窄到 16lp。
                ui.add_space(11.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 18.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(16.0);
                        for (text, color) in &items {
                            status_item(ui, text, *color);
                        }
                    },
                );
                ui.add_space(9.0);
            });
        let mut content_h = 0.0f32;
        egui::CentralPanel::default().show(ui, |ui| {
            let top0 = ui.cursor().top();
            self.header(ui);
            let header_h = ui.cursor().top() - top0;
            // 垂直呼吸（spec §39）：上:下 ≈ φ:1；内容净高基准取正常态观感与最坏态防裁的
            // 折中（430lp：正常态 331lp 重心 ≈0.44 略偏下；最坏态 445lp 顶 79 + 列 445 仍
            // 在可用区 618 内，底部留白被裁亦不伤内容）。
            let avail_h = ui.available_height();
            let breathing = (avail_h - 430.0).max(0.0);
            ui.add_space(breathing * 0.618);
            // 内容列 max-width 居中，内部左对齐（Primary CTA / Secondary 行靠左，见原型）。
            let col_w = MAIN_COLUMN_WIDTH.min(ui.available_width());
            let pad = ((ui.available_width() - col_w) / 2.0).max(0.0);
            let mut col_h = 0.0f32;
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    let c0 = ui.cursor().top();
                    self.build_main_column(ui, &ctx);
                    col_h = ui.cursor().top() - c0;
                });
            });
            ui.add_space(breathing * 0.382);
            content_h = header_h + col_h;
        });
        content_h
    }

    /// 内容列：Hero → Health Warning → Secondary Group（spec §5/§24.1 定稿顺序）。
    fn build_main_column(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // 检查结论快照：当场向 update_flow 查询并转 owned（版本永不渲染，ADR-0014）。
        let derived = self.update_flow.derived(self.local_known_version());
        let input = crate::main_page::MainPageInput {
            deploy: self.status,
            steam_running: self.steam_running,
            busy: self.gate.current(),
            dlls_present: dll::dlls_present(),
            update_downloadable: derived.download.is_some(),
            update_notice: derived.notice.map(|n| match n {
                crate::update_flow::UpdateNotice::UpToDate => UpdateConclusion::UpToDate,
                crate::update_flow::UpdateNotice::NewVersion => UpdateConclusion::NewVersion,
                crate::update_flow::UpdateNotice::CheckFailed(_) => UpdateConclusion::CheckFailed,
            }),
            apply_failed: matches!(
                self.notice,
                Some(Notice::WorkflowDone(Action::ApplyAndLaunch, Err(_)))
            ),
        };
        let vm = main_page::derive(&input);
        self.hero_surface(ui, ctx, &vm);
        // 内容列统一 14 间隙（原型 content gap）：hero→health→secondary。
        ui.add_space(14.0);
        self.health_warning_line(ui);
        self.secondary_group(ui, ctx, &vm);
    }
}

impl eframe::App for App {
    /// 最小化检测、托盘事件处理与 repaint 续命必须放在这里（eframe 0.36 窗口不可见时不调用 `App::ui`），否则窗口一最小化逻辑就停摆。
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.pending_focus {
            self.pending_focus = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // 向导激活时拦截窗口关闭：取消退出并等同「跳过」落到主界面（永不把用户锁在向导里）。
        if self.wizard.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.wizard_event(ctx, wizard::Event::Closed);
        }

        self.handle_tray_events();

        if let Some(event) = self.steam_monitor.tick() {
            self.steam_running = event == SteamEvent::Started;
            if let Some(visible) = auto_tray_policy(event, self.window_visible) {
                self.set_window_visible(visible);
            }
        }

        let repaint_interval = if self.window_visible {
            process::STEAM_REFRESH_INTERVAL
        } else {
            std::time::Duration::from_millis(100)
        };
        ctx.request_repaint_after(repaint_interval);

        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        if minimized && !self.was_minimized && self.minimize_to_tray {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.window_visible = false;
        }
        self.was_minimized = minimized;

        self.handle_messages();

        // 每帧同步当前色板（放在 logic 而非 ui：窗口隐藏/托盘隐身期 ui() 不调用，托盘图标
        // 仍需随 System 模式 OS 切深浅换版）。mark 按模式重载（Dark 烤白 / L1 保留黑 mark）。
        let palette = match ctx.theme() {
            egui::Theme::Dark => Palette::dark(),
            egui::Theme::Light => Palette::light(),
        };
        if palette != self.palette {
            self.palette = palette;
            if palette.dark_mode != self.mark_dark {
                self.mark_dark = palette.dark_mode;
                self.github_mark = load_github_mark(ctx, palette);
                // 图标双态（ADR-0016 / GLOSSARY「应用图标」）：主页面 LOGO、窗口标题栏 +
                // 任务栏按钮、托盘图标随主题同切，与 mark 共用同一模式变化钩子。
                self.logo = brand::logo_texture(ctx, palette.dark_mode);
                self.ctx
                    .send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(
                        brand::window_icon(palette.dark_mode),
                    ))));
                if let Some(tray) = &self.tray
                    && let Err(e) = tray.set_icon(brand::tray_icon(palette.dark_mode))
                {
                    log_warn(format!("set tray icon: {e}"));
                }
            }
        }
        // 首帧窗口图标同步：实测 ViewportCommand::Icon 在窗口显示前不生效（App::new 期间
        // 发送被吞，标题栏沿用 ViewportBuilder 图标，见 main.rs），故等窗口真实显示后再发
        // 一次，校正 System 模式按系统深浅的主题（隐藏期不触发，恢复显示后下一帧补上）。
        // 幂等，仅首帧触发。
        if !self.icon_synced && ctx.input(|i| i.viewport().visible()).unwrap_or(false) {
            self.icon_synced = true;
            self.ctx
                .send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(
                    brand::window_icon(self.palette.dark_mode),
                ))));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 色板与图标已在 logic() 按当前主题同步，此处只消费 self.palette / self.logo。
        let ctx = ui.ctx().clone();

        let mut content_h = 0.0f32;
        egui::CentralPanel::default().show(ui, |ui| {
            if self.wizard.is_some() {
                self.wizard_ui(&ctx, ui);
                content_h = ui.cursor().top();
            } else {
                content_h = self.main_content(ui);
            }
        });

        // 首帧自适应：仅在内容放不下时增长窗口，从不缩窗（保持 spec §40 初始 620×520）。
        if self.wizard.is_none() && !self.autosized && content_h > 0.0 {
            self.autosized = true;
            let need_h = autosize_inner_height(content_h + Self::STATUS_DOCK_H);
            let cur_h = ctx.input(|i| i.viewport().inner_rect.map_or(0.0, |r| r.height()));
            if need_h > cur_h + 1.0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    ui.available_width().max(620.0),
                    need_h,
                )));
            }
        }

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
                    if styled_button(
                        ui,
                        self.strings.yes,
                        ButtonStyle::Primary,
                        egui::vec2(72.0, 30.0),
                        true,
                        self.palette,
                    )
                    .clicked()
                    {
                        confirmed = true;
                    }
                    ui.add_space(4.0);
                    if styled_button(
                        ui,
                        self.strings.no,
                        ButtonStyle::Neutral,
                        egui::vec2(72.0, 30.0),
                        true,
                        self.palette,
                    )
                    .clicked()
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

        self.settings_dialog(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_fallback_enables_chinese_glyph() {
        let Some(font_data) = read_system_cjk_font() else {
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

    #[test]
    fn action_symbols_render_real_glyphs_in_app_font_stack() {
        let mut defs = egui::FontDefinitions::default();
        if let Some(font_data) = read_system_cjk_font() {
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
        }
        let mut fonts =
            egui::epaint::text::Fonts::new(egui::epaint::text::TextOptions::default(), defs);
        let uv = |fonts: &mut egui::epaint::text::Fonts, text: &str| {
            let mut view = fonts.with_pixels_per_point(1.0);
            let galley = view.layout(
                text.to_owned(),
                egui::FontId::proportional(14.0),
                Palette::dark().white,
                f32::INFINITY,
            );
            galley
                .rows
                .iter()
                .flat_map(|r| r.glyphs.iter().map(|g| g.uv_rect))
                .collect::<Vec<_>>()
        };
        let tofu = [uv(&mut fonts, "◻"), uv(&mut fonts, "?")];
        for (sym, name) in [('▶', "应用/启动"), ('↻', "重启"), ('⏏', "卸载类")] {
            let rendered = uv(&mut fonts, &sym.to_string());
            assert_eq!(
                rendered.len(),
                1,
                "{name} 符号 {sym:?} 布局应恰好产出一个字形"
            );
            assert!(
                rendered != tofu[0] && rendered != tofu[1],
                "{name} 符号 {sym:?}（U+{:04X}）渲染成了替换框（tofu）",
                sym as u32
            );
        }
    }

    #[test]
    fn default_fonts_lack_cjk_glyph() {
        let defs = egui::FontDefinitions::default();
        let mut fonts =
            egui::epaint::text::Fonts::new(egui::epaint::text::TextOptions::default(), defs);
        assert!(!fonts.has_glyph(&egui::FontId::proportional(14.0), '中'));
    }

    #[test]
    fn auto_tray_policy_table() {
        assert_eq!(auto_tray_policy(SteamEvent::Started, true), Some(false));
        assert_eq!(auto_tray_policy(SteamEvent::Stopped, false), Some(true));
        assert_eq!(auto_tray_policy(SteamEvent::Started, false), None);
        assert_eq!(auto_tray_policy(SteamEvent::Stopped, true), None);
    }

    #[test]
    fn health_warning_only_for_pending_and_missing() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            for (summary, expect) in [
                (CompatSummary::Checking, None),
                (CompatSummary::Ready, None),
                (CompatSummary::Online, None),
                (CompatSummary::Network, None),
                (CompatSummary::Pending, Some(s.main_warning_pending)),
                (CompatSummary::Missing, Some(s.main_warning_missing)),
            ] {
                assert_eq!(health_warning(&s, summary), expect, "{lang:?} {summary:?}");
            }
            assert!(s.main_warning_pending.contains("Steam"), "{lang:?}");
            assert!(s.main_warning_missing.contains("Steam"), "{lang:?}");
        }
    }

    #[test]
    fn render_notice_update_checked_is_defensive() {
        let s = Strings::new(Lang::Zh);
        assert_eq!(
            render_notice(&s, &Notice::UpdateChecked),
            (true, String::new())
        );
    }

    #[test]
    fn render_notice_other_branches_both_langs() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            let e = updater::UpdateError::Network("t".into());
            let wf = workflow::WorkflowError {
                op: workflow::Op::Launch,
                message: "m".into(),
            };
            assert_eq!(
                render_notice(&s, &Notice::WorkflowDone(workflow::Action::Launch, Ok(()))),
                (true, s.success_text(workflow::Action::Launch).to_string())
            );
            assert_eq!(
                render_notice(
                    &s,
                    &Notice::WorkflowDone(workflow::Action::Launch, Err(wf.clone()))
                ),
                (false, s.workflow_error_text(&wf))
            );
            assert_eq!(
                render_notice(&s, &Notice::Precheck(workflow::Precheck::InvalidSteamDir)),
                (false, s.precheck_text(&workflow::Precheck::InvalidSteamDir))
            );
            assert_eq!(
                render_notice(&s, &Notice::Downloaded(Err(e.clone()))),
                (false, s.update_error(&e))
            );
            assert_eq!(
                render_notice(&s, &Notice::Downloaded(Ok(()))),
                (true, s.ok_downloaded.to_string())
            );
        }
    }

    #[test]
    fn status_bar_drops_launch_success_when_steam_stopped() {
        let zh = Strings::new(Lang::Zh);
        let texts = |running: bool, n: &Notice| -> Vec<String> {
            status_bar_items(running, None, Some(n), &zh, Palette::dark())
                .iter()
                .map(|(t, _)| t.clone())
                .collect()
        };

        let launched = Notice::WorkflowDone(workflow::Action::Launch, Ok(()));
        let stopped = texts(false, &launched);
        assert!(stopped.iter().any(|t| t == "Steam 未运行"));
        assert!(
            !stopped.iter().any(|t| t == "Steam 已启动"),
            "Steam 退出后不得再显示「Steam 已启动」：{stopped:?}"
        );
        let running = texts(true, &launched);
        assert!(running.iter().any(|t| t == "Steam 已启动"));

        let restarted = Notice::WorkflowDone(workflow::Action::Restart, Ok(()));
        assert!(!texts(false, &restarted).iter().any(|t| t == "Steam 已重启"));
        assert!(texts(true, &restarted).iter().any(|t| t == "Steam 已重启"));

        let uninstalled = Notice::WorkflowDone(workflow::Action::ExitAndUninstall, Ok(()));
        assert!(texts(false, &uninstalled).iter().any(|t| t == "已卸载补丁"));
        let applied = Notice::WorkflowDone(workflow::Action::ApplyAndLaunch, Ok(()));
        assert!(texts(false, &applied).iter().any(|t| t == "补丁已应用"));
    }

    #[test]
    fn status_bar_keeps_interaction_busy_but_drops_update_flow() {
        // §32/§34：交互类忙碌可占状态栏；检查/下载（更新流程）只在更新按钮位原位，不进状态栏。
        let zh = Strings::new(Lang::Zh);
        let texts = |busy: Option<BusyKind>| -> Vec<String> {
            status_bar_items(false, busy, None, &zh, Palette::dark())
                .iter()
                .map(|(t, _)| t.clone())
                .collect()
        };
        let deploying = texts(Some(BusyKind::Deploying));
        assert!(
            deploying.iter().any(|t| t == zh.busy_deploying),
            "交互类 busy 应占状态栏：{deploying:?}"
        );
        for kind in [BusyKind::Checking, BusyKind::Downloading] {
            let got = texts(Some(kind));
            assert!(
                !got.iter().any(|t| t == zh.busy_label(kind)),
                "{kind:?} 不进状态栏：{got:?}"
            );
        }
    }

    #[test]
    fn primary_cta_fits_longest_english_label() {
        // spec §42：英文 Primary 单行不换行、不缩字——在 340lp 基线宽度内放得下。
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 520.0),
            )),
            ..Default::default()
        };
        let mut w = 0.0f32;
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let font = egui::FontId::proportional(15.0);
                let s = Strings::new(Lang::En);
                let text = s.btn_apply_and_launch;
                w = text_width(ui, text, &font, Palette::dark().white) + 19.0 + 8.0;
            });
        });
        full.textures_delta.clear();
        assert!(
            w <= 340.0,
            "最长 Primary 英文文案 + 图标在 340lp 内放不下（实际 {w:.0}lp）"
        );
    }

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
                let short = styled_button(
                    ui,
                    "检查更新",
                    ButtonStyle::Neutral,
                    egui::vec2(96.0, 32.0),
                    true,
                    Palette::dark(),
                )
                .rect
                .width();
                let long = styled_button(
                    ui,
                    "Download & Extract New Version",
                    ButtonStyle::Primary,
                    egui::vec2(150.0, 32.0),
                    true,
                    Palette::dark(),
                )
                .rect
                .width();
                let short_en = styled_button(
                    ui,
                    "Check Update",
                    ButtonStyle::Neutral,
                    egui::vec2(96.0, 32.0),
                    true,
                    Palette::dark(),
                )
                .rect
                .width();
                out = Some((short, long, short_en));
            });
        });
        full.textures_delta.clear();
        let (short, long, short_en) = out.unwrap();
        assert_eq!(short, 96.0, "检查更新 在 96px 内放下即不撑宽");
        assert!(
            short_en > 96.0,
            "Check Update 超出 96px 时应撑宽按钮，实际 {short_en}px"
        );
        assert!(
            long > short_en,
            "Download & Extract New Version 应比 Check Update 更宽，实际 {long}px"
        );
    }

    #[test]
    fn steam_path_editor_submit_commits_valid_dir_only() {
        let dir = std::env::temp_dir().join(format!("ost_steam_tab_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut e = SteamPathEditor::new("");
        assert_eq!(e.submit(""), SteamPathCommit::Unchanged);
        assert!(!e.invalid, "空串不应触发内联错误");
        assert_eq!(
            e.submit("C:/Steam"),
            SteamPathCommit::Changed(String::new())
        );
        assert!(!e.invalid);

        e.buffer = "Z:/definitely/not/a/real/dir_7f3a".into();
        assert_eq!(e.submit("C:/Steam"), SteamPathCommit::Invalid);
        assert!(e.invalid, "非法提交应置内联错误");

        let p = dir.display().to_string();
        e.buffer = format!("  {p}  ");
        assert_eq!(e.submit("C:/Steam"), SteamPathCommit::Changed(p.clone()));
        assert!(!e.invalid, "有效提交应清除内联错误");

        assert_eq!(e.submit(&p), SteamPathCommit::Unchanged);
        assert!(!e.invalid);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn steam_path_editor_seeds_buffer_from_working_path() {
        let e = SteamPathEditor::new("C:/Program Files (x86)/Steam");
        assert_eq!(e.buffer, "C:/Program Files (x86)/Steam");
        assert!(!e.invalid);
    }

    #[test]
    fn settings_steam_compat_row_fits_dialog_width() {
        let ctx = egui::Context::default();
        install_theme(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 520.0),
            )),
            ..Default::default()
        };
        let mut violations = Vec::new();
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.set_width(SETTINGS_DIALOG_WIDTH);
                for lang in [Lang::Zh, Lang::En] {
                    let s = Strings::new(lang);
                    ui.horizontal(|ui| {
                        ui.allocate_exact_size(egui::vec2(10.0, 2.0), egui::Sense::hover());
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(s.compat_title).size(13.5).strong());
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let content_right = ui.cursor().right();
                        let badge = egui::Frame::new()
                            .corner_radius(egui::CornerRadius::same(12))
                            .inner_margin(egui::Margin::symmetric(10, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!("● {}", s.compat_status_online))
                                        .size(12.5),
                                );
                            })
                            .response
                            .rect;
                        if badge.right() > content_right + 0.5 {
                            violations.push(format!(
                                "{lang:?} 徽章右缘 {} 超过内容右界 {content_right}",
                                badge.right()
                            ));
                        }
                        let block_left = ui.cursor().left();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let details = styled_button(
                                ui,
                                s.compat_btn_details,
                                ButtonStyle::Neutral,
                                egui::vec2(88.0, 26.0),
                                true,
                                Palette::dark(),
                            );
                            let precache = styled_button(
                                ui,
                                s.compat_btn_precache,
                                ButtonStyle::Neutral,
                                egui::vec2(132.0, 26.0),
                                true,
                                Palette::dark(),
                            );
                            if precache.rect.left() + 0.5 < block_left {
                                violations.push(format!(
                                    "{lang:?} 预热按钮左缘 {} 越过右块起点 {block_left}（与徽章重叠）",
                                    precache.rect.left()
                                ));
                            }
                            if details.rect.right() > content_right + 0.5 {
                                violations.push(format!(
                                    "{lang:?} 详细信息右缘 {} 超过内容右界 {content_right}",
                                    details.rect.right()
                                ));
                            }
                        });
                    });
                }
            });
        });
        full.textures_delta.clear();
        assert!(
            violations.is_empty(),
            "兼容性小节在对话框宽度 {SETTINGS_DIALOG_WIDTH} 下溢出:\n{}",
            violations.join("\n")
        );
    }

    #[test]
    fn wizard_card_is_horizontally_centered() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 520.0),
            )),
            ..Default::default()
        };
        let mut card = None;
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let view = wizard::View {
                    step: WizardStep::Language,
                    language: Language::Auto,
                    theme: ThemePreference::System,
                    steam_path: String::new(),
                    path_valid: false,
                    download: wizard::DownloadState::Idle,
                };
                let (_, rect) = wizard_steps_ui(ui, Strings::new(Lang::Zh), &view, Palette::dark());
                card = Some(rect);
            });
        });
        full.textures_delta.clear();
        let rect = card.expect("向导卡片应有矩形");
        assert!(
            (rect.center().x - 320.0).abs() < 0.5,
            "卡片应水平居中，center.x={}，期望 320",
            rect.center().x
        );
        let card_w = WIZARD_CARD_WIDTH + card_frame(Palette::dark()).total_margin().sum().x;
        assert!(
            (rect.width() - card_w).abs() < 1.0,
            "卡片宽应≈{card_w}，实际 {}",
            rect.width()
        );
        assert!(
            rect.width() < 500.0,
            "卡片不应是整窗宽（实际 {}",
            rect.width()
        );
    }

    #[test]
    fn wizard_step_numbers_run_1_to_4() {
        assert_eq!(wizard_step_number(WizardStep::Language), 1);
        assert_eq!(wizard_step_number(WizardStep::Theme), 2);
        assert_eq!(wizard_step_number(WizardStep::SteamPath), 3);
        assert_eq!(wizard_step_number(WizardStep::Download), 4);
    }

    #[test]
    fn wizard_step3_ready_presents_done_not_download() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            let c = wizard_step3_content(&s, &wizard::DownloadState::Ready);
            assert_eq!(c.prompt, s.wizard_download_ready);
            assert!(!c.danger);
            assert_eq!(
                c.primary,
                Some((s.wizard_btn_done.to_string(), wizard::Event::SkipDownload))
            );
            assert!(!c.show_skip, "{lang:?} 就绪态不应显示跳过（完成即收尾）");
            assert_ne!(
                c.prompt, s.wizard_download_prompt,
                "{lang:?} 就绪态不得宣称「尚未下载」"
            );
        }
    }

    #[test]
    fn wizard_step3_other_states_unchanged() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            let idle = wizard_step3_content(&s, &wizard::DownloadState::Idle);
            assert_eq!(idle.prompt, s.wizard_download_prompt);
            assert!(!idle.danger);
            assert_eq!(
                idle.primary,
                Some((
                    s.wizard_btn_download.to_string(),
                    wizard::Event::DownloadRequested
                ))
            );
            assert!(idle.show_skip);

            let running = wizard_step3_content(&s, &wizard::DownloadState::Running);
            assert_eq!(running.prompt, s.wizard_download_running);
            assert!(running.primary.is_none());
            assert!(running.show_skip);

            let e = updater::UpdateError::Network("t".into());
            let failed = wizard_step3_content(&s, &wizard::DownloadState::Failed(e.clone()));
            assert_eq!(
                failed.prompt,
                s.wizard_download_failed
                    .replace("{err}", &s.update_error(&e))
            );
            assert!(failed.danger);
            assert_eq!(
                failed.primary,
                Some((
                    s.wizard_btn_retry.to_string(),
                    wizard::Event::DownloadRequested
                ))
            );
            assert!(failed.show_skip);
        }
    }

    fn slim_main_content_height(ctx: &egui::Context, lang: Lang) -> f32 {
        // 最简主页面一行（无 supporting / health / secondary 时）的内容高：
        // header + Hero（eyebrow + 状态 + Primary + update），供设置对话框自适应窗口测量。
        let mut h = 0.0f32;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 800.0),
            )),
            ..Default::default()
        };
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let s = Strings::new(lang);
                // header 简化（品牌 mark + 标题 + 齿轮 + hairline）
                ui.horizontal(|ui| {
                    let _ = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                    ui.add_space(11.0);
                    ui.label(
                        egui::RichText::new(s.app_title)
                            .size(15.0)
                            .strong()
                            .color(Palette::dark().ink),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let _ = gear_button(ui, s.settings_gear_tooltip, Palette::dark());
                    });
                });
                let y = ui.cursor().top();
                ui.painter().line_segment(
                    [
                        egui::pos2(ui.min_rect().left(), y),
                        egui::pos2(ui.min_rect().left() + ui.available_width(), y),
                    ],
                    egui::Stroke::new(1.0, Palette::dark().border),
                );
                ui.add_space(12.0);
                // Hero 最简形态
                egui::Frame::new()
                    .fill(Palette::dark().card)
                    .stroke(egui::Stroke::new(1.0, Palette::dark().border))
                    .corner_radius(egui::CornerRadius::same(R_HERO))
                    .inner_margin(egui::Margin::symmetric(30, 24))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(10.0, 2.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 1.0, Palette::dark().border);
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(main_page::EYEBROW)
                                    .size(12.0)
                                    .strong()
                                    .color(Palette::dark().weak),
                            );
                        });
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new(s.status_not_deployed)
                                .size(36.0)
                                .strong()
                                .color(Palette::dark().ink),
                        );
                        ui.add_space(20.0);
                        let _ = styled_button(
                            ui,
                            s.btn_apply_and_launch,
                            ButtonStyle::Primary,
                            egui::vec2(340.0, 44.0),
                            true,
                            Palette::dark(),
                        );
                        ui.add_space(12.0);
                        let _ = styled_button(
                            ui,
                            s.settings_btn_patch_update_check,
                            ButtonStyle::Neutral,
                            egui::vec2(150.0, 30.0),
                            true,
                            Palette::dark(),
                        );
                    });
                h = ui.cursor().top();
            });
        });
        full.textures_delta.clear();
        h
    }

    fn settings_dialog_footer_bottom(lang: Lang, inner_h: f32) -> (f32, f32, f32) {
        let ctx = egui::Context::default();
        install_cjk_font(&ctx); // 与应用同字体：中文字体行高与默认字体不同，骨架测量须同源。
        install_theme(&ctx);
        let mut footer = 0.0f32;
        let mut modal_top = 0.0f32;
        let mut scroll_bottom = 0.0f32;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, inner_h),
            )),
            ..Default::default()
        };
        let mut full = ctx.run_ui(raw, |ui| {
            egui::Modal::new(egui::Id::new("settings_dialog_repro"))
                .frame(settings_dialog_frame(Palette::dark()))
                .show(ui, |ui| {
                    let s = Strings::new(lang);
                    modal_top = ui.cursor().top();
                    ui.set_width(SETTINGS_DIALOG_WIDTH);
                    ui.heading(s.settings_title);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let _ = styled_button(
                            ui,
                            s.settings_tab_general,
                            ButtonStyle::Primary,
                            egui::vec2(88.0, 28.0),
                            true,
                            Palette::dark(),
                        );
                        ui.add_space(4.0);
                        let _ = styled_button(
                            ui,
                            s.settings_tab_steam,
                            ButtonStyle::Neutral,
                            egui::vec2(88.0, 28.0),
                            true,
                            Palette::dark(),
                        );
                        ui.add_space(4.0);
                        let _ = styled_button(
                            ui,
                            s.settings_tab_about,
                            ButtonStyle::Neutral,
                            egui::vec2(88.0, 28.0),
                            true,
                            Palette::dark(),
                        );
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    let max_scroll_h = settings_scroll_height(inner_h);
                    let scroll_resp = egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .max_height(max_scroll_h)
                        .show(ui, |ui| {
                            for i in 0..12 {
                                ui.label(format!("settings section line {i}"));
                            }
                        });
                    scroll_bottom = scroll_resp.inner_rect.bottom();
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let r = styled_button(
                                ui,
                                s.btn_close,
                                ButtonStyle::Neutral,
                                egui::vec2(80.0, 30.0),
                                true,
                                Palette::dark(),
                            );
                            footer = r.rect.bottom();
                        });
                    });
                });
        });
        full.textures_delta.clear();
        (modal_top, scroll_bottom, footer)
    }

    #[test]
    fn settings_dialog_footer_visible_at_autosized_window() {
        let ctx = egui::Context::default();
        install_theme(&ctx);
        for lang in [Lang::Zh, Lang::En] {
            let content_h = slim_main_content_height(&ctx, lang);
            let inner_h = autosize_inner_height(content_h);
            let (top, _, footer) = settings_dialog_footer_bottom(lang, inner_h);
            assert!(
                top >= 0.0 && footer <= inner_h + 0.5,
                "{lang:?}: 自适应内高 {inner_h}（内容 {content_h}）下设置对话框顶 {top} / 页脚底缘 {footer} 越出窗口（被裁掉）"
            );
        }
    }

    #[test]
    fn autosize_inner_height_never_below_settings_min() {
        for content_h in [150.0, 215.0, 250.0, 280.0, 350.0, 500.0, 700.0] {
            let inner_h = autosize_inner_height(content_h);
            let min = SETTINGS_DIALOG_SKELETON_H + SETTINGS_SCROLL_MIN_H;
            assert!(
                inner_h >= min,
                "content_h={content_h} -> 自适应内高 {inner_h} 低于设置对话框最小内高 {min}"
            );
            assert!(
                SETTINGS_DIALOG_SKELETON_H + settings_scroll_height(inner_h) <= inner_h + 0.5,
                "content_h={content_h}: 骨架+滚动区超出自适应内高 {inner_h}"
            );
        }
    }

    #[test]
    fn settings_tab_row_fits_dialog_width() {
        let ctx = egui::Context::default();
        install_cjk_font(&ctx);
        install_theme(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 520.0),
            )),
            ..Default::default()
        };
        let mut violations = Vec::new();
        let mut full = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.set_width(SETTINGS_DIALOG_WIDTH);
                for lang in [Lang::Zh, Lang::En] {
                    let s = Strings::new(lang);
                    ui.horizontal(|ui| {
                        let avail_right = ui.available_width();
                        let mut last_tab_right = 0.0f32;
                        for label in [
                            s.settings_tab_general,
                            s.settings_tab_steam,
                            s.settings_tab_about,
                        ] {
                            let r = styled_button(
                                ui,
                                label,
                                ButtonStyle::Neutral,
                                egui::vec2(88.0, 28.0),
                                true,
                                Palette::dark(),
                            );
                            last_tab_right = r.rect.right();
                            ui.add_space(4.0);
                        }
                        if last_tab_right > avail_right + 0.5 {
                            violations.push(format!(
                                "{lang:?} 页签右缘 {last_tab_right} 超过可用宽 {avail_right}"
                            ));
                        }
                    });
                }
            });
        });
        full.textures_delta.clear();
        assert!(
            violations.is_empty(),
            "页签行在对话框宽度 {SETTINGS_DIALOG_WIDTH} 下溢出:\n{}",
            violations.join("\n")
        );
    }

    #[test]
    fn settings_scroll_height_clamps() {
        assert_eq!(settings_scroll_height(100.0), 200.0);
        assert_eq!(settings_scroll_height(251.0), 200.0);
        assert_eq!(settings_scroll_height(408.0), 200.0);
        assert_eq!(settings_scroll_height(444.0), 208.0);
        assert_eq!(settings_scroll_height(656.0), 420.0);
        assert_eq!(settings_scroll_height(1000.0), 420.0);
    }
}
