//! 双语文案与系统语言检测。

use crate::config_editor::ConfigError;
use crate::onlinefix::VdfError;
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
    pub window_title: &'static str,
    pub card1_title: &'static str,
    pub steam_path_label: &'static str,
    pub browse: &'static str,
    pub card2_title: &'static str,
    pub status_invalid: &'static str,
    pub status_deployed: &'static str,
    pub status_not_deployed: &'static str,
    pub btn_apply_and_launch: &'static str,
    pub btn_launch_normal: &'static str,
    pub btn_exit_and_uninstall: &'static str,
    pub btn_uninstall_and_restart: &'static str,
    pub card3_title: &'static str,
    pub local_version: &'static str,
    pub local_ver_ready_no_record: &'static str,
    pub local_ver_missing: &'static str,
    pub online_version: &'static str,
    pub online_check_fail: &'static str,
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
    pub btn_settings: &'static str,
    pub settings_title: &'static str,
    /// 设置对话框「配置编辑器」页签标签（OnlineFix 页签复用 of_title）。
    pub settings_tab_config: &'static str,
    /// 「目标文件：」前缀（设置对话框显示正在编辑的路径）。
    pub settings_target: &'static str,
    pub settings_no_steam_dir: &'static str,
    pub settings_file_missing: &'static str,
    pub btn_load_template: &'static str,
    pub btn_undo: &'static str,
    /// 「从示例模板创建」覆盖确认弹窗文案（仅存在未保存修改时出现）。
    pub confirm_template_overwrite: &'static str,
    pub btn_save: &'static str,
    pub btn_close: &'static str,
    /// 校验错误前缀（行列定位由 config_error_text 拼接）。
    pub err_config_parse: &'static str,
    pub ok_config_saved: &'static str,
    pub err_config_load: &'static str,
    pub err_config_save: &'static str,
    /// OnlineFix 启动预设（PR-2）。
    pub of_title: &'static str,
    pub of_steam_running: &'static str,
    pub of_no_account: &'static str,
    pub of_account_label: &'static str,
    pub of_appid_label: &'static str,
    pub of_status_enabled: &'static str,
    pub of_status_disabled: &'static str,
    pub of_btn_enable: &'static str,
    pub of_btn_disable: &'static str,
    pub of_btn_copy: &'static str,
    pub of_copied: &'static str,
    pub err_of_op: &'static str,
    pub err_of_invalid_appid: &'static str,
    pub of_err_root_chain: &'static str,
    /// 上游限制提示：同一时间仅一个 onlinefix 游戏可运行。
    pub of_single_limit: &'static str,
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

    /// TOML 配置校验错误 → 当前语言提示文案（带行列定位）。
    pub fn config_error_text(&self, lang: Lang, e: &ConfigError) -> String {
        match lang {
            Lang::Zh => format!("{}（第 {} 行，第 {} 列）：{}", self.err_config_parse, e.line, e.col, e.message),
            Lang::En => format!("{} (line {}, column {}): {}", self.err_config_parse, e.line, e.col, e.message),
        }
    }

    /// OnlineFix 写入/读取错误 → 当前语言提示文案。
    pub fn onlinefix_error(&self, e: &VdfError) -> String {
        match e {
            VdfError::Io(detail) => format!("{}: {detail}", self.err_of_op),
            VdfError::Structure(code) => match *code {
                "missing_root_chain" => self.of_err_root_chain.to_string(),
                other => format!("{}: structure {other}", self.err_of_op),
            },
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
            window_title: "OpenSteamTool 一键管理工具",
            card1_title: "STEAM 安装路径",
            steam_path_label: "路径",
            browse: "浏览...",
            card2_title: "本地应用状态",
            status_invalid: "【未应用】请先指定有效的 Steam 安装路径",
            status_deployed: "【已应用】OpenSteamTool 补丁已成功生效",
            status_not_deployed: "【未应用】检测到补丁文件未完整部署",
            btn_apply_and_launch: "▶ 应用补丁并启动 Steam",
            btn_launch_normal: "▶ 正常启动 Steam",
            btn_exit_and_uninstall: "◀ 退出 Steam 并卸载补丁",
            btn_uninstall_and_restart: "◀ 卸载补丁并重启 Steam",
            card3_title: "在线版本更新",
            local_version: "当前本地版本：",
            local_ver_ready_no_record: "已本地就绪 (未记录版本)",
            local_ver_missing: "未下载 (dlls 文件夹缺失文件)",
            online_version: "最新线上版本：",
            online_check_fail: "检查失败",
            btn_check_update: "检查更新",
            btn_download_and_extract: "下载并解压新版本",
            checking: "正在检查更新...",
            up_to_date: "(本地已是最新版)",
            new_version: "(发现可更新版本)",
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
            btn_settings: "设置",
            settings_title: "设置",
            settings_tab_config: "配置编辑器",
            settings_target: "目标文件：",
            settings_no_steam_dir: "请先指定有效的 Steam 安装路径，再编辑配置",
            settings_file_missing: "文件不存在，保存后创建；也可从示例模板开始",
            btn_load_template: "从示例模板创建",
            btn_undo: "撤销",
            confirm_template_overwrite: "从示例模板创建将覆盖当前编辑内容，是否继续？",
            btn_save: "保存",
            btn_close: "关闭",
            err_config_parse: "配置格式错误",
            ok_config_saved: "已保存",
            err_config_load: "读取配置失败",
            err_config_save: "保存失败",
            of_title: "OnlineFix 启动预设",
            of_steam_running: "Steam 正在运行：请先关闭 Steam 再修改启动参数",
            of_no_account: "未找到账号配置（userdata/*/config/localconfig.vdf）",
            of_account_label: "账号：",
            of_appid_label: "游戏 AppID：",
            of_status_enabled: "该游戏已启用 -onlinefix",
            of_status_disabled: "该游戏未启用 -onlinefix",
            of_btn_enable: "启用 OnlineFix",
            of_btn_disable: "停用 OnlineFix",
            of_btn_copy: "复制参数",
            of_copied: "已复制 -onlinefix",
            err_of_op: "OnlineFix 操作失败",
            err_of_invalid_appid: "AppID 无效，请输入数字",
            of_err_root_chain: "localconfig.vdf 结构异常（缺少 UserLocalConfigStore 根块）",
            of_single_limit: "注意：同一时间仅一个 onlinefix 游戏可运行",
        }
    }

    fn en() -> Self {
        Self {
            app_title: "OpenSteamTool Manager",
            window_title: "OpenSteamTool Manager",
            card1_title: "STEAM INSTALLATION PATH",
            steam_path_label: "Path",
            browse: "Browse...",
            card2_title: "LOCAL PATCH STATUS",
            status_invalid: "[Not Applied] Please specify a valid Steam path",
            status_deployed: "[Applied] OpenSteamTool patch is now active",
            status_not_deployed: "[Not Applied] Patch files incomplete or missing",
            btn_apply_and_launch: "▶ Apply Patch & Launch Steam",
            btn_launch_normal: "▶ Launch Steam Normally",
            btn_exit_and_uninstall: "◀ Exit Steam & Uninstall Patch",
            btn_uninstall_and_restart: "◀ Uninstall Patch & Restart Steam",
            card3_title: "ONLINE VERSION & UPDATE",
            local_version: "Current Local Version: ",
            local_ver_ready_no_record: "Ready locally (No version log)",
            local_ver_missing: "Not downloaded (Missing files in 'dlls')",
            online_version: "Latest Online Version: ",
            online_check_fail: "Check failed",
            btn_check_update: "Check Update",
            btn_download_and_extract: "Download & Extract New Version",
            checking: "Checking for updates...",
            up_to_date: "(Up to date)",
            new_version: "(Update available)",
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
            btn_settings: "Settings",
            settings_title: "Settings",
            settings_tab_config: "Config Editor",
            settings_target: "Target file: ",
            settings_no_steam_dir: "Set a valid Steam install path to edit the config",
            settings_file_missing: "File does not exist — save to create it, or start from the example template",
            btn_load_template: "Load Example Template",
            btn_undo: "Undo",
            confirm_template_overwrite: "Loading the example template will overwrite your current edits. Continue?",
            btn_save: "Save",
            btn_close: "Close",
            err_config_parse: "Invalid config",
            ok_config_saved: "Saved",
            err_config_load: "Failed to read config",
            err_config_save: "Save failed",
            of_title: "OnlineFix Launch Preset",
            of_steam_running: "Steam is running — close Steam before changing launch options",
            of_no_account: "No account config found (userdata/*/config/localconfig.vdf)",
            of_account_label: "Account: ",
            of_appid_label: "Game App ID: ",
            of_status_enabled: "-onlinefix enabled for this game",
            of_status_disabled: "-onlinefix not enabled",
            of_btn_enable: "Enable OnlineFix",
            of_btn_disable: "Disable OnlineFix",
            of_btn_copy: "Copy Argument",
            of_copied: "Copied -onlinefix",
            err_of_op: "OnlineFix operation failed",
            err_of_invalid_appid: "Invalid App ID — enter a number",
            of_err_root_chain: "localconfig.vdf is malformed (missing UserLocalConfigStore root)",
            of_single_limit: "Note: only one onlinefix game can run at a time",
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
