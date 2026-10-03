//! 「首次运行向导」状态机：首次运行（配置缺失或已存 Steam 路径无效）的三步引导。
//! 纯状态机：无 IO、无线程、无 egui、无 i18n；触发判据与终局语义见 ADR-0012。

use std::path::{Path, PathBuf};

use crate::config::{self, Language};
use crate::dll;
use crate::updater::UpdateError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Language,
    SteamPath,
    Download,
}

/// 步骤 3 的下载子状态：进入停在 `Idle`，由用户点击「下载并解压」才开始（不自动下载，#29）；失败可重试，跳过/关窗不阻塞完成。
#[derive(Clone, Debug)]
pub enum DownloadState {
    Idle,
    /// 补丁已下载（注入 `dll_dir` 下三目标 DLL 齐全，ADR-0011 文件本位判据）：
    /// 步骤 3 呈现「就绪」无需下载；点「完成」（复用跳过事件）即收敛终局。
    Ready,
    Running,
    Failed(UpdateError),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// 步骤 1 选定语言：立即本地化后续步骤，但**不推进**（#34 修订：选完语言需点「下一步」才进步骤 2，下拉选择不再直达）。
    LanguageChosen(Language),
    LanguageSubmitted,
    PathEdited(String),
    /// 步骤 2 提交路径：有效则推进到步骤 3；无效则停留。
    PathSubmitted,
    DownloadRequested,
    DownloadDone(Result<(), UpdateError>),
    /// 显式跳过下载（完成向导，但不视为下载成功）。
    SkipDownload,
    /// 关闭窗口：等同跳过并进入主界面。
    Closed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Download,
    Finish {
        language: Language,
        steam_path: String,
    },
}

#[derive(Clone, Debug)]
pub struct View {
    pub step: Step,
    pub language: Language,
    pub steam_path: String,
    pub path_valid: bool,
    pub download: DownloadState,
}

pub struct Wizard {
    step: Step,
    language: Language,
    steam_path: String,
    /// 步骤 3 就绪判据的文件系统注入点（App 传 `dll::dll_dir()`）。
    dll_dir: PathBuf,
    download: DownloadState,
    finished: bool,
}

impl Wizard {
    /// `dll_dir` 是步骤 3 就绪判据的文件系统注入点（App 传 `dll::dll_dir()`，测试用临时目录）。重跑向导时以现值播种，使重跑是「编辑」而非「重置」。
    pub fn new(language: Language, steam_path: String, dll_dir: PathBuf) -> Self {
        Self {
            step: Step::Language,
            language,
            steam_path,
            dll_dir,
            download: DownloadState::Idle,
            finished: false,
        }
    }

    pub fn step(&mut self, event: Event) -> (View, Vec<Effect>) {
        if self.finished {
            return (self.view(), Vec::new());
        }
        let effects = match event {
            Event::LanguageChosen(language) if self.step == Step::Language => {
                // 只应用语言（立即本地化后续步骤文案），不推进——由
                self.language = language;
                Vec::new()
            }
            Event::LanguageSubmitted if self.step == Step::Language => {
                self.step = Step::SteamPath;
                Vec::new()
            }
            Event::PathEdited(path) if self.step == Step::SteamPath => {
                self.steam_path = path;
                Vec::new()
            }
            Event::PathSubmitted if self.step == Step::SteamPath => {
                if dll::is_valid_steam_dir(&self.steam_path) {
                    // 只推进到步骤 3，不自动下载：由用户点击「下载并解压」显式开始
                    self.step = Step::Download;
                    // 补丁是否已下载是文件系统事实（ADR-0011 文件本位判据）：已下载
                    self.download = if dll::target_dlls_present(&self.dll_dir) {
                        DownloadState::Ready
                    } else {
                        DownloadState::Idle
                    };
                    Vec::new()
                } else {
                    Vec::new() // 无效路径停留步骤 2（关窗仍可逃生）。
                }
            }
            Event::DownloadRequested
                if self.step == Step::Download
                    && !matches!(self.download, DownloadState::Running) =>
            {
                self.download = DownloadState::Running;
                vec![Effect::Download]
            }
            Event::DownloadDone(result)
                if self.step == Step::Download
                    && matches!(self.download, DownloadState::Running) =>
            {
                match result {
                    // 成功即终局：不经「成功态」中间态（终局由 `finished` 表达）。
                    Ok(()) => self.finish(),
                    Err(e) => {
                        self.download = DownloadState::Failed(e);
                        Vec::new()
                    }
                }
            }
            Event::SkipDownload if self.step == Step::Download => self.finish(),
            // 关窗从任意步骤都等同跳过（永不把用户锁在向导里）。
            Event::Closed => self.finish(),
            // 当前步骤不接受的事件：忽略（迟到/错序消息不改状态）。
            _ => Vec::new(),
        };
        (self.view(), effects)
    }

