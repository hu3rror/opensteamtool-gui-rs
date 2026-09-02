//! egui 界面：顶栏 + 3 卡片 + 确认弹窗。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use eframe::egui;
use egui::Frame;

use crate::dll::{self, DeployStatus};
use crate::i18n::{Lang, Strings};
use crate::process;
use crate::steam;
use crate::updater::{self, OnlineInfo, UpdateError};
use crate::workflow::{self, Action, BusyKind};
use crate::tray::{Tray, TrayAction};

// ---------- kill-ai-slop 约束的浅色主题 (Apple-inspired refined palette) ----------
// 单一 accent（Steam 蓝）+ 中性底 + hairline 卡片；无渐变/毛玻璃/发光点。
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0x71, 0xE3); // Apple / Steam Vibrant Blue
const ACCENT_ACTIVE: egui::Color32 = egui::Color32::from_rgb(0x00, 0x58, 0xB0);
const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(0xF5, 0xF5, 0xF7); // macOS Light System Gray
const CARD_BG: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF);
const BORDER: egui::Color32 = egui::Color32::from_rgb(0xE5, 0xE5, 0xEA); // Hairline System Border
const BORDER_SUBTLE: egui::Color32 = egui::Color32::from_rgb(0xEB, 0xEB, 0xF0);
const FILL_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xF2, 0xF7); // System Fill
const TEXT_INK: egui::Color32 = egui::Color32::from_rgb(0x1D, 0x1D, 0x1F); // Apple Primary Label
const TEXT_WEAK: egui::Color32 = egui::Color32::from_rgb(0x6E, 0x6E, 0x73); // Apple Secondary Label
const DOT_RUNNING: egui::Color32 = egui::Color32::from_rgb(0x34, 0xC7, 0x59); // Apple Green
const DOT_STOPPED: egui::Color32 = egui::Color32::from_rgb(0x8E, 0x8E, 0x93); // Apple Gray
const ERR_RED: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x3B, 0x30); // Apple Red

fn install_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = PANEL_BG;
    visuals.window_fill = CARD_BG;
    visuals.faint_bg_color = FILL_SECONDARY;
    visuals.extreme_bg_color = CARD_BG;
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
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
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

/// 主操作按钮：accent 填充 + 白字。
fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .size(13.0)
            .strong()
            .color(egui::Color32::WHITE),
    )
    .fill(ACCENT)
    .stroke(egui::Stroke::NONE)
    .corner_radius(egui::CornerRadius::same(8))
    .min_size(egui::vec2(0.0, 34.0))
}

/// 次操作按钮：透明/浅灰底 + hairline 边框。
fn secondary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).size(13.0).color(TEXT_INK))
        .fill(FILL_SECONDARY)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(8))
        .min_size(egui::vec2(0.0, 34.0))
}

/// 幽灵/小号操作按钮（如语言切换）。
fn ghost_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).size(12.0).strong().color(TEXT_WEAK))
        .fill(FILL_SECONDARY)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(7))
        .min_size(egui::vec2(40.0, 28.0))
}

/// 真实状态用小圆点 + 词（kill-ai-slop：平色小点，无光晕无脉冲）。
fn status_badge(ui: &mut egui::Ui, color: egui::Color32, text: &str) {
    Frame::new()
        .fill(FILL_SECONDARY)
        .stroke(egui::Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 3.5, color);
                ui.add_space(6.0);
                ui.label(egui::RichText::new(text).size(12.5).color(TEXT_INK));
            });
        });
}

/// 键值展示标签（版本信息卡片用）。
fn value_tag(ui: &mut egui::Ui, label: &str, value: &str, is_highlight: bool) {
    Frame::new()
        .fill(FILL_SECONDARY)
        .stroke(egui::Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).size(12.0).color(TEXT_WEAK));
                ui.add_space(6.0);
                let val_color = if is_highlight { ACCENT } else { TEXT_INK };
                ui.label(
                    egui::RichText::new(value)
                        .size(12.5)
                        .strong()
                        .color(val_color),
                );
            });
        });
}

