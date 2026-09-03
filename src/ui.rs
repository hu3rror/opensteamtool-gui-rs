//! egui 界面：顶栏 + 3 卡片 + 确认弹窗。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use egui::Frame;

use crate::dll::{self, DeployStatus};
use crate::i18n::{Lang, Strings};
use crate::process::{self, SteamEvent, SteamMonitor};
use crate::steam;
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

/// Python 风格按钮：手动绘制底/描边/文字，hover 换色（效仿 tkinter <Enter>/<Leave>）。
/// `enabled=false` 时文字弱化为 muted 且不响应点击。
fn py_button(
    ui: &mut egui::Ui,
    text: &str,
    style: PyStyle,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let fill = if enabled && response.hovered() { style.hover } else { style.bg };
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

/// 卡片标题：3px 蓝色 accent bar + 标题（Python 版样式）。
fn card_title(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 13.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, ACCENT);
        ui.add_space(8.0);
        ui.label(egui::RichText::new(text).size(13.5).strong().color(TEXT_INK));
    });
}

/// 状态行：纯文字 + 颜色（效仿 Python，无圆点徽章）。
fn status_line(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).size(13.0).strong().color(color));
}

/// 版本信息行：整行单 label（效仿 Python 纯文本，非胶囊标签）。
fn version_line(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).size(12.5).color(color));
}

/// 绿色主按钮（应用补丁并启动 Steam）。
fn deploy_button(ui: &mut egui::Ui, text: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle { bg: BTN_DEPLOY_BG, hover: BTN_DEPLOY_HOVER, fg: egui::Color32::WHITE, border: None },
        size,
        enabled,
    )
}

/// 白色次按钮（正常启动 Steam）。
fn launch_button(ui: &mut egui::Ui, text: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle { bg: CARD_BG, hover: BTN_SECONDARY_HOVER, fg: TEXT_SUB, border: Some(ENTRY_BORDER) },
        size,
        enabled,
    )
}

/// 浅蓝描边按钮（退出 Steam 并卸载补丁）。
fn uninstall_a_button(ui: &mut egui::Ui, text: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle {
            bg: BTN_UNINSTALL_A_BG,
            hover: BTN_UNINSTALL_A_HOVER,
            fg: BTN_UNINSTALL_A_FG,
            border: Some(BTN_UNINSTALL_A_BORDER),
        },
        size,
        enabled,
    )
}

/// 蓝色实心按钮（卸载补丁并重启 Steam）。
fn uninstall_b_button(ui: &mut egui::Ui, text: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle { bg: BTN_UNINSTALL_B_BG, hover: BTN_UNINSTALL_B_HOVER, fg: egui::Color32::WHITE, border: None },
        size,
        enabled,
    )
}

/// 主蓝按钮（下载/确认弹窗）。
fn primary_button(ui: &mut egui::Ui, text: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle { bg: ACCENT, hover: ACCENT_ACTIVE, fg: egui::Color32::WHITE, border: None },
        size,
        enabled,
    )
}

/// 次灰按钮（检查更新/浏览/取消）。
fn secondary_button(ui: &mut egui::Ui, text: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle {
            bg: FILL_SECONDARY,
            hover: BTN_SECONDARY_HOVER,
            fg: TEXT_SUB,
            border: Some(ENTRY_BORDER),
        },
        size,
        enabled,
    )
}

/// 语言切换按钮：白底 + 蓝字 + 描边（Python btn_lang 样式）。
fn lang_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle { bg: CARD_BG, hover: FILL_SECONDARY, fg: ACCENT, border: Some(ENTRY_BORDER) },
        egui::vec2(56.0, 26.0),
        true,
    )
}

/// 后台线程 → UI 线程的消息。
enum Msg {
    /// 后台阶段变化（如 kill Steam 完成后进入部署阶段）。
    Phase(BusyKind),
    UpdateChecked(Result<OnlineInfo, UpdateError>),
    Downloaded(Result<(), UpdateError>),
    /// 组合操作完成（成功/失败，携带动作以取成功文案）。
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
}

