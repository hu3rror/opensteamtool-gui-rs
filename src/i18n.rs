//! 双语文案与系统语言检测。

use crate::updater::UpdateError;
use crate::workflow::{Action, BusyKind, Op, Precheck, WorkflowError};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn toggle(self) -> Self {
        match self {
            Lang::Zh => Lang::En,
            Lang::En => Lang::Zh,
        }
    }

    /// 手动切换按钮上的文案：中文界面显示 "EN"，英文界面显示 "中文"。
    pub fn toggle_label(self) -> &'static str {
        match self {
            Lang::Zh => "EN",
            Lang::En => "中文",
        }
    }
}

/// 检测系统语言：`GetUserDefaultUILanguage` 返回 0x0804/0x1004（简体中文）→ 中文，否则英文。
pub fn detect_system_lang() -> Lang {
    #[cfg(windows)]
    {
        // SAFETY: 无指针参数的 Win32 API，无额外安全约束。
        let lang_id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        if lang_id == 0x0804 || lang_id == 0x1004 {
            return Lang::Zh;
        }
    }
    Lang::En
}

/// 全部界面文案，按语言取值。
#[derive(Clone, Copy)]
pub struct Strings {
    pub app_title: &'static str,
    pub card1_title: &'static str,
    pub steam_path_label: &'static str,
    pub browse: &'static str,
    pub card2_title: &'static str,
    pub status_invalid: &'static str,
    pub status_deployed: &'static str,
    pub status_not_deployed: &'static str,
    pub steam_running: &'static str,
    pub steam_not_running: &'static str,
    pub btn_apply_and_launch: &'static str,
    pub btn_launch_normal: &'static str,
    pub btn_exit_and_uninstall: &'static str,
    pub btn_uninstall_and_restart: &'static str,
    pub card3_title: &'static str,
    pub local_version: &'static str,
    pub online_version: &'static str,
    pub btn_check_update: &'static str,
    pub btn_download_and_extract: &'static str,
    pub checking: &'static str,
    pub up_to_date: &'static str,
    pub new_version: &'static str,
    pub unknown: &'static str,
    pub confirm_title: &'static str,
    pub confirm_close_steam: &'static str,
    pub yes: &'static str,
    pub no: &'static str,
    pub err_no_steam_dir: &'static str,
    pub err_no_dlls: &'static str,
    pub err_steam_exe_missing: &'static str,
    pub err_kill_steam: &'static str,
    pub err_deploy: &'static str,
    pub err_uninstall: &'static str,
    pub err_launch: &'static str,
    pub err_network: &'static str,
    pub err_no_zip: &'static str,
    pub err_parse_version: &'static str,
    pub err_write_local: &'static str,
    pub ok_deployed: &'static str,
    pub ok_uninstalled: &'static str,
    pub ok_launched: &'static str,
    pub ok_downloaded: &'static str,
    pub busy_deploying: &'static str,
    pub busy_uninstalling: &'static str,
    pub busy_launching: &'static str,
    pub busy_downloading: &'static str,
    pub busy_killing: &'static str,
    pub tray_show: &'static str,
    pub tray_quit: &'static str,
    /// Steam 未运行时「卸载补丁」按钮（无需先退出 Steam）。
    pub btn_uninstall: &'static str,
    /// 托盘菜单「最小化时自动隐藏到托盘」勾选项。
    pub tray_minimize: &'static str,
}

impl Strings {
    /// 在线更新错误 → 当前语言提示文案。
    pub fn update_error(&self, e: &UpdateError) -> String {
        match e {
            UpdateError::Network(detail) => format!("{}: {detail}", self.err_network),
            UpdateError::NoZip => self.err_no_zip.to_string(),
            UpdateError::Parse(_) => self.err_parse_version.to_string(),
            UpdateError::NoTargetDll => self.err_no_dlls.to_string(),
            UpdateError::Io(detail) => format!("{}: {detail}", self.err_write_local),
        }
    }

    /// 「操作」执行阶段错误 → 当前语言提示文案（按失败步骤取前缀）。
    pub fn workflow_error_text(&self, e: &WorkflowError) -> String {
        let prefix = match e.op {
            Op::CloseSteam => self.err_kill_steam,
            Op::Deploy => self.err_deploy,
            Op::Uninstall => self.err_uninstall,
            Op::Launch => self.err_launch,
        };
        format!("{}: {}", prefix, e.message)
    }

