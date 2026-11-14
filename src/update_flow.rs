//! 在线更新「检查更新」结果的唯一事实源与派生。

use crate::patch_source::PatchSource;
use crate::updater::{OnlineInfo, UpdateError};

/// 线上更新检查结果状态（唯一事实源；「更新流程」词条见 GLOSSARY.md）。
#[derive(Clone, Debug)]
pub enum FlowState {
    Idle,
    Checking,
    Checked(Result<OnlineInfo, UpdateError>),
}

/// **不携带版本号**（#30/#32）：版本比较只在流程内部完成，版本永不渲染。
#[derive(Clone, Debug)]
pub enum UpdateNotice<'a> {
    UpToDate,
    NewVersion,
    CheckFailed(&'a UpdateError),
    /// 客户端构建号与部署快照不一致（两种方向共用一条提示，不阻断下载或部署——ADR-0023）。
    ClientBuildMismatch,
}

#[derive(Clone, Debug, Default)]
pub struct DerivedInput<'a> {
    pub local_version: Option<&'a str>,
    pub current_source: PatchSource,
    pub current_build: Option<&'a str>,
    pub recorded_build: Option<&'a str>,
}

impl<'a> DerivedInput<'a> {
    fn client_build_mismatch(&self) -> bool {
        matches!((self.current_build, self.recorded_build), (Some(current), Some(recorded)) if current != recorded)
    }
}

/// 版本永不渲染：这里只给结论与下载载荷。
#[derive(Clone, Debug)]
pub struct UpdateDerived<'a> {
    pub notice: Option<UpdateNotice<'a>>,
    pub download: Option<&'a OnlineInfo>,
}

pub struct UpdateFlow {
    state: FlowState,
}

impl UpdateFlow {
    pub fn new() -> Self {
        Self {
            state: FlowState::Idle,
        }
    }

    /// 检查开始（门禁放行后调用）：→ Checking（覆盖旧结论）。
    pub fn check_started(&mut self) {
        self.state = FlowState::Checking;
    }

    /// 检查完成：结果只存这一份（单一事实源）。
    pub fn check_done(&mut self, result: Result<OnlineInfo, UpdateError>) {
        self.state = FlowState::Checked(result);
    }

    pub fn derived(&self, input: DerivedInput<'_>) -> UpdateDerived<'_> {
        let local = input.local_version.unwrap_or("");
        match &self.state {
            FlowState::Idle => UpdateDerived {
                notice: None,
                download: None,
            },
            FlowState::Checking => UpdateDerived {
                notice: None,
                download: None,
            },
            FlowState::Checked(Ok(info)) => {
                let version = info.version.as_str();
                // 文件本位（ADR-0011）：`local_version` 为 None ⇔ `dlls/` 目标 DLL 缺失。
                // 「已是最新」只属于文件齐全且版本一致的情形；文件缺失时无论线上版本
                // 是否为空都不能落「已是最新」（误导，见 #26 US 20），下载按钮也须出现
                // ——下载是修复动作，用 `zip_url` 不依赖 version。
                let files_missing = input.local_version.is_none();
                // 换源后旧结果不得落「已是最新」（ADR-0023）：源相同才可比较版本。
                let same_source = info.source == input.current_source;
                let up_to_date = local == version && !files_missing && same_source;
                let mismatch = input.client_build_mismatch();
                UpdateDerived {
                    notice: Some(if mismatch {
                        // 构建号不一致优先于版本结论：两侧并排提示是唯一可执行线索。
                        UpdateNotice::ClientBuildMismatch
                    } else if up_to_date {
                        UpdateNotice::UpToDate
                    } else {
                        UpdateNotice::NewVersion
                    }),
                    // 空 tag 无法判定「更新」，文件齐全时按不可下载处理（现状守卫）。
                    download: if mismatch {
                        Some(info)
                    } else if up_to_date || (!files_missing && version.is_empty()) {
                        None
                    } else {
                        Some(info)
                    },
                }
            }
            FlowState::Checked(Err(e)) => UpdateDerived {
                notice: Some(UpdateNotice::CheckFailed(e)),
                download: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn online(version: &str) -> OnlineInfo {
        OnlineInfo {
            version: version.into(),
            zip_url: "https://x/z.zip".into(),
            source: PatchSource::Upstream,
        }
    }

    fn input<'a>(local: Option<&'a str>) -> DerivedInput<'a> {
        DerivedInput {
            local_version: local,
            current_source: PatchSource::Upstream,
            current_build: None,
            recorded_build: None,
        }
    }

    #[test]
    fn derived_idle_is_unknown() {
        let flow = UpdateFlow::new();
        let d = flow.derived(input(None));
        assert!(d.notice.is_none());
        assert!(d.download.is_none());
    }

    #[test]
    fn check_started_enters_checking() {
        let mut flow = UpdateFlow::new();
        flow.check_started();
        let d = flow.derived(input(None));
        assert!(d.notice.is_none());
        assert!(d.download.is_none());
    }

    #[test]
    fn derived_up_to_date_when_local_matches() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(input(Some("1.4.8")));
        assert!(matches!(d.notice, Some(UpdateNotice::UpToDate)));
        assert!(d.download.is_none());
    }

    #[test]
    fn derived_new_version_when_local_differs() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(input(Some("1.4.7")));
        assert!(matches!(d.notice, Some(UpdateNotice::NewVersion)));
        let info = d.download.expect("可下载应携带 OnlineInfo");
        assert_eq!(info.version, "1.4.8");
        assert_eq!(info.zip_url, "https://x/z.zip");
    }