/// 线上更新状态。
enum UpdateState {
    Idle,
    Checking,
    Checked(Result<OnlineInfo, UpdateError>),
}

pub struct App {
    lang: Lang,
    strings: Strings,
    steam_path: String,
    status: DeployStatus,
    steam_running: bool,
    steam_monitor: SteamMonitor,
    local_version: Option<String>,
    update_state: UpdateState,
    busy: bool,
    /// 忙碌时当前操作类型（用于显示进度文案）。
    busy_kind: Option<BusyKind>,
    /// 待确认「关闭 Steam」的操作。
    confirm: Option<Action>,
    /// 最近一次结果提示（成功/失败）。
    notice: Option<(bool, String)>,
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
        install_cjk_font(&cc.egui_ctx);
        install_theme(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let lang = crate::i18n::detect_system_lang();
        let strings = Strings::new(lang);
        // 窗口标题随语言（zh: OpenSteamTool 一键管理工具 / en: OpenSteamTool Manager）。
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Title(strings.window_title.to_owned()));
        let steam_path = steam::detect_steam_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let steam_dir = Path::new(&steam_path);
        let status = dll::check_status(steam_dir);
        let local_version = dll::read_local_version(&dll::dll_dir());
        let steam_monitor = SteamMonitor::new();
        let steam_running = steam_monitor.is_running();

        let tray = Tray::new(
            crate::tray::load_icon(),
            strings.app_title,
            strings.tray_show,
            strings.tray_quit,
            strings.tray_minimize,
        );

