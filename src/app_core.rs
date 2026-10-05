//! 编排核心（App Core）：消息决策纯状态机。
//! 零 IO / 零 egui / 零 i18n：壳提交事件、执行返回的效果、把观察结果喂回，渲染只读快照。
//! 与 compat_flow / wizard 同构（ADR-0019，GLOSSARY「编排核心」）。

use crate::busy::{BusyGate, BusyKind};
use crate::compat::OverallHealthReport;
use crate::compat_flow::{self, CompatFlow, CompatSummary, Epoch};
use crate::dll::DeploymentFacts;
use crate::update_flow::{UpdateFlow, UpdateNotice};
use crate::updater::{OnlineInfo, UpdateError};
use crate::workflow::{self, Action, Op, Precheck};
use std::path::PathBuf;

/// 渲染层交互意图（Hero / Secondary / Update 按钮位收集后统一处理）：
/// Action/Check/Download 映射为 AppEvent；FixPath 是壳导航（打开设置页），不进编排核心。
#[derive(Clone, Debug)]
pub enum MainEvent {
    Action(Action),
    FixPath,
    Check,
    Download,
}

/// 事件输入：消息决策 / 交互意图 / 观察结果（壳执行效果后以观察事件喂回）。
#[derive(Clone, Debug)]
pub enum AppEvent {
    /// 工作流阶段回调：替换忙碌门禁为当前阶段（仅忙碌中合法，debug 断言拦截）。
    Phase(BusyKind),
    UpdateChecked(Result<OnlineInfo, UpdateError>),
    Downloaded(Result<(), UpdateError>),
    WorkflowDone {
        action: Action,
        result: Result<(), workflow::WorkflowError>,
    },
    /// 交互按钮点击 → 编排决策（确认流入口：卸载类动作在 Steam 运行时先弹确认，§20.3/#36）。
    ActionRequested(Action),
    /// 免确认请求（仅「卸载补丁」行，§45/§20.3）：Steam 运行中也不弹确认，直接编排。
    ActionRequestedQuiet(Action),
    /// 确认框「是」→ 放行（仅 asks_to_close_steam 类动作会走到确认框）。
    ActionConfirmed(Action),
    ConfirmCanceled,
    /// 「检查更新」按钮；忙碌门禁拒绝时静默忽略。
    Check,
    /// 下载按钮（payload 版：AppCore 自查 update_flow 派生，版本永不渲染）。
    Download,
    /// 壳侧 plan() 成功（IO 由壳执行，结果回喂）。
    PlanOk {
        action: Action,
        ops: Vec<Op>,
    },
    /// 壳侧 plan() 拒绝（快照过期信号：通知 + 刷新事实）。
    PlanRejected(Precheck),
    /// 壳侧 probe_facts 完成（端口注入，渲染零 IO）。
    FactsRefreshed(DeploymentFacts),
    /// 壳侧 Steam 运行状态观察（monitor tick / 工作流后 rescan）。
    SteamRunningChanged(bool),
    /// Steam 启动边沿（monitor tick）：auto-tray 决策入参。
    SteamStarted,
    /// Steam 退出边沿（monitor tick）：auto-tray 决策入参。
    SteamStopped,
    /// 窗口显隐镜像同步（壳物理写入点统一回喂；存态不产效果）。
    WindowVisibleChanged(bool),
    /// 工作路径提交（唯一写入点；D4/ADR-0020）：设置页提交与向导终局收敛到这里。
    CommitPath(String),
    /// 体检流程路径变更（含启动首次）：推进代数、防抖由流程内部承担。
    CompatPathChanged(String),
    /// 体检快速探针完成（观察回喂；迟到代数由流程丢弃）。
    CompatProbeDone {
        epoch: Epoch,
        report: OverallHealthReport,
    },
    /// 体检网络刷新完成（观察回喂；迟到代数由流程丢弃）。
    CompatRefreshDone {
        epoch: Epoch,
        report: OverallHealthReport,
    },
    /// 体检预热完成（观察回喂；迟到代数由流程丢弃）。
    CompatPrecacheDone {
        epoch: Epoch,
        result: Result<(), crate::compat::CompatError>,
    },
    /// 手动「一键缓存签名」按钮（目标选择与在途去重由流程承担）。
    CompatPrecacheRequested,
}

/// 效果：壳执行（spawn / plan / probe / rescan），观察结果以事件喂回。
#[derive(Clone, Debug, PartialEq)]
pub enum AppEffect {
    SpawnUpdateCheck,
    SpawnUpdateDownload {
        info: OnlineInfo,
    },
    RunPlan {
        action: Action,
        kill_first: bool,
        /// 决策时注入的 Steam 工作目录（效果自足，执行不二次派生——ADR-0020）。
        steam_dir: PathBuf,
    },
    SpawnWorkflow {
        action: Action,
        ops: Vec<Op>,
        steam_dir: PathBuf,
    },
    RefreshFacts,
    RescanSteam,
    /// 路径变更后重探体检流程（决策在核心，回喂经效果队列）。
    FeedCompatPath(String),
    /// 体检快速探针（扁平效果：compat_flow::Effect 收编进家族，ADR-0019）。
    CompatProbe {
        epoch: Epoch,
        path: String,
    },
    /// 体检网络刷新（扁平效果）。
    CompatRefresh {
        epoch: Epoch,
        path: String,
    },
    /// 体检预热（扁平效果）。
    CompatPrecache {
        epoch: Epoch,
        path: String,
        targets: Vec<(crate::compat::ProbeTarget, String)>,
    },
    /// 向导下载补丁并解压（向导留壳，效果收编进家族——ADR-0019）。
    WizardDownload,
    /// 向导终局：壳持久化语言/主题/路径三字段后收敛 CommitPath（ADR-0020；原子写留壳）。
    WizardFinish {
        language: crate::config::Language,
        theme: crate::config::ThemePreference,
        steam_path: String,
    },
    /// 窗口显隐执行（auto-tray/工作流决策产出；壳执行 ViewportCommand 并回喂镜像）。
    SetWindowVisible(bool),
}

/// 通知（状态栏文案判定；UpdateChecked 无 payload，结果文案由 update_flow 派生——单一事实源）。
#[derive(Clone, Debug)]
pub enum Notice {
    UpdateChecked,
    Downloaded(Result<(), UpdateError>),
    WorkflowDone(Action, Result<(), workflow::WorkflowError>),
    Precheck(workflow::Precheck),
}

