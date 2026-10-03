//! 「体检流程」状态机：兼容性体检生命周期的唯一编排者。
//! 纯状态机：无 IO、无线程、无 egui、无 i18n；状态转移全部可经 `step` 单测。

use crate::compat::{self, CompatError, OverallHealthReport, ProbeTarget};

/// 体检代数：路径变更推进；预热完成后的复检不推进（同一体检会话）。
/// 所有完成事件携带发起时代数，与当前代数不符 → 丢弃（陈旧防护）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Epoch(pub u64);

/// 预热模式：自动（体检落定 Online 自动触发，失败静默）/ 手动（按钮触发，失败就地报错）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrecacheMode {
    Auto,
    Manual,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// Steam 路径变更（含启动首次）：推进代数；与上次路径相同 → 无动作（防抖）。
    PathChanged(String),
    ProbeDone {
        epoch: Epoch,
        report: OverallHealthReport,
    },
    RefreshDone {
        epoch: Epoch,
        report: OverallHealthReport,
    },
    PrecacheDone {
        epoch: Epoch,
        result: Result<(), CompatError>,
    },
    PrecacheRequested,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Probe {
        epoch: Epoch,
        path: String,
    },
    Refresh {
        epoch: Epoch,
        path: String,
    },
    Precache {
        epoch: Epoch,
        path: String,
        targets: Vec<(ProbeTarget, String)>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompatSummary {
    Checking,
    Ready,
    Online,
    Pending,
    Missing,
    Network,
}

#[derive(Clone, Debug)]
pub struct Display<'a> {
    pub report: Option<&'a OverallHealthReport>,
    pub checking: bool,
    pub precaching: bool,
    /// 预热失败错误（手动失败就地显示；自动失败静默）。
    pub precache_error: Option<&'a CompatError>,
    pub precache_done: bool,
    pub summary: CompatSummary,
}

pub struct CompatFlow {
    epoch: Epoch,
    path: Option<String>,
    report: Option<OverallHealthReport>,
    checking: bool,
    /// 本代数是否已消耗网络刷新预算（每代数限一次）。
    network_refreshed: bool,
    precaching: bool,
    precaching_auto: bool,
    precache_error: Option<CompatError>,
    precache_done: bool,
}

impl CompatFlow {
    pub fn new() -> Self {
        Self {
            epoch: Epoch(0),
            path: None,
            report: None,
            checking: false,
            network_refreshed: false,
            precaching: false,
            precaching_auto: false,
            precache_error: None,
            precache_done: false,
        }
    }

    pub fn step(&mut self, event: Event) -> (Display<'_>, Vec<Effect>) {
        let effects = match event {
            Event::PathChanged(path) => self.on_path_changed(path),
            Event::ProbeDone { epoch, report } => self.on_probe_done(epoch, report),
            Event::RefreshDone { epoch, report } => self.on_refresh_done(epoch, report),
            Event::PrecacheDone { epoch, result } => self.on_precache_done(epoch, result),
            Event::PrecacheRequested => self.on_precache_requested(),
        };
        (self.display(), effects)
    }

    fn on_path_changed(&mut self, path: String) -> Vec<Effect> {
        // 防抖：与上次体检路径相同 → 无动作（防逐字符起线程）。
        if self.path.as_deref() == Some(path.as_str()) {
            return Vec::new();
        }
        // 路径变更推进代数：新会话重置全部状态（在途旧结果靠代数丢弃）。
        self.epoch.0 += 1;
        self.path = Some(path.clone());
        self.report = None;
        self.checking = true;
        self.network_refreshed = false;
        self.precaching = false;
        self.precaching_auto = false;
        self.precache_error = None;
        self.precache_done = false;
        vec![Effect::Probe {
            epoch: self.epoch,
            path,
        }]
    }

    fn on_probe_done(&mut self, epoch: Epoch, report: OverallHealthReport) -> Vec<Effect> {
        if epoch != self.epoch {
            return Vec::new();
        }
        self.checking = false;
        self.report = Some(report.clone());
        let mut effects = Vec::new();
        // 每代数一次网络刷新：含短路/乐观项且本代数尚未刷新才产出；产出后置位。
        if needs_network_refresh(&report) && !self.network_refreshed {
            self.network_refreshed = true;
            if let Some(path) = self.path.clone() {
                effects.push(Effect::Refresh {
                    epoch: self.epoch,
                    path,
                });
            }
        }
        if should_auto_precache(&report, self.precaching) {
            let targets = precache_targets(&report);
            if !targets.is_empty() {
                self.start_precache(PrecacheMode::Auto, targets, &mut effects);
            }
        }
        effects
    }

