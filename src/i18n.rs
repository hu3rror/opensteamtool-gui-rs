//! 双语文案与系统语言检测。

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
    pub status_applied: &'static str,
    pub status_not_applied: &'static str,
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
}

impl Strings {
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
            status_applied: "已应用",
            status_not_applied: "未应用",
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
            status_applied: "Applied",
            status_not_applied: "Not Applied",
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
        }
    }
}
