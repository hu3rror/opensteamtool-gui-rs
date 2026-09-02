//! 「操作」模块：组合动作的判定表与顺序执行。
//!
//! 深模块——小接口（`plan` / `execute`），大实现（动作判定表、前置校验、
//! 分阶段执行、首错即停）。不依赖 i18n 与 UI；文案 helpers 由 ui.rs 提供。

use std::path::{Path, PathBuf};

use crate::dll::{self, TARGET_DLLS};
use crate::process;
use crate::steam;

/// 用户从按钮触发的组合操作（见 CONTEXT.md「操作（Action）」）。
/// 可混含补丁操作（部署/卸载）与 Steam 启动/退出。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// 应用补丁并启动 Steam（部署 + 启动）。
    ApplyAndLaunch,
    /// 正常启动 Steam。
    Launch,
    /// 退出 Steam 并卸载补丁。
    ExitAndUninstall,
    /// 卸载补丁并重启 Steam。
    UninstallAndRestart,
}

impl Action {
    /// 需要先关闭 Steam 才能执行的操作。
    pub fn needs_close(self) -> bool {
        matches!(
            self,
            Action::ApplyAndLaunch | Action::ExitAndUninstall | Action::UninstallAndRestart
        )
    }
}

/// 后台操作期间显示的忙碌/阶段文案类型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BusyKind {
    Deploying,
    Uninstalling,
    Launching,
    Checking,
    Downloading,
    ClosingSteam,
}

/// 一条执行步骤。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    CloseSteam,
    Deploy,
    Uninstall,
    Launch,
}

impl Op {
    /// 执行本步骤期间的阶段文案类型。
    pub fn phase(self) -> BusyKind {
        match self {
            Op::CloseSteam => BusyKind::ClosingSteam,
            Op::Deploy => BusyKind::Deploying,
            Op::Uninstall => BusyKind::Uninstalling,
            Op::Launch => BusyKind::Launching,
        }
    }
}

/// 执行所需路径上下文。
#[derive(Clone, Debug)]
pub struct WorkflowCtx {
    pub dll_dir: PathBuf,
    pub steam_dir: PathBuf,
}

/// 前置校验失败（plan 阶段判定，未进入忙碌状态）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Precheck {
    /// Steam 路径为空或不是目录。
    NoSteamDir,
    /// `dlls/` 缺少目标 DLL（仅部署类操作）。
    NoTargetDlls,
    /// Steam 目录缺少 `steam.exe`。
    NoSteamExe,
}

/// 执行阶段失败：哪一步 + 底层原始信息（本地化前缀由 ui.rs 映射）。
#[derive(Clone, Debug)]
pub struct WorkflowError {
    pub op: Op,
    pub message: String,
}