    fn on_refresh_done(&mut self, epoch: Epoch, report: OverallHealthReport) -> Vec<Effect> {
        if epoch != self.epoch {
            return Vec::new();
        }
        // 合并报告；不触碰预热标志与提示（刷新与预热并行、在途独立）。
        self.report = Some(report);
        Vec::new()
    }

    fn on_precache_done(&mut self, epoch: Epoch, result: Result<(), CompatError>) -> Vec<Effect> {
        if epoch != self.epoch {
            return Vec::new();
        }
        self.precaching = false;
        let was_auto = self.precaching_auto;
        self.precaching_auto = false;
        match result {
            Ok(()) => {
                self.precache_done = true;
                self.precache_error = None;
                // 复检：同代数快速体检（刷新预算已消耗 → 不再触发网络刷新）。
                self.checking = true;
                self.report = None;
                let Some(path) = self.path.clone() else {
                    return Vec::new();
                };
                vec![Effect::Probe {
                    epoch: self.epoch,
                    path,
                }]
            }
            Err(e) => {
                // 自动失败静默（徽章保持 Online、手动入口保留）；手动失败就地显示。
                if !was_auto {
                    self.precache_error = Some(e);
                }
                Vec::new()
            }
        }
    }

    fn on_precache_requested(&mut self) -> Vec<Effect> {
        if self.precaching {
            return Vec::new(); // 已在途 → 去重。
        }
        let Some(report) = &self.report else {
            return Vec::new();
        };
        let targets = precache_targets(report);
        if targets.is_empty() {
            return Vec::new();
        }
        let mut effects = Vec::new();
        self.start_precache(PrecacheMode::Manual, targets, &mut effects);
        effects
    }

    fn start_precache(
        &mut self,
        mode: PrecacheMode,
        targets: Vec<(ProbeTarget, String)>,
        effects: &mut Vec<Effect>,
    ) {
        self.precaching = true;
        self.precaching_auto = mode == PrecacheMode::Auto;
        self.precache_error = None;
        self.precache_done = false;
        let Some(path) = self.path.clone() else {
            return;
        };
        effects.push(Effect::Precache {
            epoch: self.epoch,
            path,
            targets,
        });
    }

    pub fn display(&self) -> Display<'_> {
        Display {
            report: self.report.as_ref(),
            checking: self.checking,
            precaching: self.precaching,
            precache_error: self.precache_error.as_ref(),
            precache_done: self.precache_done,
            summary: compat_summary(self.checking, self.report.as_ref()),
        }
    }
}

fn probes(report: &OverallHealthReport) -> [&compat::ProbeReport; 3] {
    [
        &report.steamclient_pattern,
        &report.steamui_pattern,
        &report.steamclient_ipc,
    ]
}

/// 汇总分类（纯函数）：优先级 检查中 > 缺文件 > 上游未适配 > 网络错误 > 未缓存 > 全就绪。
pub fn compat_summary(checking: bool, report: Option<&OverallHealthReport>) -> CompatSummary {
    if checking {
        return CompatSummary::Checking;
    }
    let Some(r) = report else {
        return CompatSummary::Checking;
    };
    let st = probes(r).map(|p| &p.status);
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
    if r.is_all_compatible {
        CompatSummary::Ready
    } else {
        CompatSummary::Online
    }
}

/// 待预热目标：RemoteAvailable{cached:false} 或验证缓存命中但无离线缓存的项，以 `signature_cached` 存在性事实为准（流程不落盘）。
pub fn precache_targets(report: &OverallHealthReport) -> Vec<(ProbeTarget, String)> {
    probes(report)
        .iter()
        .filter_map(|r| match &r.status {
            compat::ProbeStatus::RemoteAvailable { .. } if !r.signature_cached => {
                r.sha256.clone().map(|sha| (r.target, sha))
            }
            _ => None,
        })
        .collect()
}

/// 快速体检后是否需要后台网络刷新：存在短路项（CompatibleOffline）或乐观项（RemoteAvailable{cached:false}）即需补查；预算消耗由流程守卫拦截。
fn needs_network_refresh(report: &OverallHealthReport) -> bool {
    probes(report).map(|p| &p.status).iter().any(|s| {
        matches!(
            s,
            compat::ProbeStatus::CompatibleOffline
                | compat::ProbeStatus::RemoteAvailable { cached: false }
        )
    })
}

