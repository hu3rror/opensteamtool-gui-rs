//! egui 界面：主页面（部署状态 + 操作按钮组 + 健康风险警示）+ 设置对话框 + 向导 + 确认弹窗。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use egui::Frame;

use crate::busy::{BusyGate, BusyKind};
use crate::compat;
use crate::compat_flow::{self, CompatFlow, CompatSummary};
use crate::config::{self, Config, Language};
use crate::dll::{self, DeployStatus};
use crate::i18n::{Lang, Strings};
use crate::process::{self, SteamEvent, SteamMonitor};
use crate::steam;
use crate::steam_state::SteamState;
use crate::theme::{self, ButtonStyle};
use crate::tray::{Tray, TrayAction};
use crate::update_flow::{UpdateFlow, UpdateNotice};
use crate::updater::{self, OnlineInfo, UpdateError};
use crate::wizard::{self, Step as WizardStep};
use crate::workflow::{self, Action};

// 颜色一律取自 theme.rs 语义色板（ADR-0010）：仓库唯一色值来源，勿在此处写内联色值。

/// 向导卡片内容宽度：窄于主界面卡片并整体水平居中，视觉更聚焦、更均衡。
const WIZARD_CARD_WIDTH: f32 = 430.0;

/// 设置对话框内容宽度：须容纳 Settings — Steam 页签兼容性小节首行（标题 + 徽章 +
/// 预热/详细信息按钮，英文为最宽组合，见 `settings_steam_compat_row_fits_dialog_width`）。
const SETTINGS_DIALOG_WIDTH: f32 = 580.0;

/// 设置对话框固定骨架高度（标题 + 页签行 + 分隔线 + 页脚按钮 + 窗口边距 + 模态框帧边距）。
/// 实测（测试夹具，真实 `egui::Modal` + 应用字体/主题）内容骨架约 185；取 208 保守偏大，
/// 使「骨架 + 滚动区」在任意内高下都小于等于窗口（页脚及模态框帧完整可见）。
/// 历史值 184 偏小 8px：滚动区随窗口放大后页脚始终悬在窗口底缘外 8px（#32 回归放大此缺陷）。
const SETTINGS_DIALOG_SKELETON_H: f32 = 208.0;
/// 设置对话框滚动区高度下限（低窗口时仍保留最小可滚动内容区）。
const SETTINGS_SCROLL_MIN_H: f32 = 200.0;

/// 设置对话框滚动区高度：窗口内高扣固定骨架（保守偏大，见 SETTINGS_DIALOG_SKELETON_H）。
/// 内高 ≥ 骨架 + 滚动区下限时对话框恰好放下（余量由滚动区吸收，页脚恒可见）；
/// 更低时滚动区缩到下限、页脚被窗口底部裁掉（#32 主页面精简回归的症状，
/// 见测试 `settings_dialog_footer_visible_at_autosized_window`）。
fn settings_scroll_height(window_inner_h: f32) -> f32 {
    (window_inner_h - SETTINGS_DIALOG_SKELETON_H).clamp(SETTINGS_SCROLL_MIN_H, 420.0)
}

/// 首帧自适应窗口内高：内容高 + 底部余量，且不低于设置对话框可完整呈现的最小内高
/// （骨架 + 滚动区下限）。主页面精简（#32）后内容显著变矮（实测最矮形态 215）：若只随
/// 内容收缩，窗口 251 会裁掉设置对话框页脚（用户须手动放大窗口才看得全——报告的症状），
/// 见 `settings_dialog_footer_visible_at_autosized_window`。
fn autosize_inner_height(content_h: f32) -> f32 {
    (content_h + 36.0).max(SETTINGS_DIALOG_SKELETON_H + SETTINGS_SCROLL_MIN_H)
}

fn install_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = theme::PANEL;
    visuals.window_fill = theme::CARD;
    visuals.faint_bg_color = theme::PANEL;
    visuals.extreme_bg_color = theme::PANEL; // TextEdit 底色（旧 FILL_SECONDARY 并入 panel 槽）
    // ComboBox 按钮底色走 widgets.*.weak_bg_fill（egui 默认偏灰）：统一为 CARD，
    // 与 styled_button 的 Neutral 底一致（#34 定稿：语言下拉与其他按钮同色）。
    visuals.widgets.inactive.weak_bg_fill = theme::CARD;
    visuals.widgets.open.weak_bg_fill = theme::CARD;
    visuals.override_text_color = Some(theme::INK);
    let radius = egui::CornerRadius::same(8);
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        w.corner_radius = radius;
    }
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, theme::ENTRY);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, theme::ENTRY);
    visuals.widgets.inactive.bg_fill = theme::CARD;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme::ACCENT);
    visuals.widgets.hovered.bg_fill = theme::PANEL;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, theme::accent_hover());
    visuals.selection.bg_fill = theme::selection_bg();
    visuals.selection.stroke = egui::Stroke::new(1.0, theme::ACCENT);

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
        .fill(theme::CARD)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(18, 16))
}

/// 单行文本宽度（按钮自适应撑宽与顶栏标题测宽共用）。
fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId, color: egui::Color32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font.clone(), color)
        .size()
        .x
}

/// 语义色板按钮：手动绘制底/描边/文字，hover 换色（底色由 palette 派生）。
/// `enabled=false` 时文字弱化为 muted 且不响应点击。
/// 字号随高度派生（#34 致密化）：主页面操作按钮（44px 高）用 14px，其余按钮 13px。
fn styled_button(
    ui: &mut egui::Ui,
    text: &str,
    style: ButtonStyle,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    let font_size = if size.y >= 40.0 { 14.0 } else { 13.0 };
    let palette = style.palette();
    // 长文案（如英文 "Download & Extract New Version"）超出固定宽度时会被绘制在
    // 按钮边界外截断；按文本宽度自适应，最小仍为调用方指定的 size。
    let font_id = egui::FontId::proportional(font_size);
    let text_w = text_width(ui, text, &font_id, palette.fg);
    let size = egui::vec2(size.x.max(text_w + 28.0), size.y);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let fill = if enabled && response.hovered() {
            palette.hover
        } else {
            palette.bg
        };
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
        let color = if enabled { palette.fg } else { theme::WEAK };
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

/// 顶栏设置入口：程序化绘制的齿轮图标按钮（#34）——字体无关，不依赖任何系统的
/// 字形覆盖（⚙ 在无 CJK 字体的英文系统上会变豆腐块）。配色取自 Neutral 按钮样式
/// （ADR-0010）；悬停 tooltip 保留文字名，保住可发现性。
fn gear_button(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    let palette = ButtonStyle::Neutral.palette();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let response = response.on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
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
        let color = if hovered { theme::ACCENT } else { palette.fg };
        paint_gear(ui.painter(), rect.center(), color);
    }
    response
}

/// 齿轮（最初版本，#34 定稿）：圆环 + 8 颗圆齿 + 中心点。多轮预览比选后，用户
/// 最终回到最初款式。色值只能来自语义色板槽（ADR-0010）。
fn paint_gear(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let ring_r = 6.5;
    let ring_w = 2.0;
    let tooth_r = 1.7;
    painter.circle_stroke(center, ring_r, egui::Stroke::new(ring_w, color));
    let tooth_center_r = ring_r + ring_w / 2.0 + tooth_r * 0.7;
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        let p = center + egui::vec2(a.cos() * tooth_center_r, a.sin() * tooth_center_r);
        painter.circle_filled(p, tooth_r, color);
    }
    painter.circle_filled(center, 1.6, color);
}

/// 关于页项目主页链接按钮（#34 定稿）：GitHub mark 绘制在按钮内部左侧 + 项目名，
/// 整钮可点（悬停显示完整 URL）。底色同 Neutral 按钮（CARD 底 + ENTRY 描边）。
fn github_link_button(
    ui: &mut egui::Ui,
    mark: Option<&egui::TextureHandle>,
    label: &str,
    tooltip: &str,
) -> egui::Response {
    let palette = ButtonStyle::Neutral.palette();
    let font_id = egui::FontId::proportional(13.0);
    let text_w = text_width(ui, label, &font_id, palette.fg);
    let icon_w = if mark.is_some() { 16.0 + 8.0 } else { 0.0 };
    let size = egui::vec2((text_w + 28.0 + icon_w).max(200.0), 32.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
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
        // 内容组（图标 + 文字）整体居中于按钮。
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
                theme::WHITE,
            );
        }
        ui.painter().text(
            egui::pos2(left + icon_w, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font_id,
            palette.fg,
        );
    }
    response
}

/// 内嵌的 GitHub 单色 mark（官方 logo 资源缩至 64px；点击入口跳仓库页，符合 GitHub
/// logo 使用政策的「链接到项目」场景）。
const GITHUB_MARK_PNG: &[u8] = include_bytes!("../assets/github-mark.png");

/// 解码内嵌 GitHub mark 为纹理；失败返回 None（渲染层退回纯文字入口，不崩）。
fn load_github_mark(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(GITHUB_MARK_PNG).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img);
    Some(ctx.load_texture("github-mark", color, egui::TextureOptions::LINEAR))
}

/// 卡片标题：3px accent 色 bar + 标题。
fn card_title(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 13.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, theme::ACCENT);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(text)
                .size(13.5)
                .strong()
                .color(theme::INK),
        );
    });
}

/// 底部状态栏条目：色点 + 同色文案（颜色语义见 status_bar；条目间自带间距）。
fn status_item(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).size(12.5).color(color));
    ui.add_space(10.0);
}

/// 状态行：纯文字 + 颜色（无圆点徽章）。
fn status_line(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).size(13.0).strong().color(color));
}
/// 防御分支日志（理论不可达路径，统一前缀便于过滤）。
fn log_warn(msg: impl std::fmt::Display) {
    eprintln!("[opensteamtool-manager] {msg}");
}