/// 动作判定表 + 前置校验。返回有序执行步骤；前置不满足返回 `Precheck`。
///
/// `kill_first` 表示先关闭 Steam（确认弹窗同意后为 true；仅 `needs_close`
/// 的动作会走到该分支）。
pub fn plan(
    action: Action,
    kill_first: bool,
    steam_dir: &Path,
    dll_dir: &Path,
) -> Result<Vec<Op>, Precheck> {
    // 前置校验（保序：目录 → DLL → steam.exe）。
    if !steam_dir.is_dir() {
        return Err(Precheck::NoSteamDir);
    }
    if action == Action::ApplyAndLaunch && !TARGET_DLLS.iter().all(|d| dll_dir.join(d).is_file()) {
        return Err(Precheck::NoTargetDlls);
    }
    let needs_exe = matches!(
        action,
        Action::Launch | Action::ApplyAndLaunch | Action::UninstallAndRestart
    );
    if needs_exe && !steam_dir.join("steam.exe").is_file() {
        return Err(Precheck::NoSteamExe);
    }

    // 判定表。
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
            Op::CloseSteam => process::kill_steam(),
            Op::Deploy => dll::deploy(&ctx.dll_dir, &ctx.steam_dir),
            Op::Uninstall => dll::uninstall(&ctx.steam_dir),
            Op::Launch => steam::launch_steam(&ctx.steam_dir),
        };
        res.map_err(|message| WorkflowError { op, message })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 建临时目录并写入三个目标 DLL（返回目录）。
    fn tmp_dlls(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_wf_{}_{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for d in TARGET_DLLS {
            std::fs::write(dir.join(d), b"x").unwrap();
        }
        dir
    }

    /// 建临时 Steam 目录（可选写入 steam.exe）。
    fn tmp_steam(name: &str, with_exe: bool) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_wf_{}_{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        if with_exe {
            std::fs::write(dir.join("steam.exe"), b"x").unwrap();
        }
        dir
    }

    // 1. plan 判定表：4 action × kill_first ∈ {false, true} → 期望 ops 序列。
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

        std::fs::remove_dir_all(&dlls).ok();
        std::fs::remove_dir_all(&steam).ok();
    }

    // 2. plan 前置校验：三种 Precheck 各一例，且按序（目录 → DLL → steam.exe）。
    #[test]
    fn plan_prechecks() {
        // 无效目录 → NoSteamDir（空路径 is_dir 恒 false）。
        assert_eq!(
            plan(Action::Launch, false, Path::new(""), Path::new("")),
            Err(Precheck::NoSteamDir)
        );

        // 部署类缺 DLL → NoTargetDlls（先于 steam.exe 校验）。
        let empty_dlls = tmp_steam("plan_empty_dlls", false);
        let steam = tmp_steam("plan_steam_nodlls", false);
        assert_eq!(
            plan(Action::ApplyAndLaunch, false, &steam, &empty_dlls),
            Err(Precheck::NoTargetDlls)
        );
        std::fs::remove_dir_all(&empty_dlls).ok();
        std::fs::remove_dir_all(&steam).ok();

        // 需启动类缺 steam.exe → NoSteamExe。
        let dlls = tmp_dlls("plan_exe_dlls");
        let steam_no_exe = tmp_steam("plan_steam_noexe", false);
        assert_eq!(
            plan(Action::Launch, false, &steam_no_exe, &dlls),
            Err(Precheck::NoSteamExe)
        );
        assert_eq!(
            plan(Action::UninstallAndRestart, false, &steam_no_exe, &dlls),
            Err(Precheck::NoSteamExe)
        );
        std::fs::remove_dir_all(&dlls).ok();
        std::fs::remove_dir_all(&steam_no_exe).ok();
    }

    // 3. execute 真实接口：Deploy 把三个 DLL 复制进 Steam 目录，并创建 config/lua。
    #[test]
    fn execute_deploys_to_steam_dir() {
        let dlls = tmp_dlls("exec_dlls");
        let steam = tmp_steam("exec_steam", false);
        let ctx = WorkflowCtx {
            dll_dir: dlls.clone(),
            steam_dir: steam.clone(),
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

    // 4. execute 卸载路径：DLL 被删除。
    #[test]
    fn execute_uninstalls() {
        let dlls = tmp_dlls("exec_un_dlls");
        let steam = tmp_dlls("exec_un_steam"); // 预置三个 DLL
        let ctx = WorkflowCtx {
            dll_dir: dlls.clone(),
            steam_dir: steam.clone(),
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

    // 5. 首错即停 + 错误定位：Uninstall 成功 → Launch 失败（无 steam.exe），
    //    返回 Err(op=Launch)，且 Uninstall 副作用已生效。
    #[test]
    fn execute_stops_at_first_error() {
        let dlls = tmp_dlls("exec_err_dlls");
        let steam = tmp_dlls("exec_err_steam"); // 有 DLL，但无 steam.exe
        let ctx = WorkflowCtx {
            dll_dir: dlls.clone(),
            steam_dir: steam.clone(),
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
        // Uninstall 已生效，DLL 应已被删除。
        for d in TARGET_DLLS {
            assert!(!steam.join(d).exists(), "still present {d}");
        }

        std::fs::remove_dir_all(&dlls).ok();
        std::fs::remove_dir_all(&steam).ok();
    }

    // 6. Action::needs_close 表：三个需关 Steam，Launch 不需。
    #[test]
    fn action_needs_close_table() {
        assert!(Action::ApplyAndLaunch.needs_close());
        assert!(!Action::Launch.needs_close());
        assert!(Action::ExitAndUninstall.needs_close());
        assert!(Action::UninstallAndRestart.needs_close());
    }
}
