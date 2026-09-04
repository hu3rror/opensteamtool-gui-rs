//! egui 界面：顶栏 + 3 卡片 + 确认弹窗。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use egui::Frame;

use crate::config_editor;
use crate::onlinefix;
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
    // 长文案（如英文 "Download & Extract New Version"）超出固定宽度时会被绘制在
    // 按钮边界外截断；按文本宽度自适应，最小仍为调用方指定的 size。
    let font_id = egui::FontId::proportional(13.0);
    let text_w = ui.painter().layout_no_wrap(text.to_owned(), font_id, style.fg).size().x;
    let size = egui::vec2(size.x.max(text_w + 28.0), size.y);
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

/// 两枚等宽按钮并排时的单按钮宽度：`available` 减去手动 gap 与 egui 自动插入的
/// item_spacing 后再均分。公式漏掉 item_spacing 会导致按钮行实际占宽超过可用宽度，
/// 溢出并把下方依赖 `available_width` 撑满的卡片顶到窗口右缘（历史 bug）。
fn twin_button_width(available: f32, gap: f32, item_spacing: f32) -> f32 {
    ((available - gap - item_spacing) / 2.0).max(150.0)
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
        PyStyle {
            bg: BTN_DEPLOY_BG,
            hover: BTN_DEPLOY_HOVER,
            fg: egui::Color32::WHITE,
            border: None,
        },
        size,
        enabled,
    )
}

/// 白色次按钮（正常启动 Steam）。
fn launch_button(ui: &mut egui::Ui, text: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle {
            bg: CARD_BG,
            hover: BTN_SECONDARY_HOVER,
            fg: TEXT_SUB,
            border: Some(ENTRY_BORDER),
        },
        size,
        enabled,
    )
}

/// 浅蓝描边按钮（退出 Steam 并卸载补丁）。
fn uninstall_exit_button(
    ui: &mut egui::Ui,
    text: &str,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
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
fn uninstall_restart_button(
    ui: &mut egui::Ui,
    text: &str,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle {
            bg: BTN_UNINSTALL_B_BG,
            hover: BTN_UNINSTALL_B_HOVER,
            fg: egui::Color32::WHITE,
            border: None,
        },
        size,
        enabled,
    )
}

/// 主蓝按钮（下载/确认弹窗）。
fn primary_button(
    ui: &mut egui::Ui,
    text: &str,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
    py_button(
        ui,
        text,
        PyStyle {
            bg: ACCENT,
            hover: ACCENT_ACTIVE,
            fg: egui::Color32::WHITE,
            border: None,
        },
        size,
        enabled,
    )
}

