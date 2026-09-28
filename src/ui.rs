//! egui 界面：顶栏 + 3 卡片 + 确认弹窗。

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
use crate::update_flow::{UpdateFlow, UpdateLine, UpdateNotice};
use crate::updater::{self, OnlineInfo, UpdateError};
use crate::wizard::{self, DownloadState, Step as WizardStep};
use crate::workflow::{self, Action};

// 颜色一律取自 theme.rs 语义色板（ADR-0010）：仓库唯一色值来源，勿在此处写内联色值。

/// 向导卡片内容宽度：窄于主界面卡片并整体水平居中，视觉更聚焦、更均衡。
const WIZARD_CARD_WIDTH: f32 = 430.0;

/// 设置对话框非滚动行的固定高度占用（标题+页签行+顶部固定行+底部固定行+页脚+窗口边距）。
/// 数值保守偏大：低估会让页脚越界（Modal 是 Area 不约束屏幕），过估只浪费一点滚动区。
fn install_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = theme::PANEL;
    visuals.window_fill = theme::CARD;
    visuals.faint_bg_color = theme::PANEL;
    visuals.extreme_bg_color = theme::PANEL; // TextEdit 底色（旧 FILL_SECONDARY 并入 panel 槽）
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
fn styled_button(
    ui: &mut egui::Ui,
    text: &str,
    style: ButtonStyle,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    let palette = style.palette();
    // 长文案（如英文 "Download & Extract New Version"）超出固定宽度时会被绘制在
    // 按钮边界外截断；按文本宽度自适应，最小仍为调用方指定的 size。
    let font_id = egui::FontId::proportional(13.0);
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
            egui::FontId::proportional(13.0),
            color,
        );
    }
    response
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
/// 检查更新结果的分类来自「更新流程」派生（`notice_text` 的 UpdateChecked 分支），此处不处理。
fn render_notice(s: &Strings, notice: &Notice) -> (bool, String) {
    match notice {
        Notice::Downloaded(Ok(())) => (true, s.ok_downloaded.to_string()),
        Notice::Downloaded(Err(e)) => (false, s.update_error(e)),
        Notice::WorkflowDone(action, Ok(())) => (true, s.success_text(*action).to_string()),
        Notice::WorkflowDone(_, Err(e)) => (false, s.workflow_error_text(e)),
        Notice::Precheck(p) => (false, s.precheck_text(p)),
        Notice::UpdateChecked => {
            // 防御分支：正常路径该变体经 notice_text 分流，不直达此处；直达则记日志并降级为中性空提示。
            log_warn("render_notice 收到 Notice::UpdateChecked（应经 notice_text 分流）");
            (true, String::new())
        }
    }
}

/// 检查更新结果通知 → 当前语言文案（分类来自「更新流程」派生，比较已在流程内完成）。
fn render_update_notice(s: &Strings, n: &UpdateNotice) -> (bool, String) {
    match n {
        UpdateNotice::UpToDate { version } => (true, format!("v{} {}", version, s.up_to_date)),
        UpdateNotice::NewVersion { version } => (true, format!("v{} {}", version, s.new_version)),
        UpdateNotice::CheckFailed(e) => (false, s.update_error(e)),
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
/// 版本信息行：整行单 label（纯文本，非胶囊标签）。
fn version_line(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).size(12.5).color(color));
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
}

/// 最近一次结果提示的结构化数据。
/// 渲染时（`notice_bar`）才按当前语言生成文案，切换语言无需重建。
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

    /// 设置对话框是否打开（重建落地前为空壳：仅标题 + 关闭）。
    settings_open: bool,
    /// 首次运行向导（`Some` = 向导激活并替代主界面；完成/跳过/关窗后置 `None`）。
    wizard: Option<wizard::Wizard>,
    /// 体检流程状态机（编排见 compat_flow）。
    flow: CompatFlow,
    /// 兼容性明细展开开关（纯 UI 状态，不属于流程）。
    compat_details_open: bool,
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