/// 更新派生快照（版本永不渲染，ADR-0014）：文案判定 + 下载可用性。
#[derive(Clone, Debug, PartialEq)]
pub enum SnapshotUpdateNotice {
    UpToDate,
    NewVersion,
    CheckFailed(UpdateError),
}

/// 渲染只读快照（属主拷贝，每帧一次由壳构建）。
/// 体检域快照：渲染只消费快照，strings 映射留壳（零 i18n）。
#[derive(Clone, Debug)]
pub struct SnapshotCompat {
    pub report: Option<OverallHealthReport>,
    pub checking: bool,
    pub precaching: bool,
    pub precache_error: Option<crate::compat::CompatError>,
    pub precache_done: bool,
    pub summary: CompatSummary,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub facts: DeploymentFacts,
    pub steam_running: bool,
    pub steam_path: String,
    pub busy: Option<BusyKind>,
    pub notice: Option<Notice>,
    pub confirm: Option<Action>,
    pub update: SnapshotUpdate,
    pub compat: SnapshotCompat,
}

#[derive(Clone, Debug)]
pub struct SnapshotUpdate {
    pub notice: Option<SnapshotUpdateNotice>,
    pub download: Option<OnlineInfo>,
}

/// 消息决策纯状态机（模块头 doc 已述契约；ADR-0019）。
pub struct AppCore {
    update_flow: UpdateFlow,
    gate: BusyGate,
    confirm: Option<Action>,
    notice: Option<Notice>,
    steam_running: bool,
    steam_path: String,
    facts: DeploymentFacts,
    /// 体检流程：内部 seam 子域状态机（ADR-0019）。
    flow: CompatFlow,
    /// 窗口显隐镜像（决策入参；物理写入点在壳，经 WindowVisibleChanged 同步）。
    window_visible: bool,
    /// RescanSteam 合成隐窗的待决标志：WorkflowDone 置位，下次 SteamRunningChanged 决定是否隐（一次性）。
    pending_auto_hide: bool,
}

/// compat 域效果扁平映射：compat_flow::Effect 作为内部枚举被包裹，不进 interface（ADR-0019）。
fn compat_effects(effects: Vec<compat_flow::Effect>) -> Vec<AppEffect> {
    effects
        .into_iter()
        .map(|e| match e {
            compat_flow::Effect::Probe { epoch, path } => AppEffect::CompatProbe { epoch, path },
            compat_flow::Effect::Refresh { epoch, path } => {
                AppEffect::CompatRefresh { epoch, path }
            }
            compat_flow::Effect::Precache {
                epoch,
                path,
                targets,
            } => AppEffect::CompatPrecache {
                epoch,
                path,
                targets,
            },
        })
        .collect()
}

impl AppCore {
    pub fn new(facts: DeploymentFacts, steam_running: bool, steam_path: impl Into<String>) -> Self {
        let steam_path = steam_path.into();
        Self {
            update_flow: UpdateFlow::new(),
            gate: BusyGate::new(),
            confirm: None,
            notice: None,
            steam_running,
            // 启动注入值也归一到 trim（与 CommitPath 同一视界：快照恒干净，ADR-0020）。
            steam_path: steam_path.trim().to_string(),
            facts,
            flow: CompatFlow::new(),
            window_visible: true,
            pending_auto_hide: false,
        }
    }

