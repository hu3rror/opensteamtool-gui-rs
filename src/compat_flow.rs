//! 「体检流程」状态机：兼容性体检生命周期的唯一编排者。
//!
//! 深模块——小接口（`step` / `display`），大实现（代数戳、每代数一次网络刷新、
//! 刷新/预热独立在途、复检守卫）。纯状态机：无 IO、无线程、无 egui、无 i18n；
//! App 只喂事件、执行返回的效果（spawn 后台线程）、渲染展示态。
//! 状态转移全部可经 `step` 单测（接缝 = 状态机接口）。

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

/// 流程事件（App 喂入）。
#[derive(Clone, Debug)]
pub enum Event {
    /// Steam 路径变更（含启动首次）：推进代数；与上次路径相同 → 无动作（防抖）。
    PathChanged(String),
    /// 快速体检完成。
    ProbeDone {
        epoch: Epoch,
        report: OverallHealthReport,
    },
    /// 网络刷新完成。
    RefreshDone {
        epoch: Epoch,
        report: OverallHealthReport,
    },
    /// 预热完成。
    PrecacheDone {
        epoch: Epoch,
        result: Result<(), CompatError>,
    },
    /// 手动「一键缓存签名」（自动预热由流程内部在 ProbeDone 时触发）。
    PrecacheRequested,
}

/// 待办效果（App 执行）。效果携带发起时路径——spawn 时钉住，不在消息处理时重读。
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// 发起快速体检（零网络）。
    Probe { epoch: Epoch, path: String },
    /// 发起网络刷新。
    Refresh { epoch: Epoch, path: String },
    /// 发起预热下载。
    Precache {
        epoch: Epoch,
        path: String,
        targets: Vec<(ProbeTarget, String)>,
    },
}

/// 汇总徽标分类（issue #23 §7.7 状态视觉）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompatSummary {
    Checking,
    Ready,
    Online,
    Pending,
    Missing,
    Network,
}

/// 展示态（App 渲染；全部字段归流程所有，App 只读）；report/precache_error 为流程内部引用，热路径零拷贝。
#[derive(Clone, Debug)]
pub struct Display<'a> {
    pub report: Option<&'a OverallHealthReport>,
    /// 体检/复检进行中（显示 Checking / 禁用按钮）。
    pub checking: bool,
    /// 预热进行中（按钮显示「正在缓存...」）。
    pub precaching: bool,
    /// 预热失败错误（手动失败就地显示；自动失败静默）。
    pub precache_error: Option<&'a CompatError>,
    /// 预热成功提示（复检/刷新合并后保留；下次预热开始或路径变更清除）。
    pub precache_done: bool,
    /// 汇总分类（徽章渲染）。
    pub summary: CompatSummary,
}

/// 体检流程状态机。
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

    /// 推进事件：返回展示态与待办效果。App 执行效果、渲染展示态。
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
            return Vec::new(); // 陈旧代数：丢弃。
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
        // 自动预热：体检落定 Online 且无预热进行中 → 自动触发。
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

    /// 启动一次预热（自动/手动共用）：置在途标志、清提示、产出效果。
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

    /// 展示态（借用流程内部状态，零拷贝；App 渲染后即释放借用）。
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

/// 三项探针报告（状态判定与目标收集共用的形状，收敛重复）。
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

/// 待预热目标（RemoteAvailable{cached:false} 或验证缓存命中但无离线缓存），
/// 以报告携带的 `signature_cached` 存在性事实为准（算子层探针时判定，流程不落盘）。
/// App 详情按钮可见性也读它。
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

/// 快速体检后是否需要后台网络刷新：存在短路项（CompatibleOffline）或乐观项
/// （RemoteAvailable{cached:false}）即需补查。本代数刷新预算已消耗时由流程守卫拦截。
fn needs_network_refresh(report: &OverallHealthReport) -> bool {
    probes(report)
        .map(|p| &p.status)
        .iter()
        .any(|s| {
        matches!(
            s,
            compat::ProbeStatus::CompatibleOffline
                | compat::ProbeStatus::RemoteAvailable { cached: false }
        )
    })
}