/// 渲染最近一次结果提示为当前语言文案。
/// 纯函数（不依赖 App）：切换语言后无需重建 notice，重渲染即得新语言。
/// 检查更新结果的分类来自「更新流程」派生（`status_bar` 对 UpdateChecked 单独分流），此处不处理。
fn render_notice(s: &Strings, notice: &Notice) -> (bool, String) {
    match notice {
        Notice::Downloaded(Ok(())) => (true, s.ok_downloaded.to_string()),
        Notice::Downloaded(Err(e)) => (false, s.update_error(e)),
        Notice::WorkflowDone(action, Ok(())) => (true, s.success_text(*action).to_string()),
        Notice::WorkflowDone(_, Err(e)) => (false, s.workflow_error_text(e)),
        Notice::Precheck(p) => (false, s.precheck_text(p)),
        Notice::UpdateChecked => {
            // 防御分支：正常路径该变体经 status_bar 分流（update_flow 派生），不直达此处；直达则记日志并降级为中性空提示。
            log_warn("render_notice 收到 Notice::UpdateChecked（应经 status_bar 分流）");
            (true, String::new())
        }
    }
}

/// 补丁更新检查结果 → 当前语言文案（分类来自「更新流程」派生；同时服务设置对话框与主页面通知栏）。
/// **永不渲染补丁版本号**（#30 验收）：版本比较只在流程内部完成，
/// 「已是最新 / 发现新补丁 / 检查失败」三种结果均无版本数字。
fn render_patch_notice(s: &Strings, n: &UpdateNotice) -> (bool, String) {
    match n {
        UpdateNotice::UpToDate => (true, s.settings_patch_up_to_date.to_string()),
        UpdateNotice::NewVersion => (true, s.settings_patch_new_version.to_string()),
        UpdateNotice::CheckFailed(e) => (false, s.update_error(e)),
    }
}

/// 主页面健康风险警示文案（#32）：仅「上游尚未适配 / 未找到核心 DLL」两态产出，
/// 其余健康态（检查中 / 网络不可用 / 已适配未缓存）一律 `None`——瞬时态与正常态不打扰。
fn health_warning(s: &Strings, summary: CompatSummary) -> Option<&'static str> {
    match summary {
        CompatSummary::Pending => Some(s.main_warning_pending),
        CompatSummary::Missing => Some(s.main_warning_missing),
        _ => None,
    }
}

/// n 枚等宽按钮并排时的单按钮宽度：`available` 减去 (n-1) 个手动 gap 与
/// (n-1) 个 egui 自动插入的 item_spacing 后再均分（egui 在每个 widget 后
/// 都追加 item_spacing，见 `Layout::advance_after_rects`）。公式漏掉任一项
/// 会导致按钮行实际占宽超过可用宽度，溢出并把下方依赖 `available_width`
/// 撑满的卡片顶到窗口右缘（历史 bug）。
fn row_button_width(available: f32, gap: f32, item_spacing: f32, count: u32) -> f32 {
    let n = count.max(1) as f32;
    ((available - (n - 1.0) * (gap + item_spacing)) / n).max(150.0)
}

/// 后台线程 → UI 线程的消息。
enum Msg {
    /// 后台阶段变化（如 kill Steam 完成后进入部署阶段）。
    Phase(BusyKind),
    UpdateChecked(Result<OnlineInfo, UpdateError>),
    Downloaded(Result<(), UpdateError>),
    /// 组合操作完成（成功/失败，携带动作以取成功文案）。
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
    /// Steam 核心兼容性体检完成（携带发起时代数，陈旧结果由流程丢弃）。
    Compat {
        epoch: compat_flow::Epoch,
        report: compat::OverallHealthReport,
    },
    /// 后台网络刷新完成（覆盖短路态的网络适配明细）。
    CompatRefreshed {
        epoch: compat_flow::Epoch,
        report: compat::OverallHealthReport,
    },
    /// 缓存预热完成（成功/失败）。
    CompatPrecached {
        epoch: compat_flow::Epoch,
        result: Result<(), compat::CompatError>,
    },
    /// 首次运行向导的「检查更新 → 下载并解压」完成。
    WizardDownload(Result<(), UpdateError>),
    /// 应用更新检查完成（Settings — 更新页签；只读查询，不进忙碌门禁）。
    AppUpdateChecked(Result<updater::AppUpdateCheckResult, UpdateError>),
}

/// 最近一次结果提示的结构化数据。
/// 渲染时（`status_bar`）才按当前语言生成文案，切换语言无需重建。
enum Notice {
    /// 检查更新完成标记（无 payload）：结果文案由「更新流程」派生（单一事实源）。
    UpdateChecked,
    /// 下载解压结果。
    Downloaded(Result<(), UpdateError>),
    /// 组合操作完成（成功/失败，携带动作以取成功文案）。
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
    /// 前置校验失败（类型化错误 → 本地化文案）。
    Precheck(workflow::Precheck),
}

/// 设置对话框页签。#30 落地 General；#31 追加 Steam；#34 重组为 通用/关于/Steam
/// （关于 = 应用更新 + 仓库信息 + 设置向导，取代原「更新」页签）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsTab {
    General,
    About,
    Steam,
}

/// Settings — Steam 页签的路径编辑状态（纯逻辑：编辑缓冲 + 提交判定，可单测）。
///
/// 与工作路径 `steam_path` 分离（ADR-0012 精神）：手输文本先落缓冲，失焦/回车提交
/// 时才判定——有效输入产出提交值（由调用方更新工作路径、持久化并喂体检流程）；非法
/// 输入不产出提交值，仅置内联错误（#31 验收：非法输入永不落盘、不污染工作路径）。
/// 渲染在 UI 层（`settings_steam`）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SteamPathEditor {
    /// 文本框当前内容（TextEdit 直接改写；打开对话框时以工作路径播种）。
    buffer: String,
    /// 上次提交是否非法（内联错误显示；编辑后清除）。
    invalid: bool,
}

/// 路径提交结果（调用方按结果动作；判据与分支全部可单测）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum SteamPathCommit {
    /// 有效且值已变更：调用方更新工作路径、落盘 App Config 并喂体检流程。
    Changed(String),
    /// 有效但未变更（含未设置的空串与现值相同）：无需落盘/刷新/喂流程。
    Unchanged,
    /// 非空且非目录：非法（置内联错误，不产出提交值，调用方不落盘）。
    Invalid,
}

impl SteamPathEditor {
    /// 以当前工作路径为初值（打开设置对话框时调用；编辑起点 = 现值，编辑而非重置）。
    fn new(seed: &str) -> Self {
        Self {
            buffer: seed.to_string(),
            invalid: false,
        }
    }

    /// 提交（失焦/回车）：判据与主页面路径提交同口径（ADR-0012「空 = 未设置」是合法
    /// 终态，`config.toml` 允许空 `steam_path`、启动回退注册表检测）——空串视为未设置
    /// （不报错、值未变更时无动作），非空则须为有效目录。与向导步骤 2 的严格判据
    /// （空也拒绝）不同：那是「必须有路径才能继续」的流程，设置页是「可留空」的编辑。
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

/// 「Steam 核心兼容性」一帧的展示快照（`CompatFlow::display` 零拷贝借用，渲染期无流程借用）。
struct CompatView {
    summary: CompatSummary,
    checking: bool,
    precaching: bool,
    precache_done: bool,
    /// 预热失败文案（本地化快照；None = 无错误）。
    precache_error: Option<String>,
    /// 详情展开时按需克隆的报告（热路径为 None）。
    detail_report: Option<compat::OverallHealthReport>,
}

impl CompatView {
    /// 从流程状态一次性快照（借用收在本函数内，调用方随后可自由 &mut self）。
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
    /// 已持久化的应用配置（config.toml 内存镜像）：只含用户显式提交的选择。语言
    /// 切换/路径提交先更新它再原子落盘；`steam_path` 工作值（可能来自注册表检测
    /// 或未提交的编辑缓冲）与之分开，检测结果从不写回（ADR-0012）。
    config: Config,
    /// 语言偏好（三态，config.toml 持久化）；`lang` 是其解析出的生效语言。
    lang_pref: Language,
    lang: Lang,
    strings: Strings,
    steam_path: String,
    status: DeployStatus,
    steam_running: bool,
    steam_monitor: SteamMonitor,
    /// 共享 Steam 运行状态（进程表 + alive/group_running/kill 三查询）。
    steam_state: Arc<SteamState>,
    local_version: Option<String>,
    /// 在线更新「检查更新」流程（唯一事实源 + 派生，见 CONTEXT.md「更新流程」）。
    update_flow: UpdateFlow,
    /// 交互类后台操作互斥门禁（同时刻仅一个操作在途；见 CONTEXT.md「忙碌门禁」）。
    gate: BusyGate,
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

