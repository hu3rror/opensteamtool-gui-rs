//! egui 界面：顶栏 + 3 卡片 + 确认弹窗。

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use eframe::egui;
use egui::Frame;

use crate::dll::{self, DeployStatus};
use crate::i18n::{Lang, Strings};
use crate::process;
use crate::steam;
use crate::updater::{self, OnlineInfo, UpdateError};

/// Steam 运行状态缓存刷新间隔。
const STEAM_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// 后台线程 → UI 线程的消息。
enum Msg {
    /// 后台阶段变化（如 kill Steam 完成后进入部署阶段）。
    Phase(BusyKind),
    UpdateChecked(Result<OnlineInfo, UpdateError>),
    Downloaded(Result<(), UpdateError>),
    Deployed(Result<(), String>),
    Uninstalled(Result<(), String>),
    Launched(Result<(), String>),
}

/// 用户触发的操作类型。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    /// 应用补丁并启动 Steam。
    ApplyAndLaunch,
    /// 正常启动 Steam。
    Launch,
    /// 退出 Steam 并卸载补丁。
    ExitAndUninstall,
    /// 卸载补丁并重启 Steam。
    UninstallAndRestart,
}


/// 后台操作期间显示的忙碌文案类型。
#[derive(Clone, Copy)]
enum BusyKind {
    Deploying,
    Uninstalling,
    Launching,
    Checking,
    Downloading,
    ClosingSteam,
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
    /// 需要先关闭 Steam 才能执行的操作。
    fn needs_close(self) -> bool {
        matches!(
            self,
            Action::ApplyAndLaunch | Action::ExitAndUninstall | Action::UninstallAndRestart
        )
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
        let (tx, rx) = mpsc::channel();
        let lang = crate::i18n::detect_system_lang();
        let steam_path = steam::detect_steam_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let steam_dir = Path::new(&steam_path);
        let status = dll::check_status(steam_dir);
        let local_version = dll::read_local_version(&dll::dll_dir());
        let steam_running = process::is_steam_running();

        let mut app = Self {
            lang,
            strings: Strings::new(lang),
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
        self.steam_running = process::is_steam_running();
        self.last_steam_check = Instant::now();
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
                Msg::Deployed(res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    match res {
                        Ok(()) => {
                            self.refresh_status();
                            self.notice = Some((true, self.strings.ok_deployed.to_string()));
                        }
                        Err(e) => self.notice = Some((false, e)),
                    }
                    self.refresh_steam_running();
                }
                Msg::Uninstalled(res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    match res {
                        Ok(()) => {
                            self.refresh_status();
                            self.notice = Some((true, self.strings.ok_uninstalled.to_string()));
                        }
                        Err(e) => self.notice = Some((false, e)),
                    }
                    self.refresh_steam_running();
                }
                Msg::Launched(res) => {
                    self.busy = false;
                    self.busy_kind = None;
                    match res {
                        Ok(()) => self.notice = Some((true, self.strings.ok_launched.to_string())),
                        Err(e) => self.notice = Some((false, e)),
                    }
                    self.refresh_steam_running();
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
        let steam_dir = self.steam_path.trim().to_string();
        let steam_path = Path::new(&steam_dir);

        // 前置校验（本地化错误文案），失败则不进入忙碌状态。
        if !steam_path.is_dir() {
            self.confirm = None;
            self.notice = Some((false, self.strings.err_no_steam_dir.to_string()));
            return;
        }
        if action == Action::ApplyAndLaunch {
            let dll_dir = dll::dll_dir();
            if !dll::TARGET_DLLS.iter().all(|d| dll_dir.join(d).is_file()) {
                self.confirm = None;
                self.notice = Some((false, self.strings.err_no_dlls.to_string()));
                return;
            }
        }
        if (action == Action::Launch
            || action == Action::ApplyAndLaunch
            || action == Action::UninstallAndRestart)
            && !steam_path.join("steam.exe").is_file() {
                self.confirm = None;
                self.notice = Some((false, self.strings.err_steam_exe_missing.to_string()));
                return;
            }

        self.busy = true;
        self.busy_kind = Some(match action {
            Action::ApplyAndLaunch => BusyKind::Deploying,
            Action::Launch => BusyKind::Launching,
            Action::ExitAndUninstall | Action::UninstallAndRestart => BusyKind::Uninstalling,
        });
        self.confirm = None;
        let dll_dir = dll::dll_dir();

        match action {
            Action::ApplyAndLaunch => {
                let s = self.strings;
                let tx = self.tx.clone();
                let ctx2 = ctx.clone();
                self.spawn(ctx, move || {
                    let res = (|| {
                        if kill_first {
                            let _ = tx.send(Msg::Phase(BusyKind::ClosingSteam));
                            ctx2.request_repaint();
                            process::kill_steam()
                                .map_err(|e| format!("{}: {e}", s.err_kill_steam))?;
                            let _ = tx.send(Msg::Phase(BusyKind::Deploying));
                            ctx2.request_repaint();
                        }
                        dll::deploy(&dll_dir, Path::new(&steam_dir))
                            .map_err(|e| format!("{}: {e}", s.err_deploy))?;
                        steam::launch_steam(Path::new(&steam_dir))
                            .map_err(|e| format!("{}: {e}", s.err_launch))
                    })();
                    Msg::Deployed(res)
                });
            }
            Action::Launch => {
                self.spawn(ctx, move || Msg::Launched(steam::launch_steam(Path::new(&steam_dir))));
            }
            Action::ExitAndUninstall => {
                let s = self.strings;
                let tx = self.tx.clone();
                let ctx2 = ctx.clone();
                self.spawn(ctx, move || {
                    let res = (|| {
                        if kill_first {
                            let _ = tx.send(Msg::Phase(BusyKind::ClosingSteam));
                            ctx2.request_repaint();
                            process::kill_steam()
                                .map_err(|e| format!("{}: {e}", s.err_kill_steam))?;
                            let _ = tx.send(Msg::Phase(BusyKind::Uninstalling));
                            ctx2.request_repaint();
                        }
                        dll::uninstall(Path::new(&steam_dir))
                            .map_err(|e| format!("{}: {e}", s.err_uninstall))
                    })();
                    Msg::Uninstalled(res)
                });
            }
            Action::UninstallAndRestart => {
                let s = self.strings;
                let tx = self.tx.clone();
                let ctx2 = ctx.clone();
                self.spawn(ctx, move || {
                    let res = (|| {
                        if kill_first {
                            let _ = tx.send(Msg::Phase(BusyKind::ClosingSteam));
                            ctx2.request_repaint();
                            process::kill_steam()
                                .map_err(|e| format!("{}: {e}", s.err_kill_steam))?;
                            let _ = tx.send(Msg::Phase(BusyKind::Uninstalling));
                            ctx2.request_repaint();
                        }
                        dll::uninstall(Path::new(&steam_dir))
                            .map_err(|e| format!("{}: {e}", s.err_uninstall))?;
                        steam::launch_steam(Path::new(&steam_dir))
                            .map_err(|e| format!("{}: {e}", s.err_launch))
                    })();
                    Msg::Uninstalled(res)
                });
            }
        }
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
            ui.heading(self.strings.app_title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(self.lang.toggle_label()).clicked() {
                    self.toggle_lang();
                }
            });
        });
        ui.separator();
    }

    fn card1(&mut self, ui: &mut egui::Ui) {
        Frame::group(ui.style()).show(ui, |ui| {
            ui.strong(self.strings.card1_title);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(self.strings.steam_path_label);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.steam_path)
                        .desired_width(ui.available_width() - 90.0),
                );
                if resp.changed() {
                    self.refresh_status();
                }
                if ui.button(self.strings.browse).clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.steam_path = dir.display().to_string();
                        self.refresh_status();
                    }
            });
        });
        ui.add_space(6.0);
    }

    fn card2(&mut self, ui: &mut egui::Ui) {
        Frame::group(ui.style()).show(ui, |ui| {
            ui.strong(self.strings.card2_title);
            ui.add_space(4.0);

            let status_text = match self.status {
                DeployStatus::InvalidPath => self.strings.status_invalid,
                DeployStatus::Applied => self.strings.status_applied,
                DeployStatus::NotApplied => self.strings.status_not_applied,
            };
            ui.label(status_text);
            ui.add_space(2.0);
            ui.label(if self.steam_running {
                self.strings.steam_running
            } else {
                self.strings.steam_not_running
            });
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let ctx = ui.ctx().clone();
                match self.status {
                    DeployStatus::Applied => {
                        if ui
                            .add_enabled(
                                !self.busy,
                                egui::Button::new(self.strings.btn_exit_and_uninstall),
                            )
                            .clicked()
                        {
                            self.request_action(&ctx, Action::ExitAndUninstall);
                        }
                        if ui
                            .add_enabled(
                                !self.busy,
                                egui::Button::new(self.strings.btn_uninstall_and_restart),
                            )
                            .clicked()
                        {
                            self.request_action(&ctx, Action::UninstallAndRestart);
                        }
                    }
                    DeployStatus::NotApplied => {
                        if ui
                            .add_enabled(
                                !self.busy,
                                egui::Button::new(self.strings.btn_apply_and_launch),
                            )
                            .clicked()
                        {
                            self.request_action(&ctx, Action::ApplyAndLaunch);
                        }
                        if ui
                            .add_enabled(!self.busy, egui::Button::new(self.strings.btn_launch_normal))
                            .clicked()
                        {
                            self.request_action(&ctx, Action::Launch);
                        }
                    }
                    DeployStatus::InvalidPath => {
                        // 无有效路径时禁用操作按钮。
                        ui.add_enabled(false, egui::Button::new(self.strings.btn_apply_and_launch));
                        ui.add_enabled(false, egui::Button::new(self.strings.btn_launch_normal));
                    }
                }
            });
        });
        ui.add_space(6.0);
    }

    fn card3(&mut self, ui: &mut egui::Ui) {
        Frame::group(ui.style()).show(ui, |ui| {
            ui.strong(self.strings.card3_title);
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.strings.local_version,
                    self.local_version
                        .as_deref()
                        .unwrap_or(self.strings.unknown)
                ));
            });

            let online_version: String = match &self.update_state {
                UpdateState::Idle => self.strings.unknown.to_string(),
                UpdateState::Checking => self.strings.checking.to_string(),
                UpdateState::Checked(Ok(info)) => info.version.clone(),
                UpdateState::Checked(Err(_)) => self.strings.unknown.to_string(),
            };
            ui.horizontal(|ui| {
                ui.label(format!("{}: {}", self.strings.online_version, online_version));
            });
            ui.add_space(6.0);

            let ctx = ui.ctx().clone();
            let mut do_check = false;
            let mut do_download: Option<OnlineInfo> = None;

            // 先只收集按钮意图，避免借用冲突。
            {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.busy, egui::Button::new(self.strings.btn_check_update))
                        .clicked()
                    {
                        do_check = true;
                    }

                    if let UpdateState::Checked(Ok(info)) = &self.update_state {
                        let local = self.local_version.as_deref().unwrap_or("");
                        if info.version != local && !info.version.is_empty()
                            && ui
                                .add_enabled(
                                    !self.busy,
                                    egui::Button::new(self.strings.btn_download_and_extract),
                                )
                                .clicked()
                            {
                                do_download = Some(info.clone());
                            }
                    }
                });
            }

            if do_check {
                self.check_update(&ctx);
            }
            if let Some(info) = do_download {
                self.download_update(&ctx, info);
            }

            ui.add_space(2.0);

            // 线上状态行：已是最新 / 发现新版本 / 错误。
            if let UpdateState::Checked(res) = &self.update_state {
                match res {
                    Ok(info) if self.local_version.as_deref() == Some(info.version.as_str()) => {
                        ui.label(self.strings.up_to_date);
                    }
                    Ok(_) => {
                        ui.label(self.strings.new_version);
                    }
                    Err(e) => {
                        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), self.update_error_text(e));
                    }
                }
            }
        });
        ui.add_space(6.0);
    }

    fn notice_bar(&mut self, ui: &mut egui::Ui) {
        if let Some(kind) = self.busy_kind {
            ui.colored_label(
                egui::Color32::from_rgb(120, 160, 220),
                kind.label(&self.strings),
            );
            return;
        }
        if let Some((ok, text)) = &self.notice {
            let color = if *ok {
                egui::Color32::from_rgb(90, 170, 90)
            } else {
                egui::Color32::from_rgb(220, 80, 80)
            };
            ui.colored_label(color, text);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 定时刷新 Steam 运行状态（缓存 + 事件驱动）。
        if self.last_steam_check.elapsed() >= STEAM_REFRESH_INTERVAL {
            self.refresh_steam_running();
        }
        ctx.request_repaint_after(STEAM_REFRESH_INTERVAL);

        self.handle_messages();

        // eframe 0.36：root Ui 无背景色，须用 CentralPanel 填充整个窗口并绘制背景。
        egui::CentralPanel::default().show(ui, |ui| {
            self.top_bar(ui);
            self.card1(ui);
            self.card2(ui);
            self.card3(ui);
            self.notice_bar(ui);
        });

        // 「关闭 Steam 并继续」确认弹窗。
        if let Some(action) = self.confirm {
            let mut confirmed = false;
            let mut cancelled = false;
            egui::Modal::new(egui::Id::new("confirm_close_steam")).show(&ctx, |ui| {
                ui.set_width(320.0);
                ui.heading(self.strings.confirm_title);
                ui.add_space(6.0);
                ui.label(self.strings.confirm_close_steam);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(self.strings.yes).clicked() {
                        confirmed = true;
                    }
                    if ui.button(self.strings.no).clicked() {
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

        let mut fonts = egui::epaint::text::Fonts::new(
            egui::epaint::text::TextOptions::default(),
            defs,
        );
        assert!(fonts.has_glyph(&egui::FontId::proportional(14.0), 'X')); // latin sanity
        assert!(fonts.has_glyph(&egui::FontId::proportional(14.0), '中'));
    }

    /// 未注册任何 CJK 字体时，默认字体确实无中文字形（红/绿判别用）。
    #[test]
    fn default_fonts_lack_cjk_glyph() {
        let defs = egui::FontDefinitions::default();
        let mut fonts = egui::epaint::text::Fonts::new(
            egui::epaint::text::TextOptions::default(),
            defs,
        );
        assert!(!fonts.has_glyph(&egui::FontId::proportional(14.0), '中'));
    }
}