fn should_auto_precache(report: &OverallHealthReport, precaching: bool) -> bool {
    !precaching && compat_summary(false, Some(report)) == CompatSummary::Online
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_report(
        target: ProbeTarget,
        status: compat::ProbeStatus,
        sha: Option<&str>,
    ) -> compat::ProbeReport {
        compat::ProbeReport {
            target,
            sha256: sha.map(String::from),
            status,
            signature_cached: false,
        }
    }

    fn report_with(
        statuses: [compat::ProbeStatus; 3],
        has_missing_cache: bool,
    ) -> OverallHealthReport {
        OverallHealthReport {
            steamclient_pattern: probe_report(
                ProbeTarget::PatternSteamClient,
                statuses[0].clone(),
                Some("abc"),
            ),
            steamui_pattern: probe_report(
                ProbeTarget::PatternSteamUi,
                statuses[1].clone(),
                Some("abc"),
            ),
            steamclient_ipc: probe_report(
                ProbeTarget::IpcSteamClient,
                statuses[2].clone(),
                Some("abc"),
            ),
            is_all_compatible: !has_missing_cache,
            has_missing_cache,
        }
    }

    use compat::ProbeStatus as S;

    fn all(status: compat::ProbeStatus) -> [compat::ProbeStatus; 3] {
        [status.clone(), status.clone(), status]
    }

    fn start_probe(flow: &mut CompatFlow, path: &str) -> Epoch {
        let (_, effects) = flow.step(Event::PathChanged(path.to_string()));
        match effects.as_slice() {
            [Effect::Probe { epoch, .. }] => *epoch,
            other => panic!("expected SpawnProbe, got {other:?}"),
        }
    }

    fn find_precache(effects: &[Effect]) -> Option<&Effect> {
        effects
            .iter()
            .find(|e| matches!(e, Effect::Precache { .. }))
    }

    #[test]
    fn compat_summary_checking_when_in_progress() {
        assert_eq!(compat_summary(true, None), CompatSummary::Checking);
        assert_eq!(compat_summary(false, None), CompatSummary::Checking);
    }

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

    #[test]
    fn precache_targets_picks_available_without_signature() {
        let r = report_with(
            [
                S::RemoteAvailable { cached: false },
                S::RemoteAvailable { cached: true },
                S::IncompatiblePending,
            ],
            true,
        );
        let targets = precache_targets(&r);
        assert_eq!(targets.len(), 2);
        let ts: Vec<_> = targets.iter().map(|(t, _)| *t).collect();
        assert!(ts.contains(&ProbeTarget::PatternSteamClient));
        assert!(ts.contains(&ProbeTarget::PatternSteamUi));
    }

    #[test]
    fn precache_targets_excludes_existing_signature() {
        let mut r = report_with(
            [
                S::RemoteAvailable { cached: true },
                S::RemoteAvailable { cached: true },
                S::IncompatiblePending,
            ],
            true,
        );
        r.steamclient_pattern.signature_cached = true;
        let targets = precache_targets(&r);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, ProbeTarget::PatternSteamUi);
    }

    #[test]
    fn path_change_bumps_epoch_and_spawns_probe() {
        let mut flow = CompatFlow::new();
        let (d, effects) = flow.step(Event::PathChanged("A".into()));
        assert_eq!(
            effects,
            vec![Effect::Probe {
                epoch: Epoch(1),
                path: "A".into()
            }]
        );
        assert!(d.checking);
        assert!(d.report.is_none());
        assert_eq!(d.summary, CompatSummary::Checking);

        let (_, effects) = flow.step(Event::PathChanged("A".into()));
        assert!(effects.is_empty());

        let (_, effects) = flow.step(Event::PathChanged("B".into()));
        assert_eq!(
            effects,
            vec![Effect::Probe {
                epoch: Epoch(2),
                path: "B".into()
            }]
        );
    }

    #[test]
    fn probe_done_updates_display_without_refresh_when_confirmed() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        let (d, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: true }), false),
        });
        assert!(effects.is_empty());
        assert!(!d.checking);
        assert_eq!(d.summary, CompatSummary::Ready);
        assert!(d.report.is_some());
    }

    #[test]
    fn stale_epoch_messages_are_dropped() {
        let mut flow = CompatFlow::new();
        let old = start_probe(&mut flow, "A");
        let new = start_probe(&mut flow, "B");
        assert_ne!(old, new);

        let (d, effects) = flow.step(Event::ProbeDone {
            epoch: old,
            report: report_with(all(S::RemoteAvailable { cached: true }), false),
        });
        assert!(effects.is_empty());
        assert!(d.report.is_none());
        assert!(d.checking);

        let (_, effects) = flow.step(Event::RefreshDone {
            epoch: old,
            report: report_with(all(S::RemoteAvailable { cached: true }), false),
        });
        assert!(effects.is_empty());
        let (_, effects) = flow.step(Event::PrecacheDone {
            epoch: old,
            result: Ok(()),
        });
        assert!(effects.is_empty());
    }

    #[test]
    fn refresh_fires_once_per_epoch() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        let shortcut = report_with(all(S::CompatibleOffline), false);

        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: shortcut.clone(),
        });
        assert_eq!(
            effects,
            vec![Effect::Refresh {
                epoch,
                path: "A".into()
            }]
        );

        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: shortcut,
        });
        assert!(effects.is_empty());
    }

    #[test]
    fn failed_refresh_consumes_epoch_budget() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::CompatibleOffline), false),
        });
        assert_eq!(effects.len(), 1);

        let (d, effects) = flow.step(Event::RefreshDone {
            epoch,
            report: report_with(all(S::NetworkError("timeout".into())), false),
        });
        assert!(effects.is_empty());
        assert_eq!(d.summary, CompatSummary::Network);

        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::CompatibleOffline), false),
        });
        assert!(effects.is_empty());
    }

    #[test]
    fn refresh_merge_preserves_inflight_precache() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        assert!(effects.iter().any(|e| matches!(e, Effect::Refresh { .. })));
        assert!(find_precache(&effects).is_some());

        let confirmed = report_with(all(S::RemoteAvailable { cached: true }), false);
        let (d, effects) = flow.step(Event::RefreshDone {
            epoch,
            report: confirmed.clone(),
        });
        assert!(effects.is_empty());
        assert!(d.precaching, "刷新不得抹掉在途预热");
        assert!(d.report.is_some());

        let (d, effects) = flow.step(Event::PrecacheDone {
            epoch,
            result: Ok(()),
        });
        assert!(d.precache_done);
        assert_eq!(
            effects,
            vec![Effect::Probe {
                epoch,
                path: "A".into()
            }]
        );
    }

    #[test]
    fn auto_precache_failure_is_silent() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        let online = report_with(all(S::RemoteAvailable { cached: false }), true);

        let (d, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: online.clone(),
        });
        assert!(d.precaching);
        let precache = find_precache(&effects).expect("auto precache effect");
        match precache {
            Effect::Precache {
                epoch: e,
                path,
                targets,
            } => {
                assert_eq!(*e, epoch);
                assert_eq!(path, "A");
                assert_eq!(targets.len(), 3);
            }
            other => panic!("expected auto precache, got {other:?}"),
        }

        let (d, effects) = flow.step(Event::PrecacheDone {
            epoch,
            result: Err(CompatError::Network("boom".into())),
        });
        assert!(effects.is_empty());
        assert!(!d.precaching);
        assert!(d.precache_error.is_none());
        assert_eq!(d.summary, CompatSummary::Online);
    }

    #[test]
    fn manual_precache_requires_report_and_targets() {
        let mut flow = CompatFlow::new();
        let (_, effects) = flow.step(Event::PrecacheRequested);
        assert!(effects.is_empty());

        let epoch = start_probe(&mut flow, "A");
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::IncompatiblePending), false),
        });
        assert!(effects.is_empty());
        let (_, effects) = flow.step(Event::PrecacheRequested);
        assert!(effects.is_empty());

        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        assert!(find_precache(&effects).is_some());
        let (_, effects) = flow.step(Event::PrecacheRequested);
        assert!(effects.is_empty()); // 已在途 → 去重

        let _ = flow.step(Event::PrecacheDone {
            epoch,
            result: Err(CompatError::Network("auto boom".into())),
        });
        let (d, effects) = flow.step(Event::PrecacheRequested);
        assert!(d.precaching);
        assert!(find_precache(&effects).is_some(), "manual precache effect");

        let (d, _) = flow.step(Event::PrecacheDone {
            epoch,
            result: Err(CompatError::Io("manual boom".into())),
        });
        assert!(matches!(d.precache_error, Some(CompatError::Io(_))));
    }

    #[test]
    fn precache_success_reprobes_same_epoch_without_re_refresh() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        assert!(effects.iter().any(|e| matches!(e, Effect::Refresh { .. })));

        let (d, effects) = flow.step(Event::PrecacheDone {
            epoch,
            result: Ok(()),
        });
        assert!(d.precache_done);
        assert!(d.checking);
        assert!(d.report.is_none());
        assert_eq!(
            effects,
            vec![Effect::Probe {
                epoch,
                path: "A".into()
            }]
        );

        let (d, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::CompatibleOffline), false),
        });
        assert!(effects.is_empty(), "复检不得再次触发网络刷新: {effects:?}");
        assert_eq!(d.summary, CompatSummary::Ready);
        assert!(d.precache_done);
    }

    #[test]
    fn precache_done_lifecycle() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        let _ = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        let (d, _) = flow.step(Event::PrecacheDone {
            epoch,
            result: Ok(()),
        });
        assert!(d.precache_done);
        let (d, _) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::CompatibleOffline), false),
        });
        assert!(d.precache_done);

        let (d, _) = flow.step(Event::RefreshDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: true }), false),
        });
        assert!(d.precache_done);

        let (d, _) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        assert!(!d.precache_done);

        let (d, _) = flow.step(Event::PathChanged("B".into()));
        assert!(!d.precache_done);
        assert!(!d.precaching);
        assert!(d.precache_error.is_none());
    }
}
