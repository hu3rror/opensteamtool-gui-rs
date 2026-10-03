//! Steam 安装路径检测（注册表）与 steam.exe 启动。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::steam_state::SteamState;
use winreg::enums::*;
use winreg::{HKEY, RegKey};

const STEAM_REG_PATHS: [(HKEY, &str); 3] = [
    (HKEY_CURRENT_USER, r"Software\Valve\Steam"),
    (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
    (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam"),
];

pub fn detect_steam_path() -> Option<PathBuf> {
    for (hive, subkey) in STEAM_REG_PATHS {
        let key = match RegKey::predef(hive).open_subkey(subkey) {
            Ok(k) => k,
            Err(_) => continue,
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

const LAUNCH_VERIFY_WINDOW: Duration = Duration::from_secs(2);
const LAUNCH_RETRY_DELAY: Duration = Duration::from_secs(1);
const LAUNCH_RETRIES: u32 = 1;
const SHUTDOWN_POLL_BUDGET: Duration = Duration::from_secs(10);
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

/// 先 `steam.exe -shutdown` 触发干净关闭，避免硬杀（`TerminateProcess`）触发 Steam 看门狗自重启——「重启后起不来」的根因（ADR-0004）；超时回退硬杀。
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
        return Ok(());
    }
    // 仅当本目录的 steam.exe 实例在运行才发优雅信号：-shutdown 在无实例时可能意外启动 Steam（他处安装不算）；纯孤儿进程直接硬杀。
    let exe = steam_dir.join("steam.exe");
    if steam.steam_exe_running(steam_dir) && exe.is_file() {
        let sent = Command::new(&exe)
            .current_dir(steam_dir)
            .arg("-shutdown")
            .spawn()
            .is_ok();
        if sent && steam.wait_group_empty(steam_dir, shutdown_budget) {
            return Ok(());
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

    #[test]
    fn close_steam_not_running_short_circuits() {
        let dir = std::env::temp_dir().join(format!("ost_close_short_{}", std::process::id()));
        let steam = SteamState::new();
        close_steam(&steam, &dir).unwrap();
    }

    static FAKE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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

    #[cfg(windows)]
    #[test]
    fn close_steam_falls_back_to_kill_when_graceful_shutdown_fails() {
        let (mut child, dir) = spawn_fake_steam("steam.exe");
        let steam = SteamState::new();
        assert!(steam.group_running(&dir), "前置：进程组应在运行");

        close_steam_with_budget(&steam, &dir, Duration::from_millis(50)).unwrap();

        assert!(!steam.group_running(&dir), "关闭后进程组应清空");
        let _ = child.wait();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(windows)]
    #[test]
    fn steam_exe_running_ignores_other_installs() {
        let (mut child_b, dir_b) = spawn_fake_steam("steam.exe");
        let (mut child_a, dir_a) = spawn_fake_steam("steamwebhelper.exe");
        let steam = SteamState::new();
        assert!(steam.alive(), "前置：系统内确有 steam.exe（在 dir_b）");
        assert!(steam.group_running(&dir_a), "前置：dir_a 有孤儿进程在跑");
        // spawn 后 exe 路径可能瞬时不可解析（sysinfo 快照）：steam_exe_running 不做名字回退，
        // 先轮询直到 dir_b 的 steam.exe 本体可被路径识别，再做跨目录判定。
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !steam.steam_exe_running(&dir_b) {
            assert!(
                std::time::Instant::now() < deadline,
                "dir_b 的 steam.exe 应在窗口内被识别"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            !steam.steam_exe_running(&dir_a),
            "dir_a 无自己的 steam.exe 实例"
        );
        let _ = child_b.kill();
        let _ = child_a.kill();
        let _ = child_b.wait();
        let _ = child_a.wait();
        std::fs::remove_dir_all(&dir_a).ok();
        std::fs::remove_dir_all(&dir_b).ok();
    }
}