    /// 事件决策：返回壳待执行的效果；观察结果经事件回喂（反馈环）。
    pub fn step(&mut self, evt: AppEvent) -> Vec<AppEffect> {
        match evt {
            AppEvent::Phase(kind) => self.gate.replace(kind),
            AppEvent::UpdateChecked(res) => {
                self.gate.clear();
                self.update_flow.check_done(res);
                self.notice = Some(Notice::UpdateChecked);
            }
            AppEvent::Downloaded(res) => {
                self.gate.clear();
                self.notice = Some(Notice::Downloaded(res.clone()));
                if res.is_ok() {
                    return vec![AppEffect::RefreshFacts];
                }
            }
            AppEvent::WorkflowDone { action, result } => {
                self.gate.clear();
                self.notice = Some(Notice::WorkflowDone(action, result.clone()));
                // RescanSteam 合成隐窗：工作流结束后若 Steam 仍在运行则隐窗一次——置待决标志，
                // 由下个 SteamRunningChanged 观察结果决定（Steam 已退出则自然不隐）。
                self.pending_auto_hide = true;
                let mut effects = Vec::new();
                if result.is_ok() {
                    effects.push(AppEffect::RefreshFacts);
                }
                // 无论成败都重读运行状态：Steam 在工作流后仍在运行则隐窗（WorkflowDone 反馈回环的末环）。
                effects.push(AppEffect::RescanSteam);
                return effects;
            }
            AppEvent::ActionRequested(action) => {
                if self.gate.is_busy() {
                    return Vec::new();
                }
                // #36：卸载类动作在 Steam 运行中进确认流；「应用补丁并启动」直接放行（kill_first 派生）。
                if action.asks_to_close_steam() && self.steam_running {
                    self.confirm = Some(action);
                    return Vec::new();
                }
                let kill_first = self.steam_running && action == Action::ApplyAndLaunch;
                return vec![AppEffect::RunPlan {
                    action,
                    kill_first,
                    steam_dir: PathBuf::from(&self.steam_path),
                }];
            }
            AppEvent::ActionRequestedQuiet(action) => {
                if self.gate.is_busy() {
                    return Vec::new();
                }
                // 免确认路径：不因点击瞬间 Steam 已启动误弹确认框（§20.3）。
                return vec![AppEffect::RunPlan {
                    action,
                    kill_first: false,
                    steam_dir: PathBuf::from(&self.steam_path),
                }];
            }
            AppEvent::ActionConfirmed(action) => {
                // 弹窗只在卸载类动作且 Steam 运行时出现（§20.3）；确认后必须落下，避免悬挂。
                self.confirm = None;
                // 确认框只承载 asks_to_close_steam 动作：同意即带 kill_first 执行。
                return vec![AppEffect::RunPlan {
                    action,
                    kill_first: true,
                    steam_dir: PathBuf::from(&self.steam_path),
                }];
            }
            AppEvent::ConfirmCanceled => {
                self.confirm = None;
            }
            AppEvent::Check => {
                if !self.gate.start(BusyKind::Checking) {
                    return Vec::new();
                }
                self.update_flow.check_started();
                // 以当前磁盘为基准：文件在发起时若已缺失（如外部删除 dlls/），派生将反映为
                // NewVersion + 可下载（修复动作），不会落在陈旧快照上误报「已是最新」（ADR-0011/#26）。
                return vec![AppEffect::RefreshFacts, AppEffect::SpawnUpdateCheck];
            }
            AppEvent::Download => {
                // 下载目标自查（payload 移除；版本永不渲染）：按钮可否点由快照派生保证，
                // 此处防御性兜底（派生过时竞态下静默丢弃，等价现状点击只发生在可下载时）。
                let Some(info) = self
                    .update_flow
                    .derived(self.facts.known_local_version())
                    .download
                    .cloned()
                else {
                    return Vec::new();
                };
                if !self.gate.start(BusyKind::Downloading) {
                    return Vec::new();
                }
                return vec![AppEffect::SpawnUpdateDownload { info }];
            }
            AppEvent::PlanOk { action, ops } => {
                self.confirm = None; // 无论门禁是否放行都收掉确认弹窗，避免悬挂。
                let first_phase = ops.first().expect("plan never returns empty").phase();
                if !self.gate.start(first_phase) {
                    // 竞态拒绝（确认框悬挂期 Modal 阻断交互，Release 下理论上到不了这里）。
                    return Vec::new();
                }
                return vec![AppEffect::SpawnWorkflow {
                    action,
                    ops,
                    steam_dir: PathBuf::from(&self.steam_path),
                }];
            }
            AppEvent::PlanRejected(precheck) => {
                self.confirm = None;
                self.notice = Some(Notice::Precheck(precheck));
                // plan 拒绝是「快照过期」信号（如补丁文件被外部删除）：刷新事实，按钮立即归灰。
                return vec![AppEffect::RefreshFacts];
            }
            AppEvent::FactsRefreshed(facts) => self.facts = facts,
            AppEvent::SteamRunningChanged(running) => {
                self.steam_running = running;
                // 隐窗是一次性合成动作：待决（WorkflowDone 后重读运行状态）且窗口仍可见时才产出一次；
                // Steam 已退出或窗口已隐藏则仅清待决。
                if self.pending_auto_hide && running && self.window_visible {
                    self.pending_auto_hide = false;
                    return vec![AppEffect::SetWindowVisible(false)];
                }
                self.pending_auto_hide = false;
            }
            // auto-tray 边沿决策（表语义）：
            AppEvent::SteamStarted => {
                self.steam_running = true;
                if self.window_visible {
                    return vec![AppEffect::SetWindowVisible(false)];
                }
            }
            AppEvent::SteamStopped => {
                self.steam_running = false;
                if !self.window_visible {
                    return vec![AppEffect::SetWindowVisible(true)];
                }
            }
            // 镜像同步事件：存态即可（幂等；不产效果防回环）。
            AppEvent::WindowVisibleChanged(visible) => self.window_visible = visible,
            AppEvent::CommitPath(p) => {
                // 唯一写入点：trim 单一职责归此（ADR-0020）。
                self.steam_path = p.trim().to_string();
                return vec![
                    AppEffect::RefreshFacts,
                    AppEffect::FeedCompatPath(self.steam_path.clone()),
                ];
            }
            AppEvent::CompatPathChanged(path) => {
                let (_display, effects) = self.flow.step(compat_flow::Event::PathChanged(path));
                return compat_effects(effects);
            }
            AppEvent::CompatProbeDone { epoch, report } => {
                let (_display, effects) = self
                    .flow
                    .step(compat_flow::Event::ProbeDone { epoch, report });
                return compat_effects(effects);
            }
            AppEvent::CompatRefreshDone { epoch, report } => {
                let (_display, effects) = self
                    .flow
                    .step(compat_flow::Event::RefreshDone { epoch, report });
                return compat_effects(effects);
            }
            AppEvent::CompatPrecacheDone { epoch, result } => {
                let (_display, effects) = self
                    .flow
                    .step(compat_flow::Event::PrecacheDone { epoch, result });
                return compat_effects(effects);
            }
            AppEvent::CompatPrecacheRequested => {
                let (_display, effects) = self.flow.step(compat_flow::Event::PrecacheRequested);
                return compat_effects(effects);
            }
        }
        Vec::new()
    }