    #[test]
    fn derived_new_version_when_local_missing() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(input(None));
        assert!(matches!(d.notice, Some(UpdateNotice::NewVersion)));
        assert!(d.download.is_some());
    }

    #[test]
    fn derived_files_missing_with_empty_online_version_still_downloadable() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("")));
        let d = flow.derived(input(None));
        assert!(
            !matches!(d.notice, Some(UpdateNotice::UpToDate)),
            "文件缺失时不得显示「已是最新」"
        );
        assert!(
            d.download.is_some(),
            "文件缺失时应可下载（修复动作不依赖空 tag）"
        );
    }

    #[test]
    fn derived_empty_online_version_hides_download() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("")));
        let d = flow.derived(input(Some("1.4.8")));
        assert!(matches!(d.notice, Some(UpdateNotice::NewVersion)));
        assert!(d.download.is_none(), "空线上版本不应出现下载按钮");
    }

    #[test]
    fn derived_check_failed_on_error() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Err(UpdateError::Network("t".into())));
        let d = flow.derived(input(Some("1.4.8")));
        assert!(matches!(
            d.notice,
            Some(UpdateNotice::CheckFailed(UpdateError::Network(_)))
        ));
        assert!(d.download.is_none());
    }

    #[test]
    fn derived_after_local_update_becomes_up_to_date() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        assert!(flow.derived(input(Some("1.4.7"))).download.is_some());
        let d = flow.derived(input(Some("1.4.8")));
        assert!(matches!(d.notice, Some(UpdateNotice::UpToDate)));
        assert!(d.download.is_none());
    }

    #[test]
    fn recheck_overwrites_previous_result() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.7")));
        flow.check_started();
        assert!(flow.derived(input(None)).notice.is_none());
        flow.check_done(Ok(online("1.5.0")));
        let d = flow.derived(input(Some("1.4.7")));
        assert!(matches!(d.notice, Some(UpdateNotice::NewVersion)));
        assert_eq!(d.download.expect("新版本应可下载").version, "1.5.0");
    }

    #[test]
    fn stale_result_from_other_source_never_up_to_date() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let mut other = input(Some("1.4.8"));
        other.current_source = PatchSource::Custom {
            url: "https://github.com/o/r".into(),
            layout: crate::patch_source::Layout::OpenSteamTool,
        };
        let d = flow.derived(other);
        assert!(
            matches!(d.notice, Some(UpdateNotice::NewVersion)),
            "换源后旧结论不得落「已是最新」"
        );
        assert!(d.download.is_some(), "换源应仍可下载新源补丁");
    }

    #[test]
    fn client_build_mismatch_wins_over_version_verdict() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let mut i = input(Some("1.4.8"));
        i.current_build = Some("1769731672");
        i.recorded_build = Some("1700000000");
        let d = flow.derived(i);
        assert!(matches!(d.notice, Some(UpdateNotice::ClientBuildMismatch)));
        assert!(d.download.is_some(), "不一致时「更新 DLL」应可用");
    }

    #[test]
    fn client_build_match_or_missing_keeps_version_verdict() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));

        let mut same = input(Some("1.4.8"));
        same.current_build = Some("1769731672");
        same.recorded_build = Some("1769731672");
        let d = flow.derived(same);
        assert!(matches!(d.notice, Some(UpdateNotice::UpToDate)));
        assert!(d.download.is_none());

        let mut unknown = input(Some("1.4.8"));
        unknown.current_build = Some("1769731672");
        let d = flow.derived(unknown);
        assert!(
            matches!(d.notice, Some(UpdateNotice::UpToDate)),
            "任一侧缺失时应静默不提示不一致"
        );
    }
}