/// 是否应自动预热：体检落定 Online（上游已适配未缓存）且当前无预热进行中。
fn should_auto_precache(report: &OverallHealthReport, precaching: bool) -> bool {
    !precaching && compat_summary(false, Some(report)) == CompatSummary::Online
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造探针报告样本（signature_cached 占位，summary 判定不依赖它）。
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

    /// 三份同状态报告（ProbeStatus 非 Copy，不用数组重复语法）。
    fn all(status: compat::ProbeStatus) -> [compat::ProbeStatus; 3] {
        [status.clone(), status.clone(), status]
    }

    /// 便捷：喂 PathChanged 并取回首个体检效果的代数。
    fn start_probe(flow: &mut CompatFlow, path: &str) -> Epoch {
        let (_, effects) = flow.step(Event::PathChanged(path.to_string()));
        match effects.as_slice() {
            [Effect::Probe { epoch, .. }] => *epoch,
            other => panic!("expected SpawnProbe, got {other:?}"),
        }
    }

    /// 取效果列表中的首个预热效果。
    fn find_precache(effects: &[Effect]) -> Option<&Effect> {
        effects
            .iter()
            .find(|e| matches!(e, Effect::Precache { .. }))
    }

    // ==================== compat_summary / precache_targets（自 ui.rs 迁入） ====================

    /// 检查中 / 无报告 → Checking（骨架态）。
    #[test]
    fn compat_summary_checking_when_in_progress() {
        assert_eq!(compat_summary(true, None), CompatSummary::Checking);
        assert_eq!(compat_summary(false, None), CompatSummary::Checking);
    }

    /// 任一 DLL 缺失 → Missing（最高优先级）。
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

    /// 上游未适配 → Pending（优先于 Network/Online）。
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

    /// 网络错误（无 Pending）→ Network。
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

    /// 存在未缓存项 → Online（提示预热）。
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

    /// 全缓存就绪（在线或离线）→ Ready。
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
            [S::CompatibleOffline, S::CompatibleOffline, S::CompatibleOffline],
            false,
        );
        assert_eq!(compat_summary(false, Some(&offline)), CompatSummary::Ready);
    }

    /// 待预热目标：收集「已适配但离线缓存缺失」的项（以 signature_cached 存在性为准）。
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

    /// 离线缓存已就绪的项（signature_cached=true）不被收集。
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
        // 第一项离线缓存已存在（算子层探针时判定的事实）→ 不收集。
        r.steamclient_pattern.signature_cached = true;
        let targets = precache_targets(&r);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, ProbeTarget::PatternSteamUi);
    }

    // ==================== 状态机转移 ====================

    /// 路径变更推进代数、产出快速体检效果、展示检查态；同路径防抖无动作。
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

        // 同路径 → 无动作。
        let (_, effects) = flow.step(Event::PathChanged("A".into()));
        assert!(effects.is_empty());

        // 新路径 → 代数推进。
        let (_, effects) = flow.step(Event::PathChanged("B".into()));
        assert_eq!(
            effects,
            vec![Effect::Probe {
                epoch: Epoch(2),
                path: "B".into()
            }]
        );
    }

    /// 当前代数体检完成：更新报告、退出检查态；无短路项不产出任何效果。
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

    /// 陈旧代数（路径已变更）的完成事件一律丢弃，不触碰展示态。
    #[test]
    fn stale_epoch_messages_are_dropped() {
        let mut flow = CompatFlow::new();
        let old = start_probe(&mut flow, "A");
        let new = start_probe(&mut flow, "B");
        assert_ne!(old, new);

        // 旧代数的快速体检结果晚到 → 忽略（展示仍为 B 的检查态）。
        let (d, effects) = flow.step(Event::ProbeDone {
            epoch: old,
            report: report_with(all(S::RemoteAvailable { cached: true }), false),
        });
        assert!(effects.is_empty());
        assert!(d.report.is_none());
        assert!(d.checking);

        // 旧代数的刷新/预热完成 → 同样忽略。
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

    /// 含短路/乐观项的报告触发一次网络刷新；同代数内不再触发（刷新预算一次性）。
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

        // 同代数再来一份短路报告（复检场景）→ 不再刷新。
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: shortcut,
        });
        assert!(effects.is_empty());
    }

    /// 刷新失败（NetworkError）也视同已消耗本代数预算（一次性语义，不隐式重试）。
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

        // 网络恢复后同代数复检 → 刷新预算已消耗，不重试。
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::CompatibleOffline), false),
        });
        assert!(effects.is_empty());
    }

    /// 刷新完成合并报告，但保留在途预热标志与提示（并行、在途独立）。
    #[test]
    fn refresh_merge_preserves_inflight_precache() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        // Online 报告 → 同时产出刷新 + 自动预热。
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        assert!(effects.iter().any(|e| matches!(e, Effect::Refresh { .. })));
        assert!(find_precache(&effects).is_some());

        // 刷新晚到：合并报告、清刷新在途，不得抹掉在途预热。
        let confirmed = report_with(all(S::RemoteAvailable { cached: true }), false);
        let (d, effects) = flow.step(Event::RefreshDone {
            epoch,
            report: confirmed.clone(),
        });
        assert!(effects.is_empty());
        assert!(d.precaching, "刷新不得抹掉在途预热");
        assert!(d.report.is_some());

        // 预热完成：成功 → 复检 + 预热成功提示。
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

    /// Online 态自动预热：产出 Auto 模式效果；失败静默（错误不显示、手动入口保留）。
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

        // 自动失败 → 静默：无错误提示，报告保持 Online（手动入口保留）。
        let (d, effects) = flow.step(Event::PrecacheDone {
            epoch,
            result: Err(CompatError::Network("boom".into())),
        });
        assert!(effects.is_empty());
        assert!(!d.precaching);
        assert!(d.precache_error.is_none());
        assert_eq!(d.summary, CompatSummary::Online);
    }

    /// 手动预热：无报告/无目标/已在途时不产出效果；失败就地显示错误。
    #[test]
    fn manual_precache_requires_report_and_targets() {
        let mut flow = CompatFlow::new();
        // 从未体检 → 无动作。
        let (_, effects) = flow.step(Event::PrecacheRequested);
        assert!(effects.is_empty());

        let epoch = start_probe(&mut flow, "A");
        // 无目标（上游未适配）→ 无动作。
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::IncompatiblePending), false),
        });
        assert!(effects.is_empty());
        let (_, effects) = flow.step(Event::PrecacheRequested);
        assert!(effects.is_empty());

        // Online 态 → 自动预热启动；手动请求在途去重。
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        assert!(find_precache(&effects).is_some());
        let (_, effects) = flow.step(Event::PrecacheRequested);
        assert!(effects.is_empty()); // 已在途 → 去重

        // 自动失败静默后手动重试 → 手动预热启动（手动/自动差异经失败路径断言）。
        let _ = flow.step(Event::PrecacheDone {
            epoch,
            result: Err(CompatError::Network("auto boom".into())),
        });
        let (d, effects) = flow.step(Event::PrecacheRequested);
        assert!(d.precaching);
        assert!(find_precache(&effects).is_some(), "manual precache effect");

        // 手动失败 → 就地显示错误（CompatError 类型活到渲染，逐分支双语映射）。
        let (d, _) = flow.step(Event::PrecacheDone {
            epoch,
            result: Err(CompatError::Io("manual boom".into())),
        });
        assert!(matches!(d.precache_error, Some(CompatError::Io(_))));
    }

    /// 预热成功 → 同代数复检；复检的短路项不再触发二次网络刷新（循环守卫盖全两条腿）。
    #[test]
    fn precache_success_reprobes_same_epoch_without_re_refresh() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        let (_, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        assert!(effects.iter().any(|e| matches!(e, Effect::Refresh { .. })));

        // 预热成功 → 复检（同代数！）+ 预热成功提示。
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

        // 复检落定全缓存（短路项）→ 刷新预算已消耗，不产出二次刷新。
        let (d, effects) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::CompatibleOffline), false),
        });
        assert!(effects.is_empty(), "复检不得再次触发网络刷新: {effects:?}");
        assert_eq!(d.summary, CompatSummary::Ready);
        // 预热成功提示在复检后保留。
        assert!(d.precache_done);
    }

    /// 预热成功提示生命周期：新预热开始或路径变更清除；复检/刷新合并保留。
    #[test]
    fn precache_done_lifecycle() {
        let mut flow = CompatFlow::new();
        let epoch = start_probe(&mut flow, "A");
        // 自动预热触发（Online）→ 成功 → 提示置位 → 复检落定缓存就位（短路项）→ 提示保留。
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

        // 晚到的网络刷新合并 → 提示保留。
        let (d, _) = flow.step(Event::RefreshDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: true }), false),
        });
        assert!(d.precache_done);

        // 新预热开始（Online 报告再次触发自动预热）→ 清除。
        let (d, _) = flow.step(Event::ProbeDone {
            epoch,
            report: report_with(all(S::RemoteAvailable { cached: false }), true),
        });
        assert!(!d.precache_done);

        // 路径变更 → 全清（提示/在途/错误）。
        let (d, _) = flow.step(Event::PathChanged("B".into()));
        assert!(!d.precache_done);
        assert!(!d.precaching);
        assert!(d.precache_error.is_none());
    }
}