/// 首次运行向导的下载动作：检查线上版本后下载解压到 `dlls/`（向导只需一个成败结果）。
fn wizard_download() -> Result<(), UpdateError> {
    let info = updater::check_update()?;
    updater::download_and_extract(&info, &dll::dll_dir())
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
                            // 选定即推进到步骤 2（该选择立即局部化后续步骤）。
                            for (lang, label) in [
                                (Language::Auto, strings.wizard_language_auto),
                                (Language::Zh, strings.wizard_language_zh),
                                (Language::En, strings.wizard_language_en),
                            ] {
                                if styled_button(
                                    ui,
                                    label,
                                    ButtonStyle::Neutral,
                                    egui::vec2(240.0, 36.0),
                                    true,
                                )
                                .clicked()
                                {
                                    event = Some(wizard::Event::LanguageChosen(lang));
                                }
                                ui.add_space(8.0);
                            }
                        }
                        WizardStep::SteamPath => {
                            ui.label(egui::RichText::new(strings.wizard_path_prompt).size(13.0));
                            ui.add_space(12.0);
                            let mut buf = view.steam_path.clone();
                            // 输入行占满卡片宽度（表单惯例：字段左对齐、撑满可用宽）。
                            ui.horizontal(|ui| {
                                let edit_width = (ui.available_width() - 92.0).max(120.0);
                                let resp = ui.add_sized(
                                    egui::vec2(edit_width, 34.0),
                                    egui::TextEdit::singleline(&mut buf)
                                        .margin(egui::Margin::symmetric(10, 7))
                                        .hint_text(strings.steam_path_label),
                                );
                                if resp.changed() {
                                    event = Some(wizard::Event::PathEdited(buf.clone()));
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
                                    event =
                                        Some(wizard::Event::PathEdited(dir.display().to_string()));
                                }
                            });
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
                            // 子状态文案：Idle 提示需显式开始；Running 进行中；Failed 就地报错。
                            match &view.download {
                                DownloadState::Idle => {
                                    ui.label(
                                        egui::RichText::new(strings.wizard_download_prompt)
                                            .size(13.0),
                                    );
                                }
                                DownloadState::Running => {
                                    ui.label(
                                        egui::RichText::new(strings.wizard_download_running)
                                            .size(13.0),
                                    );
                                }
                                DownloadState::Failed(e) => {
                                    ui.label(
                                        egui::RichText::new(
                                            strings
                                                .wizard_download_failed
                                                .replace("{err}", &strings.update_error(e)),
                                        )
                                        .size(13.0)
                                        .color(theme::DANGER),
                                    );
                                }
                            }
                            ui.add_space(16.0);
                            // 按钮行手动居中（horizontal 占满宽，需前置空间补偿）。
                            ui.horizontal(|ui| {
                                let item_gap = ui.spacing().item_spacing.x;
                                // 主按钮：Idle = 开始下载；Failed = 重试；Running 无主按钮。
                                let start_label = match &view.download {
                                    DownloadState::Idle => Some(strings.wizard_btn_download),
                                    DownloadState::Failed(_) => Some(strings.wizard_btn_retry),
                                    DownloadState::Running => None,
                                };
                                // 副按钮：跳过（任何子状态都可用，跳过不阻塞完成）。
                                let mut row_w = 120.0; // 跳过按钮恒显示
                                let mut count = 1;
                                if start_label.is_some() {
                                    count += 1;
                                    row_w += 140.0;
                                }
                                row_w += (count - 1) as f32 * item_gap;
                                ui.add_space(((ui.available_width() - row_w) / 2.0).max(0.0));

                                if let Some(label) = start_label
                                    && styled_button(
                                        ui,
                                        label,
                                        ButtonStyle::Primary,
                                        egui::vec2(140.0, 34.0),
                                        true,
                                    )
                                    .clicked()
                                {
                                    event = Some(wizard::Event::DownloadRequested);
                                }
                                if styled_button(
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
        let steam_path = if config.steam_path.is_empty() || !Path::new(&config.steam_path).is_dir()
        {
            steam::detect_steam_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        } else {
            config.steam_path.clone()
        };
        // 首次运行向导：配置缺失/损坏或已存路径无效时激活（`should_show` 为纯谓词）。
        // 以当前会话值（配置或注册表检测）播种路径、以配置偏好播种语言；重跑即编辑。
        let wizard = wizard::should_show(&config::config_path())
            .then(|| wizard::Wizard::new(lang_pref, steam_path.clone()));
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
            wizard,
            flow,
            compat_details_open: false,
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

    fn toggle_lang(&mut self) {
        // 顶栏切换把「语言偏好」钉到另一侧（auto 时相对当前生效语言切换）；选择
        // 持久化，重启后恢复（不再每次回到系统检测）。
        self.set_language(self.lang_pref.toggled(self.lang));
        self.config.language = self.lang_pref;
        self.persist_config();
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

    /// 路径编辑会话结束（失焦/浏览选定）后提交：仅当空（未设置）或为有效目录时
    /// 更新持久化镜像并落盘，避免把无效路径固化到下次启动（无效路径仅会话内
    /// 生效，重启后回退注册表检测——直到 #26 的 wizard 接管「保存路径无效时引导修复」）。
    fn commit_steam_path(&mut self) {
        let p = self.steam_path.trim();
        if p.is_empty() || Path::new(p).is_dir() {
            self.config.steam_path = p.to_string();
            self.persist_config();
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

    // ---------- 设置对话框（重建前占位：空壳，仅标题 + 关闭） ----------

    /// 打开设置：置位。对话框本体见 `settings_dialog`（Config Editor / OnlineFix 已随 #27 移除）。
    fn open_settings(&mut self) {
        self.settings_open = true;
    }

    /// 设置对话框（模态空壳）：重建落地前仅标题 + 关闭按钮，无任何页签。
    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut close_clicked = false;
        egui::Modal::new(egui::Id::new("settings_dialog")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading(self.strings.settings_title);
            ui.add_space(8.0);
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
            self.settings_open = false;
        }
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
                // RTL 先放者靠右：语言切换最右，设置在其左（次序维持现状）。
                if styled_button(
                    ui,
                    self.lang.toggle_label(),
                    ButtonStyle::Neutral,
                    egui::vec2(72.0, 28.0),
                    true,
                )
                .clicked()
                {
                    self.toggle_lang();
                }
                if styled_button(
                    ui,
                    self.strings.btn_settings,
                    ButtonStyle::Neutral,
                    egui::vec2(72.0, 28.0),
                    true,
                )
                .clicked()
                {
                    self.open_settings();
                }
            });
        });
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
                    self.feed_path_changed(&ctx);
                }
                // 编辑会话结束（失焦）即把有效路径落盘到 config.toml（无效路径不固化）。
                if resp.lost_focus() {
                    self.commit_steam_path();
                }
                if styled_button(
                    ui,
                    self.strings.browse,
                    ButtonStyle::Neutral,
                    egui::vec2(82.0, 34.0),
                    true,
                )
                .clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.steam_path = dir.display().to_string();
                    self.refresh_status();
                    let ctx = self.ctx.clone();
                    self.feed_path_changed(&ctx);
                    self.commit_steam_path();
                }
            });
            self.compat_section(ui);
        });
        ui.add_space(10.0);
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

    /// Card 1 底部「Steam 核心兼容性」小节：第一行标题+状态徽章+操作靠右，第二行辅助说明弱化。
    fn compat_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        // 无分隔线：竖条标题 + 徽章自成边界，直接衔接上方路径输入区。

        let v = CompatView::snapshot(self);

        // 第一行：左侧（竖条标题 + 状态徽章）| 右侧操作（预热 + 详细信息，贴右边缘）。
        ui.horizontal(|ui| {
            // 标题：与其他卡片一致的蓝色竖条指示器。
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
            self.compat_badge(ui, v.summary);

            // 右侧操作区：right_to_left 首项（详细信息）贴最右，预热按钮在其左侧。
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !v.checking && ui.button(self.strings.compat_btn_details).clicked() {
                    self.compat_details_open = !self.compat_details_open;
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

        // 辅助说明：五态一句话（Checking 为瞬时态不显示），小字号弱灰 + 缩进对齐标题竖条。
        if v.summary != CompatSummary::Checking {
            let tip = match v.summary {
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
                ui.label(egui::RichText::new(tip).size(11.5).color(theme::WEAK));
            });
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

        // 详情明细（展开时）：快照阶段按需克隆的报告，展开期间每帧一次，热路径零拷贝。
        if let Some(report) = &v.detail_report {
            self.compat_details(ui, report);
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
    }

    fn card2(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度
            card_title(ui, self.strings.card2_title);
            ui.add_space(10.0);

            // 部署状态：纯文字 + 颜色（无圆点徽章）。
            let (text, color) = match self.status {
                DeployStatus::InvalidPath => (self.strings.status_invalid, theme::WEAK),
                DeployStatus::Deployed => (self.strings.status_deployed, theme::SUCCESS),
                DeployStatus::NotDeployed => (self.strings.status_not_deployed, theme::WEAK),
            };
            status_line(ui, text, color);
        });
        ui.add_space(10.0);
    }

    /// 独立操作区：等宽按钮并排（位于卡片 2 与卡片 3 之间）。
    /// 已应用时三枚：运行中为（退出并卸载 / 重启 Steam / 卸载并重启），
    /// 未运行为（启动 Steam / 卸载补丁 / 卸载补丁并重启 Steam）；其余两枚。
    fn action_area(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            let gap = 12.0;
            let spacing = ui.spacing().item_spacing.x;
            // 宽度公式必须扣除 egui 自动插入的 item_spacing 与手动 gap（见 row_button_width），
            // 否则按钮行实际占宽溢出，把下方卡片（card3 依赖 available_width 撑满）顶到窗口右缘。
            match self.status {
                DeployStatus::Deployed if self.steam_running => {
                    // 「退出 Steam 并卸载补丁」（唯一警戒）/「重启 Steam」/「卸载补丁并重启 Steam」。
                    let size = egui::vec2(
                        row_button_width(ui.available_width(), gap, spacing, 3),
                        36.0,
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
                        36.0,
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
                        36.0,
                    );
                    if styled_button(
                        ui,
                        self.strings.btn_apply_and_launch,
                        ButtonStyle::Deploy,
                        size,
                        // 补丁未下载（dlls/ 缺文件）时置灰，避免点了才报 NoTargetDlls（见 ADR-0011）。
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
                        36.0,
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

    fn card3(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度
            card_title(ui, self.strings.card3_title);
            ui.add_space(10.0);

            // 本地版本行（文件本位，ADR-0011）：文件缺失即「未下载」，version.txt 记录不作数；
            // 文件齐全且有记录 → v+版本；有文件无记录 → 已本地就绪 (未记录版本)。
            let (local_text, local_color) = if !dll::dlls_present() {
                (
                    format!(
                        "{}{}",
                        self.strings.local_version, self.strings.local_ver_missing
                    ),
                    theme::WEAK,
                )
            } else {
                match &self.local_version {
                    Some(v) => (
                        format!(
                            "{}v{}",
                            self.strings.local_version,
                            v.trim_start_matches('v')
                        ),
                        theme::INK,
                    ),
                    None => (
                        format!(
                            "{}{}",
                            self.strings.local_version, self.strings.local_ver_ready_no_record
                        ),
                        theme::SUB,
                    ),
                }
            };
            version_line(ui, &local_text, local_color);
            ui.add_space(6.0);

            // 线上版本行：未知 / 正在检查更新 / v+版本+后缀 / 检查失败。
            let prefix = self.strings.online_version;
            // 更新对比同用文件本位：文件缺失时无视版本记录按「本地缺失」比较，
            // 保证检查后下载按钮必然出现（否则记录版本=线上版本时死路，ADR-0011）。
            let derived = self.update_flow.derived(self.local_known_version());
            let (online_text, online_color) = match &derived.line {
                UpdateLine::Unknown => (format!("{}{}", prefix, self.strings.unknown), theme::SUB),
                UpdateLine::Checking => {
                    (format!("{}{}", prefix, self.strings.checking), theme::SUB)
                }
                UpdateLine::UpToDate { version } => (
                    format!("{}v{} {}", prefix, version, self.strings.up_to_date),
                    theme::SUB,
                ),
                UpdateLine::NewVersion { version } => (
                    format!("{}v{} {}", prefix, version, self.strings.new_version),
                    theme::SUB,
                ),
                UpdateLine::CheckFailed(e) => (
                    format!(
                        "{}{} ({})",
                        prefix,
                        self.strings.online_check_fail,
                        self.strings.update_error(e),
                    ),
                    theme::DANGER,
                ),
            };
            version_line(ui, &online_text, online_color);
            ui.add_space(14.0);

            let ctx = ui.ctx().clone();
            let mut do_check = false;
            let mut do_download: Option<OnlineInfo> = None;

            // 先只收集按钮意图，避免借用冲突。
            ui.horizontal(|ui| {
                if styled_button(
                    ui,
                    self.strings.btn_check_update,
                    ButtonStyle::Neutral,
                    egui::vec2(96.0, 32.0),
                    !self.gate.is_busy(),
                )
                .clicked()
                {
                    do_check = true;
                }

                if let Some(info) = derived.download
                    && styled_button(
                        ui,
                        self.strings.btn_download_and_extract,
                        ButtonStyle::Primary,
                        egui::vec2(150.0, 32.0),
                        !self.gate.is_busy(),
                    )
                    .clicked()
                {
                    do_download = Some(info.clone());
                }
            });

            if do_check {
                self.check_update(&ctx);
            }
            if let Some(info) = do_download {
                self.download_update(&ctx, info);
            }
        });
        ui.add_space(10.0);
    }

    /// 文件本位（ADR-0011）：`dlls/` 缺文件时无视版本记录按「本地缺失」比较，
    /// `derived` 的全部消费者（本地版本行 / 通知文案 / 下载可用性）共用同一判据。
    fn local_known_version(&self) -> Option<&str> {
        dll::dlls_present()
            .then_some(self.local_version.as_deref())
            .flatten()
    }

    /// 最近一次结果提示 → 当前语言渲染（切换语言后无需重建 notice，逐帧取当前 strings）。
    fn notice_text(&self) -> Option<(bool, String)> {
        match &self.notice {
            Some(Notice::UpdateChecked) => self
                .update_flow
                .derived(self.local_known_version())
                .notice
                .map(|n| render_update_notice(&self.strings, &n)),
            Some(n) => Some(render_notice(&self.strings, n)),
            None => None,
        }
    }

    fn notice_bar(&mut self, ui: &mut egui::Ui) {
        // busy / 成功改中性文字；错误保留红色（kill-ai-slop：收敛语义三连）。
        if let Some(kind) = self.gate.current() {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(rect.center(), 3.5, theme::ACCENT);
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(self.strings.busy_label(kind))
                        .size(12.5)
                        .color(theme::WEAK),
                );
            });
            return;
        }
        if let Some((ok, text)) = self.notice_text() {
            let (color, dot_color) = if ok {
                (theme::INK, theme::SUCCESS)
            } else {
                (theme::DANGER, theme::DANGER)
            };
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 3.5, dot_color);
                ui.add_space(6.0);
                ui.label(egui::RichText::new(text).size(12.5).color(color));
            });
        }
    }

    /// 主界面内容（向导未激活时渲染）。
    fn main_content(&mut self, ui: &mut egui::Ui) {
        self.top_bar(ui);
        self.card1(ui);
        self.card2(ui);
        self.action_area(ui);
        self.card3(ui);
        self.notice_bar(ui);
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
        if self.wizard.is_none() && !self.autosized && content_h > 0.0 {
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

    /// 检查更新结果通知文案：分类（已最新/可更新/失败）由「更新流程」派生，此处验证文案映射。
    #[test]
    fn update_notice_text_up_to_date_vs_new_version() {
        let zh = Strings::new(Lang::Zh);
        assert_eq!(
            render_update_notice(&zh, &UpdateNotice::UpToDate { version: "1.4.8" }),
            (true, "v1.4.8 (本地已是最新版)".to_string())
        );
        assert_eq!(
            render_update_notice(&zh, &UpdateNotice::NewVersion { version: "1.4.8" }),
            (true, "v1.4.8 (发现可更新版本)".to_string())
        );
        let e = updater::UpdateError::Network("t".into());
        assert_eq!(
            render_update_notice(&zh, &UpdateNotice::CheckFailed(&e)),
            (false, zh.update_error(&e))
        );
    }

    /// 切换语言后，检查更新通知重新映射即得新语言文案（结构化分类不锁死语言）。
    #[test]
    fn update_notice_follows_language_switch() {
        let zh = render_update_notice(
            &Strings::new(Lang::Zh),
            &UpdateNotice::UpToDate { version: "1.4.8" },
        );
        let en = render_update_notice(
            &Strings::new(Lang::En),
            &UpdateNotice::UpToDate { version: "1.4.8" },
        );
        assert_eq!(zh, (true, "v1.4.8 (本地已是最新版)".to_string()));
        assert_eq!(en, (true, "v1.4.8 (Up to date)".to_string()));
        // 英文界面不应出现中文。
        assert!(!en.1.contains('本'), "en notice 不应含中文: {}", en.1);
    }

    /// 防御分支：Notice::UpdateChecked 正常经 notice_text 分流，不直达 render_notice；
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
                render_notice(&s, &Notice::Precheck(workflow::Precheck::NoSteamDir)),
                (false, s.precheck_text(&workflow::Precheck::NoSteamDir))
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
                    download: DownloadState::Idle,
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
}
