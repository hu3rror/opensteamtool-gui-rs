//! 在线更新「检查更新」结果的唯一事实源与派生。

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
}

/// 一次 `derived(local)` 输出通知文案 / 下载可用性两份消费（版本永不渲染）。
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

    pub fn derived(&self, local_version: Option<&str>) -> UpdateDerived<'_> {
        let local = local_version.unwrap_or("");
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
                let files_missing = local_version.is_none();
                let up_to_date = local == version && !files_missing;
                UpdateDerived {
                    notice: Some(if up_to_date {
                        UpdateNotice::UpToDate
                    } else {
                        UpdateNotice::NewVersion
                    }),
                    // 下载按钮：新版本可用（本地 ≠ 线上）或文件缺失时出现；
                    // 文件齐全但线上版本为空串时隐藏（现状守卫：空 tag 无法判定「更新」）。
                    download: if up_to_date || (!files_missing && version.is_empty()) {
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
        }
    }

    #[test]
    fn derived_idle_is_unknown() {
        let flow = UpdateFlow::new();
        let d = flow.derived(None);
        assert!(d.notice.is_none());
        assert!(d.download.is_none());
    }

    #[test]
    fn check_started_enters_checking() {
        let mut flow = UpdateFlow::new();
        flow.check_started();
        let d = flow.derived(None);
        assert!(d.notice.is_none());
        assert!(d.download.is_none());
    }

    #[test]
    fn derived_up_to_date_when_local_matches() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(Some("1.4.8"));
        assert!(matches!(d.notice, Some(UpdateNotice::UpToDate)));
        assert!(d.download.is_none());
    }

    #[test]
    fn derived_new_version_when_local_differs() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(Some("1.4.7"));
        assert!(matches!(d.notice, Some(UpdateNotice::NewVersion)));
        let info = d.download.expect("可下载应携带 OnlineInfo");
        assert_eq!(info.version, "1.4.8");
        assert_eq!(info.zip_url, "https://x/z.zip");
    }

    #[test]
    fn derived_new_version_when_local_missing() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(None);
        assert!(matches!(d.notice, Some(UpdateNotice::NewVersion)));
        assert!(d.download.is_some());
    }

    #[test]
    fn derived_files_missing_with_empty_online_version_still_downloadable() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("")));
        let d = flow.derived(None);
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
        let d = flow.derived(Some("1.4.8"));
        assert!(matches!(d.notice, Some(UpdateNotice::NewVersion)));
        assert!(d.download.is_none(), "空线上版本不应出现下载按钮");
    }

    #[test]
    fn derived_check_failed_on_error() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Err(UpdateError::Network("t".into())));
        let d = flow.derived(Some("1.4.8"));
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
        assert!(flow.derived(Some("1.4.7")).download.is_some());
        let d = flow.derived(Some("1.4.8"));
        assert!(matches!(d.notice, Some(UpdateNotice::UpToDate)));
        assert!(d.download.is_none());
    }

    #[test]
    fn recheck_overwrites_previous_result() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.7")));
        flow.check_started();
        assert!(flow.derived(None).notice.is_none());
        flow.check_done(Ok(online("1.5.0")));
        let d = flow.derived(Some("1.4.7"));
        assert!(matches!(d.notice, Some(UpdateNotice::NewVersion)));
        assert_eq!(d.download.expect("新版本应可下载").version, "1.5.0");
    }
}
