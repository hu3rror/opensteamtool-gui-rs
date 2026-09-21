//! Steam 安装路径检测（注册表）与 steam.exe 启动。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::steam_state::SteamState;
use winreg::enums::*;
use winreg::{HKEY, RegKey};

/// 按顺序尝试的注册表位置：(hive, 子键路径)。
const STEAM_REG_PATHS: [(HKEY, &str); 3] = [
    (HKEY_CURRENT_USER, r"Software\Valve\Steam"),
    (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
    (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam"),
];

/// 检测 Steam 安装路径：按顺序尝试注册表，返回第一个有效且 `exists()` 的路径。
pub fn detect_steam_path() -> Option<PathBuf> {
    for (hive, subkey) in STEAM_REG_PATHS {
        let key = match RegKey::predef(hive).open_subkey(subkey) {
            Ok(k) => k,
            Err(_) => continue, // 尝试下一个注册表位置
        };
        let value: Result<String, _> = key.get_value("SteamPath");
        if let Ok(path) = value {
            let p = PathBuf::from(path);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    None
}

/// spawn 后确认 steam.exe 仍存活的等待窗口。
const LAUNCH_VERIFY_WINDOW: Duration = Duration::from_secs(2);
/// 验证失败后重试前的等待。
const LAUNCH_RETRY_DELAY: Duration = Duration::from_secs(1);
/// 验证失败后的重试次数（1 次 → 最多 spawn 2 次）。
const LAUNCH_RETRIES: u32 = 1;
/// 优雅退出轮询预算：Steam 干净退出（保存状态/关闭连接）比硬杀慢，给足 10s。
const SHUTDOWN_POLL_BUDGET: Duration = Duration::from_secs(10);
/// 校验 `Steam 目录\steam.exe` 存在，以 Steam 目录为 cwd 启动，并验证进程存活。
/// spawn 成功不算成功：等待窗口后确认 steam.exe 仍在运行，失败重试 `LAUNCH_RETRIES` 次。
pub fn launch_steam(steam: &SteamState, steam_dir: &Path) -> Result<(), String> {
    let exe = steam_dir.join("steam.exe");
    if !exe.is_file() {
        return Err("steam.exe not found".into());
    }
    for attempt in 0..=LAUNCH_RETRIES {
        Command::new(&exe)
            .current_dir(steam_dir)
            .spawn()
            .map_err(|e| format!("spawn steam.exe: {e}"))?;
        // spawn 成功不算成功：旧实例残留可能让新实例随即退出，等待窗口后验证存活。
        std::thread::sleep(LAUNCH_VERIFY_WINDOW);
        if steam.alive() {
            return Ok(());
        }
        if attempt < LAUNCH_RETRIES {
            std::thread::sleep(LAUNCH_RETRY_DELAY);
        }
    }
    Err("steam.exe exited shortly after launch".into())
}

/// 关闭 Steam：优雅退出优先，超时硬杀兜底。
///
/// 先 `steam.exe -shutdown` 触发与 Steam 菜单「退出」等价的干净关闭，避免硬杀
/// （`TerminateProcess`）触发 Steam 看门狗自重启——那正是「重启后起不来」的根因
/// （ADR-0004）。轮询等进程组消失（预算 10s），超时仍未消失则回退硬杀。
pub fn close_steam(steam: &SteamState, steam_dir: &Path) -> Result<(), String> {
    close_steam_with_budget(steam, steam_dir, SHUTDOWN_POLL_BUDGET)
}

/// 内部缝：优雅退出预算可注入，测试用极小值保持秒级。
fn close_steam_with_budget(
    steam: &SteamState,
    steam_dir: &Path,
    shutdown_budget: Duration,
) -> Result<(), String> {
    if !steam.group_running(steam_dir) {
        return Ok(()); // 本就未运行
    }
    // 仅当本目录的 steam.exe 实例在运行才发优雅信号：`-shutdown` 在无实例时可能
    // 意外启动 Steam（他处安装的 steam.exe 不算），纯孤儿进程应直接硬杀。
    let exe = steam_dir.join("steam.exe");
    if steam.steam_exe_running(steam_dir) && exe.is_file() {
        // 信号发出成功才进入等待；spawn 失败（或未发信号）直接回退硬杀，不空等预算。
        let sent = Command::new(&exe)
            .current_dir(steam_dir)
            .arg("-shutdown")
            .spawn()
            .is_ok();
        if sent && steam.wait_group_empty(steam_dir, shutdown_budget) {
            return Ok(()); // 优雅退出完成
        }
    }
    steam.kill(steam_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_missing_steam_errors() {
        let dir = std::env::temp_dir().join(format!("ost_launch_{}", std::process::id()));
        let err = launch_steam(&SteamState::new(), &dir).unwrap_err();
        assert!(err.contains("steam.exe"), "err: {err}");
    }

    /// 组未运行时 `close_steam` 直接成功（不 spawn、不 kill）。
    #[test]
    fn close_steam_not_running_short_circuits() {
        let dir = std::env::temp_dir().join(format!("ost_close_short_{}", std::process::id()));
        let steam = SteamState::new();
        close_steam(&steam, &dir).unwrap();
    }

    /// 每次调用生成唯一临时目录（测试并行时同名 exe 会撞同一路径）。
    static FAKE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    /// 造一个 exe 路径在临时 Steam 目录下的常驻进程：复制 PING.EXE 充当指定文件名。
    /// 返回 (子进程句柄, 临时目录)。仅 Windows（依赖 `C:\Windows\System32\PING.EXE`）。
    #[cfg(windows)]
    fn spawn_fake_steam(exe_name: &str) -> (std::process::Child, PathBuf) {
        use std::sync::atomic::Ordering;
        let seq = FAKE_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "ost_close_{}_{}_{}",
            exe_name,
            std::process::id(),
            seq
        ));
        let _ = std::fs::remove_dir_all(&dir); // 清理上次失败运行的残留
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join(exe_name);
        std::fs::copy(r"C:\Windows\System32\PING.EXE", &fake).unwrap();
        let child = Command::new(&fake)
            .args(["-n", "240", "127.0.0.1"])
            .spawn()
            .unwrap();
        (child, dir)
    }

    /// 核心回归：优雅退出无法清空进程组时，硬杀兜底仍能关闭成功。
    /// ping 副本对 `-shutdown` 无响应 → 优雅退出失败 → 回退硬杀。
    #[cfg(windows)]
    #[test]
    fn close_steam_falls_back_to_kill_when_graceful_shutdown_fails() {
        let (mut child, dir) = spawn_fake_steam("steam.exe");
        let steam = SteamState::new();
        assert!(steam.group_running(&dir), "前置：进程组应在运行");

        // 极小优雅退出预算（50ms）：`-shutdown` 无效 → 组仍在 → 立即回退硬杀。
        close_steam_with_budget(&steam, &dir, Duration::from_millis(50)).unwrap();

        assert!(!steam.group_running(&dir), "关闭后进程组应清空");
        let _ = child.wait();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 优雅信号只发给本目录的 steam.exe：他处安装的 steam.exe 不算（路径过滤）。
    #[cfg(windows)]
    #[test]
    fn steam_exe_running_ignores_other_installs() {
        // dir_b 有 steam.exe 在跑（他处实例），dir_a 只有孤儿（steamwebhelper.exe）。
        let (mut child_b, dir_b) = spawn_fake_steam("steam.exe");
        let (mut child_a, dir_a) = spawn_fake_steam("steamwebhelper.exe");
        let steam = SteamState::new();
        // 全局口径会误判为 true；路径口径必须排除他处实例。
        assert!(steam.alive(), "前置：系统内确有 steam.exe（在 dir_b）");
        assert!(steam.group_running(&dir_a), "前置：dir_a 有孤儿进程在跑");
        assert!(
            !steam.steam_exe_running(&dir_a),
            "dir_a 无自己的 steam.exe 实例"
        );
        assert!(
            steam.steam_exe_running(&dir_b),
            "dir_b 有自己的 steam.exe 实例"
        );
        // 收尾：先杀再 wait（ping 副本常驻 240s，直接 wait 会阻塞）。
        let _ = child_b.kill();
        let _ = child_a.kill();
        let _ = child_b.wait();
        let _ = child_a.wait();
        std::fs::remove_dir_all(&dir_a).ok();
        std::fs::remove_dir_all(&dir_b).ok();
    }
}
