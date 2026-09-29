//! 「操作」模块：组合动作的判定表与顺序执行（`plan` / `execute`）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::busy::BusyKind;
use crate::dll;
use crate::steam;
use crate::steam_state::SteamState;

/// 用户从按钮触发的组合操作（见 CONTEXT.md「操作（Action）」）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    ApplyAndLaunch,
    /// 启动 Steam（不含补丁操作；已部署补丁时即带补丁启动，UI 写「启动 Steam」而非「正常启动」——见 ADR-0011）。
    Launch,
    ExitAndUninstall,
    UninstallAndRestart,
    Restart,
}

impl Action {
    /// 点击时若 Steam 在运行**需要先弹「关闭确认」框**的操作（确认流判定）。
    /// #36 修订：只有两个卸载类复合动作（退出并卸载 / 卸载并重启）需要确认；
    /// 「应用补丁并启动」Steam 运行中直接放行（`kill_first` 派生）；`Restart` 恒含关闭步骤但不弹确认框（按钮语义即「关闭并重启」，再确认是冗余打扰）。
    pub fn asks_to_close_steam(self) -> bool {
        matches!(self, Action::ExitAndUninstall | Action::UninstallAndRestart)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    CloseSteam,
    Deploy,
    Uninstall,
    Launch,
}

impl Op {
    pub fn phase(self) -> BusyKind {
        match self {
            Op::CloseSteam => BusyKind::ClosingSteam,
            Op::Deploy => BusyKind::Deploying,
            Op::Uninstall => BusyKind::Uninstalling,
            Op::Launch => BusyKind::Launching,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorkflowCtx {
    pub dll_dir: PathBuf,
    pub steam_dir: PathBuf,
    pub steam: Arc<SteamState>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Precheck {
    InvalidSteamDir,
    MissingTargetDlls,
    MissingSteamExe,
}

/// 执行阶段失败：哪一步 + 底层原始信息（本地化前缀由 ui.rs 映射）。
#[derive(Clone, Debug)]
pub struct WorkflowError {
    pub op: Op,
    pub message: String,
}

/// `kill_first` 表示先关闭 Steam：确认弹窗同意后为 true，或「应用补丁并启动」在
/// Steam 运行中直接放行时同样为 true（#36：ApplyAndLaunch 不再弹确认框）。
pub fn plan(
    action: Action,
    kill_first: bool,
    steam_dir: &Path,
    dll_dir: &Path,
) -> Result<Vec<Op>, Precheck> {
    if !steam_dir.is_dir() {
        return Err(Precheck::InvalidSteamDir);
    }
    if action == Action::ApplyAndLaunch && !dll::target_dlls_present(dll_dir) {
        return Err(Precheck::MissingTargetDlls);
    }
    let needs_exe = matches!(
        action,
        Action::Launch | Action::ApplyAndLaunch | Action::UninstallAndRestart | Action::Restart
    );
    if needs_exe && !steam_dir.join("steam.exe").is_file() {
        return Err(Precheck::MissingSteamExe);
    }

    let mut ops = Vec::new();
    if kill_first {
        ops.push(Op::CloseSteam);
    }
    match action {
        Action::ApplyAndLaunch => {
            ops.push(Op::Deploy);
            ops.push(Op::Launch);
        }
        Action::Launch => ops.push(Op::Launch),
        Action::ExitAndUninstall => ops.push(Op::Uninstall),
        Action::UninstallAndRestart => {
            ops.push(Op::Uninstall);
            ops.push(Op::Launch);
        }
        Action::Restart => {
            // 重启恒含关闭步骤；kill_first 由确认流产生，Restart 不经确认直接执行，
            // 但 `plan(Restart, true)` 组合下避免重复关闭（二次关闭幂等无害，保表整洁）。
            if !kill_first {
                ops.push(Op::CloseSteam);
            }
            ops.push(Op::Launch);
        }
    }
    Ok(ops)
}

/// 顺序执行步骤：每步执行前回调其阶段（含首步），首错即停。
pub fn execute<F>(ops: &[Op], ctx: &WorkflowCtx, mut on_phase: F) -> Result<(), WorkflowError>
where
    F: FnMut(BusyKind),
{
    for &op in ops {
        on_phase(op.phase());
        let res = match op {
            Op::CloseSteam => steam::close_steam(&ctx.steam, &ctx.steam_dir),
            Op::Deploy => dll::deploy(&ctx.dll_dir, &ctx.steam_dir),
            Op::Uninstall => dll::uninstall(&ctx.steam_dir),
            Op::Launch => steam::launch_steam(&ctx.steam, &ctx.steam_dir),
        };
        res.map_err(|message| WorkflowError { op, message })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dll::TARGET_DLLS;

    fn tmp_dlls(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_wf_{}_{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for d in TARGET_DLLS {
            std::fs::write(dir.join(d), b"x").unwrap();
        }
        dir
    }

    fn tmp_steam(name: &str, with_exe: bool) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_wf_{}_{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        if with_exe {
            std::fs::write(dir.join("steam.exe"), b"x").unwrap();
        }
        dir
    }

    #[test]
    fn plan_table() {
        let dlls = tmp_dlls("plan_dlls");
        let steam = tmp_steam("plan_steam", true);

        assert_eq!(
            plan(Action::ApplyAndLaunch, false, &steam, &dlls).unwrap(),
            vec![Op::Deploy, Op::Launch]
        );
        assert_eq!(
            plan(Action::ApplyAndLaunch, true, &steam, &dlls).unwrap(),
            vec![Op::CloseSteam, Op::Deploy, Op::Launch]
        );
        assert_eq!(
            plan(Action::Launch, false, &steam, &dlls).unwrap(),
            vec![Op::Launch]
        );
        assert_eq!(
            plan(Action::ExitAndUninstall, false, &steam, &dlls).unwrap(),
            vec![Op::Uninstall]
        );
        assert_eq!(
            plan(Action::ExitAndUninstall, true, &steam, &dlls).unwrap(),
            vec![Op::CloseSteam, Op::Uninstall]
        );
        assert_eq!(
            plan(Action::UninstallAndRestart, false, &steam, &dlls).unwrap(),
            vec![Op::Uninstall, Op::Launch]
        );
        assert_eq!(
            plan(Action::UninstallAndRestart, true, &steam, &dlls).unwrap(),
            vec![Op::CloseSteam, Op::Uninstall, Op::Launch]
        );
        assert_eq!(
            plan(Action::Restart, false, &steam, &dlls).unwrap(),
            vec![Op::CloseSteam, Op::Launch]
        );
        assert_eq!(
            plan(Action::Restart, true, &steam, &dlls).unwrap(),
            vec![Op::CloseSteam, Op::Launch]
        );

        std::fs::remove_dir_all(&dlls).ok();
        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn plan_prechecks() {
        assert_eq!(
            plan(Action::Launch, false, Path::new(""), Path::new("")),
            Err(Precheck::InvalidSteamDir)
        );

        let empty_dlls = tmp_steam("plan_empty_dlls", false);
        let steam = tmp_steam("plan_steam_nodlls", false);
        assert_eq!(
            plan(Action::ApplyAndLaunch, false, &steam, &empty_dlls),
            Err(Precheck::MissingTargetDlls)
        );
        std::fs::remove_dir_all(&empty_dlls).ok();
        std::fs::remove_dir_all(&steam).ok();

        let dlls = tmp_dlls("plan_exe_dlls");
        let steam_no_exe = tmp_steam("plan_steam_noexe", false);
        assert_eq!(
            plan(Action::Launch, false, &steam_no_exe, &dlls),
            Err(Precheck::MissingSteamExe)
        );
        assert_eq!(
            plan(Action::UninstallAndRestart, false, &steam_no_exe, &dlls),
            Err(Precheck::MissingSteamExe)
        );
        assert_eq!(
            plan(Action::Restart, false, &steam_no_exe, &dlls),
            Err(Precheck::MissingSteamExe)
        );
        std::fs::remove_dir_all(&dlls).ok();
        std::fs::remove_dir_all(&steam_no_exe).ok();
    }

    #[test]
    fn execute_deploys_to_steam_dir() {
        let dlls = tmp_dlls("exec_dlls");
        let steam = tmp_steam("exec_steam", false);
        let ctx = WorkflowCtx {
            dll_dir: dlls.clone(),
            steam_dir: steam.clone(),
            steam: Arc::new(SteamState::new()),
        };

        let mut phases = Vec::new();
        execute(&[Op::Deploy], &ctx, |p| phases.push(p)).unwrap();

        assert_eq!(phases, vec![BusyKind::Deploying]);
        for d in TARGET_DLLS {
            assert!(steam.join(d).is_file(), "missing {d}");
        }
        assert!(steam.join("config").join("lua").is_dir());

        std::fs::remove_dir_all(&dlls).ok();
        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn execute_uninstalls() {
        let dlls = tmp_dlls("exec_un_dlls");
        let steam = tmp_dlls("exec_un_steam"); // 预置三个 DLL
        let ctx = WorkflowCtx {
            dll_dir: dlls.clone(),
            steam_dir: steam.clone(),
            steam: Arc::new(SteamState::new()),
        };

        let mut phases = Vec::new();
        execute(&[Op::Uninstall], &ctx, |p| phases.push(p)).unwrap();

        assert_eq!(phases, vec![BusyKind::Uninstalling]);
        for d in TARGET_DLLS {
            assert!(!steam.join(d).exists(), "still present {d}");
        }

        std::fs::remove_dir_all(&dlls).ok();
        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn execute_stops_at_first_error() {
        let dlls = tmp_dlls("exec_err_dlls");
        let steam = tmp_dlls("exec_err_steam"); // 有 DLL，但无 steam.exe
        let ctx = WorkflowCtx {
            dll_dir: dlls.clone(),
            steam_dir: steam.clone(),
            steam: Arc::new(SteamState::new()),
        };

        let mut phases = Vec::new();
        let err = execute(&[Op::Uninstall, Op::Launch], &ctx, |p| phases.push(p)).unwrap_err();

        assert_eq!(phases, vec![BusyKind::Uninstalling, BusyKind::Launching]);
        assert_eq!(err.op, Op::Launch);
        assert!(
            err.message.contains("steam.exe"),
            "err message: {}",
            err.message
        );
        for d in TARGET_DLLS {
            assert!(!steam.join(d).exists(), "still present {d}");
        }

        std::fs::remove_dir_all(&dlls).ok();
        std::fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn action_asks_to_close_steam_table() {
        assert!(!Action::ApplyAndLaunch.asks_to_close_steam());
        assert!(!Action::Launch.asks_to_close_steam());
        assert!(Action::ExitAndUninstall.asks_to_close_steam());
        assert!(Action::UninstallAndRestart.asks_to_close_steam());
        assert!(!Action::Restart.asks_to_close_steam());
    }
}
