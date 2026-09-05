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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_missing_steam_errors() {
        let dir = std::env::temp_dir().join(format!("ost_launch_{}", std::process::id()));
        let err = launch_steam(&SteamState::new(), &dir).unwrap_err();
        assert!(err.contains("steam.exe"), "err: {err}");
    }
}
