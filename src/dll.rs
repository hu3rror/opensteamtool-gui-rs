//! 目标 DLL 定义、本地版本、部署状态检测、部署/卸载。

use std::fs;
use std::path::{Path, PathBuf};

/// 三个目标 DLL，部署/卸载/提取都以此集合为准。
pub const TARGET_DLLS: [&str; 3] = ["OpenSteamTool.dll", "dwmapi.dll", "xinput1_4.dll"];

/// 本地版本记录文件名（位于 dlls/ 目录）。
pub const VERSION_FILE: &str = "version.txt";

/// 本地部署状态。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeployStatus {
    /// 路径为空或不是目录。
    InvalidPath,
    /// 三个目标 DLL 全部存在于 Steam 目录。
    Applied,
    /// 未应用。
    NotApplied,
}

/// dlls/ 目录：exe 同目录下的 `dlls` 文件夹（便携版）。
pub fn dll_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dlls")
}

/// 读取本地版本记录 `dlls/version.txt`，失败返回 None。
pub fn read_local_version(dll_dir: &Path) -> Option<String> {
    fs::read_to_string(dll_dir.join(VERSION_FILE))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 根据 Steam 路径判断本地部署状态。
pub fn check_status(steam_dir: &Path) -> DeployStatus {
    if steam_dir.as_os_str().is_empty() || !steam_dir.is_dir() {
        return DeployStatus::InvalidPath;
    }
    if TARGET_DLLS
        .iter()
        .all(|dll| steam_dir.join(dll).is_file())
    {
        DeployStatus::Applied
    } else {
        DeployStatus::NotApplied
    }
}

/// 部署：从 `dlls/` 复制三个 DLL 到 Steam 目录，并创建 `config/lua` 目录。
pub fn deploy(dll_dir: &Path, steam_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(steam_dir.join("config").join("lua"))
        .map_err(|e| format!("create config/lua: {e}"))?;

    for dll in TARGET_DLLS {
        let src = dll_dir.join(dll);
        let dst = steam_dir.join(dll);
        fs::copy(&src, &dst).map_err(|e| format!("copy {dll}: {e}"))?;
    }
    Ok(())
}

/// 卸载：删除 Steam 目录下的三个目标 DLL（已不存在的跳过）。
pub fn uninstall(steam_dir: &Path) -> Result<(), String> {
    for dll in TARGET_DLLS {
        let target = steam_dir.join(dll);
        if target.exists() {
            fs::remove_file(&target).map_err(|e| format!("remove {dll}: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_path_is_invalid() {
        assert_eq!(check_status(Path::new("")), DeployStatus::InvalidPath);
    }

    #[test]
    fn nonexistent_dir_is_invalid() {
        assert_eq!(
            check_status(Path::new("Z:/definitely/not/a/real/dir_12345")),
            DeployStatus::InvalidPath
        );
    }

    #[test]
    fn empty_dir_is_not_applied() {
        let dir = std::env::temp_dir().join(format!("ost_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(check_status(&dir), DeployStatus::NotApplied);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn full_dll_set_is_applied() {
        let dir = std::env::temp_dir().join(format!("ost_applied_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for dll in TARGET_DLLS {
            fs::write(dir.join(dll), b"x").unwrap();
        }
        assert_eq!(check_status(&dir), DeployStatus::Applied);
        fs::remove_dir_all(&dir).ok();
    }
}