    pub fn view(&self) -> View {
        View {
            step: self.step,
            language: self.language,
            steam_path: self.steam_path.clone(),
            path_valid: dll::is_valid_steam_dir(&self.steam_path),
            download: self.download.clone(),
        }
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    fn finish(&mut self) -> Vec<Effect> {
        self.finished = true;
        vec![Effect::Finish {
            language: self.language,
            steam_path: self.steam_path.clone(),
        }]
    }
}

pub fn should_show(config_path: &Path) -> bool {
    match config::load(config_path) {
        Ok(config) => !dll::is_valid_steam_dir(&config.steam_path),
        // 损坏/版本不符 → 启动已降级默认值（ADR-0012），等同未配置。
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CONFIG_FILE, CONFIG_VERSION, Config, ThemePreference};

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_wiz_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_config(dir: &Path, steam_path: &str) {
        let cfg = Config {
            version: CONFIG_VERSION,
            steam_path: steam_path.into(),
            language: Language::Auto,
            minimize_to_tray: true,
            theme: ThemePreference::System,
        };
        config::save(&dir.join(CONFIG_FILE), &cfg).unwrap();
    }

    #[test]
    fn trigger_missing_vs_valid_path() {
        let dir = tmp_dir("trigger");
        assert!(should_show(&dir.join(CONFIG_FILE)), "缺文件应显示向导");

        let steam = dir.join("Steam");
        std::fs::create_dir_all(&steam).unwrap();
        write_config(&dir, &steam.display().to_string());
        assert!(!should_show(&dir.join(CONFIG_FILE)), "有效路径不应显示向导");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn trigger_empty_or_nonexistent_path() {
        let dir = tmp_dir("trigger_bad");
        write_config(&dir, "");
        assert!(should_show(&dir.join(CONFIG_FILE)), "空路径应显示向导");
        write_config(&dir, "Z:/definitely/not/a/real/dir_7f3a");
        assert!(
            should_show(&dir.join(CONFIG_FILE)),
            "不存在的目录应显示向导"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn trigger_malformed_or_unsupported_shows() {
        let dir = tmp_dir("trigger_bad_file");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(&path, "version = 1\nsteam_path = ").unwrap();
        assert!(should_show(&path), "损坏配置应显示向导");
        std::fs::write(
            &path,
            "version = 2\nsteam_path = \"C:/S\"\nlanguage = \"zh\"",
        )
        .unwrap();
        assert!(should_show(&path), "版本不符应显示向导");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn valid_path_requires_nonempty_existing_dir() {
        let dir = tmp_dir("valid_path");
        assert!(!dll::is_valid_steam_dir(""));
        assert!(!dll::is_valid_steam_dir("   "));
        assert!(!dll::is_valid_steam_dir("Z:/nope_12345"));
        assert!(dll::is_valid_steam_dir(&dir.display().to_string()));
        assert!(dll::is_valid_steam_dir(&format!("  {}  ", dir.display())));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn step_progression_to_completion() {
        let steam = tmp_dir("progress");
        let mut w = Wizard::new(Language::Auto, String::new(), tmp_dir("progress_dlls"));

        let (v, fx) = w.step(Event::LanguageChosen(Language::Zh));
        assert_eq!(v.step, Step::Language, "选语言后不推进，等「下一步」");
        assert_eq!(
            v.language,
            Language::Zh,
            "步骤 1 选择立即生效（本地化后续文案）"
        );
        assert!(fx.is_empty());
        assert!(!w.finished());

        let (v, fx) = w.step(Event::LanguageSubmitted);
        assert_eq!(v.step, Step::SteamPath, "点「下一步」才推进");
        assert!(fx.is_empty());

        let (v, fx) = w.step(Event::PathEdited(steam.display().to_string()));
        assert_eq!(v.step, Step::SteamPath);
        assert!(v.path_valid);
        assert!(fx.is_empty());

        let (v, fx) = w.step(Event::PathSubmitted);
        assert_eq!(v.step, Step::Download);
        assert!(
            matches!(v.download, DownloadState::Idle),
            "进入步骤 3 不自动下载，等待用户显式点击"
        );
        assert!(fx.is_empty(), "进入步骤 3 不应产出下载效果");

        let (v, fx) = w.step(Event::DownloadRequested);
        assert!(matches!(v.download, DownloadState::Running));
        assert_eq!(fx, vec![Effect::Download]);

        let (_, fx) = w.step(Event::DownloadDone(Ok(())));
        assert_eq!(
            fx,
            vec![Effect::Finish {
                language: Language::Zh,
                steam_path: steam.display().to_string()
            }]
        );
        assert!(w.finished());

        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn invalid_path_submit_stays() {
        let steam = tmp_dir("invalid_submit");
        let mut w = Wizard::new(Language::En, String::new(), tmp_dir("invalid_submit_dlls"));
        w.step(Event::LanguageChosen(Language::En));
        w.step(Event::LanguageSubmitted);

        let (v, fx) = w.step(Event::PathEdited("Z:/nope_98765".into()));
        assert!(!v.path_valid);
        let (v, fx2) = w.step(Event::PathSubmitted);
        assert_eq!(v.step, Step::SteamPath, "无效路径不得推进");
        assert!(fx2.is_empty());

        w.step(Event::PathEdited(steam.display().to_string()));
        let (v, fx3) = w.step(Event::PathSubmitted);
        assert_eq!(v.step, Step::Download);
        assert!(fx3.is_empty(), "有效提交也不自动下载");
        assert!(matches!(v.download, DownloadState::Idle));
        let _ = fx; // 无效提交本身不产出效果。

        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn download_failure_then_retry_succeeds() {
        let steam = tmp_dir("retry");
        let mut w = entered_download(&steam, Language::Zh);

        let (v, fx) = w.step(Event::DownloadDone(Err(UpdateError::Network("x".into()))));
        assert_eq!(v.step, Step::Download);
        assert!(matches!(v.download, DownloadState::Failed(_)));
        assert!(fx.is_empty(), "失败不结束向导");
        assert!(!w.finished());

        let (v, fx) = w.step(Event::DownloadRequested);
        assert!(matches!(v.download, DownloadState::Running));
        assert_eq!(fx, vec![Effect::Download]);

        let (_, fx) = w.step(Event::DownloadDone(Ok(())));
        assert!(w.finished());
        assert_eq!(fx.len(), 1);
        assert!(matches!(fx[0], Effect::Finish { .. }));

        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn download_failure_then_skip_finishes() {
        let steam = tmp_dir("skip");
        let mut w = entered_download(&steam, Language::En);
        w.step(Event::DownloadDone(Err(UpdateError::NoZip)));
        assert!(!w.finished());

        let (_, fx) = w.step(Event::SkipDownload);
        assert!(w.finished());
        assert_eq!(
            fx,
            vec![Effect::Finish {
                language: Language::En,
                steam_path: steam.display().to_string()
            }]
        );
        std::fs::remove_dir_all(&steam).ok();
    }
    #[test]
    fn skip_while_running_finishes() {
        let steam = tmp_dir("skip_running");
        let mut w = entered_download(&steam, Language::En);
        assert!(!w.finished());
        let (v, fx) = w.step(Event::SkipDownload);
        assert!(
            matches!(v.download, DownloadState::Running),
            "跳过不改下载子状态"
        );
        assert!(w.finished());
        assert_eq!(fx.len(), 1);
        assert!(matches!(fx[0], Effect::Finish { .. }));
        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn skip_and_close_converge_on_same_end_state() {
        let steam = tmp_dir("converge");
        let mut skipped = entered_download(&steam, Language::Zh);
        skipped.step(Event::DownloadDone(Err(UpdateError::Network("x".into()))));
        let mut closed = entered_download(&steam, Language::Zh);
        closed.step(Event::DownloadDone(Err(UpdateError::Network("x".into()))));

        let (sv, sfx) = skipped.step(Event::SkipDownload);
        let (cv, cfx) = closed.step(Event::Closed);

        assert!(
            skipped.finished() && closed.finished(),
            "两种终局都应结束并落到主界面"
        );
        assert_eq!(sfx, cfx, "跳过与关窗终局效果应逐字段相等");
        assert_eq!(cv.step, Step::Download);
        assert_eq!(cv.steam_path, sv.steam_path);
        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn close_from_language_step_finishes() {
        let mut w = Wizard::new(
            Language::Auto,
            "C:/prefilled".into(),
            tmp_dir("close_lang_dlls"),
        );
        let (_, fx) = w.step(Event::Closed);
        assert!(w.finished());
        assert_eq!(
            fx,
            vec![Effect::Finish {
                language: Language::Auto,
                steam_path: "C:/prefilled".into()
            }]
        );
    }

    #[test]
    fn events_after_finish_are_ignored() {
        let mut w = Wizard::new(Language::Zh, String::new(), tmp_dir("after_finish_dlls"));
        w.step(Event::Closed);
        let (_, fx) = w.step(Event::DownloadDone(Ok(())));
        assert!(fx.is_empty());
        let (v, fx) = w.step(Event::LanguageChosen(Language::En));
        assert!(fx.is_empty());
        assert!(w.finished());
        assert_eq!(v.language, Language::Zh, "终局后语言不再被改写");
    }

    #[test]
    fn language_choice_can_change_without_advancing() {
        let mut w = Wizard::new(Language::Auto, String::new(), tmp_dir("lang_change_dlls"));
        let (v, _) = w.step(Event::LanguageChosen(Language::En));
        assert_eq!(v.step, Step::Language, "改语言不推进");
        assert_eq!(v.language, Language::En);
        let (v, _) = w.step(Event::LanguageChosen(Language::Zh));
        assert_eq!(v.step, Step::Language, "再次改语言仍不推进");
        assert_eq!(v.language, Language::Zh);
        let (v, fx) = w.step(Event::PathEdited("C:/x".into()));
        assert_eq!(v.step, Step::Language);
        assert!(fx.is_empty());
        let (v, _) = w.step(Event::LanguageSubmitted);
        assert_eq!(v.step, Step::SteamPath);
    }

    #[test]
    fn download_request_dedupes_while_running() {
        let steam = tmp_dir("dedupe");
        let mut w = entered_download(&steam, Language::En);
        let (_, fx) = w.step(Event::DownloadRequested);
        assert!(fx.is_empty(), "已在途不应重复发起下载");
        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn idle_request_starts_once() {
        let steam = tmp_dir("idle");
        let mut w = Wizard::new(Language::En, String::new(), tmp_dir("idle_dlls"));
        w.step(Event::LanguageChosen(Language::En));
        w.step(Event::LanguageSubmitted);
        w.step(Event::PathEdited(steam.display().to_string()));
        w.step(Event::PathSubmitted);

        let (v, fx) = w.step(Event::DownloadRequested);
        assert!(matches!(v.download, DownloadState::Running));
        assert_eq!(fx, vec![Effect::Download], "Idle 首次点击产出下载效果");
        let (_, fx) = w.step(Event::DownloadRequested);
        assert!(fx.is_empty());
        std::fs::remove_dir_all(&steam).ok();
    }

    fn entered_download(steam: &Path, language: Language) -> Wizard {
        let dlls = std::env::temp_dir().join(format!(
            "ost_wiz_{}_dlls_{}",
            steam.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dlls);
        std::fs::create_dir_all(&dlls).unwrap();
        let mut w = Wizard::new(language, String::new(), dlls);
        w.step(Event::LanguageChosen(language));
        w.step(Event::LanguageSubmitted);
        w.step(Event::PathEdited(steam.display().to_string()));
        w.step(Event::PathSubmitted);
        w.step(Event::DownloadRequested);
        w
    }

    #[test]
    fn patch_present_enters_ready_state() {
        let steam = tmp_dir("ready_steam");
        let dlls = tmp_dir("ready_dlls");
        for name in dll::TARGET_DLLS {
            std::fs::write(dlls.join(name), b"x").unwrap();
        }

        let mut w = Wizard::new(Language::Zh, steam.display().to_string(), dlls.clone());
        w.step(Event::LanguageChosen(Language::Zh));
        w.step(Event::LanguageSubmitted);
        w.step(Event::PathEdited(steam.display().to_string()));
        let (v, fx) = w.step(Event::PathSubmitted);
        assert_eq!(v.step, Step::Download);
        assert!(
            matches!(v.download, DownloadState::Ready),
            "补丁已下载时应呈现就绪态，而非提示「尚未下载」的 Idle"
        );
        assert!(fx.is_empty(), "就绪不产出下载效果");

        let (_, fx) = w.step(Event::SkipDownload);
        assert!(w.finished());
        assert_eq!(
            fx,
            vec![Effect::Finish {
                language: Language::Zh,
                steam_path: steam.display().to_string()
            }]
        );

        std::fs::remove_dir_all(&steam).ok();
        std::fs::remove_dir_all(&dlls).ok();
    }

    #[test]
    fn patch_absent_keeps_idle_in_all_missing_shapes() {
        let steam = tmp_dir("absent_steam");
        let missing =
            std::env::temp_dir().join(format!("ost_wiz_absent_missing_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);
        let empty = tmp_dir("absent_empty");
        let partial = tmp_dir("absent_partial");
        std::fs::write(partial.join(dll::TARGET_DLLS[0]), b"x").unwrap();
        std::fs::write(partial.join(dll::TARGET_DLLS[1]), b"x").unwrap();
        let version_only = tmp_dir("absent_version");
        std::fs::write(version_only.join(dll::VERSION_FILE), b"1.4.8").unwrap();

        for dlls in [&missing, &empty, &partial, &version_only] {
            let mut w = Wizard::new(Language::En, steam.display().to_string(), dlls.clone());
            w.step(Event::LanguageChosen(Language::En));
            w.step(Event::LanguageSubmitted);
            w.step(Event::PathEdited(steam.display().to_string()));
            let (v, _) = w.step(Event::PathSubmitted);
            assert_eq!(v.step, Step::Download);
            assert!(
                matches!(v.download, DownloadState::Idle),
                "补丁未下载（{dlls:?}）时应停留在 Idle 等待显式下载"
            );
        }

        std::fs::remove_dir_all(&steam).ok();
        std::fs::remove_dir_all(&empty).ok();
        std::fs::remove_dir_all(&partial).ok();
        std::fs::remove_dir_all(&version_only).ok();
    }
}