    /// 设置对话框是否打开。
    settings_open: bool,
    /// 设置对话框当前页签（#30 General + #31 Steam）。
    settings_tab: SettingsTab,
    /// Settings — Steam 页签路径编辑状态（缓冲 + 内联错误；打开对话框时播种）。
    settings_steam: SteamPathEditor,
    /// 应用更新检查结果（Settings — 更新页签；None = 尚未检查）。
    /// 检查中由 `app_update_checking` 表达（发起时覆盖旧结果）。
    app_update: Option<Result<updater::AppUpdateCheckResult, UpdateError>>,
    /// 应用更新检查是否在途（自管忙碌：只读查询不进 Busy Gate，按钮在途自禁用）。
    app_update_checking: bool,
    /// 首次运行向导（`Some` = 向导激活并替代主界面；完成/跳过/关窗后置 `None`）。
    wizard: Option<wizard::Wizard>,
    /// 体检流程状态机（编排见 compat_flow）。
    flow: CompatFlow,
    /// 兼容性明细展开开关（纯 UI 状态，不属于流程）。
    compat_details_open: bool,
    /// 明细刚展开当帧需要滚动到可见（#34 复看：点击「详细信息」即滚到明细，
    /// 无需手动滚动；仅置位一帧，避免与用户滚动打架）。
    compat_scroll_pending: bool,
    /// 设置对话框页签行右侧 GitHub 入口的 mark 纹理（None = 解码失败，退纯文字）。
    github_mark: Option<egui::TextureHandle>,
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

/// Steam 路径编辑行（单行文本 + 浏览按钮）——向导步骤 2 与 Settings — Steam 共用
/// 同一布局块（宽度公式、TextEdit 边距/hint、浏览按钮尺寸），一处演化两处跟随。
/// 直接改写调用方传入的缓冲；返回本帧的交互快照，调用方按自己的状态处理
/// （向导只消费 `changed` 喂 `PathEdited`；设置页 `changed` 清内联错误、`commit` 提交）。
/// 两个信号相互独立：同帧「编辑 + 失焦」（如 CJK 输入法提交与点击同帧）两者都须
/// 送达——合并成单枚举后写覆盖会丢其一（向导漏喂事件/设置页漏提交）。
#[derive(Clone, Copy, Debug, Default)]
struct PathEditRow {
    /// 文本被编辑或浏览选定（缓冲已就地更新）。
    changed: bool,
    /// 失焦（回车/点击别处）或浏览选定：提交信号（向导不消费）。
    commit: bool,
}

fn path_edit_row(ui: &mut egui::Ui, strings: Strings, buffer: &mut String) -> PathEditRow {
    let mut row = PathEditRow::default();
    // 输入行占满可用宽（表单惯例：字段左对齐、撑满可用宽）。
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
        // 失焦或回车提交：egui 0.36 单行 TextEdit 回车即放弃焦点，`lost_focus` 覆盖两者。
        if resp.lost_focus() {
            row.commit = true;
        }
        if styled_button(
            ui,
            strings.browse,
            ButtonStyle::Neutral,
            egui::vec2(82.0, 34.0),
            true,
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

/// 语言下拉（设置通用页签与向导步骤 1 共用，#34）：同一选项表 + 同构控件，
/// 未来加语言只改 `Strings::language_options` 一处。返回本次选中（None = 未改动）。
fn language_combo(
    ui: &mut egui::Ui,
    options: [(Language, &'static str); 3],
    selected: Language,
    width: f32,
    salt: &'static str,
) -> Option<Language> {
    let current = options
        .iter()
        .find(|(lang, _)| *lang == selected)
        .map(|(_, label)| *label)
        .unwrap_or_default();
    let mut chosen = None;
    egui::ComboBox::from_id_salt(salt)
        .selected_text(current)
        .width(width)
        .show_ui(ui, |ui| {
            for (lang, label) in options {
                if ui.selectable_label(selected == lang, label).clicked() {
                    chosen = Some(lang);
                }
            }
        });
    chosen
}

/// 首次运行向导的下载动作：检查线上版本后下载解压到 `dlls/`（向导只需一个成败结果）。
fn wizard_download() -> Result<(), UpdateError> {
    let info = updater::check_update()?;
    updater::download_and_extract(&info, &dll::dll_dir())
}

/// 步骤 3 子状态 → 展示决策：提示文案、是否错误色、主按钮（文案 + 点击事件；None =
/// 无主按钮）、是否显示跳过。渲染层只消费此决策，陈述永远派生自状态——就绪态不提供
/// 下载按钮、隐藏跳过（「完成」即收尾，复用跳过终局收敛），杜绝「补丁已下载却提示
/// 尚未下载」一类陈述失真（文件本位判据见 ADR-0011）。
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

/// 向导步骤渲染（纯函数：视图 + 文案 → 用户意图 + 卡片矩形；不触碰 App 状态，可单测）。
///
/// 布局：窄卡片整体水平居中。`card_frame().show` 的落点不随父布局对齐（Frame 的
/// `allocate_rect` 原样记录），placer 对齐用的也是父级布局——故外层 `vertical_centered`
/// 负责把「内容宽 + 边框内外边距」的子区居中，子区内再 `top_down(Center)`（Frame
/// 子区继承），使卡片内标题/提示/按钮按行居中；输入行占满卡片宽度。
fn wizard_steps_ui(
    ui: &mut egui::Ui,
    strings: Strings,
    view: &wizard::View,
) -> (Option<wizard::Event>, egui::Rect) {
    let mut event: Option<wizard::Event> = None;
    let mut card_rect = egui::Rect::NOTHING;

    ui.add_space(24.0);
    let card_w = WIZARD_CARD_WIDTH + card_frame().total_margin().sum().x;
    // 父级居中布局负责把 card_w 宽的子区水平居中（placer 对齐用的是父布局，传参的
    // layout 只作用于子区内容）；子区内再 top_down(Center)，使卡片内元素按行居中。
    ui.vertical_centered(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(card_w, ui.available_height()),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                let resp = card_frame().show(ui, |ui| {
                    ui.set_width(WIZARD_CARD_WIDTH);
                    // 标题、步骤指示、提示、单选按钮均按行居中。
                    ui.label(
                        egui::RichText::new(strings.wizard_title)
                            .size(16.0)
                            .strong()
                            .color(theme::INK),
                    );
                    ui.add_space(4.0);
                    let n = match view.step {
                        WizardStep::Language => 1,
                        WizardStep::SteamPath => 2,
                        WizardStep::Download => 3,
                    };
                    ui.label(
                        egui::RichText::new(strings.wizard_step_of.replace("{n}", &n.to_string()))
                            .size(12.0)
                            .color(theme::WEAK),
                    );
                    ui.add_space(18.0);

                    match view.step {
                        WizardStep::Language => {
                            ui.label(
                                egui::RichText::new(strings.wizard_language_prompt).size(13.0),
                            );
                            ui.add_space(12.0);
                            // 下拉选择（与设置通用页签共用同一选项表与控件，#34）；
                            // 选中即本地化后续步骤文案，但不推进——点「下一步」才进步骤 2。
                            let options = strings.language_options();
                            if let Some(lang) =
                                language_combo(ui, options, view.language, 240.0, "wizard_language")
                            {
                                event = Some(wizard::Event::LanguageChosen(lang));
                            }
                            ui.add_space(14.0);
                            if styled_button(
                                ui,
                                strings.wizard_btn_next,
                                ButtonStyle::Primary,
                                egui::vec2(140.0, 34.0),
                                true,
                            )
                            .clicked()
                            {
                                event = Some(wizard::Event::LanguageSubmitted);
                            }
                        }
                        WizardStep::SteamPath => {
                            ui.label(egui::RichText::new(strings.wizard_path_prompt).size(13.0));
                            ui.add_space(12.0);
                            let mut buf = view.steam_path.clone();
                            // 文本编辑或浏览选定都喂 PathEdited（缓冲已就地更新）；失焦
                            // 向导不消费——推进只由「下一步」按钮的 PathSubmitted 触发（行为与抽取前一致）。
                            let row = path_edit_row(ui, strings, &mut buf);
                            if row.changed {
                                event = Some(wizard::Event::PathEdited(buf.clone()));
                            }
                            // 有效性来自状态机快照（PathEdited 同一帧内已推进，下一帧即为最新）。
                            if !view.path_valid && !view.steam_path.trim().is_empty() {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new(strings.wizard_path_invalid)
                                        .size(12.0)
                                        .color(theme::DANGER),
                                );
                            }
                            ui.add_space(14.0);
                            if styled_button(
                                ui,
                                strings.wizard_btn_next,
                                ButtonStyle::Primary,
                                egui::vec2(140.0, 34.0),
                                view.path_valid,
                            )
                            .clicked()
                            {
                                event = Some(wizard::Event::PathSubmitted);
                            }
                        }
                        WizardStep::Download => {
                            // 子状态文案与按钮完全由 `wizard_step3_content` 决策（纯函数，
                            // 单测覆盖：Idle 提示需显式开始；Ready 补丁已下载；Running
                            // 进行中；Failed 就地报错）。
                            let content = wizard_step3_content(&strings, &view.download);
                            let prompt = egui::RichText::new(content.prompt.as_str()).size(13.0);
                            let prompt = if content.danger {
                                prompt.color(theme::DANGER)
                            } else {
                                prompt
                            };
                            ui.label(prompt);
                            ui.add_space(16.0);
                            // 按钮行手动居中（horizontal 占满宽，需前置空间补偿）。
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
        install_cjk_font(&cc.egui_ctx);
        install_theme(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        // 启动即恢复应用配置（语言偏好 + Steam 路径，见 ADR-0012）：缺失文件 =
        // 未配置（默认值）；损坏/版本不符 = 类型化错误，降级默认值继续（不 panic、
        // 不崩溃，错误仅记日志——wizard 落地后再把错误显式呈现到 UI）。
        let config = match config::load(&config::config_path()) {
            Ok(cfg) => cfg,
            Err(e) => {
                log_warn(format!("load config.toml: {e}；使用默认值"));
                config::Config::defaults()
            }
        };
        let lang_pref = config.language;
        // auto 跟随系统检测，zh/en 固定（三态解析在 config::Language::effective）。
        let lang = lang_pref.effective();
        let strings = Strings::new(lang);
        // 窗口标题随语言（zh: OpenSteamTool 一键管理工具 / en: OpenSteamTool Manager）。
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            strings.window_title.to_owned(),
        ));
        // 配置优先恢复 Steam 路径；已持久化路径失效（手改/目录已删）或未设置时
        // 回退注册表检测（与写入侧同一 is_dir 判据）。检测结果不自动写回配置——
        // 配置只反映用户显式选择，检测是建议不是选择。
        let steam_path = if !dll::is_valid_steam_dir(&config.steam_path) {
            steam::detect_steam_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        } else {
            config.steam_path.clone()
        };
        // 首次运行向导：配置缺失/损坏或已存路径无效时激活（`should_show` 为纯谓词）。
        // 以当前会话值（配置或注册表检测）播种路径、以配置偏好播种语言；重跑即编辑。
        let wizard = wizard::should_show(&config::config_path())
            .then(|| wizard::Wizard::new(lang_pref, steam_path.clone(), dll::dll_dir()));
        let steam_dir = Path::new(&steam_path);
        let status = dll::check_status(steam_dir);
        let local_version = dll::read_local_version(&dll::dll_dir());
        let steam_state = Arc::new(SteamState::new());
        let steam_monitor = SteamMonitor::new(&steam_state);
        let steam_running = steam_monitor.is_running();
        let tray = Tray::new(
            crate::tray::load_icon(),
            strings.app_title,
            strings.tray_show,
            strings.tray_quit,
            strings.tray_minimize,
            strings.tray_restart,
        );

        let flow = CompatFlow::new();

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
            minimize_to_tray: true,
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
            github_mark: load_github_mark(&cc.egui_ctx),
        };
        // 初始同步托盘「重启 Steam」可用性（跟随初始 Steam 路径有效性）。
        app.sync_tray_restart_enabled();
        // 启动即喂首次路径：产出首次快速体检效果（初始 checking 骨架态，零白屏）。
        app.on_compat_event(
            &cc.egui_ctx,
            compat_flow::Event::PathChanged(app.steam_path.clone()),
        );
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

    /// 按当前 Steam 路径有效性同步托盘「重启 Steam」项可用性（路径无效置灰）。
    /// 语义：Steam 运行中点击为「重启」；未运行时点击等价「直接启动」
    /// （Restart 的关闭步骤对未运行进程组是 no-op，见 workflow::plan）。
    /// 在 `status` 每次变化处调用（收敛进 `refresh_status`）。
    fn sync_tray_restart_enabled(&mut self) {
        if let Some(tray) = &self.tray {
            tray.set_restart_enabled(self.status != DeployStatus::InvalidPath);
        }
    }
    /// 处理托盘事件：切换显隐 / 显示 / 退出。
    fn handle_tray_events(&mut self) {
        let Some(tray) = &self.tray else { return };
        let ctx = self.ctx.clone(); // 避免 `&self.ctx` 与 `&mut self` 借用冲突。
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
                    // eframe's 100ms invisible-window repaint throttle delays ViewportCommand::Close by a second frame (~220ms); drop the tray and exit directly instead.
                    self.tray = None;
                    std::process::exit(0);
                }
                TrayAction::ToggleMinimizeToTray => self.minimize_to_tray = minimize_checked,
                TrayAction::RestartSteam => self.request_action(&ctx, Action::Restart),
            }
        }
    }

    fn refresh_status(&mut self) {
        self.status = dll::check_status(Path::new(self.steam_path.trim()));
        self.sync_tray_restart_enabled();
    }

    /// 处理后台消息：更新状态与提示。
    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Phase(kind) => self.gate.replace(kind),
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
                    // 启动/重启类成功后 Steam 已运行 → 直接隐藏到托盘（不依赖边沿检测）；
                    // 仅退出并卸载（ExitAndUninstall）Steam 未运行 → 保持显示。
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
                        // 向导已跳过/关窗，但在途下载仍完成：刷新本地版本，避免主界面
                        // 继续显示陈旧的「补丁缺失」（跳过不等于取消网络请求，以磁盘为准）。
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

    /// 用户点击操作按钮：Steam 在运行且操作需关闭 Steam → 弹确认框；否则直接执行。
    fn request_action(&mut self, ctx: &egui::Context, action: Action) {
        if self.gate.is_busy() {
            return;
        }
        if action.asks_to_close_steam() && self.steam_running {
            self.confirm = Some(action);
            return;
        }
        self.start_action(ctx, action, false);
    }

    fn start_action(&mut self, ctx: &egui::Context, action: Action, kill_first: bool) {
        let dll_dir = dll::dll_dir();
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

    /// 应用语言偏好：更新生效语言、文案、窗口标题（不落盘；持久化由调用方决定）。
    fn set_language(&mut self, pref: Language) {
        self.lang_pref = pref;
        self.lang = pref.effective();
        self.strings = Strings::new(self.lang);
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            self.strings.window_title.to_owned(),
        ));
    }

    /// 把已持久化的配置镜像原子落盘到 `config.toml`；失败仅记日志不中断操作
    /// （配置是尽力持久化，不是数据正确性关键路径）。
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
    /// Settings — 更新页签。不下载、不自替换（明确非目标）。
    fn check_app_update(&mut self, ctx: &egui::Context) {
        if self.app_update_checking {
            return;
        }
        self.app_update_checking = true;
        self.app_update = None; // 覆盖旧结果，回到「检查中」。
        self.spawn(ctx, || Msg::AppUpdateChecked(updater::check_app_update()));
    }

    // ---------- 首次运行向导（替代主界面，同一窗口） ----------

    /// 向导一帧：渲染当前步骤、收集用户意图、推进状态机并执行效果。
    fn wizard_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(view) = self.wizard.as_ref().map(|w| w.view()) else {
            return;
        };
        let strings = self.strings; // Copy：渲染期自由借用 self。
        let (event, _) = wizard_steps_ui(ui, strings, &view);
        if let Some(event) = event {
            self.wizard_event(ctx, event);
        }
    }

    /// 推进向导一个事件：更新语言并执行效果（向导帧 / 下载完成消息 / 关窗兜底共用）。
    fn wizard_event(&mut self, ctx: &egui::Context, event: wizard::Event) {
        let (v, effects) = self.wizard.as_mut().unwrap().step(event);
        self.set_language(v.language);
        self.exec_wizard_effects(ctx, effects);
    }

    /// 执行向导效果：spawn 下载，或在结束时持久化语言/路径并切回主界面。
    fn exec_wizard_effects(&mut self, ctx: &egui::Context, effects: Vec<wizard::Effect>) {
        for effect in effects {
            match effect {
                wizard::Effect::Download => {
                    let ctx2 = ctx.clone();
                    self.spawn(&ctx2, || Msg::WizardDownload(wizard_download()));
                }
                wizard::Effect::Finish {
                    language,
                    steam_path,
                } => {
                    // 完成（下载成功）与跳过（显式跳过/关窗）收敛到同一终局：
                    // 持久化语言与路径，切回主界面。
                    debug_assert!(
                        self.wizard.as_ref().is_some_and(|w| w.finished()),
                        "Finish 效果只能由已结束的向导产出"
                    );
                    self.config.language = language;
                    self.config.steam_path = steam_path.clone();
                    self.persist_config();
                    // 语言已在 `wizard_event`（同一 Finish 路径的调用方）应用；此处不重复。
                    self.steam_path = steam_path;
                    self.refresh_status();
                    self.wizard = None;
                    self.feed_path_changed(ctx);
                }
            }
        }
    }

    // ---------- 设置对话框（General 页签；Steam 页签见 #31） ----------

    /// 打开设置：置位、切到指定页签，并以当前工作路径播种 Steam 页签的编辑缓冲（重跑即编辑）。
    /// 顶栏入口传会话内记忆的页签（历史行为「会话内记忆上次页签」）；主页面健康警示显式切 Steam。
    fn open_settings(&mut self, tab: SettingsTab) {
        self.settings_open = true;
        self.settings_tab = tab;
        self.settings_steam = SteamPathEditor::new(&self.steam_path);
    }

    /// 设置对话框：页签骨架 + 通用 / 关于 / Steam 页签。全部控件即改即生效并持久化
    /// （无 OK/Cancel）；补丁更新检查 / 下载并解压仍走忙碌门禁互斥。
    /// 布局：标题 + 页签行固定，中间内容区滚动（窗口高度有限，Modal 是 Area 不约束屏幕）。
    /// GitHub 入口在关于页签内（#34 修订：不再占用页签行）。
    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut close_clicked = false;
        egui::Modal::new(egui::Id::new("settings_dialog")).show(ctx, |ui| {
            ui.set_width(SETTINGS_DIALOG_WIDTH);
            ui.heading(self.strings.settings_title);
            ui.add_space(8.0);
            // 页签行（#34 定稿：通用 / Steam / 关于）。
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
                    if styled_button(ui, label, style, egui::vec2(88.0, 28.0), true).clicked()
                        && self.settings_tab != tab
                    {
                        // 离开 Steam 页签前提交未落地路径编辑：页签行先于内容区渲染，
                        // 点击页签会夺走焦点但 Steam 文本框当帧不再渲染，失焦提交被吞。
                        if self.settings_tab == SettingsTab::Steam {
                            self.commit_settings_steam_path(ctx);
                        }
                        self.settings_tab = tab;
                    }
                    ui.add_space(4.0);
                }
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            // 内容区滚动：窗口高度有限且 Modal 是 Area 不约束屏幕，滚动区高度由窗口
            // 高度扣除非滚动行的固定占用（标题+页签行+分隔线+页脚+窗口边距，数值保守
            // 偏大）得到，保证页脚恒可见；过低窗口下仍保留最小滚动区。
            let max_scroll_h = settings_scroll_height(ctx.content_rect().height());
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(max_scroll_h)
                .show(ui, |ui| match self.settings_tab {
                    SettingsTab::General => self.settings_general(ui),
                    SettingsTab::About => self.settings_about(ui, ctx),
                    SettingsTab::Steam => self.settings_steam(ui, ctx),
                });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if styled_button(
                        ui,
                        self.strings.btn_close,
                        ButtonStyle::Neutral,
                        egui::vec2(80.0, 30.0),
                        true,
                    )
                    .clicked()
                    {
                        close_clicked = true;
                    }
                });
            });
        });
        if close_clicked {
            // 关闭前提交 Steam 页签未落地编辑（兜底：无论焦点时序，有效提交都不丢失；
            // 未变更的判定让重复提交是 no-op）。
            self.commit_settings_steam_path(ctx);
            self.settings_open = false;
        }
    }

    /// 通用页签（#34 重组）：语言下拉（即改即存）→ 重新运行向导（低频操作，Neutral 样式）。
    /// 语言选项与向导步骤 1 共用同一份选项表（`Strings::language_options`），新增语言只改一处。
    fn settings_general(&mut self, ui: &mut egui::Ui) {
        // ---- 语言（下拉；选中即应用并持久化，无重启） ----
        card_title(ui, self.strings.settings_language_title);
        ui.add_space(8.0);
        let options = self.strings.language_options();
        if let Some(lang) = language_combo(ui, options, self.lang_pref, 220.0, "settings_language")
            && lang != self.lang_pref
        {
            self.set_language(lang);
            self.config.language = lang;
            self.persist_config();
        }
    }

    /// 关于页签（#34 修订）：应用更新（软件版本 + 应用更新检查）→ 项目主页
    /// （GitHub mark + 链接）→ 设置向导（重新运行）。补丁更新入口已回主页面（#34）。
    fn settings_about(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // ---- 应用更新（软件版本 + 应用更新检查；只查询并打开下载页，不下载/自替换） ----
        card_title(ui, self.strings.settings_app_update_title);
        ui.add_space(8.0);
        // 软件版本：crate 版本（构建时固化的单一来源）。
        ui.label(
            egui::RichText::new(format!(
                "{} v{}",
                self.strings.settings_version_label,
                env!("CARGO_PKG_VERSION")
            ))
            .size(13.0)
            .color(theme::SUB),
        );
        ui.add_space(8.0);
        // 应用更新检查：只查询并打开下载页，不做任何下载/自替换。
        // 结果/检查中文案内联于按钮右侧（#34 复看定稿：不再换行置底）。
        let mut do_app_check = false;
        ui.horizontal(|ui| {
            if styled_button(
                ui,
                self.strings.settings_btn_app_update_check,
                ButtonStyle::Neutral,
                egui::vec2(150.0, 28.0),
                !self.app_update_checking,
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
                            .color(theme::WEAK),
                    );
                }
                (Some(Ok(r)), false) if r.newer => {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}{}",
                            self.strings.settings_app_update_new_version, r.latest_version
                        ))
                        .size(12.5)
                        .color(theme::INK),
                    );
                    if styled_button(
                        ui,
                        self.strings.settings_btn_open_download_page,
                        ButtonStyle::Primary,
                        egui::vec2(120.0, 28.0),
                        true,
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
                            .color(theme::INK),
                    );
                }
                (Some(Err(e)), false) => {
                    ui.label(
                        egui::RichText::new(self.strings.update_error(e))
                            .size(12.5)
                            .color(theme::DANGER),
                    );
                }
                (None, false) => {}
            }
        });
        if do_app_check {
            self.check_app_update(ctx);
        }
        ui.add_space(12.0);

        // ---- 项目主页（GitHub 链接按钮；mark 在按钮内部，#34 定稿） ----
        card_title(ui, self.strings.settings_github_title);
        ui.add_space(8.0);
        if github_link_button(
            ui,
            self.github_mark.as_ref(),
            self.strings.settings_github_label,
            updater::APP_REPO_PAGE,
        )
        .clicked()
        {
            updater::open_in_browser(updater::APP_REPO_PAGE);
        }
        ui.add_space(12.0);

        // ---- 设置向导（重新运行；低频操作不用主色） ----
        card_title(ui, self.strings.settings_wizard_title);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(self.strings.settings_rerun_wizard_hint)
                .size(12.0)
                .color(theme::WEAK),
        );
        ui.add_space(8.0);
        if styled_button(
            ui,
            self.strings.settings_btn_rerun_wizard,
            ButtonStyle::Neutral,
            egui::vec2(160.0, 32.0),
            !self.gate.is_busy(),
        )
        .clicked()
        {
            self.rerun_wizard();
        }
    }

    /// Settings — Steam 页签（#31）：Steam 路径编辑（文本 + 浏览；失焦/回车校验提交），
    /// 以及兼容性小节（#32 从主页面迁入：徽章六态 / 自动与手动预热 / 详情行为不变）。
    /// 路径编辑与工作路径分离：非法输入只显示内联错误，永不落盘、不污染工作路径。
    fn settings_steam(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card_title(ui, self.strings.settings_steam_title);
        ui.add_space(8.0);
        let mut do_commit = false;
        let row = path_edit_row(ui, self.strings, &mut self.settings_steam.buffer);
        // 编辑即作废上次提交的错误判定（错误只属于「提交时」的判据）。
        if row.changed {
            self.settings_steam.invalid = false;
        }
        // 浏览选定 = 有效目录：直接写入缓冲并提交（与手输同一收敛）；失焦/回车同样提交。
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
                    .color(theme::DANGER),
            );
        }
        // 兼容性小节：#32 从主页面迁入（行为与健康度六态不变）。
        self.compat_section(ui);
    }

    /// Settings — Steam 页签路径提交（失焦/回车/浏览选定/关闭对话框/离开页签统一收敛）：
    /// 编辑器判定有效且值已变更 → 更新工作路径、原子落盘 App Config、刷新部署状态并喂
    /// 体检流程（代数推进 + 防抖与主页面路径变更同一机制，见 `feed_path_changed`）；
    /// 判定非法 → 仅置内联错误，不触碰工作路径与配置（#31 验收：非法输入不落盘）；
    /// 判定未变更 → 无动作（不重复落盘/刷新/喂流程）。
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

    /// 「重新运行向导」：以当前会话配置（语言偏好 + Steam 路径）为初值启动向导，并关闭
    /// 设置对话框（向导替代主界面渲染）。重跑是「编辑」而非「重置」（#29 播种约定）。
    fn rerun_wizard(&mut self) {
        self.settings_open = false;
        self.wizard = Some(wizard::Wizard::new(
            self.lang_pref,
            self.steam_path.clone(),
            dll::dll_dir(),
        ));
    }
    // ---------- UI ----------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        // 标题按固定行高手绘 LEFT_CENTER 锚点，与右侧按钮垂直同轴（勿回退 with_layout，见 ADR-0010）。
        ui.horizontal(|ui| {
            let h = 28.0;
            let font = egui::FontId::proportional(16.0);
            let tw = text_width(ui, self.strings.app_title, &font, theme::INK);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(tw, h), egui::Sense::hover());
            ui.painter().text(
                rect.left_center(),
                egui::Align2::LEFT_CENTER,
                self.strings.app_title,
                font,
                theme::INK,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // #34：语言切换入口移除（改语言唯一入口 = 设置通用页签 / 向导弹窗），
                // 顶栏只剩齿轮设置入口（图标按钮 + tooltip）。
                if gear_button(ui, self.strings.settings_gear_tooltip).clicked() {
                    self.open_settings(self.settings_tab);
                }
            });
        });
        ui.add_space(6.0);
    }

    /// 主页面健康风险警示行（#32）：仅「上游尚未适配 / 未找到核心 DLL」两态渲染，
    /// 点击整行跳 Settings — Steam（兼容性细节已迁至该页签）。其余健康态不占位不打扰。
    fn health_warning_line(&mut self, ui: &mut egui::Ui) {
        // 快照取自体检流程展示态（零拷贝借用，渲染期无流程借用）。
        let summary = self.flow.display().summary;
        let Some(text) = health_warning(&self.strings, summary) else {
            return;
        };
        // 全宽警示行：danger 描边 + 语义浅底（由 DANGER 槽派生，不新增色值，ADR-0010）。
        let resp = ui.add(
            egui::Button::new(egui::RichText::new(text).size(12.5).color(theme::DANGER))
                .fill(theme::badge_bg(theme::DANGER))
                .stroke(egui::Stroke::new(1.0, theme::DANGER))
                .corner_radius(egui::CornerRadius::same(8))
                .min_size(egui::vec2(ui.available_width(), 34.0)),
        );
        if resp.clicked() {
            self.open_settings(SettingsTab::Steam);
        }
        ui.add_space(8.0);
    }

    /// 喂事件给体检流程并执行其效果（消息臂 / 路径变更 / 手动预热共用）。
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

    /// 执行体检流程产出的效果：spawn 后台线程，完成消息携带发起时代数回传。
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

    /// Settings — Steam 兼容性小节：第一行标题+状态徽章+操作靠右，第二行辅助说明弱化。
    fn compat_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        // 无分隔线：竖条标题自成边界，直接衔接上方路径输入区。

        let v = CompatView::snapshot(self);

        // 标题：与其他卡片一致的蓝色竖条指示器（#34 定稿：徽章移到标题之下）。
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 13.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 0.0, theme::ACCENT);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(self.strings.compat_title)
                    .size(13.5)
                    .strong(),
            );
        });
        ui.add_space(8.0);
        // 徽章行：徽章在左，右侧操作（详细信息/预热）贴最右、与徽章同排（#34 复看
        // 定稿：状态说明收进详细信息，不再占独立说明行）。
        ui.horizontal(|ui| {
            self.compat_badge(ui, v.summary);
            // 右侧操作区：right_to_left 首项（详细信息）贴最右，预热按钮在其左侧。
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !v.checking
                    && styled_button(
                        ui,
                        self.strings.compat_btn_details,
                        ButtonStyle::Neutral,
                        egui::vec2(88.0, 26.0),
                        true,
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
                    )
                    .clicked()
                    {
                        let ctx = self.ctx.clone();
                        self.request_precache(&ctx);
                    }
                }
            });
        });
        // 展开明细：置于徽章行正下方（点击「详细信息」即展开，无需滚动即可见）。
        if let Some(report) = &v.detail_report {
            self.compat_details(ui, report, v.summary);
        }
        if let Some(text) = &v.precache_error {
            ui.label(egui::RichText::new(text).size(12.0).color(theme::DANGER));
        }
        if v.precache_done {
            ui.label(
                egui::RichText::new(self.strings.compat_precache_done)
                    .size(12.0)
                    .color(theme::SUCCESS),
            );
        }
    }

    /// 状态徽章（pill badge）：浅色底 + 深色文字 + 状态图标，视觉低于标题、高于辅助行。
    fn compat_badge(&self, ui: &mut egui::Ui, summary: CompatSummary) {
        let (icon, text, fg, bg) = match summary {
            CompatSummary::Checking => (
                "○",
                self.strings.compat_checking,
                theme::WEAK,
                theme::BORDER,
            ),
            CompatSummary::Ready => (
                "✔",
                self.strings.compat_status_ready,
                theme::SUCCESS,
                theme::badge_bg(theme::SUCCESS),
            ),
            CompatSummary::Online => (
                "●",
                self.strings.compat_status_online,
                theme::WARN,
                theme::badge_bg(theme::WARN),
            ),
            CompatSummary::Pending => (
                "▲",
                self.strings.compat_status_pending,
                theme::DANGER,
                theme::badge_bg(theme::DANGER),
            ),
            CompatSummary::Missing => (
                "?",
                self.strings.compat_status_missing,
                theme::WEAK,
                theme::BORDER,
            ),
            CompatSummary::Network => (
                "?",
                self.strings.compat_status_network,
                theme::WEAK,
                theme::BORDER,
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

    /// 单项探针状态文案与颜色（详情明细行）。
    fn compat_status_of(&self, status: &compat::ProbeStatus) -> (&'static str, egui::Color32) {
        use compat::ProbeStatus::*;
        match status {
            Checking => (self.strings.compat_checking, theme::WEAK),
            RemoteAvailable { cached: true } => (self.strings.compat_status_ready, theme::SUCCESS),
            RemoteAvailable { cached: false } => (self.strings.compat_status_online, theme::WARN),
            CompatibleOffline => (self.strings.compat_status_offline, theme::SUCCESS),
            IncompatiblePending => (self.strings.compat_status_pending, theme::DANGER),
            NetworkError(_) => (self.strings.compat_status_network, theme::WEAK),
            FileNotFound => (self.strings.compat_status_missing, theme::WEAK),
        }
    }

    /// 明细（展开态，#34 复看定稿）：状态说明（原独立说明行收进此处）+ 逐 DLL 行
    /// + 一键缓存签名。展开于徽章行正下方，无需滚动即可见。
    fn compat_details(
        &mut self,
        ui: &mut egui::Ui,
        report: &compat::OverallHealthReport,
        summary: CompatSummary,
    ) {
        ui.add_space(6.0);
        // 状态说明：Checking 为瞬时态不显示。
        if summary != CompatSummary::Checking {
            let tip = match summary {
                CompatSummary::Ready => self.strings.compat_tip_ready,
                CompatSummary::Online => self.strings.compat_tip_online,
                CompatSummary::Pending => self.strings.compat_tip_pending,
                CompatSummary::Missing => self.strings.compat_tip_missing,
                CompatSummary::Network => self.strings.compat_tip_network,
                CompatSummary::Checking => "",
            };
            ui.label(egui::RichText::new(tip).size(11.5).color(theme::WEAK));
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
                ui.label(egui::RichText::new(row).size(12.5).color(theme::SUB));
                ui.monospace(egui::RichText::new(sha).size(12.0).color(theme::WEAK));
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
            )
            .clicked()
            {
                self.request_precache(&ctx);
            }
        }
        // 展开当帧滚动到明细可见（#34 复看：点击「详细信息」即见 steamclient64.dll
        // (IPC) 等明细行，无需手动滚动；仅一帧，随后交还滚动控制）。
        if self.compat_scroll_pending {
            self.compat_scroll_pending = false;
            ui.scroll_to_cursor(Some(egui::Align::Center));
        }
    }

    /// 部署状态卡片（#34 致密化 + 方案 B 定稿）：状态 16px bold + 补丁更新按钮。
    /// 补丁检查/下载共用同一按钮位；检查中/结果文案内联于按钮右侧（不进底部状态栏）。
    fn card2(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度
            card_title(ui, self.strings.card2_title);
            ui.add_space(10.0);

            // 部署状态：16px bold 主行（#34 致密化：状态一眼可读）。
            let (text, color) = match self.status {
                DeployStatus::InvalidPath => (self.strings.status_invalid, theme::WEAK),
                DeployStatus::Deployed => (self.strings.status_deployed, theme::SUCCESS),
                DeployStatus::NotDeployed => (self.strings.status_not_deployed, theme::WEAK),
            };
            ui.label(egui::RichText::new(text).size(16.0).strong().color(color));
            ui.add_space(10.0);

            // 方案 B：补丁更新按钮（检查/下载两阶段共用同一按钮位）；检查中/结果文案
            // 内联于按钮右侧（#34 复看定稿：与「关于」页应用更新检查同式，不进底部状态栏）。
            // 可用性与文案由「更新流程」派生（ADR-0011 文件本位判据）；版本号永不渲染（ADR-0014）。
            let derived = self.update_flow.derived(self.local_known_version());
            let patch_result = derived
                .notice
                .map(|n| render_patch_notice(&self.strings, &n));
            let patch_info = derived.download.cloned();
            let checking = self.gate.current() == Some(BusyKind::Checking);
            let mut do_check = false;
            let mut do_download: Option<OnlineInfo> = None;
            ui.horizontal(|ui| {
                if let Some(info) = &patch_info {
                    if styled_button(
                        ui,
                        self.strings.btn_download_and_extract,
                        ButtonStyle::Primary,
                        egui::vec2(180.0, 32.0),
                        !self.gate.is_busy(),
                    )
                    .clicked()
                    {
                        do_download = Some(info.clone());
                    }
                } else if styled_button(
                    ui,
                    self.strings.settings_btn_patch_update_check,
                    ButtonStyle::Neutral,
                    egui::vec2(150.0, 32.0),
                    !self.gate.is_busy(),
                )
                .clicked()
                {
                    do_check = true;
                }
                // 检查中 / 结果内联于按钮右侧（SUCCESS=已是最新/发现新补丁，DANGER=失败）。
                if checking {
                    ui.label(
                        egui::RichText::new(self.strings.checking)
                            .size(12.5)
                            .color(theme::WEAK),
                    );
                } else if let Some((ok, text)) = patch_result {
                    ui.label(egui::RichText::new(text).size(12.5).color(if ok {
                        theme::SUCCESS
                    } else {
                        theme::DANGER
                    }));
                }
            });
            if do_check {
                self.check_update(ctx);
            }
            if let Some(info) = do_download {
                self.download_update(ctx, info);
            }
        });
        ui.add_space(10.0);
    }

    /// 独立操作区（主页面核心闭环，位于部署状态卡片之下）：等宽按钮并排。
    /// 已应用时三枚：运行中为（退出并卸载 / 重启 Steam / 卸载并重启），
    /// 未运行为（启动 Steam / 卸载补丁 / 卸载补丁并重启 Steam）；其余两枚。
    fn action_area(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            let gap = 12.0;
            let spacing = ui.spacing().item_spacing.x;
            // 宽度公式必须扣除 egui 自动插入的 item_spacing 与手动 gap（见 row_button_width），
            // 否则按钮行实际占宽溢出，把下方依赖 available_width 撑满的内容顶到窗口右缘。
            match self.status {
                DeployStatus::Deployed if self.steam_running => {
                    // 「退出 Steam 并卸载补丁」（唯一警戒）/「重启 Steam」/「卸载补丁并重启 Steam」。
                    let size = egui::vec2(
                        row_button_width(ui.available_width(), gap, spacing, 3),
                        44.0,
                    );
                    if styled_button(
                        ui,
                        self.strings.btn_exit_and_uninstall,
                        ButtonStyle::Caution,
                        size,
                        !self.gate.is_busy(),
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::ExitAndUninstall);
                    }
                    ui.add_space(gap);
                    if styled_button(
                        ui,
                        self.strings.btn_restart_steam,
                        ButtonStyle::Neutral,
                        size,
                        !self.gate.is_busy(),
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::Restart);
                    }
                    ui.add_space(gap);
                    if styled_button(
                        ui,
                        self.strings.btn_uninstall_and_restart,
                        ButtonStyle::Neutral,
                        size,
                        !self.gate.is_busy(),
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::UninstallAndRestart);
                    }
                }
                DeployStatus::Deployed => {
                    // Steam 已退出 → 「启动 Steam」/「卸载补丁」/「卸载补丁并重启 Steam」。
                    // 按钮写「启动」而非「正常启动」——带补丁启动不是正常启动（见 ADR-0011）。
                    let size = egui::vec2(
                        row_button_width(ui.available_width(), gap, spacing, 3),
                        44.0,
                    );
                    if styled_button(
                        ui,
                        self.strings.btn_launch,
                        ButtonStyle::Neutral,
                        size,
                        !self.gate.is_busy(),
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::Launch);
                    }
                    ui.add_space(gap);
                    if styled_button(
                        ui,
                        self.strings.btn_uninstall,
                        ButtonStyle::Neutral,
                        size,
                        !self.gate.is_busy(),
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::ExitAndUninstall);
                    }
                    ui.add_space(gap);
                    if styled_button(
                        ui,
                        self.strings.btn_uninstall_and_restart,
                        ButtonStyle::Neutral,
                        size,
                        !self.gate.is_busy(),
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::UninstallAndRestart);
                    }
                }
                DeployStatus::NotDeployed => {
                    let size = egui::vec2(
                        row_button_width(ui.available_width(), gap, spacing, 2),
                        44.0,
                    );
                    if styled_button(
                        ui,
                        self.strings.btn_apply_and_launch,
                        ButtonStyle::Deploy,
                        size,
                        // 补丁未下载（dlls/ 缺文件）时置灰，避免点了才报 MissingTargetDlls（见 ADR-0011）。
                        !self.gate.is_busy() && dll::dlls_present(),
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::ApplyAndLaunch);
                    }
                    ui.add_space(gap);
                    if styled_button(
                        ui,
                        self.strings.btn_launch_normal,
                        ButtonStyle::Neutral,
                        size,
                        !self.gate.is_busy(),
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::Launch);
                    }
                }
                DeployStatus::InvalidPath => {
                    // 无有效路径时禁用操作按钮。
                    let size = egui::vec2(
                        row_button_width(ui.available_width(), gap, spacing, 2),
                        44.0,
                    );
                    styled_button(
                        ui,
                        self.strings.btn_apply_and_launch,
                        ButtonStyle::Deploy,
                        size,
                        false,
                    );
                    ui.add_space(gap);
                    styled_button(
                        ui,
                        self.strings.btn_launch_normal,
                        ButtonStyle::Neutral,
                        size,
                        false,
                    );
                }
            }
        });
        // 补丁未下载引导（ADR-0011）：未部署 + dlls/ 缺文件时「应用补丁并启动」已置灰，
        // 提示按「检查更新 → 下载并解压」两步走（决策显式不自动联网检查）。
        if self.status == DeployStatus::NotDeployed && !dll::dlls_present() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(self.strings.hint_download_patch)
                    .size(12.0)
                    .color(theme::WEAK),
            );
        }
        ui.add_space(10.0);
    }

    /// 文件本位（ADR-0011）：`dlls/` 缺文件时无视版本记录按「本地缺失」比较，
    /// `derived` 的全部消费者（设置页补丁检查结果 / 通知文案 / 下载可用性）共用同一判据。
    fn local_known_version(&self) -> Option<&str> {
        dll::dlls_present()
            .then_some(self.local_version.as_deref())
            .flatten()
    }

    /// 底部状态显示栏（#34 复看定稿）：Steam 运行状态（恒显示）+ 交互类忙碌
    /// （补丁检查的检查中由卡片按钮旁内联呈现，不在此处）+ 最近操作结果，多条并排、
    /// 各自独立成项、互不覆盖。色点语义只走既有语义槽：SUCCESS=运行/成功；WEAK=未运行；
    /// ACCENT=忙碌；DANGER=失败。
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // 1) Steam 运行状态（恒显示；与操作结果区分，不互相覆盖）。
            if self.steam_running {
                status_item(ui, self.strings.status_steam_running, theme::SUCCESS);
            } else {
                status_item(ui, self.strings.status_steam_stopped, theme::WEAK);
            }
            // 2) 交互类忙碌（Checking 由卡片按钮旁内联呈现，此处只画其余阶段）。
            if let Some(kind) = self.gate.current()
                && kind != BusyKind::Checking
            {
                status_item(ui, self.strings.busy_label(kind), theme::ACCENT);
            }
            // 3) 最近操作结果（UpdateChecked 由卡片按钮旁内联呈现）。
            if let Some(n) = &self.notice
                && !matches!(n, Notice::UpdateChecked)
            {
                let (ok, text) = render_notice(&self.strings, n);
                status_item(ui, &text, if ok { theme::SUCCESS } else { theme::DANGER });
            }
        });
    }

    /// 主界面内容（向导未激活时渲染）：核心闭环 = 部署状态卡片（含补丁更新按钮，#34
    /// 方案 B）+ 健康风险警示 + 操作按钮组 + 底部状态栏。已移除：Steam 路径编辑、
    /// 兼容性小节（迁往设置对话框）；Steam 运行状态与补丁检查结果在底部状态栏（#34
    /// 定稿：多条状态并排、互不覆盖）；本地/线上版本退为内部概念，永不渲染。
    fn main_content(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.top_bar(ui);
        self.card2(ui, &ctx);
        self.health_warning_line(ui);
        self.action_area(ui);
        self.status_bar(ui);
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

        // 向导激活时拦截窗口关闭：取消退出并等同「跳过」落到主界面（永不把用户锁在向导里）。
        if self.wizard.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.wizard_event(ctx, wizard::Event::Closed);
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
            // 首次运行向导替代主界面（同一窗口，无第二原生窗口）。
            if self.wizard.is_some() {
                self.wizard_ui(&ctx, ui);
            } else {
                self.main_content(ui);
            }

            // 用布局游标测内容底部（min_rect 被 CentralPanel 撑满，不可用）。
            content_h = ui.cursor().top();
        });

        // 首帧按内容高度自适应窗口（消除底部大留白），只设置一次。向导期间各步
        // 高度不同，不参与自适应（否则窗口会缩到某一步的高度）。
        // #32 回归修复：内容变矮后仍不得低于设置对话框的最小内高（见 autosize_inner_height）。
        if self.wizard.is_none() && !self.autosized && content_h > 0.0 {
            self.autosized = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                ui.available_width().max(620.0),
                autosize_inner_height(content_h),
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
                    if styled_button(
                        ui,
                        self.strings.yes,
                        ButtonStyle::Primary,
                        egui::vec2(72.0, 30.0),
                        true,
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

    /// #32 验收：主页面健康风险警示仅「上游尚未适配 / 未找到核心 DLL」两态产出文案，
    /// 其余健康态（检查中 / 完美兼容 / 上游已适配未缓存 / 网络不可用）一律不打扰；
    /// 双语文案均含「Steam」指向（警示行整行点击跳 Settings — Steam）。
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
            // 警示文案含「Steam」指向（整行点击跳 Settings — Steam 页签）。
            assert!(s.main_warning_pending.contains("Steam"), "{lang:?}");
            assert!(s.main_warning_missing.contains("Steam"), "{lang:?}");
        }
    }

    /// #30 验收：补丁更新检查结果文案永不渲染补丁版本号——
    /// 分类来自「更新流程」派生，但三种结果均只呈现定性文案。
    /// #32 后该映射同时服务设置对话框与主页面通知栏（Card 3 移除，通知栏为唯一去向）。
    #[test]
    fn patch_notice_never_renders_version_numbers() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            let up = render_patch_notice(&s, &UpdateNotice::UpToDate);
            let new = render_patch_notice(&s, &UpdateNotice::NewVersion);
            assert_eq!(up, (true, s.settings_patch_up_to_date.to_string()));
            assert_eq!(new, (true, s.settings_patch_new_version.to_string()));
            for (_, text) in [&up, &new] {
                assert!(
                    !text.contains("1.4.8") && !text.contains("v1"),
                    "{lang:?} 补丁结果文案不应出现版本号: {text}"
                );
            }
            // 检查失败：错误文案照常映射（错误本身不含版本号）。
            let e = UpdateError::Network("t".into());
            let failed = render_patch_notice(&s, &UpdateNotice::CheckFailed(&e));
            assert_eq!(failed, (false, s.update_error(&e)));
        }
    }

    /// 防御分支：Notice::UpdateChecked 正常经 status_bar 分流，不直达 render_notice；
    /// 若直达，降级为中性空提示而非 panic（不中断渲染）。
    #[test]
    fn render_notice_update_checked_is_defensive() {
        let s = Strings::new(Lang::Zh);
        assert_eq!(
            render_notice(&s, &Notice::UpdateChecked),
            (true, String::new())
        );
    }

    /// 其余 notice 分支（下载/工作流/precheck）跨语言映射一致。
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

    /// 布局回归：等宽按钮 + (n-1) 个手动 gap + (n-1) 个自动 item_spacing 必须恰好
    /// 等于可用宽度，不得溢出（历史 bug：溢出把下方 card3 顶到窗口右缘贴边）。
    #[test]
    fn row_button_width_exactly_fills_row() {
        let gap = 12.0;
        for available in [500.0, 580.0, 620.0, 800.0, 1000.0] {
            for item_spacing in [6.0, 8.0, 10.0, 12.0] {
                // 双按钮行（未部署 / 已应用未运行 / 路径无效）。
                let w = row_button_width(available, gap, item_spacing, 2);
                let total = w * 2.0 + gap + item_spacing;
                assert!(
                    (total - available).abs() < 0.01,
                    "n=2 available={available} gap={gap} spacing={item_spacing} -> w={w}, total={total} 应等于可用宽度"
                );
                // 三按钮行（已应用且 Steam 运行中）。
                let w3 = row_button_width(available, gap, item_spacing, 3);
                let total3 = w3 * 3.0 + 2.0 * (gap + item_spacing);
                assert!(
                    (total3 - available).abs() < 0.01,
                    "n=3 available={available} gap={gap} spacing={item_spacing} -> w={w3}, total={total3} 应等于可用宽度"
                );
            }
        }
        // 极窄窗口：最小宽度兜底（max(150)），允许溢出避免按钮被压扁。
        assert_eq!(row_button_width(200.0, 12.0, 10.0, 2), 150.0);
        assert_eq!(row_button_width(200.0, 12.0, 10.0, 3), 150.0);
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
                let short = styled_button(
                    ui,
                    "检查更新",
                    ButtonStyle::Neutral,
                    egui::vec2(96.0, 32.0),
                    true,
                )
                .rect
                .width();
                let long = styled_button(
                    ui,
                    "Download & Extract New Version",
                    ButtonStyle::Primary,
                    egui::vec2(150.0, 32.0),
                    true,
                )
                .rect
                .width();
                let short_en = styled_button(
                    ui,
                    "Check Update",
                    ButtonStyle::Neutral,
                    egui::vec2(96.0, 32.0),
                    true,
                )
                .rect
                .width();
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

    /// Settings — Steam 页签编辑状态（#31 验收核心）：非法输入提交 → 不产出提交值且
    /// 置内联错误；有效输入 → 产出 trim 后的路径并清错误。提交结果以类型化枚举表达——
    /// 调用方只能对 `Changed` 落盘/喂体检流程，`Invalid` 无从写入（类型级不变量）。
    #[test]
    fn steam_path_editor_submit_commits_valid_dir_only() {
        let dir = std::env::temp_dir().join(format!("ost_steam_tab_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // 空缓冲 = 未设置：设置语义下是合法终态（ADR-0012），不报错；
        // 现值也为空 → Unchanged（不重复落盘）。
        let mut e = SteamPathEditor::new("");
        assert_eq!(e.submit(""), SteamPathCommit::Unchanged);
        assert!(!e.invalid, "空串不应触发内联错误");
        // 现值非空而缓冲为空（用户清空）→ 提交未设置（落空串，等同主页面提交语义）。
        assert_eq!(
            e.submit("C:/Steam"),
            SteamPathCommit::Changed(String::new())
        );
        assert!(!e.invalid);

        // 非空且非目录 → 非法：不产出提交值、置内联错误。
        e.buffer = "Z:/definitely/not/a/real/dir_7f3a".into();
        assert_eq!(e.submit("C:/Steam"), SteamPathCommit::Invalid);
        assert!(e.invalid, "非法提交应置内联错误");

        // 有效目录 → 产出 trim 后路径，错误清除。
        let p = dir.display().to_string();
        e.buffer = format!("  {p}  ");
        assert_eq!(e.submit("C:/Steam"), SteamPathCommit::Changed(p.clone()));
        assert!(!e.invalid, "有效提交应清除内联错误");

        // 与现值相同 → Unchanged（跳过落盘/刷新/喂流程；错误态已清）。
        assert_eq!(e.submit(&p), SteamPathCommit::Unchanged);
        assert!(!e.invalid);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 打开设置对话框以当前工作路径播种编辑缓冲（重跑即编辑：起点 = 现值）。
    #[test]
    fn steam_path_editor_seeds_buffer_from_working_path() {
        let e = SteamPathEditor::new("C:/Program Files (x86)/Steam");
        assert_eq!(e.buffer, "C:/Program Files (x86)/Steam");
        assert!(!e.invalid);
    }

    /// 设置对话框内容宽度须容纳 Steam 页签兼容性小节首行（复刻 `compat_section` 首行
    /// 的排布：竖条标题 + 徽章 + 右侧「预热/详细信息」按钮）。健康度 Online 是宽度的
    /// 最坏组合（徽章文案最长 + 两个按钮都出现），英文又宽于中文；两种语言都不能溢出。
    /// 断言两件事：左块（标题+徽章）右缘不超内容右界；右块最左按钮左缘不小于其起点
    /// （right_to_left 溢出会向左侧出界，落在对话框边框之外）。
    #[test]
    /// #34 复看定稿布局回归：Steam 兼容性小节 = 标题行 → 徽章行（徽章左 + 操作右）。
    /// 断言：徽章与右侧操作（详细信息/预热）不溢出内容右界、右侧操作不与徽章重叠
    /// （Online 是宽度的最坏组合：徽章文案最长 + 两按钮都出现；英文又宽于中文）。
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
                    // 标题行（竖条标题）。
                    ui.horizontal(|ui| {
                        ui.allocate_exact_size(egui::vec2(3.0, 13.0), egui::Sense::hover());
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(s.compat_title).size(13.5).strong());
                    });
                    ui.add_space(8.0);
                    // 徽章行：徽章 + 右侧操作（详细信息 + 预热），与 compat_section 同构。
                    ui.horizontal(|ui| {
                        let content_right = ui.cursor().right();
                        // 徽章（Online 文案为最宽）。
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
                        // 右侧动作块起点（徽章之后）。
                        let block_left = ui.cursor().left();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let details = styled_button(
                                ui,
                                s.compat_btn_details,
                                ButtonStyle::Neutral,
                                egui::vec2(88.0, 26.0),
                                true,
                            );
                            let precache = styled_button(
                                ui,
                                s.compat_btn_precache,
                                ButtonStyle::Neutral,
                                egui::vec2(132.0, 26.0),
                                true,
                            );
                            // 右块最左元素（预热）不得越过起点（否则溢出到左侧/与徽章重叠）。
                            if precache.rect.left() + 0.5 < block_left {
                                violations.push(format!(
                                    "{lang:?} 预热按钮左缘 {} 越过右块起点 {block_left}（与徽章重叠）",
                                    precache.rect.left()
                                ));
                            }
                            // 右块整体不越出内容右边界。
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

    /// 向导布局回归：窄卡片在窗口内水平居中（此前整宽贴左，视觉失衡）；
    /// 卡片宽度 = 内容宽 + 边框内外边距（含描边），远小于整窗宽。
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
                    steam_path: String::new(),
                    path_valid: false,
                    download: wizard::DownloadState::Idle,
                };
                let (_, rect) = wizard_steps_ui(ui, Strings::new(Lang::Zh), &view);
                card = Some(rect);
            });
        });
        full.textures_delta.clear();
        let rect = card.expect("向导卡片应有矩形");
        // 窗口 640 宽（中央面板边距对称）：卡片中心应与窗口中心重合。
        assert!(
            (rect.center().x - 320.0).abs() < 0.5,
            "卡片应水平居中，center.x={}，期望 320",
            rect.center().x
        );
        // 窄卡片：宽度为内容宽 + 边框内外边距，不铺满整窗。
        let card_w = WIZARD_CARD_WIDTH + card_frame().total_margin().sum().x;
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

    /// 步骤 3 呈现接缝（纯函数）：补丁已下载（Ready）呈现「已下载」+「完成」，绝不
    /// 出现「尚未下载」提示或下载按钮；跳过隐藏（完成即收尾）。双语文案均成立。
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

    /// 步骤 3 其余子状态决策不变：Idle = 下载提示 + 下载按钮 + 跳过；Running = 进行中
    /// 无主按钮 + 跳过；Failed = 就地报错（错误色）+ 重试 + 跳过。
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

    // ==================== #32 回归：自适应窗口 vs 设置对话框 ====================

    /// 复刻主页面精简后（#32）的首帧内容：顶栏 + 部署状态卡片 + 操作按钮行（通知栏空）。
    /// 返回与 `App::ui` 同口径的内容底缘（`ui.cursor().top()`）。
    fn slim_main_content_height(ctx: &egui::Context, lang: Lang) -> f32 {
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
                // top_bar 复刻。
                ui.horizontal(|ui| {
                    let h2 = 28.0;
                    let font = egui::FontId::proportional(16.0);
                    let tw = text_width(ui, s.app_title, &font, theme::INK);
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(tw, h2), egui::Sense::hover());
                    ui.painter().text(
                        rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        s.app_title,
                        font,
                        theme::INK,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // #34：顶栏只剩齿轮设置入口（语言切换已移除）。
                        let _ = gear_button(ui, s.settings_gear_tooltip);
                    });
                });
                ui.add_space(6.0);
                // card2 复刻（部署状态主行 + 补丁更新按钮，#34 方案 B；Steam 状态已
                // 移至底部状态栏）。
                card_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    card_title(ui, s.card2_title);
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(s.status_not_deployed)
                            .size(16.0)
                            .strong()
                            .color(theme::WEAK),
                    );
                    ui.add_space(10.0);
                    let _ = styled_button(
                        ui,
                        s.settings_btn_patch_update_check,
                        ButtonStyle::Neutral,
                        egui::vec2(150.0, 32.0),
                        true,
                    );
                });
                ui.add_space(10.0);
                // 健康警示：正常态（Ready）不渲染，跳过。
                // action_area 复刻（NotDeployed 两按钮；dlls 就绪 → 无引导行）。
                ui.horizontal(|ui| {
                    let gap = 12.0;
                    let spacing = ui.spacing().item_spacing.x;
                    let size = egui::vec2(
                        row_button_width(ui.available_width(), gap, spacing, 2),
                        44.0,
                    );
                    let _ =
                        styled_button(ui, s.btn_apply_and_launch, ButtonStyle::Deploy, size, true);
                    ui.add_space(gap);
                    let _ =
                        styled_button(ui, s.btn_launch_normal, ButtonStyle::Neutral, size, true);
                });
                ui.add_space(10.0);
                // status_bar 复刻（Steam 未运行项；其余项空）。
                ui.horizontal(|ui| {
                    status_item(ui, s.status_steam_stopped, theme::WEAK);
                });
                h = ui.cursor().top();
            });
        });
        full.textures_delta.clear();
        h
    }

    /// 复刻设置对话框骨架（真实 `egui::Modal` + 固定行 + 滚动区 + 页脚），返回页脚
    /// 「关闭」按钮底缘 Y（> 窗口内高 = 被裁掉）。**每次自建全新 Context**：Modal/Area
    /// 的位置与尺寸记忆挂在 Context 上，复用同一 Context 会让后续渲染沿用上次放置（
    /// 位置漂移，测量失真）；每次首放置（左上角 + 边距）才是真实打开对话框的几何。
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
            egui::Modal::new(egui::Id::new("settings_dialog_repro")).show(ui, |ui| {
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
                    );
                    ui.add_space(4.0);
                    let _ = styled_button(
                        ui,
                        s.settings_tab_steam,
                        ButtonStyle::Neutral,
                        egui::vec2(88.0, 28.0),
                        true,
                    );
                    ui.add_space(4.0);
                    let _ = styled_button(
                        ui,
                        s.settings_tab_about,
                        ButtonStyle::Neutral,
                        egui::vec2(88.0, 28.0),
                        true,
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
                        );
                        footer = r.rect.bottom();
                    });
                });
            });
        });
        full.textures_delta.clear();
        (modal_top, scroll_bottom, footer)
    }

    /// 回归（#32）：瘦身主页面首帧自适应后的窗口内高下，设置对话框页脚（关闭按钮）必须完整
    /// 可见。修复前自适应只随内容收缩（215+36=251），页脚底缘 320 越出窗口底 69px——用户
    /// 必须手动最大化/放大窗口才能看到全部设置界面（报告的 bug）。双语各测一遍（英文高度
    /// 相同但独立验证骨架不受语言影响）。
    #[test]
    fn settings_dialog_footer_visible_at_autosized_window() {
        let ctx = egui::Context::default();
        install_theme(&ctx);
        for lang in [Lang::Zh, Lang::En] {
            // 内容最矮的现实形态（NotDeployed + dlls 就绪 + 无健康警示 + 无通知栏）。
            let content_h = slim_main_content_height(&ctx, lang);
            let inner_h = autosize_inner_height(content_h);
            let (top, _, footer) = settings_dialog_footer_bottom(lang, inner_h);
            assert!(
                top >= 0.0 && footer <= inner_h + 0.5,
                "{lang:?}: 自适应内高 {inner_h}（内容 {content_h}）下设置对话框顶 {top} / 页脚底缘 {footer} 越出窗口（被裁掉）"
            );
        }
    }

    /// 自适应窗口内高下限：任何现实内容高都不应把窗口压到设置对话框最小内高（骨架 +
    /// 滚动区下限）以下；且在自适应内高下骨架+滚动区必须放得下（页脚不越界的代数保证）。
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

    /// #34 修订：三页签（通用/关于/Steam）不溢出对话框宽度（双语）。
    /// 断言：第三个页签右缘不超内容右界。
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
                            );
                            last_tab_right = r.rect.right();
                            ui.add_space(4.0);
                        }
                        if last_tab_right > avail_right + 0.5 {
                            violations.push(format!(
                                "{lang:?} 页签右缘 {last_tab_right} 超过可用宽 {avail_right}"
                            ));
                        }
                        // 页签行最右：GitHub 入口已随 #34 修订移入关于页签，此行不再放。
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

    /// 滚动区公式：低窗口保下限 200、高窗口封顶 420、中段随窗口线性吸收余量。
    #[test]
    fn settings_scroll_height_clamps() {
        assert_eq!(settings_scroll_height(100.0), 200.0);
        assert_eq!(settings_scroll_height(251.0), 200.0);
        assert_eq!(settings_scroll_height(408.0), 200.0);
        assert_eq!(settings_scroll_height(416.0), 208.0);
        assert_eq!(settings_scroll_height(628.0), 420.0);
        assert_eq!(settings_scroll_height(1000.0), 420.0);
    }
}