        let app = Self {
            lang,
            strings,
            steam_path,
            status,
            steam_running,
            steam_monitor,
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
            window_visible: true,
            autosized: false,
            pending_focus: false,
            minimize_to_tray: true,
            was_minimized: false,
        };
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
                TrayAction::ToggleMinimizeToTray => self.minimize_to_tray = minimize_checked,
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
                    self.notice = match &res {
                        Ok(info) => Some((
                            true,
                            format!("v{} {}", info.version, self.strings.new_version),
                        )),
                        Err(e) => Some((false, self.strings.update_error(e))),
                    };
                    self.update_state = UpdateState::Checked(res);
                }
                Msg::Downloaded(res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    match res {
                        Ok(()) => {
                            self.local_version = dll::read_local_version(&dll::dll_dir());
                            self.notice = Some((true, self.strings.ok_downloaded.to_string()));
                        }
                        Err(e) => self.notice = Some((false, self.strings.update_error(&e))),
                    }
                }
                Msg::WorkflowDone(action, res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    match res {
                        Ok(()) => {
                            self.refresh_status();
                            self.notice =
                                Some((true, self.strings.success_text(action).to_string()));
                        }
                        Err(e) => self.notice = Some((false, self.strings.workflow_error_text(&e))),
                    }
                    self.steam_running = self.steam_monitor.rescan();
                    // 启动/重启类成功后 Steam 已运行 → 直接隐藏到托盘（不依赖边沿检测）；
                    // 仅退出并卸载（ExitAndUninstall）Steam 未运行 → 保持显示。
                    self.hide_if_steam_running();
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
        let dll_dir = dll::dll_dir();
        let steam_dir = PathBuf::from(self.steam_path.trim());

        // 前置校验（类型化错误 → 本地化文案），失败则不进入忙碌状态。
        let ops = match workflow::plan(action, kill_first, &steam_dir, &dll_dir) {
            Ok(ops) => ops,
            Err(precheck) => {
                self.confirm = None;
                self.notice = Some((false, self.strings.precheck_text(&precheck)));
                return;
            }
        };

        self.busy = true;
        self.busy_kind = Some(ops.first().expect("plan never returns empty").phase()); // 同步首阶段，点击即见阶段文案
        self.confirm = None;

        let ctx2 = ctx.clone();
        let tx = self.tx.clone();
        self.spawn(ctx, move || {
            let res = workflow::execute(
                &ops,
                &workflow::WorkflowCtx { dll_dir, steam_dir },
                |phase| {
                    let _ = tx.send(Msg::Phase(phase));
                    ctx2.request_repaint();
                },
            );
            Msg::WorkflowDone(action, res)
        });
    }

    fn toggle_lang(&mut self) {
        self.lang = self.lang.toggle();
        self.strings = Strings::new(self.lang);
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Title(self.strings.window_title.to_owned()));
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
        let dll_dir = dll::dll_dir();
        self.spawn(ctx, move || {
            Msg::Downloaded(updater::download_and_extract(&info, &dll_dir))
        });
    }

    // ---------- UI ----------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(self.strings.app_title)
                    .size(16.0)
                    .strong()
                    .color(TEXT_INK),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if lang_button(ui, self.lang.toggle_label()).clicked() {
                    self.toggle_lang();
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
                }
                if secondary_button(ui, self.strings.browse, egui::vec2(82.0, 34.0), true).clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.steam_path = dir.display().to_string();
                    self.refresh_status();
                }
            });
        });
        ui.add_space(10.0);
    }

    fn card2(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度
            card_title(ui, self.strings.card2_title);
            ui.add_space(10.0);

            // 部署状态：纯文字 + 颜色（效仿 Python，无圆点徽章）。
            let (text, color) = match self.status {
                DeployStatus::InvalidPath => (self.strings.status_invalid, TEXT_WEAK),
                DeployStatus::Deployed => (self.strings.status_deployed, STATUS_INSTALLED),
                DeployStatus::NotDeployed => (self.strings.status_not_deployed, TEXT_WEAK),
            };
            status_line(ui, text, color);
        });
        ui.add_space(10.0);
    }

    /// 独立操作区：两大按钮等宽并排（效仿 Python action_frame，位于卡片 2 与卡片 3 之间）。
    fn action_area(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            let gap = 12.0;
            let w = ((ui.available_width() - gap) / 2.0).max(150.0);
            let size = egui::vec2(w, 36.0);
            match self.status {
                DeployStatus::Deployed => {
                    // Steam 运行中 →「退出 Steam 并卸载补丁」；已退出 → 直接「卸载补丁」。
                    let uninstall_label = if self.steam_running {
                        self.strings.btn_exit_and_uninstall
                    } else {
                        self.strings.btn_uninstall
                    };
                    if uninstall_a_button(ui, uninstall_label, size, !self.busy).clicked() {
                        self.request_action(&ctx, Action::ExitAndUninstall);
                    }
                    ui.add_space(gap);
                    if uninstall_b_button(
                        ui,
                        self.strings.btn_uninstall_and_restart,
                        size,
                        !self.busy,
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::UninstallAndRestart);
                    }
                }
                DeployStatus::NotDeployed => {
                    if deploy_button(ui, self.strings.btn_apply_and_launch, size, !self.busy).clicked() {
                        self.request_action(&ctx, Action::ApplyAndLaunch);
                    }
                    ui.add_space(gap);
                    if launch_button(ui, self.strings.btn_launch_normal, size, !self.busy).clicked() {
                        self.request_action(&ctx, Action::Launch);
                    }
                }
                DeployStatus::InvalidPath => {
                    // 无有效路径时禁用操作按钮。
                    deploy_button(ui, self.strings.btn_apply_and_launch, size, false);
                    ui.add_space(gap);
                    launch_button(ui, self.strings.btn_launch_normal, size, false);
                }
            }
        });
        ui.add_space(10.0);
    }

    fn card3(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度
            card_title(ui, self.strings.card3_title);
            ui.add_space(10.0);

            // 本地版本行：v + 版本 / 已本地就绪 (未记录版本) / 未下载 (dlls 文件夹缺失文件)。
            let dll_dir = dll::dll_dir();
            let all_local_exist = dll::TARGET_DLLS.iter().all(|d| dll_dir.join(d).is_file());
            let (local_text, local_color) = match &self.local_version {
                Some(v) => (
                    format!("{}v{}", self.strings.local_version, v.trim_start_matches('v')),
                    TEXT_INK,
                ),
                None if all_local_exist => (
                    format!("{}{}", self.strings.local_version, self.strings.local_ver_ready_no_record),
                    TEXT_SUB,
                ),
                None => (
                    format!("{}{}", self.strings.local_version, self.strings.local_ver_missing),
                    TEXT_WEAK,
                ),
            };
            version_line(ui, &local_text, local_color);
            ui.add_space(6.0);

            // 线上版本行：未知 / 正在检查更新 / v+版本+后缀 / 检查失败。
            let prefix = self.strings.online_version;
            let (online_text, online_color) = match &self.update_state {
                UpdateState::Idle => (format!("{}{}", prefix, self.strings.unknown), TEXT_SUB),
                UpdateState::Checking => (format!("{}{}", prefix, self.strings.checking), TEXT_SUB),
                UpdateState::Checked(Ok(info)) => {
                    let local = self.local_version.as_deref().unwrap_or("");
                    let suffix = if local == info.version {
                        self.strings.up_to_date
                    } else {
                        self.strings.new_version
                    };
                    (format!("{}v{} {}", prefix, info.version, suffix), TEXT_SUB)
                }
                UpdateState::Checked(Err(e)) => (
                    format!(
                        "{}{} ({})",
                        prefix,
                        self.strings.online_check_fail,
                        self.strings.update_error(e),
                    ),
                    ERR_RED,
                ),
            };
            version_line(ui, &online_text, online_color);
            ui.add_space(14.0);

            let ctx = ui.ctx().clone();
            let mut do_check = false;
            let mut do_download: Option<OnlineInfo> = None;

            // 先只收集按钮意图，避免借用冲突。
            ui.horizontal(|ui| {
                if secondary_button(
                    ui,
                    self.strings.btn_check_update,
                    egui::vec2(96.0, 32.0),
                    !self.busy,
                )
                .clicked()
                {
                    do_check = true;
                }

                if let UpdateState::Checked(Ok(info)) = &self.update_state {
                    let local = self.local_version.as_deref().unwrap_or("");
                    if info.version != local
                        && !info.version.is_empty()
                        && primary_button(
                            ui,
                            self.strings.btn_download_and_extract,
                            egui::vec2(150.0, 32.0),
                            !self.busy,
                        )
                        .clicked()
                    {
                        do_download = Some(info.clone());
                    }
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

    fn notice_bar(&mut self, ui: &mut egui::Ui) {
        // busy / 成功改中性文字；错误保留红色（kill-ai-slop：收敛语义三连）。
        if let Some(kind) = self.busy_kind {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 3.5, ACCENT);
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(self.strings.busy_label(kind))
                        .size(12.5)
                        .color(TEXT_WEAK),
                );
            });
            return;
        }
        if let Some((ok, text)) = &self.notice {
            let (color, dot_color) = if *ok {
                (TEXT_INK, DOT_RUNNING)
            } else {
                (ERR_RED, ERR_RED)
            };
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 3.5, dot_color);
                ui.add_space(6.0);
                ui.label(egui::RichText::new(text).size(12.5).color(color));
            });
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
            self.card2(ui);
            self.action_area(ui);
            self.card3(ui);
            self.notice_bar(ui);

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
                    if primary_button(ui, self.strings.yes, egui::vec2(72.0, 30.0), true).clicked() {
                        confirmed = true;
                    }
                    ui.add_space(4.0);
                    if secondary_button(ui, self.strings.no, egui::vec2(72.0, 30.0), true).clicked() {
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
}