    /// 前置校验错误 → 当前语言提示文案。
    pub fn precheck_text(&self, precheck: &Precheck) -> String {
        match precheck {
            Precheck::NoSteamDir => self.err_no_steam_dir.to_string(),
            Precheck::NoTargetDlls => self.err_no_dlls.to_string(),
            Precheck::NoSteamExe => self.err_steam_exe_missing.to_string(),
        }
    }

    /// 「操作」成功后 → 当前语言提示文案。
    pub fn success_text(&self, action: Action) -> &'static str {
        match action {
            Action::ApplyAndLaunch => self.ok_deployed,
            Action::Launch => self.ok_launched,
            Action::ExitAndUninstall | Action::UninstallAndRestart => self.ok_uninstalled,
        }
    }

    /// 忙碌态阶段 → 当前语言提示文案。
    pub fn busy_label(&self, kind: BusyKind) -> &'static str {
        match kind {
            BusyKind::Deploying => self.busy_deploying,
            BusyKind::Uninstalling => self.busy_uninstalling,
            BusyKind::Launching => self.busy_launching,
            BusyKind::Checking => self.checking,
            BusyKind::Downloading => self.busy_downloading,
            BusyKind::ClosingSteam => self.busy_killing,
        }
    }

    pub fn new(lang: Lang) -> Self {
        match lang {
            Lang::Zh => Self::zh(),
            Lang::En => Self::en(),
        }
    }

    fn zh() -> Self {
        Self {
            app_title: "OpenSteamTool Manager",
            card1_title: "Steam 安装路径",
            steam_path_label: "路径",
            browse: "浏览...",
            card2_title: "本地部署状态",
            status_invalid: "请先指定有效的 Steam 安装路径",
            status_deployed: "已部署",
            status_not_deployed: "未部署",
            steam_running: "Steam 正在运行",
            steam_not_running: "Steam 未运行",
            btn_apply_and_launch: "应用补丁并启动 Steam",
            btn_launch_normal: "正常启动 Steam",
            btn_exit_and_uninstall: "退出 Steam 并卸载补丁",
            btn_uninstall_and_restart: "卸载补丁并重启 Steam",
            card3_title: "在线版本更新",
            local_version: "本地版本",
            online_version: "线上版本",
            btn_check_update: "检查更新",
            btn_download_and_extract: "下载并解压新版本",
            checking: "检查中...",
            up_to_date: "已是最新版本",
            new_version: "发现新版本",
            unknown: "未知",
            confirm_title: "确认",
            confirm_close_steam: "Steam 正在运行。是否自动关闭 Steam 并继续？",
            yes: "是",
            no: "否",
            err_no_steam_dir: "请先指定有效的 Steam 安装路径",
            err_no_dlls: "dlls/ 目录缺少目标 DLL 文件",
            err_steam_exe_missing: "未找到 steam.exe",
            err_kill_steam: "关闭 Steam 失败",
            err_deploy: "部署失败",
            err_uninstall: "卸载失败",
            err_launch: "启动 Steam 失败",
            err_network: "网络请求失败",
            err_no_zip: "发布包中没有 .zip 资产",
            err_parse_version: "解析线上版本失败",
            err_write_local: "写入本地文件失败",
            ok_deployed: "已部署补丁",
            ok_uninstalled: "已卸载补丁",
            ok_launched: "Steam 已启动",
            ok_downloaded: "新版本下载并解压完成",
            busy_deploying: "正在部署...",
            busy_uninstalling: "正在卸载...",
            busy_launching: "正在启动...",
            busy_downloading: "正在下载...",
            busy_killing: "正在关闭 Steam...",
            tray_show: "显示",
            tray_quit: "退出",
            btn_uninstall: "卸载补丁",
            tray_minimize: "最小化时自动隐藏到托盘",
        }
    }

    fn en() -> Self {
        Self {
            app_title: "OpenSteamTool Manager",
            card1_title: "Steam Install Path",
            steam_path_label: "Path",
            browse: "Browse...",
            card2_title: "Deployment Status",
            status_invalid: "Please specify a valid Steam install path",
            status_deployed: "Deployed",
            status_not_deployed: "Not Deployed",
            steam_running: "Steam is running",
            steam_not_running: "Steam is not running",
            btn_apply_and_launch: "Apply Patch & Launch Steam",
            btn_launch_normal: "Launch Steam",
            btn_exit_and_uninstall: "Exit Steam & Remove Patch",
            btn_uninstall_and_restart: "Remove Patch & Restart Steam",
            card3_title: "Online Update",
            local_version: "Local Version",
            online_version: "Online Version",
            btn_check_update: "Check for Updates",
            btn_download_and_extract: "Download & Extract New Version",
            checking: "Checking...",
            up_to_date: "Up to date",
            new_version: "New version available",
            unknown: "Unknown",
            confirm_title: "Confirm",
            confirm_close_steam: "Steam is running. Close Steam automatically and continue?",
            yes: "Yes",
            no: "No",
            err_no_steam_dir: "Please specify a valid Steam install path",
            err_no_dlls: "Target DLL files missing in dlls/",
            err_steam_exe_missing: "steam.exe not found",
            err_kill_steam: "Failed to close Steam",
            err_deploy: "Deploy failed",
            err_uninstall: "Uninstall failed",
            err_launch: "Failed to launch Steam",
            err_network: "Network request failed",
            err_no_zip: "No .zip asset in the release",
            err_parse_version: "Failed to parse online version",
            err_write_local: "Failed to write local files",
            ok_deployed: "Patch deployed",
            ok_uninstalled: "Patch removed",
            ok_launched: "Steam launched",
            ok_downloaded: "New version downloaded & extracted",
            busy_deploying: "Deploying...",
            busy_uninstalling: "Uninstalling...",
            busy_launching: "Launching...",
            busy_downloading: "Downloading...",
            busy_killing: "Closing Steam...",
            tray_show: "Show",
            tray_quit: "Quit",
            btn_uninstall: "Remove Patch",
            tray_minimize: "Minimize to tray automatically",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_error_maps_both_langs() {
        let zh = Strings::new(Lang::Zh);
        let en = Strings::new(Lang::En);
        for s in [&zh, &en] {
            assert_eq!(
                s.update_error(&UpdateError::Network("t".into())),
                format!("{}: t", s.err_network)
            );
            assert_eq!(s.update_error(&UpdateError::NoZip), s.err_no_zip);
            assert_eq!(
                s.update_error(&UpdateError::Parse("p".into())),
                s.err_parse_version
            );
            assert_eq!(s.update_error(&UpdateError::NoTargetDll), s.err_no_dlls);
            assert_eq!(
                s.update_error(&UpdateError::Io("i".into())),
                format!("{}: i", s.err_write_local)
            );
        }
    }

    #[test]
    fn workflow_error_text_prefixes_by_op() {
        let s = Strings::new(Lang::En);
        let cases = [
            (Op::CloseSteam, s.err_kill_steam),
            (Op::Deploy, s.err_deploy),
            (Op::Uninstall, s.err_uninstall),
            (Op::Launch, s.err_launch),
        ];
        for (op, prefix) in cases {
            let e = WorkflowError {
                op,
                message: "m".into(),
            };
            assert_eq!(s.workflow_error_text(&e), format!("{prefix}: m"));
        }
    }

    #[test]
    fn precheck_text_maps_both_langs() {
        let zh = Strings::new(Lang::Zh);
        let en = Strings::new(Lang::En);
        for s in [&zh, &en] {
            assert_eq!(s.precheck_text(&Precheck::NoSteamDir), s.err_no_steam_dir);
            assert_eq!(s.precheck_text(&Precheck::NoTargetDlls), s.err_no_dlls);
            assert_eq!(
                s.precheck_text(&Precheck::NoSteamExe),
                s.err_steam_exe_missing
            );
        }
    }

    #[test]
    fn success_text_maps_by_action() {
        let zh = Strings::new(Lang::Zh);
        let en = Strings::new(Lang::En);
        for s in [&zh, &en] {
            assert_eq!(s.success_text(Action::ApplyAndLaunch), s.ok_deployed);
            assert_eq!(s.success_text(Action::Launch), s.ok_launched);
            assert_eq!(s.success_text(Action::ExitAndUninstall), s.ok_uninstalled);
            assert_eq!(
                s.success_text(Action::UninstallAndRestart),
                s.ok_uninstalled
            );
        }
    }

    #[test]
    fn busy_label_maps_by_kind() {
        let zh = Strings::new(Lang::Zh);
        let en = Strings::new(Lang::En);
        for s in [&zh, &en] {
            assert_eq!(s.busy_label(BusyKind::Deploying), s.busy_deploying);
            assert_eq!(s.busy_label(BusyKind::Uninstalling), s.busy_uninstalling);
            assert_eq!(s.busy_label(BusyKind::Launching), s.busy_launching);
            assert_eq!(s.busy_label(BusyKind::Checking), s.checking);
            assert_eq!(s.busy_label(BusyKind::Downloading), s.busy_downloading);
            assert_eq!(s.busy_label(BusyKind::ClosingSteam), s.busy_killing);
        }
    }
}
