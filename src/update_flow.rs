//! 「更新流程」模块：在线更新「检查更新」结果的唯一事实源与派生。
//!
//! 深模块——小接口（`check_started` / `check_done` / `derived`），大实现
//! （检查结果状态 + 「本地版本 vs 线上版本」单一派生）。纯 std，无 IO、
//! 无线程、无 egui、无 i18n 依赖；文案映射仍由 i18n/ui 承担。

use crate::updater::{OnlineInfo, UpdateError};

/// 线上更新检查结果状态（唯一事实源；「更新流程」词条见 CONTEXT.md）。
#[derive(Clone, Debug)]
pub enum FlowState {
    Idle,
    Checking,
    Checked(Result<OnlineInfo, UpdateError>),
}

/// card3 线上版本行文案分类。
#[derive(Clone, Debug)]
pub enum UpdateLine<'a> {
    Unknown,
    Checking,
    UpToDate { version: &'a str },
    NewVersion { version: &'a str },
    CheckFailed(&'a UpdateError),
}

/// 通知栏检查结果文案分类。
#[derive(Clone, Debug)]
pub enum UpdateNotice<'a> {
    UpToDate { version: &'a str },
    NewVersion { version: &'a str },
    CheckFailed(&'a UpdateError),
}

/// 单一派生产物：一次 `derived(local)` 输出行文案 / 通知文案 / 下载可用性三份消费。
#[derive(Clone, Debug)]
pub struct UpdateDerived<'a> {
    pub line: UpdateLine<'a>,
    pub notice: Option<UpdateNotice<'a>>,
    pub download: Option<&'a OnlineInfo>,
}

/// 检查更新流程：持有唯一检查结果，`derived` 派生全部下游消费。
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

    /// 派生：从状态 + 当前本地版本计算行/通知/下载。
    pub fn derived(&self, local_version: Option<&str>) -> UpdateDerived<'_> {
        let local = local_version.unwrap_or("");
        match &self.state {
            FlowState::Idle => UpdateDerived {
                line: UpdateLine::Unknown,
                notice: None,
                download: None,
            },
            FlowState::Checking => UpdateDerived {
                line: UpdateLine::Checking,
                notice: None,
                download: None,
            },
            FlowState::Checked(Ok(info)) => {
                let version = info.version.as_str();
                let up_to_date = local == version;
                UpdateDerived {
                    line: if up_to_date {
                        UpdateLine::UpToDate { version }
                    } else {
                        UpdateLine::NewVersion { version }
                    },
                    notice: Some(if up_to_date {
                        UpdateNotice::UpToDate { version }
                    } else {
                        UpdateNotice::NewVersion { version }
                    }),
                    // 下载按钮：仅「发现可更新版本」且线上版本非空时可用（对齐现状守卫）。
                    download: if up_to_date || version.is_empty() {
                        None
                    } else {
                        Some(info)
                    },
                }
            }
            FlowState::Checked(Err(e)) => UpdateDerived {
                line: UpdateLine::CheckFailed(e),
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

    /// Idle（从未检查）：行「未知」、无通知、无下载。
    #[test]
    fn derived_idle_is_unknown() {
        let flow = UpdateFlow::new();
        let d = flow.derived(None);
        assert!(matches!(d.line, UpdateLine::Unknown));
        assert!(d.notice.is_none());
        assert!(d.download.is_none());
    }

    /// check_started：Idle → Checking；derived 产出「检查中」行分类、无通知、无下载。
    #[test]
    fn check_started_enters_checking() {
        let mut flow = UpdateFlow::new();
        flow.check_started();
        let d = flow.derived(None);
        assert!(matches!(d.line, UpdateLine::Checking));
        assert!(d.notice.is_none());
        assert!(d.download.is_none());
    }

    /// check_done(Ok) 且本地与线上同：行/通知「已是最新」、无下载。
    #[test]
    fn derived_up_to_date_when_local_matches() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(Some("1.4.8"));
        assert!(matches!(d.line, UpdateLine::UpToDate { version: "1.4.8" }));
        assert!(matches!(
            d.notice,
            Some(UpdateNotice::UpToDate { version: "1.4.8" })
        ));
        assert!(d.download.is_none());
    }

    /// check_done(Ok) 且本地与线上异：行/通知「发现可更新版本」、下载携带数据。
    #[test]
    fn derived_new_version_when_local_differs() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(Some("1.4.7"));
        assert!(matches!(
            d.line,
            UpdateLine::NewVersion { version: "1.4.8" }
        ));
        assert!(matches!(
            d.notice,
            Some(UpdateNotice::NewVersion { version: "1.4.8" })
        ));
        let info = d.download.expect("可下载应携带 OnlineInfo");
        assert_eq!(info.version, "1.4.8");
        assert_eq!(info.zip_url, "https://x/z.zip");
    }

    /// 本地无版本记录（None → 空串比较）：按「发现可更新版本」处理且可下载。
    #[test]
    fn derived_new_version_when_local_missing() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        let d = flow.derived(None);
        assert!(matches!(
            d.line,
            UpdateLine::NewVersion { version: "1.4.8" }
        ));
        assert!(d.download.is_some());
    }

    /// 线上版本为空串（异常上游）：行按「发现可更新版本」但下载按钮不出现（现状守卫保留）。
    #[test]
    fn derived_empty_online_version_hides_download() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("")));
        let d = flow.derived(Some("1.4.8"));
        assert!(matches!(d.line, UpdateLine::NewVersion { version: "" }));
        assert!(d.download.is_none(), "空线上版本不应出现下载按钮");
    }

    /// check_done(Err)：行/通知为检查失败分类、无下载（UpdateError 无 PartialEq，分支匹配）。
    #[test]
    fn derived_check_failed_on_error() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Err(UpdateError::Network("t".into())));
        let d = flow.derived(Some("1.4.8"));
        assert!(matches!(
            d.line,
            UpdateLine::CheckFailed(UpdateError::Network(_))
        ));
        assert!(matches!(
            d.notice,
            Some(UpdateNotice::CheckFailed(UpdateError::Network(_)))
        ));
        assert!(d.download.is_none());
    }

    /// 下载成功重派生：local 更新为线上版本后自然落「已是最新」、下载消失（决策 5 显式化）。
    #[test]
    fn derived_after_local_update_becomes_up_to_date() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.8")));
        // 下载前：可下载。
        assert!(flow.derived(Some("1.4.7")).download.is_some());
        // 下载成功后 local 重读为线上版本：重派生落 UpToDate、下载消失。
        let d = flow.derived(Some("1.4.8"));
        assert!(matches!(d.line, UpdateLine::UpToDate { version: "1.4.8" }));
        assert!(d.download.is_none());
    }

    /// 重检覆盖：Checked 后再次 check_started + check_done 新结果，derived 反映新结论。
    #[test]
    fn recheck_overwrites_previous_result() {
        let mut flow = UpdateFlow::new();
        flow.check_done(Ok(online("1.4.7")));
        flow.check_started();
        assert!(matches!(flow.derived(None).line, UpdateLine::Checking));
        flow.check_done(Ok(online("1.5.0")));
        let d = flow.derived(Some("1.4.7"));
        assert!(matches!(
            d.line,
            UpdateLine::NewVersion { version: "1.5.0" }
        ));
        assert_eq!(d.download.expect("新版本应可下载").version, "1.5.0");
    }
}