    /// 渲染消费只读快照（每帧属主拷贝；更新派生当场计算，零 IO）。
    pub fn snapshot(&self) -> Snapshot {
        let derived = self.update_flow.derived(self.facts.known_local_version());
        // 体检快照：域状态进快照，渲染零 IO（strings 映射留壳）。
        let compat_display = self.flow.display();
        Snapshot {
            facts: self.facts.clone(),
            steam_running: self.steam_running,
            steam_path: self.steam_path.clone(),
            busy: self.gate.current(),
            notice: self.notice.clone(),
            confirm: self.confirm,
            update: SnapshotUpdate {
                notice: derived.notice.map(|n| match n {
                    UpdateNotice::UpToDate => SnapshotUpdateNotice::UpToDate,
                    UpdateNotice::NewVersion => SnapshotUpdateNotice::NewVersion,
                    UpdateNotice::CheckFailed(e) => SnapshotUpdateNotice::CheckFailed(e.clone()),
                }),
                download: derived.download.cloned(),
            },
            compat: SnapshotCompat {
                report: compat_display.report.cloned(),
                checking: compat_display.checking,
                precaching: compat_display.precaching,
                precache_error: compat_display.precache_error.cloned(),
                precache_done: compat_display.precache_done,
                summary: compat_display.summary,
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

    fn facts(version: Option<&str>) -> DeploymentFacts {
        DeploymentFacts {
            status: crate::dll::DeployStatus::Deployed,
            local_version: version.map(|v| v.to_owned()),
            dlls_present: true,
        }
    }

    fn core() -> AppCore {
        AppCore::new(facts(Some("1.4.7")), false, "Z:/fake/steam/nonexistent")
    }

    // compat 域迁入编排核心（ADR-0019 主盘）

    #[test]
    fn compat_path_changed_emits_flat_probe_and_checking_snapshot() {
        let mut c = core();
        let effects = c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        assert_eq!(
            effects,
            vec![AppEffect::CompatProbe {
                epoch: Epoch(1),
                path: "Z:/fake/steam/nonexistent".to_string(),
            }]
        );
        assert!(c.snapshot().compat.checking);
        assert_eq!(c.snapshot().compat.summary, CompatSummary::Checking);
    }

    #[test]
    fn compat_path_changed_same_path_is_deduped() {
        let mut c = core();
        c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        let effects = c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        assert!(effects.is_empty());
    }

    fn probe_report(
        target: crate::compat::ProbeTarget,
        status: crate::compat::ProbeStatus,
    ) -> crate::compat::ProbeReport {
        crate::compat::ProbeReport {
            target,
            sha256: Some("abc".into()),
            status,
            signature_cached: false,
        }
    }

    fn report_with(
        statuses: [crate::compat::ProbeStatus; 3],
        has_missing_cache: bool,
    ) -> OverallHealthReport {
        use crate::compat::ProbeTarget::*;
        OverallHealthReport {
            steamclient_pattern: probe_report(PatternSteamClient, statuses[0].clone()),
            steamui_pattern: probe_report(PatternSteamUi, statuses[1].clone()),
            steamclient_ipc: probe_report(IpcSteamClient, statuses[2].clone()),
            is_all_compatible: !has_missing_cache,
            has_missing_cache,
        }
    }

    fn all_cached_report() -> OverallHealthReport {
        use crate::compat::ProbeStatus::*;
        report_with(
            [
                RemoteAvailable { cached: true },
                RemoteAvailable { cached: true },
                RemoteAvailable { cached: true },
            ],
            false,
        )
    }

    #[test]
    fn compat_probe_done_ready_report_updates_snapshot_without_effects() {
        let mut c = core();
        c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        let effects = c.step(AppEvent::CompatProbeDone {
            epoch: Epoch(1),
            report: all_cached_report(),
        });
        assert!(effects.is_empty());
        let s = c.snapshot();
        assert!(!s.compat.checking);
        assert_eq!(s.compat.summary, CompatSummary::Ready);
        assert!(s.compat.report.is_some(), "报告须进快照供渲染");
    }

    #[test]
    fn compat_stale_epoch_completions_are_dropped() {
        let mut c = core();
        c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        // 路径再次变更：代数推进到 2，旧代数在途结果全部作废。
        c.step(AppEvent::CompatPathChanged("Z:/other/steam".to_string()));
        assert!(
            c.step(AppEvent::CompatProbeDone {
                epoch: Epoch(1),
                report: all_cached_report(),
            })
            .is_empty()
        );
        assert!(
            c.step(AppEvent::CompatRefreshDone {
                epoch: Epoch(1),
                report: all_cached_report(),
            })
            .is_empty()
        );
        assert!(
            c.step(AppEvent::CompatPrecacheDone {
                epoch: Epoch(1),
                result: Ok(()),
            })
            .is_empty()
        );
        let s = c.snapshot();
        assert!(s.compat.checking, "迟到的旧结果不得改写当前体检状态");
        assert!(s.compat.report.is_none());
    }

    #[test]
    fn compat_probe_done_online_refreshes_then_precaches() {
        use crate::compat::ProbeStatus::*;
        let mut c = core();
        c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        let effects = c.step(AppEvent::CompatProbeDone {
            epoch: Epoch(1),
            report: report_with(
                [
                    RemoteAvailable { cached: false },
                    RemoteAvailable { cached: true },
                    RemoteAvailable { cached: true },
                ],
                true,
            ),
        });
        // 效果序契约：先后台网络刷新，再自动预热（compat_flow 既有语义）。
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            &effects[0],
            AppEffect::CompatRefresh { epoch: Epoch(1), path }
            if path == "Z:/fake/steam/nonexistent"
        ));
        // 三者均为 RemoteAvailable 且签名未缓存（signature_cached=false）→ 全部入预热目标。
        assert!(matches!(
            &effects[1],
            AppEffect::CompatPrecache {
                epoch: Epoch(1),
                targets,
                ..
            } if targets.len() == 3
        ));
        let s = c.snapshot();
        assert_eq!(s.compat.summary, CompatSummary::Online);
        assert!(s.compat.precaching, "自动预热在途");
    }

    #[test]
    fn compat_refresh_done_merges_report_keeping_inflight_precache() {
        use crate::compat::ProbeStatus::*;
        let mut c = core();
        c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        c.step(AppEvent::CompatProbeDone {
            epoch: Epoch(1),
            report: report_with(
                [
                    RemoteAvailable { cached: false },
                    RemoteAvailable { cached: true },
                    RemoteAvailable { cached: true },
                ],
                true,
            ),
        });
        let effects = c.step(AppEvent::CompatRefreshDone {
            epoch: Epoch(1),
            report: all_cached_report(),
        });
        assert!(effects.is_empty(), "合并报告不额外产效果");
        let s = c.snapshot();
        assert!(s.compat.precaching, "刷新不得抹掉在途预热");
        assert!(s.compat.report.is_some());
    }

    #[test]
    fn compat_precache_done_ok_reprobes_within_epoch() {
        use crate::compat::ProbeStatus::*;
        let mut c = core();
        c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        c.step(AppEvent::CompatProbeDone {
            epoch: Epoch(1),
            report: report_with(
                [
                    RemoteAvailable { cached: false },
                    RemoteAvailable { cached: true },
                    RemoteAvailable { cached: true },
                ],
                true,
            ),
        });
        // 预热成功后同一代数复检（刷新预算已消耗 → 不再产网络刷新）。
        assert!(
            matches!(
                c.step(AppEvent::CompatPrecacheDone {
                    epoch: Epoch(1),
                    result: Ok(()),
                })
                .as_slice(),
                [AppEffect::CompatProbe {
                    epoch: Epoch(1),
                    ..
                }]
            ),
            "复检快速探针应产出 CompatProbe"
        );
    }

    #[test]
    fn compat_precache_requested_dedupes_and_requires_report() {
        use crate::compat::ProbeStatus::*;
        // 无报告：手动预热无可预热目标 → 无动作。
        let mut c = core();
        assert!(c.step(AppEvent::CompatPrecacheRequested).is_empty());
        // 报告 Online 且自动预热已在途：手动入口被去重。
        c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        c.step(AppEvent::CompatProbeDone {
            epoch: Epoch(1),
            report: report_with(
                [
                    RemoteAvailable { cached: false },
                    RemoteAvailable { cached: true },
                    RemoteAvailable { cached: true },
                ],
                true,
            ),
        });
        assert!(c.step(AppEvent::CompatPrecacheRequested).is_empty());
        assert!(c.snapshot().compat.precaching);
    }

    #[test]
    fn compat_precache_done_auto_failure_silent_manual_visible() {
        use crate::compat::{CompatError, ProbeStatus::*};
        let mut c = core();
        c.step(AppEvent::CompatPathChanged(
            "Z:/fake/steam/nonexistent".to_string(),
        ));
        c.step(AppEvent::CompatProbeDone {
            epoch: Epoch(1),
            report: report_with(
                [
                    RemoteAvailable { cached: false },
                    RemoteAvailable { cached: true },
                    RemoteAvailable { cached: true },
                ],
                true,
            ),
        });
        assert!(c.snapshot().compat.precaching, "自动预热在途");
        // 自动预热失败：静默（徽章保持 Online、错误不进快照）。
        assert!(
            c.step(AppEvent::CompatPrecacheDone {
                epoch: Epoch(1),
                result: Err(CompatError::Network("auto".into())),
            })
            .is_empty()
        );
        assert!(!c.snapshot().compat.precaching);
        assert!(c.snapshot().compat.precache_error.is_none(), "自动失败静默");
        // 同一报告下手动入口重新触发：失败就地呈现。
        assert!(matches!(
            c.step(AppEvent::CompatPrecacheRequested).as_slice(),
            [AppEffect::CompatPrecache { .. }]
        ));
        assert!(
            c.step(AppEvent::CompatPrecacheDone {
                epoch: Epoch(1),
                result: Err(CompatError::Network("manual".into())),
            })
            .is_empty()
        );
        assert!(
            matches!(
                c.snapshot().compat.precache_error,
                Some(crate::compat::CompatError::Network(ref m)) if m == "manual"
            ),
            "手动失败错误进快照供渲染"
        );
    }

    #[test]
    fn path_commit_feed_chain_is_flat_at_the_seam() {
        let mut c = core();
        // 提交路径：编排效果序固定（先文件事实再体检重探，ADR-0020）。
        let effects = c.step(AppEvent::CommitPath("Z:/fake/steam/committed".into()));
        assert_eq!(
            effects,
            vec![
                AppEffect::RefreshFacts,
                AppEffect::FeedCompatPath("Z:/fake/steam/committed".to_string()),
            ]
        );
        // P5 全链（队列机制在壳执行器）：facts 回喂空效果（FactsRefreshed 无效果）；
        // 体检回喂产快速探针（首次进入 → 代数 1）。
        assert!(
            c.step(AppEvent::FactsRefreshed(facts(Some("1.4.7"))))
                .is_empty()
        );
        assert_eq!(
            c.step(AppEvent::CompatPathChanged(
                "Z:/fake/steam/committed".to_string()
            )),
            vec![AppEffect::CompatProbe {
                epoch: Epoch(1),
                path: "Z:/fake/steam/committed".to_string(),
            }]
        );
    }

    #[test]
    fn compat_summary_covers_pending_missing_network_at_seam() {
        use crate::compat::ProbeStatus::*;
        // 六态在 seam 全覆盖：Checking/Ready/Online 已有专测，这里补缺的三个汇总态。
        let cases: [([crate::compat::ProbeStatus; 3], bool, CompatSummary); 3] = [
            (
                [
                    IncompatiblePending,
                    NetworkError("x".into()),
                    RemoteAvailable { cached: false },
                ],
                true,
                CompatSummary::Pending,
            ),
            (
                [
                    FileNotFound,
                    RemoteAvailable { cached: true },
                    RemoteAvailable { cached: true },
                ],
                false,
                CompatSummary::Missing,
            ),
            (
                [
                    NetworkError("timeout".into()),
                    CompatibleOffline,
                    RemoteAvailable { cached: true },
                ],
                false,
                CompatSummary::Network,
            ),
        ];
        for (statuses, has_missing_cache, expect) in cases {
            let mut c = core();
            c.step(AppEvent::CompatPathChanged(
                "Z:/fake/steam/nonexistent".to_string(),
            ));
            c.step(AppEvent::CompatProbeDone {
                epoch: Epoch(1),
                report: report_with(statuses, has_missing_cache),
            });
            assert_eq!(c.snapshot().compat.summary, expect);
        }
    }

    // 窗口显隐 Steam 联动决策在编排核心（auto-tray）

    #[test]
    fn steam_started_hides_visible_window() {
        let mut c = core();
        let effects = c.step(AppEvent::SteamStarted);
        assert_eq!(effects, vec![AppEffect::SetWindowVisible(false)]);
        assert!(c.snapshot().steam_running);
    }

    #[test]
    fn steam_stopped_restores_hidden_window() {
        let mut c = core();
        c.step(AppEvent::SteamStarted); // 隐窗效果已产出
        c.step(AppEvent::WindowVisibleChanged(false)); // 壳执行后回喂镜像
        let effects = c.step(AppEvent::SteamStopped);
        assert_eq!(effects, vec![AppEffect::SetWindowVisible(true)]);
        assert!(!c.snapshot().steam_running);
    }

    #[test]
    fn steam_started_while_hidden_does_nothing() {
        let mut c = core();
        c.step(AppEvent::WindowVisibleChanged(false));
        let effects = c.step(AppEvent::SteamStarted);
        assert!(effects.is_empty());
        assert!(c.snapshot().steam_running);
    }

    #[test]
    fn steam_stopped_while_visible_does_nothing() {
        let mut c = core();
        let effects = c.step(AppEvent::SteamStopped);
        assert!(effects.is_empty());
        assert!(!c.snapshot().steam_running);
    }

    #[test]
    fn window_visible_change_is_state_only_and_manual_show_not_retracted() {
        let mut c = core();
        // 镜像同步事件：只存状态，不产效果。
        assert!(c.step(AppEvent::WindowVisibleChanged(false)).is_empty());
        assert!(c.step(AppEvent::WindowVisibleChanged(true)).is_empty());
        // auto-tray 首次 Started 隐窗，随后用户从托盘手动 Show：
        c.step(AppEvent::SteamStarted);
        c.step(AppEvent::WindowVisibleChanged(false));
        c.step(AppEvent::WindowVisibleChanged(true)); // 用户显式 Show
        // 没有新边沿：运行中状态回喂（如工作流 rescan）不得把显式 Show 收回。
        let effects = c.step(AppEvent::SteamRunningChanged(true));
        assert!(effects.is_empty(), "显式 Show 后不得被状态回喂收回");
    }

    #[test]
    fn workdone_rescan_hides_once_when_steam_still_running() {
        let mut c = core();
        c.step(AppEvent::WorkflowDone {
            action: Action::ApplyAndLaunch,
            result: Ok(()),
        });
        let effects = c.step(AppEvent::SteamRunningChanged(true));
        assert_eq!(effects, vec![AppEffect::SetWindowVisible(false)]);
        // 同一待决标志只隐一次。
        assert!(c.step(AppEvent::SteamRunningChanged(true)).is_empty());
    }

    #[test]
    fn workdone_rescan_no_hide_when_steam_exited() {
        let mut c = core();
        c.step(AppEvent::WorkflowDone {
            action: Action::UninstallAndRestart,
            result: Ok(()),
        });
        // 工作流后 Steam 已退出（如卸载）：rescan 回喂 false，不含隐窗。
        assert!(c.step(AppEvent::SteamRunningChanged(false)).is_empty());
    }

    #[test]
    fn rescan_without_pending_never_hides() {
        let mut c = core();
        assert!(c.step(AppEvent::SteamRunningChanged(true)).is_empty());
    }

    #[test]
    fn workdone_rescan_no_hide_when_window_already_hidden() {
        let mut c = core();
        // 窗口早已隐藏（如用户手动）：职责最小化，不重复产效果。
        c.step(AppEvent::WindowVisibleChanged(false));
        c.step(AppEvent::WorkflowDone {
            action: Action::Launch,
            result: Ok(()),
        });
        assert!(c.step(AppEvent::SteamRunningChanged(true)).is_empty());
    }

    fn busy_gate_start(c: &mut AppCore, kind: BusyKind) {
        // 通过真实事件进入忙碌（Check → Checking），避免测试依赖私有字段。
        c.step(AppEvent::Check);
        if kind != BusyKind::Checking {
            c.step(AppEvent::Phase(kind));
        }
    }

    #[test]
    fn check_starts_gate_and_spawns_check() {
        let mut c = core();
        let effects = c.step(AppEvent::Check);
        assert_eq!(
            effects,
            vec![AppEffect::RefreshFacts, AppEffect::SpawnUpdateCheck]
        );
        assert_eq!(c.snapshot().busy, Some(BusyKind::Checking));
    }

    #[test]
    fn check_while_busy_is_ignored() {
        let mut c = core();
        c.step(AppEvent::Check);
        let effects = c.step(AppEvent::Check);
        assert!(effects.is_empty());
        assert_eq!(c.snapshot().busy, Some(BusyKind::Checking));
    }

    #[test]
    fn update_checked_ok_clears_gate_and_sets_notice() {
        let mut c = core();
        c.step(AppEvent::Check);
        let effects = c.step(AppEvent::UpdateChecked(Ok(online("1.4.8"))));
        assert!(effects.is_empty());
        let s = c.snapshot();
        assert_eq!(s.busy, None);
        assert!(matches!(s.notice, Some(Notice::UpdateChecked)));
    }

    #[test]
    fn update_checked_failed_reports_notice_and_failed_conclusion() {
        let mut c = core();
        c.step(AppEvent::Check);
        let effects = c.step(AppEvent::UpdateChecked(Err(UpdateError::Network(
            "t".into(),
        ))));
        assert!(effects.is_empty());
        let s = c.snapshot();
        assert_eq!(s.busy, None);
        assert!(matches!(s.notice, Some(Notice::UpdateChecked)));
        assert!(matches!(
            s.update.notice,
            Some(SnapshotUpdateNotice::CheckFailed(UpdateError::Network(_)))
        ));
        assert!(s.update.download.is_none());
    }

    #[test]
    fn download_uses_update_flow_payload_and_starts_gate() {
        let mut c = core();
        c.step(AppEvent::Check);
        c.step(AppEvent::UpdateChecked(Ok(online("1.4.8"))));
        let effects = c.step(AppEvent::Download);
        assert_eq!(
            effects,
            vec![AppEffect::SpawnUpdateDownload {
                info: online("1.4.8")
            }]
        );
        assert_eq!(c.snapshot().busy, Some(BusyKind::Downloading));
    }

    #[test]
    fn download_without_checked_result_is_ignored() {
        let mut c = core();
        let effects = c.step(AppEvent::Download);
        assert!(effects.is_empty());
        assert_eq!(c.snapshot().busy, None);
    }

    #[test]
    fn downloaded_ok_refreshes_facts_and_clears_gate() {
        let mut c = core();
        c.step(AppEvent::Check);
        c.step(AppEvent::UpdateChecked(Ok(online("1.4.8"))));
        c.step(AppEvent::Download);
        let effects = c.step(AppEvent::Downloaded(Ok(())));
        assert_eq!(effects, vec![AppEffect::RefreshFacts]);
        assert_eq!(c.snapshot().busy, None);
        assert!(matches!(
            c.snapshot().notice,
            Some(Notice::Downloaded(Ok(())))
        ));
    }

    #[test]
    fn downloaded_err_keeps_notice_without_refreshing() {
        let mut c = core();
        c.step(AppEvent::Check);
        c.step(AppEvent::UpdateChecked(Ok(online("1.4.8"))));
        c.step(AppEvent::Download);
        let effects = c.step(AppEvent::Downloaded(Err(UpdateError::Network("t".into()))));
        assert!(effects.is_empty());
        assert_eq!(c.snapshot().busy, None);
        assert!(matches!(
            c.snapshot().notice,
            Some(Notice::Downloaded(Err(_)))
        ));
    }

    #[test]
    fn workdone_ok_refreshes_facts_then_rescans() {
        let mut c = core();
        let effects = c.step(AppEvent::WorkflowDone {
            action: Action::ApplyAndLaunch,
            result: Ok(()),
        });
        assert_eq!(
            effects,
            vec![AppEffect::RefreshFacts, AppEffect::RescanSteam]
        );
        assert_eq!(c.snapshot().busy, None);
        assert!(matches!(
            c.snapshot().notice,
            Some(Notice::WorkflowDone(Action::ApplyAndLaunch, Ok(())))
        ));
    }

    #[test]
    fn workdone_err_rescans_but_does_not_refresh() {
        let mut c = core();
        let err = workflow::WorkflowError {
            op: Op::Deploy,
            message: "boom".into(),
        };
        let effects = c.step(AppEvent::WorkflowDone {
            action: Action::ApplyAndLaunch,
            result: Err(err.clone()),
        });
        assert_eq!(effects, vec![AppEffect::RescanSteam]);
        assert_eq!(c.snapshot().busy, None);
        assert!(matches!(
            c.snapshot().notice,
            Some(Notice::WorkflowDone(Action::ApplyAndLaunch, Err(ref e))) if e.message == "boom"
        ));
    }

    #[test]
    fn phase_replaces_busy_kind_while_busy() {
        let mut c = core();
        busy_gate_start(&mut c, BusyKind::Deploying);
        let effects = c.step(AppEvent::Phase(BusyKind::Launching));
        assert!(effects.is_empty());
        assert_eq!(c.snapshot().busy, Some(BusyKind::Launching));
    }

    #[test]
    fn action_requested_plans_apply_and_launch_without_steam() {
        let mut c = core();
        let effects = c.step(AppEvent::ActionRequested(Action::ApplyAndLaunch));
        assert_eq!(
            effects,
            vec![AppEffect::RunPlan {
                action: Action::ApplyAndLaunch,
                kill_first: false,
                steam_dir: PathBuf::from("Z:/fake/steam/nonexistent")
            }]
        );
        assert_eq!(c.snapshot().confirm, None);
    }

    #[test]
    fn action_requested_apply_launch_with_running_steam_forces_kill_first() {
        let mut c = core();
        c.step(AppEvent::SteamRunningChanged(true));
        let effects = c.step(AppEvent::ActionRequested(Action::ApplyAndLaunch));
        assert_eq!(
            effects,
            vec![AppEffect::RunPlan {
                action: Action::ApplyAndLaunch,
                kill_first: true,
                steam_dir: PathBuf::from("Z:/fake/steam/nonexistent")
            }]
        );
    }

    #[test]
    fn uninstall_actions_confirmed_before_planning_when_steam_running() {
        let mut c = core();
        c.step(AppEvent::SteamRunningChanged(true));
        let effects = c.step(AppEvent::ActionRequested(Action::ExitAndUninstall));
        assert!(effects.is_empty());
        assert_eq!(c.snapshot().confirm, Some(Action::ExitAndUninstall));
    }

    #[test]
    fn action_confirmed_runs_with_kill_first() {
        let mut c = core();
        c.step(AppEvent::SteamRunningChanged(true));
        c.step(AppEvent::ActionRequested(Action::ExitAndUninstall));
        let effects = c.step(AppEvent::ActionConfirmed(Action::ExitAndUninstall));
        assert_eq!(
            effects,
            vec![AppEffect::RunPlan {
                action: Action::ExitAndUninstall,
                kill_first: true,
                steam_dir: PathBuf::from("Z:/fake/steam/nonexistent")
            }]
        );
        assert_eq!(c.snapshot().confirm, None, "确认后弹窗落下");
    }

    #[test]
    fn quiet_action_never_confirms_even_when_steam_running() {
        let mut c = core();
        c.step(AppEvent::SteamRunningChanged(true));
        let effects = c.step(AppEvent::ActionRequestedQuiet(Action::ExitAndUninstall));
        assert_eq!(
            effects,
            vec![AppEffect::RunPlan {
                action: Action::ExitAndUninstall,
                kill_first: false,
                steam_dir: PathBuf::from("Z:/fake/steam/nonexistent")
            }]
        );
        assert_eq!(c.snapshot().confirm, None);
    }

    #[test]
    fn confirm_canceled_clears_dialog() {
        let mut c = core();
        c.step(AppEvent::SteamRunningChanged(true));
        c.step(AppEvent::ActionRequested(Action::UninstallAndRestart));
        let effects = c.step(AppEvent::ConfirmCanceled);
        assert!(effects.is_empty());
        assert_eq!(c.snapshot().confirm, None);
    }

    #[test]
    fn action_requested_while_busy_is_ignored() {
        let mut c = core();
        busy_gate_start(&mut c, BusyKind::Deploying);
        let effects = c.step(AppEvent::ActionRequested(Action::Launch));
        assert!(effects.is_empty());
        assert_eq!(c.snapshot().confirm, None);
    }

    #[test]
    fn plan_ok_starts_gate_and_spawns_workflow() {
        let mut c = core();
        // 先进确认流，验证 PlanOk 落下弹窗。
        c.step(AppEvent::SteamRunningChanged(true));
        c.step(AppEvent::ActionRequested(Action::ExitAndUninstall));
        let effects = c.step(AppEvent::PlanOk {
            action: Action::ExitAndUninstall,
            ops: vec![Op::Uninstall],
        });
        assert_eq!(
            effects,
            vec![AppEffect::SpawnWorkflow {
                action: Action::ExitAndUninstall,
                ops: vec![Op::Uninstall],
                steam_dir: PathBuf::from("Z:/fake/steam/nonexistent")
            }]
        );
        assert_eq!(c.snapshot().confirm, None);
        assert_eq!(c.snapshot().busy, Some(BusyKind::Uninstalling));
    }

    #[test]
    fn plan_ok_refused_by_busy_gate_drops_workflow() {
        let mut c = core();
        busy_gate_start(&mut c, BusyKind::Deploying);
        let effects = c.step(AppEvent::PlanOk {
            action: Action::Launch,
            ops: vec![Op::Launch],
        });
        assert!(effects.is_empty());
        assert_eq!(c.snapshot().busy, Some(BusyKind::Deploying));
    }

    #[test]
    fn plan_rejected_sets_precheck_notice_and_refreshes() {
        let mut c = core();
        let effects = c.step(AppEvent::PlanRejected(
            workflow::Precheck::MissingTargetDlls,
        ));
        assert_eq!(effects, vec![AppEffect::RefreshFacts]);
        assert!(matches!(
            c.snapshot().notice,
            Some(Notice::Precheck(workflow::Precheck::MissingTargetDlls))
        ));
    }

    #[test]
    fn facts_refreshed_updates_snapshot_and_derived_notice() {
        let mut c = core();
        c.step(AppEvent::Check);
        c.step(AppEvent::UpdateChecked(Ok(online("1.4.8"))));
        c.step(AppEvent::FactsRefreshed(facts(Some("1.4.8"))));
        let s = c.snapshot();
        assert_eq!(s.update.notice, Some(SnapshotUpdateNotice::UpToDate));
        assert!(s.update.download.is_none());
    }

    #[test]
    fn facts_refreshed_to_older_version_reopens_download() {
        let mut c = core();
        c.step(AppEvent::Check);
        c.step(AppEvent::UpdateChecked(Ok(online("1.4.8"))));
        c.step(AppEvent::FactsRefreshed(facts(Some("1.4.7"))));
        let s = c.snapshot();
        assert_eq!(s.update.notice, Some(SnapshotUpdateNotice::NewVersion));
        assert_eq!(
            s.update.download.as_ref().map(|i| i.version.as_str()),
            Some("1.4.8")
        );
    }

    #[test]
    fn check_refreshes_facts_first_so_missing_dlls_never_report_up_to_date() {
        // 回归（会话内删 dlls/ 后点检查更新）：Check 先发 RefreshFacts——即便磁盘事实
        // 刷新晚于检查完成，也由 FactsRefreshed 喂回后派生（ADR-0011/#26：文件缺失不可落「已是最新」）。
        let mut c = AppCore::new(
            DeploymentFacts {
                status: crate::dll::DeployStatus::Deployed,
                local_version: Some("1.4.8".into()),
                dlls_present: true,
            },
            false,
            "Z:/fake/steam/nonexistent",
        );
        let effects = c.step(AppEvent::Check);
        assert_eq!(
            effects,
            vec![AppEffect::RefreshFacts, AppEffect::SpawnUpdateCheck]
        );
        // 检查发起后、磁盘探测尚未回喂时：仍以旧快照派生（已是最新）——中间态允许；
        c.step(AppEvent::UpdateChecked(Ok(online("1.4.8"))));
        // 磁盘探测回喂（dlls 已删除）→ 派生必须翻转为 NewVersion + 可下载。
        c.step(AppEvent::FactsRefreshed(DeploymentFacts {
            status: crate::dll::DeployStatus::Deployed,
            local_version: None,
            dlls_present: false,
        }));
        let s = c.snapshot();
        assert_eq!(s.update.notice, Some(SnapshotUpdateNotice::NewVersion));
        assert!(
            s.update.download.is_some(),
            "文件缺失时下载按钮必须出现（修复动作）"
        );
    }

    #[test]
    fn steam_running_changed_is_state_only() {
        let mut c = core();
        let effects = c.step(AppEvent::SteamRunningChanged(true));
        assert!(effects.is_empty());
        assert!(c.snapshot().steam_running);
        let effects = c.step(AppEvent::SteamRunningChanged(false));
        assert!(effects.is_empty());
        assert!(!c.snapshot().steam_running);
    }

    #[test]
    fn snapshot_defaults_for_fresh_core() {
        let c = core();
        let s = c.snapshot();
        assert_eq!(s.busy, None);
        assert_eq!(s.confirm, None);
        assert!(s.notice.is_none());
        assert!(s.update.notice.is_none());
        assert!(s.update.download.is_none());
    }

    #[test]
    fn commit_path_trims_stores_and_orchestrates() {
        let mut c = AppCore::new(facts(Some("1.4.7")), false, "Z:/fake/steam/nonexistent");
        let effects = c.step(AppEvent::CommitPath(
            "  Z:/fake/steam/committed  ".to_string(),
        ));
        // 效果顺序契约：先刷文件事实再重探体检（ADR-0020）。
        assert_eq!(
            effects,
            vec![
                AppEffect::RefreshFacts,
                AppEffect::FeedCompatPath("Z:/fake/steam/committed".to_string())
            ]
        );
        assert_eq!(c.snapshot().steam_path, "Z:/fake/steam/committed");
    }

    #[test]
    fn commit_path_only_mutates_working_path() {
        let mut c = core();
        // 其它域状态不受提交影响：门禁/通知/确认与更新派生保持原样。
        c.step(AppEvent::Check);
        c.step(AppEvent::CommitPath("Z:/fake/steam/committed".to_string()));
        let s = c.snapshot();
        assert_eq!(s.steam_path, "Z:/fake/steam/committed");
        assert_eq!(s.busy, Some(BusyKind::Checking));
    }

    #[test]
    fn later_non_path_event_keeps_committed_path() {
        let mut c = core();
        c.step(AppEvent::CommitPath("Z:/fake/steam/committed".to_string()));
        // 提交后的普通事件（如事实刷新）不得抹掉已提交的工作路径。
        c.step(AppEvent::FactsRefreshed(facts(Some("1.4.7"))));
        assert_eq!(c.snapshot().steam_path, "Z:/fake/steam/committed");
    }

    #[test]
    fn trim_only_commit_stores_trimmed_value() {
        // 与当前值仅差首尾空白的提交：仍触发编排（壳侧 submit 已按 trim 后相等判 Unchanged 过滤，
        // 此处防御核心契约——CommitPath 到达即无条件产效果）。
        let mut c = AppCore::new(facts(Some("1.4.7")), false, "Z:/fake/steam/nonexistent");
        let effects = c.step(AppEvent::CommitPath(
            "  Z:/fake/steam/nonexistent  ".to_string(),
        ));
        assert_eq!(
            effects,
            vec![
                AppEffect::RefreshFacts,
                AppEffect::FeedCompatPath("Z:/fake/steam/nonexistent".to_string())
            ]
        );
        assert_eq!(c.snapshot().steam_path, "Z:/fake/steam/nonexistent");
    }

    #[test]
    fn run_plan_flow_end_to_end() {
        let mut c = core();
        let evts = vec![
            AppEvent::ActionRequested(Action::ApplyAndLaunch),
            AppEvent::PlanOk {
                action: Action::ApplyAndLaunch,
                ops: vec![Op::Deploy, Op::Launch],
            },
            AppEvent::Phase(BusyKind::Launching),
        ];
        let mut all = Vec::new();
        for e in evts {
            all.extend(c.step(e));
        }
        assert_eq!(
            all,
            vec![
                AppEffect::RunPlan {
                    action: Action::ApplyAndLaunch,
                    kill_first: false,
                    steam_dir: PathBuf::from("Z:/fake/steam/nonexistent")
                },
                AppEffect::SpawnWorkflow {
                    action: Action::ApplyAndLaunch,
                    ops: vec![Op::Deploy, Op::Launch],
                    steam_dir: PathBuf::from("Z:/fake/steam/nonexistent")
                },
            ]
        );
    }
}
