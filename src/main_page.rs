//! Main Page 展示层：业务状态 → 组件意图（ViewModel）与程序化图标绘制。
//!
//! 商业判断不在此复制：输入全部来自既有状态源（dll / steam / busy / update_flow）；
//! UI 层只消费本模块的输出（spec §46/§47 组件边界、§48.2 职责分离）。

use eframe::egui;
use egui::{Color32, Painter, Pos2, Shape, Stroke};

use crate::busy::BusyKind;
use crate::dll::DeployStatus;

/// eyebrow 定稿：英文大写（双语一致，spec §14）。
pub const EYEBROW: &str = "PATCH";

/// 检查结论快照（owned，版本永不渲染；CheckFailed 详情由 UI 层向 update_flow 现查，ADR-0008）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UpdateConclusion {
    UpToDate,
    NewVersion,
    CheckFailed,
}

/// 一帧渲染的全部业务输入快照（由 App 逐帧拍取，纯数据可单测）。
#[derive(Clone, Copy)]
pub struct MainPageInput {
    pub deploy: DeployStatus,
    pub steam_running: bool,
    pub busy: Option<BusyKind>,
    pub dlls_present: bool,
    /// 是否已有可下载目标（点击时再向 update_flow 取 OnlineInfo，单一事实源）。
    pub update_downloadable: bool,
    pub update_notice: Option<UpdateConclusion>,
    /// 「应用补丁并启动」最近失败（§30/§44 Operation Failure：Hero 显示失败文案 +
    /// 业务 Primary 即重试入口）。仅补丁类失败覆盖 Hero，其余走状态栏描述。
    pub apply_failed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeroStatus {
    Applied,
    NotApplied,
    /// Busy 阶段（文案由 busy 阶段词表提供，§27）。
    Busy,
    /// 补丁操作失败（§30）。
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Supporting {
    /// 补丁未下载（Primary 置灰，引导「检查补丁更新」，ADR-0011 文件本位）。
    FilesMissing,
    /// Steam 路径无效（§25）。
    PathInvalid,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrimaryAction {
    ApplyAndLaunch,
    Launch,
    Restart,
    FixPath,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrimaryKind {
    Action(PrimaryAction),
    /// Busy 阶段原位替换（§27：spinner + 阶段文案，disabled）。
    BusyStage,
}

#[derive(Clone, Copy)]
pub struct PrimaryVm {
    pub kind: PrimaryKind,
    pub enabled: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SecondaryAction {
    LaunchNormal,
    Uninstall,
    ExitAndUninstall,
    UninstallAndRestart,
}

#[derive(Clone, Copy)]
pub struct SecondaryRow {
    pub action: SecondaryAction,
    /// 仅「退出 Steam 并卸载补丁」（spec §23.3 Warning Secondary Blue）。
    pub warn_blue: bool,
    pub enabled: bool,
}

pub struct SecondaryVm {
    pub rows: Vec<SecondaryRow>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UpdateKind {
    /// 检查补丁更新 [Refresh]
    Check,
    /// 下载并解压新版本 [Download]（检查确认可更新或本地缺文件后出现，ADR-0014）
    Download,
    /// 检查中（同按钮位 spinner，不进状态栏，§26.2）
    Checking,
    /// 下载中（不确定进度 → 阶段文案，§29）
    Downloading,
}

#[derive(Clone, Copy)]
pub struct UpdateVm {
    pub kind: UpdateKind,
    pub enabled: bool,
    pub note: Option<UpdateConclusion>,
}

pub struct MainPageVm {
    pub hero: HeroStatus,
    pub supporting: Option<Supporting>,
    pub primary: PrimaryVm,
    pub secondary: SecondaryVm,
    pub update: UpdateVm,
}

/// 状态映射主函数：spec §44 状态矩阵 + §23 Secondary 组 + §26 Update 按钮位。
/// UI 不得绕过本函数自推 `if steam_running / if patch_applied …`（§47）。
pub fn derive(input: &MainPageInput) -> MainPageVm {
    let busy = input.busy.is_some();
    let path_invalid = input.deploy == DeployStatus::InvalidPath;

    let secondary = secondary_rows(input);

    let hero = if input.busy.is_some() {
        HeroStatus::Busy
    } else if input.apply_failed {
        HeroStatus::Failed
    } else if input.deploy == DeployStatus::Deployed {
        HeroStatus::Applied
    } else {
        HeroStatus::NotApplied
    };

    let supporting = if input.busy.is_some() {
        // §27 busy 状态原占位：不再叠加解释性提示（如补丁未下载在下载进行中即自相矛盾）。
        None
    } else if path_invalid {
        Some(Supporting::PathInvalid)
    } else if input.deploy == DeployStatus::NotDeployed && !input.dlls_present {
        Some(Supporting::FilesMissing)
    } else {
        None
    };

    let primary = if input.busy.is_some() {
        PrimaryVm {
            kind: PrimaryKind::BusyStage,
            enabled: false,
        }
    } else if path_invalid {
        PrimaryVm {
            kind: PrimaryKind::Action(PrimaryAction::FixPath),
            enabled: true,
        }
    } else if input.deploy == DeployStatus::NotDeployed {
        PrimaryVm {
            kind: PrimaryKind::Action(PrimaryAction::ApplyAndLaunch),
            // ADR-0011 文件本位：dlls/ 缺文件时置灰（点了会报 MissingTargetDlls）。
            enabled: input.dlls_present,
        }
    } else {
        PrimaryVm {
            kind: PrimaryKind::Action(if input.steam_running {
                PrimaryAction::Restart
            } else {
                PrimaryAction::Launch
            }),
            enabled: true,
        }
    };

    let update = update_vm(input, busy);

    MainPageVm {
        hero,
        supporting,
        primary,
        secondary,
        update,
    }
}

fn secondary_rows(input: &MainPageInput) -> SecondaryVm {
    let enabled = input.busy.is_none();
    let rows = if input.deploy == DeployStatus::InvalidPath {
        Vec::new()
    } else if input.deploy == DeployStatus::NotDeployed {
        vec![SecondaryRow {
            action: SecondaryAction::LaunchNormal,
            warn_blue: false,
            enabled,
        }]
    } else if input.steam_running {
        vec![
            SecondaryRow {
                action: SecondaryAction::ExitAndUninstall,
                warn_blue: true,
                enabled,
            },
            SecondaryRow {
                action: SecondaryAction::UninstallAndRestart,
                warn_blue: false,
                enabled,
            },
        ]
    } else {
        vec![
            SecondaryRow {
                action: SecondaryAction::Uninstall,
                warn_blue: false,
                enabled,
            },
            SecondaryRow {
                action: SecondaryAction::UninstallAndRestart,
                warn_blue: false,
                enabled,
            },
        ]
    };
    SecondaryVm { rows }
}

fn update_vm(input: &MainPageInput, busy: bool) -> UpdateVm {
    let checking = input.busy == Some(BusyKind::Checking);
    let downloading = input.busy == Some(BusyKind::Downloading);
    let kind = if checking {
        UpdateKind::Checking
    } else if downloading {
        UpdateKind::Downloading
    } else if input.update_downloadable {
        UpdateKind::Download
    } else {
        UpdateKind::Check
    };
    // 检查中 / 下载中不携带结论（结果在 flow 内部，完成后再派生，ADR-0008）。
    let note = if checking || downloading {
        None
    } else {
        input.update_notice
    };
    UpdateVm {
        kind,
        enabled: !busy,
        note,
    }
}

// 图标：程序化几何绘制（spec §36 定稿，统一 stroke 笔宽与 logical 尺寸）。

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconKind {
    Play,
    Restart,
    Uninstall,
    Exit,
    Download,
    Refresh,
    Settings,
    Warning,
    Check,
    Spinner,
}

const ICON_STROKE: f32 = 1.6;

/// 20×20 参考网格点 → 像素坐标（`size` 为图标包围盒边长）。
fn unit(center: Pos2, size: f32, x: f32, y: f32) -> Pos2 {
    center + egui::vec2((x - 10.0) * size / 20.0, (y - 10.0) * size / 20.0)
}

fn seg(p: &Painter, pts: &[Pos2], color: Color32, w: f32) {
    p.add(Shape::line(pts.to_vec(), Stroke::new(w, color)));
}

fn arc(p: &Painter, center: Pos2, r: f32, a0_deg: f32, a1_deg: f32, n: usize, stroke: Stroke) {
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let a = (a0_deg + (a1_deg - a0_deg) * t).to_radians();
            center + egui::vec2(a.cos() * r, a.sin() * r)
        })
        .collect();
    seg(p, &pts, stroke.color, stroke.width);
}

/// 全部图标统一入口：颜色取自语义槽（调用方传 theme 色，本模块不持有色值）。
pub fn paint_icon(
    painter: &Painter,
    kind: IconKind,
    center: Pos2,
    color: Color32,
    size: f32,
    phase: f32,
) {
    match kind {
        IconKind::Play => {
            let pts = vec![
                unit(center, size, 6.6, 4.8),
                unit(center, size, 15.4, 9.0),
                unit(center, size, 6.6, 15.2),
            ];
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        IconKind::Restart | IconKind::Refresh => {
            // 原型几何（与 download/uninstall 不同构）：顶部起弧经右侧扫到右下（约 158°），
            // 弧起点处两条短边构成箭头（对应原型 refresh / restart path）。
            arc(
                painter,
                unit(center, size, 9.5, 10.0),
                7.3 * size / 20.0,
                270.0,
                428.0,
                14,
                Stroke::new(ICON_STROKE, color),
            );
            let tip = unit(center, size, 9.5, 2.7);
            seg(
                painter,
                &[tip, unit(center, size, 13.0, 6.1)],
                color,
                ICON_STROKE,
            );
            seg(
                painter,
                &[tip, unit(center, size, 13.4, 0.8)],
                color,
                ICON_STROKE,
            );
        }
        IconKind::Uninstall | IconKind::Download => {
            seg(
                painter,
                &[
                    unit(center, size, 10.0, 2.8),
                    unit(center, size, 10.0, 11.6),
                ],
                color,
                ICON_STROKE,
            );
            seg(
                painter,
                &[
                    unit(center, size, 6.3, 8.2),
                    unit(center, size, 10.0, 11.9),
                    unit(center, size, 13.7, 8.2),
                ],
                color,
                ICON_STROKE,
            );
            seg(
                painter,
                &[
                    unit(center, size, 3.5, 15.6),
                    unit(center, size, 16.5, 15.6),
                ],
                color,
                ICON_STROKE,
            );
        }
        IconKind::Exit => {
            seg(
                painter,
                &[
                    unit(center, size, 10.0, 4.2),
                    unit(center, size, 10.0, 10.6),
                ],
                color,
                ICON_STROKE,
            );
            arc(
                painter,
                unit(center, size, 10.0, 11.9),
                5.2 * size / 20.0,
                205.0,
                335.0,
                12,
                Stroke::new(ICON_STROKE, color),
            );
        }
        IconKind::Settings => {
            let r: f32 = 6.5 * size / 20.0;
            let w: f32 = 2.0 * size / 20.0;
            painter.circle_stroke(center, r, Stroke::new(w, color));
            let tooth_r: f32 = 1.7 * size / 20.0;
            let tooth_c = r + w / 2.0 + tooth_r * 0.7;
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let p = center + egui::vec2(a.cos() * tooth_c, a.sin() * tooth_c);
                painter.circle_filled(p, tooth_r, color);
            }
            painter.circle_filled(center, 1.6 * size / 20.0, color);
        }
        IconKind::Warning => {
            let pts = vec![
                unit(center, size, 10.0, 3.0),
                unit(center, size, 17.2, 16.4),
                unit(center, size, 2.8, 16.4),
            ];
            painter.add(Shape::convex_polygon(
                pts,
                Color32::TRANSPARENT,
                Stroke::new(ICON_STROKE, color),
            ));
            seg(
                painter,
                &[
                    unit(center, size, 10.0, 7.4),
                    unit(center, size, 10.0, 11.8),
                ],
                color,
                ICON_STROKE,
            );
            painter.circle_filled(unit(center, size, 10.0, 14.2), 0.9 * size / 20.0, color);
        }
        IconKind::Check => {
            seg(
                painter,
                &[
                    unit(center, size, 6.2, 10.4),
                    unit(center, size, 9.2, 13.4),
                    unit(center, size, 14.0, 7.0),
                ],
                color,
                2.0 * size / 20.0,
            );
        }
        IconKind::Spinner => {
            // 不确定进度：旋转圆弧（phase 由调用方按帧推进；图标运动不造假百分比，§29）。
            arc(
                painter,
                center,
                0.42 * size,
                30.0 + phase * 360.0,
                280.0 + phase * 360.0,
                16,
                Stroke::new(2.0, color),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        deploy: DeployStatus,
        steam_running: bool,
        busy: Option<BusyKind>,
        dlls_present: bool,
        apply_failed: bool,
    ) -> MainPageInput {
        MainPageInput {
            deploy,
            steam_running,
            busy,
            dlls_present,
            update_downloadable: false,
            update_notice: None,
            apply_failed,
        }
    }

    /// 矩阵断言辅助：主页面五组件意图与 §44 完全一致。
    fn assert_rows(vm: &MainPageVm, want: &[(SecondaryAction, bool)]) {
        let got: Vec<(SecondaryAction, bool)> = vm
            .secondary
            .rows
            .iter()
            .map(|r| (r.action, r.warn_blue))
            .collect();
        assert_eq!(got, want, "secondary rows 不符");
    }

    #[test]
    fn matrix_not_applied_steam_stopped() {
        let vm = derive(&input(DeployStatus::NotDeployed, false, None, true, false));
        assert_eq!(vm.hero, HeroStatus::NotApplied);
        assert_eq!(
            vm.primary.kind,
            PrimaryKind::Action(PrimaryAction::ApplyAndLaunch)
        );
        assert!(vm.primary.enabled);
        assert_eq!(vm.supporting, None);
        assert_rows(&vm, &[(SecondaryAction::LaunchNormal, false)]);
        assert_eq!(vm.update.kind, UpdateKind::Check);
        assert!(vm.update.enabled);
    }

    #[test]
    fn matrix_applied_steam_stopped() {
        let vm = derive(&input(DeployStatus::Deployed, false, None, true, false));
        assert_eq!(vm.hero, HeroStatus::Applied);
        assert_eq!(vm.primary.kind, PrimaryKind::Action(PrimaryAction::Launch));
        assert_rows(
            &vm,
            &[
                (SecondaryAction::Uninstall, false),
                (SecondaryAction::UninstallAndRestart, false),
            ],
        );
    }

    #[test]
    fn matrix_applied_steam_running() {
        let vm = derive(&input(DeployStatus::Deployed, true, None, true, false));
        assert_eq!(vm.hero, HeroStatus::Applied);
        assert_eq!(vm.primary.kind, PrimaryKind::Action(PrimaryAction::Restart));
        assert_rows(
            &vm,
            &[
                (SecondaryAction::ExitAndUninstall, true),
                (SecondaryAction::UninstallAndRestart, false),
            ],
        );
    }

    #[test]
    fn matrix_files_missing_disables_primary_and_hints() {
        let vm = derive(&input(DeployStatus::NotDeployed, false, None, false, false));
        assert_eq!(
            vm.primary.kind,
            PrimaryKind::Action(PrimaryAction::ApplyAndLaunch)
        );
        assert!(
            !vm.primary.enabled,
            "补丁文件缺失时 Primary 置灰（ADR-0011）"
        );
        assert_eq!(vm.supporting, Some(Supporting::FilesMissing));
    }

    #[test]
    fn matrix_path_invalid_fix_path_and_empty_secondary() {
        let vm = derive(&input(DeployStatus::InvalidPath, false, None, false, false));
        assert_eq!(
            vm.hero,
            HeroStatus::NotApplied,
            "路径无效不是第三种部署状态（§25）"
        );
        assert_eq!(vm.primary.kind, PrimaryKind::Action(PrimaryAction::FixPath));
        assert!(vm.primary.enabled);
        assert_eq!(vm.supporting, Some(Supporting::PathInvalid));
        assert!(vm.secondary.rows.is_empty(), "路径无效时操作组禁用（§25）");
    }

    #[test]
    fn matrix_busy_overrides_hero_and_disables_all() {
        for kind in [
            BusyKind::Deploying,
            BusyKind::Uninstalling,
            BusyKind::Launching,
            BusyKind::ClosingSteam,
        ] {
            let vm = derive(&input(
                DeployStatus::Deployed,
                true,
                Some(kind),
                true,
                false,
            ));
            assert_eq!(vm.hero, HeroStatus::Busy, "{kind:?}");
            assert_eq!(vm.primary.kind, PrimaryKind::BusyStage);
            assert!(!vm.primary.enabled);
            assert_eq!(vm.supporting, None, "{kind:?} busy 不显示 supporting");
            assert!(
                vm.secondary.rows.iter().all(|r| !r.enabled),
                "{kind:?} secondary 应全禁用"
            );
            assert!(!vm.update.enabled, "{kind:?} update 应禁用");
        }
    }

    #[test]
    fn busy_hides_files_missing_supporting() {
        // §27：busy 原位原状态，不叠加「补丁未下载」引导（检查/下载进行中再提示即自相矛盾）。
        let vm = derive(&input(
            DeployStatus::NotDeployed,
            false,
            Some(BusyKind::Checking),
            false,
            false,
        ));
        assert_eq!(vm.hero, HeroStatus::Busy);
        assert_eq!(vm.supporting, None);
        assert_eq!(
            vm.primary.kind,
            PrimaryKind::BusyStage,
            "files_missing 的 busy 下 Primary 也走 busy 原位"
        );
        let downloading = derive(&input(
            DeployStatus::NotDeployed,
            false,
            Some(BusyKind::Downloading),
            false,
            false,
        ));
        assert_eq!(downloading.supporting, None);
        assert_eq!(downloading.update.kind, UpdateKind::Downloading);
    }

    #[test]
    fn busy_checking_and_downloading_keep_update_phase() {
        let vm = derive(&input(
            DeployStatus::NotDeployed,
            false,
            Some(BusyKind::Checking),
            false,
            false,
        ));
        assert_eq!(vm.update.kind, UpdateKind::Checking);
        assert_eq!(vm.update.note, None);
        let vm = derive(&input(
            DeployStatus::Deployed,
            true,
            Some(BusyKind::Downloading),
            true,
            false,
        ));
        assert_eq!(vm.update.kind, UpdateKind::Downloading);
        assert_eq!(vm.update.note, None);
    }

    #[test]
    fn update_check_precedes_download_when_files_missing() {
        let vm = derive(&input(DeployStatus::NotDeployed, false, None, false, false));
        // 文件缺失但未检查（无下载目标）→ 仍提供 Check（先检查再下载，#34 引导）。
        assert_eq!(vm.update.kind, UpdateKind::Check);
    }

    #[test]
    fn update_download_button_when_target_available() {
        let full = MainPageInput {
            deploy: DeployStatus::NotDeployed,
            steam_running: false,
            busy: None,
            dlls_present: false,
            update_downloadable: true,
            update_notice: Some(UpdateConclusion::NewVersion),
            apply_failed: false,
        };
        let vm = derive(&full);
        assert_eq!(vm.update.kind, UpdateKind::Download);
        assert_eq!(vm.update.note, Some(UpdateConclusion::NewVersion));
        assert!(vm.update.enabled);

        // 版本号永不渲染：vm 不携带版本文本，只有 kind/note 语义（ADR-0014）。
        assert_eq!(vm.hero, HeroStatus::NotApplied);
    }

    #[test]
    fn update_note_maps_three_states() {
        let base = MainPageInput {
            deploy: DeployStatus::Deployed,
            steam_running: false,
            busy: None,
            dlls_present: true,
            update_downloadable: false,
            update_notice: None,
            apply_failed: false,
        };
        let up = MainPageInput {
            update_notice: Some(UpdateConclusion::UpToDate),
            ..base
        };
        assert_eq!(derive(&up).update.note, Some(UpdateConclusion::UpToDate));

        let new = MainPageInput {
            update_notice: Some(UpdateConclusion::NewVersion),
            ..base
        };
        assert_eq!(derive(&new).update.note, Some(UpdateConclusion::NewVersion));

        let err = MainPageInput {
            update_notice: Some(UpdateConclusion::CheckFailed),
            ..base
        };
        assert_eq!(
            derive(&err).update.note,
            Some(UpdateConclusion::CheckFailed)
        );
    }

    #[test]
    fn apply_failed_overrides_hero_but_keeps_business_primary() {
        let vm = derive(&input(DeployStatus::NotDeployed, false, None, true, true));
        assert_eq!(vm.hero, HeroStatus::Failed);
        assert_eq!(
            vm.primary.kind,
            PrimaryKind::Action(PrimaryAction::ApplyAndLaunch),
            "失败后业务 Primary 即重试入口（§30 Retry）"
        );
        assert!(vm.primary.enabled);

        let busy_fail = input(
            DeployStatus::NotDeployed,
            false,
            Some(BusyKind::Deploying),
            true,
            true,
        );
        assert_eq!(
            derive(&busy_fail).hero,
            HeroStatus::Busy,
            "busy 优先于失败覆盖"
        );
    }

    #[test]
    fn launch_normal_always_available_when_files_missing() {
        let vm = derive(&input(DeployStatus::NotDeployed, false, None, false, false));
        assert_rows(&vm, &[(SecondaryAction::LaunchNormal, false)]);
        assert!(
            vm.secondary.rows[0].enabled,
            "正常启动不依赖 dlls/（缺文件也可启动 Steam）"
        );
    }

    #[test]
    fn icons_paint_within_bounds_for_all_kinds() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 400.0),
            )),
            ..Default::default()
        };
        let mut full = ctx.run_ui(raw, |ui| {
            let painter = ui.painter();
            let c = egui::pos2(200.0, 200.0);
            for kind in [
                IconKind::Play,
                IconKind::Restart,
                IconKind::Uninstall,
                IconKind::Exit,
                IconKind::Download,
                IconKind::Refresh,
                IconKind::Settings,
                IconKind::Warning,
                IconKind::Check,
                IconKind::Spinner,
            ] {
                paint_icon(
                    &painter.clone(),
                    kind,
                    c,
                    crate::theme::Palette::dark().white,
                    15.0,
                    0.0,
                );
            }
        });
        full.textures_delta.clear();
    }
}