/// 次灰按钮（检查更新/浏览/取消）。
fn secondary_button(
    ui: &mut egui::Ui,
    text: &str,
    size: egui::Vec2,
    enabled: bool,
) -> egui::Response {
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
        PyStyle {
            bg: CARD_BG,
            hover: FILL_SECONDARY,
            fg: ACCENT,
            border: Some(ENTRY_BORDER),
        },
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

/// OnlineFix 预设区状态行（选中账号 × AppID 的当前状态）。
enum OnlineFixStatus {
    Enabled,
    Disabled,
    /// 刚点击「复制参数」（短暂显示）。
    Copied,
    Error(String),
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

    /// 设置对话框是否打开（PR-1：TOML 配置编辑器；PR-2 挂载 OnlineFix 预设）。
    settings_open: bool,
    /// 编辑器缓冲：磁盘内容载入后在此编辑，保存前不落盘。
    config_text: String,
    /// 缓冲是否已从磁盘加载（避免每帧重读覆盖用户编辑）。
    config_loaded: bool,
    /// 最近一次校验/写入失败的本地化错误文案（None = 无错误）。
    config_err: Option<String>,
    /// 最近一次保存成功（短暂显示「已保存」，再次编辑即清除）。
    config_saved: bool,

    /// OnlineFix 预设区：可用账号（userdata 扫描，对话框打开时刷新）。
    of_accounts: Vec<PathBuf>,
    of_account_idx: usize,
    /// 手动输入/候选取用的 AppID。
    of_appid: String,
    /// Lua config 扫描的候选 AppID。
    of_candidates: Vec<u32>,
    /// 当前选中 (账号, AppID) 的在线修复状态。
    of_status: Option<OnlineFixStatus>,
    /// 上次计算状态时的 (账号 idx, AppID)，避免每帧重读 VDF。
    of_status_key: Option<(usize, String)>,
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
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            strings.window_title.to_owned(),
        ));
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

        Self {
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
            settings_open: false,
            config_text: String::new(),
            config_loaded: false,
            config_err: None,
            config_saved: false,
            of_accounts: Vec::new(),
            of_account_idx: 0,
            of_appid: String::new(),
            of_candidates: Vec::new(),
            of_status: None,
            of_status_key: None,
        }
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
                    self.notice = Some(Notice::UpdateChecked(res.clone()));
                    self.update_state = UpdateState::Checked(res);
                }
                Msg::Downloaded(res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    self.notice = Some(Notice::Downloaded(res.clone()));
                    if let Ok(()) = res {
                        self.local_version = dll::read_local_version(&dll::dll_dir());
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
                self.notice = Some(Notice::Precheck(precheck));
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
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            self.strings.window_title.to_owned(),
        ));
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

    // ---------- 设置对话框（PR-1：TOML 配置编辑器） ----------

    /// 打开设置：置位并标记缓冲待加载（下次渲染时从磁盘读入）。
    fn open_settings(&mut self) {
        self.settings_open = true;
        self.config_loaded = false;
        self.config_err = None;
        self.config_saved = false;
        // OnlineFix 区：按当前 Steam 路径刷新账号与 AppID 候选。
        let steam_dir = Path::new(self.steam_path.trim());
        self.of_accounts = onlinefix::account_vdf_paths(steam_dir);
        self.of_account_idx = self.of_account_idx.min(self.of_accounts.len().saturating_sub(1));
        self.of_candidates = onlinefix::scan_lua_appids(steam_dir);
        self.of_status = None;
        self.of_status_key = None;
    }

    /// 首次渲染时把磁盘内容载入编辑器缓冲（避免每帧重读覆盖用户编辑）。
    fn ensure_config_loaded(&mut self) {
        if self.config_loaded {
            return;
        }
        self.config_loaded = true;
        let path = config_editor::target_path(Path::new(self.steam_path.trim()));
        match std::fs::read_to_string(&path) {
            Ok(text) => self.config_text = text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 文件不存在：留空缓冲，由 UI 显示「从示例模板创建」引导。
                self.config_text.clear();
            }
            Err(e) => {
                self.config_text.clear();
                self.config_err = Some(format!("{}: {e}", self.strings.err_config_load));
            }
        }
    }

    /// 保存：先校验（错误带行列定位），通过后原子写入。
    fn save_config(&mut self) {
        match config_editor::validate(&self.config_text) {
            Err(e) => {
                self.config_err = Some(self.strings.config_error_text(self.lang, &e));
                self.config_saved = false;
            }
            Ok(()) => {
                let path = config_editor::target_path(Path::new(self.steam_path.trim()));
                match config_editor::write_atomic(&path, &self.config_text) {
                    Ok(()) => {
                        self.config_err = None;
                        self.config_saved = true;
                    }
                    Err(e) => {
                        self.config_err = Some(format!("{}: {e}", self.strings.err_config_save));
                        self.config_saved = false;
                    }
                }
            }
        }
    }

    /// 账号展示名：`userdata/<id>/config/localconfig.vdf` → `<id>`。
    fn of_account_name(vdf: &Path) -> String {
        vdf.parent()
            .and_then(|c| c.parent())
            .and_then(|u| u.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_owned())
            .unwrap_or_else(|| vdf.display().to_string())
    }

    /// 重算 OnlineFix 状态（仅当选中的 (账号, AppID) 变化时读盘）。
    fn refresh_of_status(&mut self) {
        let key = (self.of_account_idx, self.of_appid.trim().to_owned());
        if self.of_status_key.as_ref() == Some(&key) {
            return;
        }
        self.of_status_key = Some(key.clone());
        let Ok(appid) = key.1.parse::<u32>() else {
            return; // 输入未成形：不显示状态。
        };
        let Some(vdf) = self.of_accounts.get(key.0).cloned() else {
            return;
        };
        self.of_status = Some(match onlinefix::is_onlinefix(&vdf, appid) {
            Ok(true) => OnlineFixStatus::Enabled,
            Ok(false) => OnlineFixStatus::Disabled,
            Err(e) => OnlineFixStatus::Error(self.strings.onlinefix_error(&e)),
        });
    }

    /// 启用 OnlineFix（写入 -onlinefix）；成功在界面内直接更新状态，键置空待下帧复读。
    fn of_enable(&mut self) {
        let Some(appid) = self.of_appid.trim().parse::<u32>().ok() else {
            self.of_status = Some(OnlineFixStatus::Error(self.strings.err_of_invalid_appid.to_string()));
            self.of_status_key = None;
            return;
        };
        let Some(vdf) = self.of_accounts.get(self.of_account_idx).cloned() else {
            return;
        };
        match onlinefix::set_onlinefix(&vdf, appid) {
            Ok(()) => {
                self.of_status = Some(OnlineFixStatus::Enabled);
                self.of_status_key = None;
            }
            Err(e) => {
                self.of_status = Some(OnlineFixStatus::Error(self.strings.onlinefix_error(&e)));
                self.of_status_key = None;
            }
        }
    }

    /// 停用 OnlineFix（移除 -onlinefix）。
    fn of_disable(&mut self) {
        let Some(appid) = self.of_appid.trim().parse::<u32>().ok() else {
            self.of_status = Some(OnlineFixStatus::Error(self.strings.err_of_invalid_appid.to_string()));
            self.of_status_key = None;
            return;
        };
        let Some(vdf) = self.of_accounts.get(self.of_account_idx).cloned() else {
            return;
        };
        match onlinefix::clear_onlinefix(&vdf, appid) {
            Ok(()) => {
                self.of_status = Some(OnlineFixStatus::Disabled);
                self.of_status_key = None;
            }
            Err(e) => {
                self.of_status = Some(OnlineFixStatus::Error(self.strings.onlinefix_error(&e)));
                self.of_status_key = None;
            }
        }
    }

    /// 复制 `-onlinefix` 参数到剪贴板。
    fn of_copy(&mut self, ctx: &egui::Context) {
        ctx.copy_text(onlinefix::ONLINEFIX_ARG.to_owned());
        self.of_status = Some(OnlineFixStatus::Copied);
    }

    /// 设置对话框主体（模态；Steam 路径无效时仅提示 + 关闭）。
    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }

        let steam_dir = Path::new(self.steam_path.trim());
        let steam_ok = dll::check_status(steam_dir) != DeployStatus::InvalidPath;
        let target = config_editor::target_path(steam_dir);
        if steam_ok {
            self.ensure_config_loaded();
        }
        let file_exists = steam_ok && target.exists();

        let mut save_clicked = false;
        let mut close_clicked = false;
        let mut template_clicked = false;
        let mut enable_clicked = false;
        let mut disable_clicked = false;
        let mut copy_clicked = false;

        egui::Modal::new(egui::Id::new("settings_dialog")).show(ctx, |ui| {
            // 内容变长后允许纵向滚动，避免超出窗口高度。
            egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
            ui.set_width(560.0);
            ui.heading(self.strings.settings_title);
            ui.add_space(8.0);

            if !steam_ok {
                ui.label(egui::RichText::new(self.strings.settings_no_steam_dir).color(TEXT_SUB));
                ui.add_space(14.0);
                if primary_button(ui, self.strings.btn_close, egui::vec2(80.0, 30.0), true).clicked() {
                    close_clicked = true;
                }
                return;
            }

            status_line(
                ui,
                &format!("{}{}", self.strings.settings_target, target.display()),
                TEXT_WEAK,
            );
            ui.add_space(8.0);

            // 编辑器：等宽字体，滚动区，高度固定。
            let editor = egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    ui.add_sized(
                        egui::vec2(ui.available_width(), 300.0),
                        egui::TextEdit::multiline(&mut self.config_text)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    )
                });
            if editor.inner.changed() {
                self.config_saved = false; // 编辑清除「已保存」提示。
            }
            ui.add_space(6.0);

            // 状态行：错误（红）/ 已保存（绿）/ 文件缺失引导（弱灰）。
            if let Some(err) = &self.config_err {
                status_line(ui, err, ERR_RED);
            } else if self.config_saved {
                status_line(ui, self.strings.ok_config_saved, STATUS_INSTALLED);
            } else if !file_exists {
                status_line(ui, self.strings.settings_file_missing, TEXT_WEAK);
            }
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if secondary_button(ui, self.strings.btn_load_template, egui::vec2(150.0, 30.0), true).clicked()
                {
                    template_clicked = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary_button(ui, self.strings.btn_save, egui::vec2(80.0, 30.0), true).clicked() {
                        save_clicked = true;
                    }
                    ui.add_space(6.0);
                    if secondary_button(ui, self.strings.btn_close, egui::vec2(80.0, 30.0), true).clicked() {
                        close_clicked = true;
                    }
                });
            });

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(egui::RichText::new(self.strings.of_title).strong().color(TEXT_INK));
            ui.add_space(6.0);

            if self.steam_running {
                status_line(ui, self.strings.of_steam_running, TEXT_WEAK);
            } else if self.of_accounts.is_empty() {
                status_line(ui, self.strings.of_no_account, TEXT_WEAK);
            } else {
                // 账号选择。
                ui.horizontal(|ui| {
                    ui.label(self.strings.of_account_label);
                    let label = App::of_account_name(&self.of_accounts[self.of_account_idx]);
                    egui::ComboBox::from_id_salt("of_account")
                        .selected_text(label)
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for (i, vdf) in self.of_accounts.iter().enumerate() {
                                let selected = self.of_account_idx == i;
                                let name = App::of_account_name(vdf);
                                if ui.selectable_label(selected, name).clicked() {
                                    self.of_account_idx = i;
                                    self.of_status_key = None;
                                }
                            }
                        });
                });
                // AppID 输入 + Lua 候选。
                ui.horizontal_wrapped(|ui| {
                    ui.label(self.strings.of_appid_label);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.of_appid)
                            .desired_width(96.0)
                            .hint_text("0"),
                    );
                    if resp.changed() {
                        self.of_status_key = None;
                    }
                    if !self.of_candidates.is_empty() {
                        ui.add_space(8.0);
                        for &id in &self.of_candidates {
                            if ui.small_button(id.to_string()).clicked() {
                                self.of_appid = id.to_string();
                                self.of_status_key = None;
                            }
                        }
                    }
                });
                ui.add_space(6.0);
                // 状态行（先刷新再渲染）。
                self.refresh_of_status();
                if let Some(status) = &self.of_status {
                    match status {
                        OnlineFixStatus::Enabled => {
                            status_line(ui, self.strings.of_status_enabled, STATUS_INSTALLED);
                        }
                        OnlineFixStatus::Disabled => {
                            status_line(ui, self.strings.of_status_disabled, TEXT_WEAK);
                        }
                        OnlineFixStatus::Copied => {
                            status_line(ui, self.strings.of_copied, STATUS_INSTALLED);
                        }
                        OnlineFixStatus::Error(msg) => {
                            status_line(ui, msg, ERR_RED);
                        }
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, self.strings.of_btn_enable, egui::vec2(120.0, 30.0), true).clicked() {
                        enable_clicked = true;
                    }
                    if secondary_button(ui, self.strings.of_btn_disable, egui::vec2(120.0, 30.0), true).clicked() {
                        disable_clicked = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if secondary_button(ui, self.strings.of_btn_copy, egui::vec2(84.0, 30.0), true).clicked() {
                            copy_clicked = true;
                        }
                    });
                });
            }
            });
        });

        if save_clicked {
            self.save_config();
        }
        if template_clicked {
            self.config_text = config_editor::EXAMPLE_TEMPLATE.to_owned();
            self.config_loaded = true;
            self.config_err = None;
            self.config_saved = false;
        }
        if enable_clicked {
            self.of_enable();
        }
        if disable_clicked {
            self.of_disable();
        }
        if copy_clicked {
            self.of_copy(ctx);
        }
        if close_clicked {
            self.settings_open = false;
        }
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
                // 设置按钮（RTL 布局中位于语言按钮左侧；样式与语言按钮一致）。
                if lang_button(ui, self.strings.btn_settings).clicked() {
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
            // 宽度公式必须扣除 egui 自动插入的 item_spacing（见 twin_button_width），
            // 否则按钮行实际占宽溢出，把下方卡片（card3 依赖 available_width 撑满）顶到窗口右缘。
            let w = twin_button_width(ui.available_width(), gap, ui.spacing().item_spacing.x);
            let size = egui::vec2(w, 36.0);
            match self.status {
                DeployStatus::Deployed => {
                    // Steam 运行中 →「退出 Steam 并卸载补丁」；已退出 → 直接「卸载补丁」。
                    let uninstall_label = if self.steam_running {
                        self.strings.btn_exit_and_uninstall
                    } else {
                        self.strings.btn_uninstall
                    };
                    if uninstall_exit_button(ui, uninstall_label, size, !self.busy).clicked() {
                        self.request_action(&ctx, Action::ExitAndUninstall);
                    }
                    ui.add_space(gap);
                    if uninstall_restart_button(
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
                    if deploy_button(ui, self.strings.btn_apply_and_launch, size, !self.busy)
                        .clicked()
                    {
                        self.request_action(&ctx, Action::ApplyAndLaunch);
                    }
                    ui.add_space(gap);
                    if launch_button(ui, self.strings.btn_launch_normal, size, !self.busy).clicked()
                    {
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
                    format!(
                        "{}v{}",
                        self.strings.local_version,
                        v.trim_start_matches('v')
                    ),
                    TEXT_INK,
                ),
                None if all_local_exist => (
                    format!(
                        "{}{}",
                        self.strings.local_version, self.strings.local_ver_ready_no_record
                    ),
                    TEXT_SUB,
                ),
                None => (
                    format!(
                        "{}{}",
                        self.strings.local_version, self.strings.local_ver_missing
                    ),
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

    /// 最近一次结果提示 → 当前语言渲染（切换语言后无需重建 notice，逐帧取当前 strings）。
    fn notice_text(&self) -> Option<(bool, String)> {
        self.notice
            .as_ref()
            .map(|n| render_notice(&self.strings, self.local_version.as_deref(), n))
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
        if let Some((ok, text)) = self.notice_text() {
            let (color, dot_color) = if ok {
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
                    if primary_button(ui, self.strings.yes, egui::vec2(72.0, 30.0), true).clicked()
                    {
                        confirmed = true;
                    }
                    ui.add_space(4.0);
                    if secondary_button(ui, self.strings.no, egui::vec2(72.0, 30.0), true).clicked()
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
                    secondary_button(ui, "检查更新", egui::vec2(96.0, 32.0), true).rect.width();
                let long = primary_button(
                    ui,
                    "Download & Extract New Version",
                    egui::vec2(150.0, 32.0),
                    true,
                )
                .rect
                .width();
                let short_en =
                    secondary_button(ui, "Check Update", egui::vec2(96.0, 32.0), true).rect.width();
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
}
