//! egui 界面：顶栏 + 3 卡片 + 确认弹窗。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::egui;
use egui::Frame;

use crate::compat;
use crate::config_editor;
use crate::onlinefix;
use crate::dll::{self, DeployStatus};
use crate::i18n::{Lang, Strings};
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

/// 设置对话框非滚动行的固定高度占用（标题+页签行+顶部固定行+底部固定行+页脚+窗口边距）。
/// 数值保守偏大：低估会让页脚越界（Modal 是 Area 不约束屏幕），过估只浪费一点滚动区。
const CFG_FIXED_H: f32 = 300.0; // 配置编辑器页（实测固定行 193 + 安全量）
const OF_FIXED_H: f32 = 340.0; // OnlineFix 页（实测固定行 234 + 安全量）
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
    /// 语言切换按钮（白底蓝字，固定小尺寸）。
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
    /// 配置编辑器（默认）。
    Config,
    /// OnlineFix 启动预设。
    OnlineFix,
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
        compat::ProbeStatus::RemoteAvailable { cached: false } => {
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
pub struct App {
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
        // 首次体检用的路径快照（Self 构造后闭包 move，避免与字段借用冲突）。
        let probe_path = steam_path.clone();
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
        );

        let app = Self {
            lang,
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
            window_visible: true,
            autosized: false,
            pending_focus: false,
            minimize_to_tray: true,
            was_minimized: false,
            settings_open: false,
            cfg: ConfigEditorState::new(),
            of: OnlineFixState::new(),
            settings_tab: SettingsTab::Config,
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
                }
                Msg::CompatRefreshed(report) => {
                    // 网络刷新结果覆盖短路态；不再次触发刷新（防止 quick→refresh 循环）。
                    let done = self.compat.precache_done;
                    self.compat = CompatUiState::ready(report);
                    self.compat.precache_done = done;
                }
                Msg::CompatPrecached(res) => {
                    self.compat.precaching = false;
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
                        Err(e) => self.compat.precache_error = Some(e),
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

    /// 打开设置：置位并标记缓冲待加载（首次进入「配置编辑器」页签时读盘）。
    fn open_settings(&mut self) {
        self.settings_open = true;
        // 配置编辑器：标记待载入（切到该页签首帧读盘）。
        self.cfg.mark_unloaded();
        // OnlineFix 区：按当前 Steam 路径刷新账号与 AppID 候选（进程组运行态写入门闩每次写前实时判定）。
        let steam_dir = Path::new(self.steam_path.trim());
        self.of.refresh(steam_dir);
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
        // 窗口内高：中间滚动区高度 = 窗口内高 − 固定行占用，总高永不越界（Modal 是 Area，不约束屏幕）。
        let win_h = ctx
            .input(|i| i.viewport().inner_rect)
            .map_or(520.0, |r| r.height());

        let mut save_clicked = false;
        let mut close_clicked = false;
        let mut template_clicked = false;
        let mut template_confirm = false;
        let mut undo_clicked = false;
        let mut enable_clicked = false;
        let mut disable_clicked = false;
        let mut copy_clicked = false;
        let mut tab_clicked = None;

        egui::Modal::new(egui::Id::new("settings_dialog")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading(self.strings.settings_title);
            ui.add_space(6.0);

            // 页签行：配置编辑器 / OnlineFix 预设（egui 0.36 无内置 TabView，selectable_label 手写）。
            ui.horizontal(|ui| {
                let selected = self.settings_tab == SettingsTab::Config;
                if ui
                    .selectable_label(selected, egui::RichText::new(self.strings.settings_tab_config).strong())
                    .clicked()
                {
                    tab_clicked = Some(SettingsTab::Config);
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
                            close_clicked = true;
                        }
                    });
                });
                return;
            }

            match self.settings_tab {
                SettingsTab::Config => {
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
                    let h_mid = (win_h - CFG_FIXED_H).max(120.0);
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
                    ui.horizontal(|ui| {
                        if styled_button(ui, self.strings.btn_load_template, ButtonStyle::Secondary, egui::vec2(150.0, 30.0), true).clicked()
                        {
                            if self.cfg.dirty {
                                template_confirm = true; // 有未保存修改：先确认再覆盖。
                            } else {
                                template_clicked = true;
                            }
                        }
                        ui.add_space(6.0);
                        if styled_button(ui, self.strings.btn_undo, ButtonStyle::Secondary, egui::vec2(64.0, 30.0), true).clicked() {
                            undo_clicked = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if styled_button(ui, self.strings.btn_save, ButtonStyle::Primary, egui::vec2(80.0, 30.0), true).clicked() {
                                save_clicked = true;
                            }
                        });
                    });
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
                            }
                        });
                        ui.add_space(6.0);

                        // 中间滚动：AppID 输入 + Lua 候选。
                        let h_mid = (win_h - OF_FIXED_H).max(120.0);
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
                                    let mut picked = None;
                                    for &id in &self.of.candidates {
                                        if ui.small_button(id.to_string()).clicked() {
                                            picked = Some(id);
                                        }
                                    }
                                    if let Some(id) = picked {
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
                        ui.horizontal(|ui| {
                            if styled_button(ui, self.strings.of_btn_enable, ButtonStyle::Primary, egui::vec2(120.0, 30.0), true).clicked() {
                                enable_clicked = true;
                            }
                            if styled_button(ui, self.strings.of_btn_disable, ButtonStyle::Secondary, egui::vec2(120.0, 30.0), true).clicked() {
                                disable_clicked = true;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if styled_button(ui, self.strings.of_btn_copy, ButtonStyle::Secondary, egui::vec2(84.0, 30.0), true).clicked() {
                                    copy_clicked = true;
                                }
                            });
                        });
                        ui.add_space(6.0);
                        // 上游限制提示（spec PR-2）：同一时间仅一个 onlinefix 游戏可运行。
                        status_line(ui, self.strings.of_single_limit, TEXT_WEAK);
                    }
                }
            }

            // 页脚：共享「关闭」（始终可见）。
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if styled_button(ui, self.strings.btn_close, ButtonStyle::Secondary, egui::vec2(80.0, 30.0), true).clicked() {
                        close_clicked = true;
                    }
                });
            });
        });

        // 「从示例模板创建」覆盖确认（顶置模态；是 → 载入模板，否 → 取消）。
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
                template_clicked = true;
            }
        }

        if let Some(tab) = tab_clicked {
            self.settings_tab = tab; // 会话内记忆上次页签。
        }
        if undo_clicked {
            self.undo_pending = true; // 下一帧合成 Ctrl+Z。
        }
        if save_clicked {
            self.cfg.save(&target);
        }
        if template_clicked {
            self.cfg.fill_template();
        }
        if enable_clicked {
            self.of.enable(steam_dir, self.steam_running, &self.steam_state);
        }
        if disable_clicked {
            self.of.disable(steam_dir, self.steam_running, &self.steam_state);
        }
        if copy_clicked {
            ctx.copy_text(onlinefix::ONLINEFIX_ARG.to_owned());
            self.of.mark_copied();
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
                if styled_button(ui, self.lang.toggle_label(), ButtonStyle::Lang, egui::vec2(56.0, 26.0), true).clicked() {
                    self.toggle_lang();
                }
                // 设置按钮（RTL 布局中位于语言按钮左侧；样式与语言按钮一致）。
                if styled_button(ui, self.strings.btn_settings, ButtonStyle::Lang, egui::vec2(56.0, 26.0), true).clicked() {
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

    /// 一键预热：后台线程逐个下载未缓存签名，完成后触发体检刷新（SPEC.md §7.7）。
    fn start_precache_all(&mut self, ctx: &egui::Context) {
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
                        self.start_precache_all(&ctx);
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
        egui::Frame::new()
            .fill(bg)
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(10, 3))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(format!("{icon} {text}")).size(12.5).color(fg));
            });
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
                self.start_precache_all(&ctx);
            }
        }
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
                    if styled_button(ui, uninstall_label, ButtonStyle::UninstallExit, size, !self.busy).clicked() {
                        self.request_action(&ctx, Action::ExitAndUninstall);
                    }
                    ui.add_space(gap);
                    if styled_button(
                        ui,
                        self.strings.btn_uninstall_and_restart,
                        ButtonStyle::UninstallRestart,
                        size,
                        !self.busy,
                    )
                    .clicked()
                    {
                        self.request_action(&ctx, Action::UninstallAndRestart);
                    }
                }
                DeployStatus::NotDeployed => {
                    if styled_button(ui, self.strings.btn_apply_and_launch, ButtonStyle::Deploy, size, !self.busy)
                        .clicked()
                    {
                        self.request_action(&ctx, Action::ApplyAndLaunch);
                    }
                    ui.add_space(gap);
                    if styled_button(ui, self.strings.btn_launch_normal, ButtonStyle::Launch, size, !self.busy).clicked()
                    {
                        self.request_action(&ctx, Action::Launch);
                    }
                }
                DeployStatus::InvalidPath => {
                    // 无有效路径时禁用操作按钮。
                    styled_button(ui, self.strings.btn_apply_and_launch, ButtonStyle::Deploy, size, false);
                    ui.add_space(gap);
                    styled_button(ui, self.strings.btn_launch_normal, ButtonStyle::Launch, size, false);
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
                if styled_button(
                    ui,
                    self.strings.btn_check_update,
                    ButtonStyle::Secondary,
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
                        && styled_button(
                            ui,
                            self.strings.btn_download_and_extract,
                            ButtonStyle::Primary,
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

    /// 待预热目标：仅收集 RemoteAvailable{cached:false} 且带哈希的项。
    #[test]
    fn precache_targets_picks_uncached_available() {
        let r = report_with(
            [
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: true },
                S::IncompatiblePending,
            ],
            true,
        );
        let targets = precache_targets(&r);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, compat::ProbeTarget::PatternSteamClient);
        assert_eq!(targets[0].1, "abc");
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
}
