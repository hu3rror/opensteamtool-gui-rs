//! 双语文案与系统语言检测。

use crate::busy::BusyKind;
use crate::compat::CompatError;
use crate::config::Language;
use crate::updater::UpdateError;
use crate::workflow::{Action, Op, Precheck, WorkflowError};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Zh,
    En,
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
    pub steam_path_label: &'static str,
    pub browse: &'static str,
    /// 部署状态卡片标题（#36：原「本地应用状态 / LOCAL PATCH STATUS」改为词表定名
    /// 「部署状态 / DEPLOY STATUS」；accent bar 形态不变）。
    pub card2_title: &'static str,
    pub status_invalid: &'static str,
    pub status_deployed: &'static str,
    pub status_not_deployed: &'static str,
    pub btn_apply_and_launch: &'static str,
    pub btn_launch_normal: &'static str,
    /// 「启动 Steam」（已应用且 Steam 未运行时；带补丁启动，不言「正常」，见 ADR-0011）。
    pub btn_launch: &'static str,
    pub btn_exit_and_uninstall: &'static str,
    pub btn_uninstall_and_restart: &'static str,
    /// 「重启 Steam」（已应用且 Steam 运行中时显示；纯 Steam 操作，无补丁）。
    pub btn_restart_steam: &'static str,
    /// 补丁未下载引导（操作区下方弱化提示；未部署且 dlls/ 缺文件时显示）。
    /// ADR-0011 修订（#32 → #34 修订）：主页面补丁更新按钮已迁回主页面，引导指向
    /// 操作区上方的「检查补丁更新」按钮（先检查 → 再下载并解压），不自动联网检查。
    pub hint_download_patch: &'static str,
    /// 主页面健康风险警示（#32）：上游尚未适配此版本（点击跳 Settings — Steam）。
    pub main_warning_pending: &'static str,
    /// 主页面健康风险警示（#32）：未找到核心 DLL（点击跳 Settings — Steam）。
    pub main_warning_missing: &'static str,
    pub btn_download_and_extract: &'static str,
    pub checking: &'static str,
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
    /// 兼容性体检本地文件操作失败（CompatError::Io）。
    pub err_compat_io: &'static str,
    pub err_write_local: &'static str,
    /// 「应用补丁并启动」成功反馈（#36：原「已部署补丁 / Patch deployed」改为
    /// 「补丁已应用 / Patch applied」——「已部署」不再作为任何状态词出现）。
    pub ok_deployed: &'static str,
    pub ok_uninstalled: &'static str,
    pub ok_launched: &'static str,
    pub ok_restarted: &'static str,
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
    /// 托盘菜单「重启 Steam」入口。
    pub tray_restart: &'static str,
    pub settings_title: &'static str,
    pub btn_close: &'static str,
    /// Steam 核心兼容性（issue #23 §7.8）。
    pub compat_title: &'static str,
    pub compat_checking: &'static str,
    pub compat_status_ready: &'static str,
    pub compat_status_online: &'static str,
    pub compat_status_offline: &'static str,
    pub compat_status_pending: &'static str,
    pub compat_status_missing: &'static str,
    pub compat_status_network: &'static str,
    pub compat_btn_precache: &'static str,
    pub compat_btn_details: &'static str,
    pub compat_btn_precache_all: &'static str,
    pub compat_precaching: &'static str,
    pub compat_precache_done: &'static str,
    pub compat_precache_failed: &'static str,
    pub compat_tip_pending: &'static str,
    pub compat_tip_ready: &'static str,
    pub compat_tip_online: &'static str,
    pub compat_tip_missing: &'static str,
    pub compat_tip_network: &'static str,
    pub compat_row_dll: &'static str,
    /// 设置对话框页签（#30 落地 General；#31 追加 Steam；#34 重组为 通用/关于/Steam）。
    pub settings_tab_general: &'static str,
    /// 设置对话框页签「关于」（#34 修订：应用更新 + 仓库信息 + 设置向导，取代原「更新」页签）。
    pub settings_tab_about: &'static str,
    /// 设置对话框页签「Steam」（#31；Steam 路径编辑 + 兼容性小节）。
    pub settings_tab_steam: &'static str,
    /// Settings — Steam 路径小节标题（#31）。
    pub settings_steam_title: &'static str,
    /// Settings — Steam 手输路径失焦/回车校验失败的内联错误（#31；非法输入不落盘）。
    pub settings_steam_path_invalid: &'static str,
    /// Settings — 通用 页签语言小节标题。
    pub settings_language_title: &'static str,
    /// Settings — 通用 页签系统托盘小节标题（#37）。
    pub settings_tray_title: &'static str,
    /// Settings — 通用 页签最小化隐身勾选项（#37；与托盘菜单勾选同一事实源）。
    pub settings_tray_minimize: &'static str,
    /// 关于页签软件版本行前缀（实际版本号由 crate 版本拼接）。
    pub settings_version_label: &'static str,
    /// Settings — 关于 页签应用更新小节标题（软件版本 + 应用更新检查）。
    pub settings_app_update_title: &'static str,
    pub settings_btn_app_update_check: &'static str,
    pub settings_app_update_checking: &'static str,
    /// App 更新检查「已是最新」。
    pub settings_app_update_up_to_date: &'static str,
    /// App 更新检查「发现新版本」前缀（后接版本号，如 v0.6.3）。
    pub settings_app_update_new_version: &'static str,
    pub settings_btn_open_download_page: &'static str,
    pub settings_btn_patch_update_check: &'static str,
    /// 补丁检查结果文案（永不出现补丁版本号，见 #30）。
    pub settings_patch_up_to_date: &'static str,
    pub settings_patch_new_version: &'static str,
    /// Settings — 关于 页签重新运行向导小节标题。
    pub settings_wizard_title: &'static str,
    pub settings_btn_rerun_wizard: &'static str,
    /// 重新运行向导提示（以当前配置为初值）。
    pub settings_rerun_wizard_hint: &'static str,
    /// 主页面部署状态卡片辅助行：Steam 正在运行。
    pub status_steam_running: &'static str,
    /// 主页面部署状态卡片辅助行：Steam 未运行。
    pub status_steam_stopped: &'static str,
    /// 顶栏设置齿轮按钮的悬停提示（图标按钮，可发现性靠 tooltip）。
    pub settings_gear_tooltip: &'static str,
    /// 设置对话框页签行右侧 GitHub 入口的项目名文字（点击打开仓库页）。
    pub settings_github_label: &'static str,
    /// 关于页签 GitHub 小节标题（仓库链接入口）。
    pub settings_github_title: &'static str,
    /// 首次运行向导（wizard 模块渲染）。
    pub wizard_title: &'static str,
    /// 步骤指示（`{n}` 由 UI 替换为 1/2/3）。
    pub wizard_step_of: &'static str,
    pub wizard_language_prompt: &'static str,
    pub wizard_language_auto: &'static str,
    pub wizard_language_zh: &'static str,
    pub wizard_language_en: &'static str,
    pub wizard_path_prompt: &'static str,
    pub wizard_path_invalid: &'static str,
    pub wizard_btn_next: &'static str,
    /// 步骤 3 未开始时的提示（下载需用户显式点击，不自动）。
    pub wizard_download_prompt: &'static str,
    /// 步骤 3 就绪态提示（补丁已下载，无需再下载）。
    pub wizard_download_ready: &'static str,
    pub wizard_download_running: &'static str,
    pub wizard_download_failed: &'static str,
    pub wizard_btn_download: &'static str,
    pub wizard_btn_retry: &'static str,
    /// 就绪态收尾按钮（补丁已下载时的「完成」）。
    pub wizard_btn_done: &'static str,
    pub wizard_btn_skip: &'static str,
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
            Precheck::InvalidSteamDir => self.err_no_steam_dir.to_string(),
            Precheck::MissingTargetDlls => self.err_no_dlls.to_string(),
            Precheck::MissingSteamExe => self.err_steam_exe_missing.to_string(),
        }
    }

    /// 兼容性体检错误 → 当前语言提示文案（与 UpdateError 同等待遇；Display 只留给日志）。
    pub fn compat_error_text(&self, e: &CompatError) -> String {
        match e {
            CompatError::Network(detail) => format!("{}: {detail}", self.err_network),
            CompatError::Io(detail) => format!("{}: {detail}", self.err_compat_io),
        }
    }
    /// 「操作」成功后 → 当前语言提示文案。
    pub fn success_text(&self, action: Action) -> &'static str {
        match action {
            Action::ApplyAndLaunch => self.ok_deployed,
            Action::Launch => self.ok_launched,
            Action::ExitAndUninstall | Action::UninstallAndRestart => self.ok_uninstalled,
            Action::Restart => self.ok_restarted,
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

    /// 语言选项表（跟随系统 / 简体中文 / English）：设置对话框通用页签与向导步骤 1
    /// 的唯一下拉数据源——新增语言只在此追加一行（#34）。
    pub fn language_options(&self) -> [(Language, &'static str); 3] {
        [
            (Language::Auto, self.wizard_language_auto),
            (Language::Zh, self.wizard_language_zh),
            (Language::En, self.wizard_language_en),
        ]
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
            steam_path_label: "路径",
            browse: "浏览...",
            card2_title: "部署状态",
            status_invalid: "【未应用】请先指定有效的 Steam 安装路径",
            status_deployed: "【已应用】OpenSteamTool 补丁已成功生效",
            status_not_deployed: "【未应用】检测到补丁文件未完整部署",
            btn_apply_and_launch: "▶ 应用补丁并启动 Steam",
            btn_launch_normal: "▶ 正常启动 Steam",
            btn_launch: "▶ 启动 Steam",
            btn_exit_and_uninstall: "⏏ 退出 Steam 并卸载补丁",
            btn_uninstall_and_restart: "⏏ 卸载补丁并重启 Steam",
            btn_restart_steam: "↻ 重启 Steam",
            hint_download_patch: "补丁未下载：请点击上方「检查补丁更新」下载新版本",
            main_warning_pending: "⚠ Steam 核心兼容性异常：上游尚未适配此版本（点击前往 设置 → Steam）",
            main_warning_missing: "⚠ 未找到核心 DLL（steamclient64.dll / steamui.dll）——点击前往 设置 → Steam",
            btn_download_and_extract: "下载并解压新版本",
            checking: "正在检查更新...",
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
            err_compat_io: "本地文件操作失败",
            err_write_local: "写入本地文件失败",
            ok_deployed: "补丁已应用",
            ok_uninstalled: "已卸载补丁",
            ok_launched: "Steam 已启动",
            ok_restarted: "Steam 已重启",
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
            tray_restart: "重启 Steam",
            settings_title: "设置",
            btn_close: "关闭",
            settings_tab_general: "通用",
            settings_tab_about: "关于",
            settings_tab_steam: "Steam",
            settings_steam_title: "Steam 路径",
            settings_steam_path_invalid: "路径无效：请输入有效的 Steam 安装目录",
            settings_language_title: "语言",
            settings_tray_title: "系统托盘",
            settings_tray_minimize: "最小化时隐藏到系统托盘",
            settings_version_label: "软件版本",
            settings_app_update_title: "应用更新",
            settings_btn_app_update_check: "检查应用更新",
            settings_app_update_checking: "正在检查应用更新...",
            settings_app_update_up_to_date: "已是最新版本",
            settings_app_update_new_version: "发现新版本 ",
            settings_btn_open_download_page: "打开下载页",
            settings_btn_patch_update_check: "检查补丁更新",
            settings_patch_up_to_date: "补丁已是最新",
            settings_patch_new_version: "发现新补丁",
            settings_wizard_title: "设置向导",
            settings_btn_rerun_wizard: "重新运行向导",
            settings_rerun_wizard_hint: "以当前语言与 Steam 路径为初值重新运行设置向导。",
            status_steam_running: "Steam 正在运行",
            status_steam_stopped: "Steam 未运行",
            settings_gear_tooltip: "设置",
            settings_github_label: "opensteamtool-gui-rs",
            settings_github_title: "项目主页",
            compat_title: "Steam 核心兼容性",
            compat_checking: "正在检查兼容性...",
            compat_status_ready: "完美兼容 (已缓存)",
            compat_status_online: "上游已适配 (未缓存)",
            compat_status_offline: "离线可用 (使用本地缓存)",
            compat_status_pending: "上游尚未适配此版本",
            compat_status_missing: "未找到核心 DLL",
            compat_status_network: "网络不可用",
            compat_btn_precache: "预热离线缓存",
            compat_btn_details: "详细信息",
            compat_btn_precache_all: "一键缓存签名",
            compat_precaching: "正在缓存...",
            compat_precache_done: "缓存已就绪",
            compat_precache_failed: "缓存预热失败：{err}",
            compat_tip_pending: "Steam 版本已更新，上游尚未发布适配签名，请等待更新。",
            compat_tip_ready: "核心特征码与 IPC 规约已全部就绪并离线缓存。",
            compat_tip_online: "上游已适配，可一键预热离线缓存。",
            compat_tip_missing: "未找到核心 DLL（steamclient64.dll / steamui.dll）。",
            compat_tip_network: "网络不可用，体检结果未知；已缓存项仍可离线使用。",
            compat_row_dll: "{dll}（{kind}）",
            wizard_title: "首次运行向导",
            wizard_step_of: "第 {n} / 3 步",
            wizard_language_prompt: "请选择界面语言：",
            wizard_language_auto: "跟随系统",
            wizard_language_zh: "简体中文",
            wizard_language_en: "English",
            wizard_path_prompt: "请确认 Steam 安装路径：",
            wizard_path_invalid: "路径无效：请选择有效的 Steam 安装目录",
            wizard_btn_next: "下一步",
            wizard_download_prompt: "补丁尚未下载。点击下方按钮开始下载并解压（可选跳过）。",
            wizard_download_ready: "补丁已下载，无需再下载。",
            wizard_download_running: "正在下载并解压补丁...",
            wizard_download_failed: "补丁下载失败：{err}",
            wizard_btn_download: "下载并解压",
            wizard_btn_retry: "重试",
            wizard_btn_done: "完成",
            wizard_btn_skip: "跳过",
        }
    }

    fn en() -> Self {
        Self {
            app_title: "OpenSteamTool Manager",
            window_title: "OpenSteamTool Manager",
            steam_path_label: "Path",
            browse: "Browse...",
            card2_title: "DEPLOY STATUS",
            status_invalid: "[Not Applied] Please specify a valid Steam path",
            status_deployed: "[Applied] OpenSteamTool patch is now active",
            status_not_deployed: "[Not Applied] Patch files incomplete or missing",
            btn_apply_and_launch: "▶ Apply Patch & Launch Steam",
            btn_launch_normal: "▶ Launch Steam Normally",
            btn_launch: "▶ Launch Steam",
            btn_exit_and_uninstall: "⏏ Exit Steam & Uninstall Patch",
            btn_uninstall_and_restart: "⏏ Uninstall Patch & Restart Steam",
            btn_restart_steam: "↻ Restart Steam",
            hint_download_patch: "Patch not downloaded: click 'Check Patch Update' above to download",
            main_warning_pending: "⚠ Steam core compatibility issue: this version is not yet supported upstream (click to open Settings → Steam)",
            main_warning_missing: "⚠ Core DLLs not found (steamclient64.dll / steamui.dll) — click to open Settings → Steam",
            btn_download_and_extract: "Download & Extract New Version",
            checking: "Checking for updates...",
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
            err_compat_io: "Local file operation failed",
            err_write_local: "Failed to write local files",
            ok_deployed: "Patch applied",
            ok_uninstalled: "Patch removed",
            ok_launched: "Steam launched",
            ok_restarted: "Steam restarted",
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
            tray_restart: "Restart Steam",
            settings_title: "Settings",
            btn_close: "Close",
            settings_tab_general: "General",
            settings_tab_about: "About",
            settings_tab_steam: "Steam",
            settings_steam_title: "Steam Path",
            settings_steam_path_invalid: "Invalid path: enter a valid Steam install folder",
            settings_language_title: "Language",
            settings_tray_title: "System Tray",
            settings_tray_minimize: "Hide to tray when minimized",
            settings_version_label: "Version",
            settings_app_update_title: "App Update",
            settings_btn_app_update_check: "Check App Update",
            settings_app_update_checking: "Checking for app update...",
            settings_app_update_up_to_date: "Up to date",
            settings_app_update_new_version: "New version available: ",
            settings_btn_open_download_page: "Open Download Page",
            settings_btn_patch_update_check: "Check Patch Update",
            settings_patch_up_to_date: "Patch up to date",
            settings_patch_new_version: "New patch available",
            settings_wizard_title: "Setup Wizard",
            settings_btn_rerun_wizard: "Re-run Wizard",
            settings_rerun_wizard_hint: "Re-runs setup with your current language and Steam path as starting values.",
            status_steam_running: "Steam is running",
            status_steam_stopped: "Steam is not running",
            settings_gear_tooltip: "Settings",
            settings_github_label: "opensteamtool-gui-rs",
            settings_github_title: "Project Home",
            compat_title: "Steam Core Compatibility",
            compat_checking: "Checking compatibility...",
            compat_status_ready: "Fully Compatible",
            compat_status_online: "Supported (Not Cached)",
            compat_status_offline: "Compatible (Offline Cache)",
            compat_status_pending: "Unsupported (Pending)",
            compat_status_missing: "DLLs Not Found",
            compat_status_network: "Network Unavailable",
            compat_btn_precache: "Pre-cache Signatures",
            compat_btn_details: "Details",
            compat_btn_precache_all: "Pre-cache All Signatures",
            compat_precaching: "Downloading...",
            compat_precache_done: "Cache pre-warmed",
            compat_precache_failed: "Pre-cache failed: {err}",
            compat_tip_pending: "Steam has been updated. Please wait for upstream signatures.",
            compat_tip_ready: "Core signatures & IPC specs ready and cached offline.",
            compat_tip_online: "Supported upstream — pre-cache now for offline use.",
            compat_tip_missing: "Core DLLs not found (steamclient64.dll / steamui.dll).",
            compat_tip_network: "Network unavailable — results unknown; cached items remain usable offline.",
            compat_row_dll: "{dll} ({kind})",
            wizard_title: "First-run Setup",
            wizard_step_of: "Step {n} of 3",
            wizard_language_prompt: "Choose your interface language:",
            wizard_language_auto: "Auto (follow system)",
            wizard_language_zh: "简体中文",
            wizard_language_en: "English",
            wizard_path_prompt: "Confirm your Steam installation path:",
            wizard_path_invalid: "Invalid path: choose a valid Steam install folder",
            wizard_btn_next: "Next",
            wizard_download_prompt: "The patch is not downloaded yet. Click below to download & extract it (or skip).",
            wizard_download_ready: "The patch is already downloaded.",
            wizard_download_running: "Downloading & extracting patch...",
            wizard_download_failed: "Patch download failed: {err}",
            wizard_btn_download: "Download & Extract",
            wizard_btn_retry: "Retry",
            wizard_btn_done: "Done",
            wizard_btn_skip: "Skip",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 语言选项表（#34，设置通用页签与向导步骤 1 共用）：覆盖全部三个偏好各一次；
    /// 简体中文标签定名（「中文」→「简体中文」）；双语结构一致。
    #[test]
    fn language_options_cover_all_variants() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            let opts = s.language_options();
            for v in [Language::Auto, Language::Zh, Language::En] {
                assert_eq!(
                    opts.iter().filter(|(l, _)| *l == v).count(),
                    1,
                    "{lang:?} 选项表应恰好含 {v:?} 一次"
                );
            }
            assert!(!opts[0].1.is_empty(), "{lang:?} 跟随系统标签不应为空");
            assert_eq!(opts[1].1, "简体中文", "{lang:?} 中文标签应定名简体中文");
            assert_eq!(opts[2].1, "English");
        }
    }

    /// 补丁未下载引导指向主页面补丁更新按钮（#34 修订后入口在主页面）。
    #[test]
    fn patch_hint_points_to_main_button() {
        let zh = Strings::new(Lang::Zh);
        let en = Strings::new(Lang::En);
        assert!(zh.hint_download_patch.contains("检查补丁更新"));
        assert!(!zh.hint_download_patch.contains("通用"));
        assert!(en.hint_download_patch.contains("Check Patch Update"));
        assert!(!en.hint_download_patch.contains("General"));
    }

    /// CompatError → 双语文案：Network 复用 err_network、Io 用 err_compat_io（与 UpdateError 同等待遇）。
    #[test]
    fn compat_error_text_both_langs() {
        for lang in [Lang::Zh, Lang::En] {
            let s = Strings::new(lang);
            assert_eq!(
                s.compat_error_text(&CompatError::Network("x".into())),
                format!("{}: x", s.err_network)
            );
            assert_eq!(
                s.compat_error_text(&CompatError::Io("y".into())),
                format!("{}: y", s.err_compat_io)
            );
        }
    }

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
            assert_eq!(
                s.precheck_text(&Precheck::InvalidSteamDir),
                s.err_no_steam_dir
            );
            assert_eq!(s.precheck_text(&Precheck::MissingTargetDlls), s.err_no_dlls);
            assert_eq!(
                s.precheck_text(&Precheck::MissingSteamExe),
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
            assert_eq!(s.success_text(Action::Restart), s.ok_restarted);
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

    /// #36：卸载类按钮符号 ⏏（启动 ▶ / 重启 ↻）；卡片标题「部署状态 / DEPLOY
    /// STATUS」；应用成功反馈「补丁已应用 / Patch applied」。覆盖携带状态词的 UI
    /// 字段，确保「◀」「已部署」与 "deployed" 不作为任何状态词残留（zh + en）。
    #[test]
    fn ui_copy_follows_v36_terminology() {
        let zh = Strings::new(Lang::Zh);
        let en = Strings::new(Lang::En);
        for (lang, s) in [(Lang::Zh, &zh), (Lang::En, &en)] {
            // 符号按动作定型：卸载类 ⏏；启动 ▶；重启 ↻。
            assert!(s.btn_exit_and_uninstall.starts_with('⏏'));
            assert!(s.btn_uninstall_and_restart.starts_with('⏏'));
            assert!(s.btn_apply_and_launch.starts_with('▶'));
            assert!(s.btn_launch.starts_with('▶'));
            assert!(s.btn_launch_normal.starts_with('▶'));
            assert!(s.btn_restart_steam.starts_with('↻'));
            // 状态词残留检查：可能携带「部署/应用」状态词的字段全量过一遍。
            let statusish = [
                s.card2_title,
                s.status_invalid,
                s.status_deployed,
                s.status_not_deployed,
                s.ok_deployed,
                s.ok_uninstalled,
                s.busy_deploying,
                s.busy_uninstalling,
                s.err_deploy,
                s.err_uninstall,
            ];
            for t in statusish {
                assert!(!t.contains('◀'), "{lang:?} 文案 {t} 残留 ◀");
                assert!(!t.contains("已部署"), "{lang:?} 文案 {t} 残留 已部署");
                assert!(
                    !t.to_lowercase().contains("deployed"),
                    "{lang:?} 文案 {t} 残留 deployed"
                );
            }
        }
        // 卡片标题与成功反馈的精确文案（#36 定稿）。
        assert_eq!(zh.card2_title, "部署状态");
        assert_eq!(en.card2_title, "DEPLOY STATUS");
        assert_eq!(zh.ok_deployed, "补丁已应用");
        assert_eq!(en.ok_deployed, "Patch applied");
    }
}