/// Steam 运行状态缓存刷新间隔。
const STEAM_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// 后台线程 → UI 线程的消息。
enum Msg {
    /// 后台阶段变化（如 kill Steam 完成后进入部署阶段）。
    Phase(BusyKind),
    UpdateChecked(Result<OnlineInfo, UpdateError>),
    Downloaded(Result<(), UpdateError>),
    /// 组合操作完成（成功/失败，携带动作以取成功文案）。
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
}

impl BusyKind {
    /// 当前语言下的忙碌文案。
    fn label(self, s: &Strings) -> &'static str {
        match self {
            BusyKind::Deploying => s.busy_deploying,
            BusyKind::Uninstalling => s.busy_uninstalling,
            BusyKind::Launching => s.busy_launching,
            BusyKind::Checking => s.checking,
            BusyKind::Downloading => s.busy_downloading,
            BusyKind::ClosingSteam => s.busy_killing,
        }
    }
}

impl Action {
    /// 组合操作成功后的提示文案。
    fn success_text(self, s: &Strings) -> &'static str {
        match self {
            Action::ApplyAndLaunch => s.ok_deployed,
            Action::Launch => s.ok_launched,
            Action::ExitAndUninstall | Action::UninstallAndRestart => s.ok_uninstalled,
        }
    }
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
    last_steam_check: Instant,
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
    /// 上次检测到的 Steam 运行状态（边沿检测用）。
    steam_was_running: bool,
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

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_cjk_font(&cc.egui_ctx);
        install_theme(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let lang = crate::i18n::detect_system_lang();
        let strings = Strings::new(lang);
        let steam_path = steam::detect_steam_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let steam_dir = Path::new(&steam_path);
        let status = dll::check_status(steam_dir);
        let local_version = dll::read_local_version(&dll::dll_dir());
        let steam_running = process::is_steam_running();

        let tray = Tray::new(
            crate::tray::load_icon(),
            strings.app_title,
            strings.tray_show,
            strings.tray_quit,
            strings.tray_minimize,
        );

        let mut app = Self {
            lang,
            strings,
            steam_path,
            status,
            steam_running,
            last_steam_check: Instant::now(),
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
            steam_was_running: steam_running,
            autosized: false,
            pending_focus: false,
            minimize_to_tray: true,
            was_minimized: false,
        };
        app.refresh_steam_running();
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

    fn refresh_steam_running(&mut self) {
        let was = self.steam_was_running;
        self.steam_running = process::is_steam_running();
        self.steam_was_running = self.steam_running;
        self.last_steam_check = Instant::now();
        // 边沿检测：Steam 启动 → 自动隐身到托盘；Steam 退出 → 自动弹出。
        if self.steam_running != was {
            // 启动（false→true）→ 隐藏到托盘；退出（true→false）→ 弹出。
            self.set_window_visible(!self.steam_running);
        }
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
    /// 不依赖 `refresh_steam_running` 的 false→true 边沿：Steam 本就运行时边沿不触发。
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

    /// 将更新错误映射为当前语言的提示文案。
    fn update_error_text(&self, e: &UpdateError) -> String {
        match e {
            UpdateError::Network(detail) => format!("{}: {detail}", self.strings.err_network),
            UpdateError::NoZip => self.strings.err_no_zip.to_string(),
            UpdateError::Parse(_) => self.strings.err_parse_version.to_string(),
            UpdateError::NoTargetDll => self.strings.err_no_dlls.to_string(),
            UpdateError::Io(detail) => format!("{}: {detail}", self.strings.err_write_local),
        }
    }

    /// 前置校验错误 → 当前语言的提示文案。
    fn precheck_text(&self, precheck: &workflow::Precheck) -> String {
        match precheck {
            workflow::Precheck::NoSteamDir => self.strings.err_no_steam_dir.to_string(),
            workflow::Precheck::NoTargetDlls => self.strings.err_no_dlls.to_string(),
            workflow::Precheck::NoSteamExe => self.strings.err_steam_exe_missing.to_string(),
        }
    }

    /// 执行阶段错误 → 当前语言的提示文案（按失败步骤取前缀）。
    fn workflow_error_text(&self, e: &workflow::WorkflowError) -> String {
        let prefix = match e.op {
            workflow::Op::CloseSteam => self.strings.err_kill_steam,
            workflow::Op::Deploy => self.strings.err_deploy,
            workflow::Op::Uninstall => self.strings.err_uninstall,
            workflow::Op::Launch => self.strings.err_launch,
        };
        format!("{}: {}", prefix, e.message)
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
                            format!("{}: {}", self.strings.new_version, info.version),
                        )),
                        Err(e) => Some((false, self.update_error_text(e))),
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
                        Err(e) => self.notice = Some((false, self.update_error_text(&e))),
                    }
                }
                Msg::WorkflowDone(action, res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    match res {
                        Ok(()) => {
                            self.refresh_status();
                            self.notice =
                                Some((true, action.success_text(&self.strings).to_string()));
                        }
                        Err(e) => self.notice = Some((false, self.workflow_error_text(&e))),
                    }
                    self.refresh_steam_running();
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
                self.notice = Some((false, self.precheck_text(&precheck)));
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
                if ui.add(ghost_button(self.lang.toggle_label())).clicked() {
                    self.toggle_lang();
                }
            });
        });
        ui.add_space(6.0);
    }

    fn card1(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度，避免堆在左侧
            ui.label(
                egui::RichText::new(self.strings.card1_title)
                    .size(13.5)
                    .strong()
                    .color(TEXT_INK),
            );
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
                if ui
                    .add_sized(
                        egui::vec2(82.0, 34.0),
                        secondary_button(self.strings.browse),
                    )
                    .clicked()
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
            ui.label(
                egui::RichText::new(self.strings.card2_title)
                    .size(13.5)
                    .strong()
                    .color(TEXT_INK),
            );
            ui.add_space(10.0);

            // 部署状态（真实状态：平色圆点 + 词）。
            ui.horizontal(|ui| {
                match self.status {
                    DeployStatus::InvalidPath => {
                        status_badge(ui, ERR_RED, self.strings.status_invalid);
                    }
                    DeployStatus::Deployed => {
                        status_badge(ui, DOT_RUNNING, self.strings.status_deployed);
                    }
                    DeployStatus::NotDeployed => {
                        status_badge(ui, DOT_STOPPED, self.strings.status_not_deployed);
                    }
                }

                if self.status != DeployStatus::InvalidPath {
                    ui.add_space(8.0);
                    if self.steam_running {
                        status_badge(ui, DOT_RUNNING, self.strings.steam_running);
                    } else {
                        status_badge(ui, DOT_STOPPED, self.strings.steam_not_running);
                    }
                }
            });

            ui.add_space(14.0);

            ui.horizontal(|ui| {
                let ctx = ui.ctx().clone();
                match self.status {
                    DeployStatus::Deployed => {
                        // Steam 运行中 →「退出 Steam 并卸载补丁」；已退出 → 直接「卸载补丁」。
                        let uninstall_label = if self.steam_running {
                            self.strings.btn_exit_and_uninstall
                        } else {
                            self.strings.btn_uninstall
                        };
                        if ui
                            .add_enabled(!self.busy, primary_button(uninstall_label))
                            .clicked()
                        {
                            self.request_action(&ctx, Action::ExitAndUninstall);
                        }
                        ui.add_space(4.0);
                        if ui
                            .add_enabled(
                                !self.busy,
                                secondary_button(self.strings.btn_uninstall_and_restart),
                            )
                            .clicked()
                        {
                            self.request_action(&ctx, Action::UninstallAndRestart);
                        }
                    }
                    DeployStatus::NotDeployed => {
                        if ui
                            .add_enabled(
                                !self.busy,
                                primary_button(self.strings.btn_apply_and_launch),
                            )
                            .clicked()
                        {
                            self.request_action(&ctx, Action::ApplyAndLaunch);
                        }
                        ui.add_space(4.0);
                        if ui
                            .add_enabled(
                                !self.busy,
                                secondary_button(self.strings.btn_launch_normal),
                            )
                            .clicked()
                        {
                            self.request_action(&ctx, Action::Launch);
                        }
                    }
                    DeployStatus::InvalidPath => {
                        // 无有效路径时禁用操作按钮。
                        ui.add_enabled(false, primary_button(self.strings.btn_apply_and_launch));
                        ui.add_space(4.0);
                        ui.add_enabled(false, secondary_button(self.strings.btn_launch_normal));
                    }
                }
            });
        });
        ui.add_space(10.0);
    }

    fn card3(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width()); // 卡片撑满窗口宽度
            ui.label(
                egui::RichText::new(self.strings.card3_title)
                    .size(13.5)
                    .strong()
                    .color(TEXT_INK),
            );
            ui.add_space(10.0);

            let local_ver = self
                .local_version
                .as_deref()
                .unwrap_or(self.strings.unknown);
            let online_version: String = match &self.update_state {
                UpdateState::Idle => self.strings.unknown.to_string(),
                UpdateState::Checking => self.strings.checking.to_string(),
                UpdateState::Checked(Ok(info)) => info.version.clone(),
                UpdateState::Checked(Err(_)) => self.strings.unknown.to_string(),
            };

            ui.horizontal(|ui| {
                value_tag(ui, self.strings.local_version, local_ver, false);
                ui.add_space(8.0);
                let is_new = match &self.update_state {
                    UpdateState::Checked(Ok(info)) => {
                        info.version != local_ver && !info.version.is_empty()
                    }
                    _ => false,
                };
                value_tag(ui, self.strings.online_version, &online_version, is_new);
            });

            ui.add_space(14.0);

            let ctx = ui.ctx().clone();
            let mut do_check = false;
            let mut do_download: Option<OnlineInfo> = None;

            // 先只收集按钮意图，避免借用冲突。
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy, secondary_button(self.strings.btn_check_update))
                    .clicked()
                {
                    do_check = true;
                }

                if let UpdateState::Checked(Ok(info)) = &self.update_state {
                    let local = self.local_version.as_deref().unwrap_or("");
                    if info.version != local
                        && !info.version.is_empty()
                        && ui
                            .add_enabled(
                                !self.busy,
                                primary_button(self.strings.btn_download_and_extract),
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

            // 线上状态行：已是最新 / 发现新版本 / 错误（错误保留红，其余中性）。
            if let UpdateState::Checked(res) = &self.update_state {
                ui.add_space(8.0);
                match res {
                    Ok(info) if self.local_version.as_deref() == Some(info.version.as_str()) => {
                        ui.label(
                            egui::RichText::new(format!("●  {}", self.strings.up_to_date))
                                .size(12.0)
                                .color(TEXT_WEAK),
                        );
                    }
                    Ok(_) => {
                        ui.label(
                            egui::RichText::new(format!("●  {}", self.strings.new_version))
                                .size(12.0)
                                .strong()
                                .color(ACCENT),
                        );
                    }
                    Err(e) => {
                        ui.label(
                            egui::RichText::new(format!("●  {}", self.update_error_text(e)))
                                .size(12.0)
                                .color(ERR_RED),
                        );
                    }
                }
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
                    egui::RichText::new(kind.label(&self.strings))
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

        // 定时刷新 Steam 运行状态（缓存 + 事件驱动）。
        if self.last_steam_check.elapsed() >= STEAM_REFRESH_INTERVAL {
            self.refresh_steam_running();
        }

        // 隐藏到托盘/最小化时窗口不可见：用短间隔驱动，托盘事件与最小化检测响应快。
        let repaint_interval = if self.window_visible {
            STEAM_REFRESH_INTERVAL
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
                    if ui.add(primary_button(self.strings.yes)).clicked() {
                        confirmed = true;
                    }
                    ui.add_space(4.0);
                    if ui.add(secondary_button(self.strings.no)).clicked() {
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
}