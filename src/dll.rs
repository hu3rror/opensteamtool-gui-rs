//! 目标 DLL 定义、本地版本、部署状态检测、部署/卸载。

use std::fs;
use std::path::{Path, PathBuf};

/// 三个目标 DLL，部署/卸载/提取都以此集合为准。
pub const TARGET_DLLS: [&str; 3] = ["OpenSteamTool.dll", "dwmapi.dll", "xinput1_4.dll"];

pub const VERSION_FILE: &str = "version.txt";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeployStatus {
    InvalidPath,
    Deployed,
    NotDeployed,
}

pub fn dll_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dlls")
}

pub fn read_local_version(dll_dir: &Path) -> Option<String> {
    fs::read_to_string(dll_dir.join(VERSION_FILE))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 目标 DLL 在 `dir` 下是否齐全，即「补丁已下载」的文件本位判据（版本记录不作数，
/// 见 ADR-0011）。部署前置校验（`workflow::plan` 的 `MissingTargetDlls`）与 UI 置灰/本地
/// 版本行/更新对比/通知文案共用同一谓词，一处演化各处跟随。
pub fn target_dlls_present(dir: &Path) -> bool {
    TARGET_DLLS.iter().all(|d| dir.join(d).is_file())
}

pub fn dlls_present() -> bool {
    target_dlls_present(&dll_dir())
}

/// 有效 Steam 目录判据：trim 后非空且为目录。空 = 未设置（ADR-0012 的合法终态）；
/// 空串是否放行由调用方按语境决定——向导步骤 2 拒绝空路径（必须有路径才能继续），
/// 设置页允许空（编辑语义下可留空）、启动回退把无效配置视为未设置。
/// 与 `wizard` 步骤 2 提交、`SteamPathEditor::submit`、启动恢复共用同一口径（ADR-0013）。
pub fn is_valid_steam_dir(path: &str) -> bool {
    let p = path.trim();
    !p.is_empty() && Path::new(p).is_dir()
}

/// 根据 Steam 路径判断本地部署状态。路径判据共用 [`is_valid_steam_dir`]（ADR-0013）：
/// 非有效目录 → `InvalidPath`（调用方先 trim）。
pub fn check_status(steam_dir: &Path) -> DeployStatus {
    if !is_valid_steam_dir(&steam_dir.to_string_lossy()) {
        return DeployStatus::InvalidPath;
    }
    if TARGET_DLLS.iter().all(|dll| steam_dir.join(dll).is_file()) {
        DeployStatus::Deployed
    } else {
        DeployStatus::NotDeployed
    }
}

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
    fn is_valid_steam_dir_requires_nonempty_existing_dir() {
        let dir = std::env::temp_dir().join(format!("ost_valid_dir_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_valid_steam_dir(""));
        assert!(!is_valid_steam_dir("   "));
        assert!(!is_valid_steam_dir("Z:/nope_12345"));
        assert!(is_valid_steam_dir(&dir.display().to_string()));
        assert!(is_valid_steam_dir(&format!("  {}  ", dir.display())));
        std::fs::remove_dir_all(&dir).ok();
    }

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
    fn empty_dir_is_not_deployed() {
        let dir = std::env::temp_dir().join(format!("ost_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(check_status(&dir), DeployStatus::NotDeployed);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn full_dll_set_is_deployed() {
        let dir = std::env::temp_dir().join(format!("ost_deployed_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for dll in TARGET_DLLS {
            fs::write(dir.join(dll), b"x").unwrap();
        }
        assert_eq!(check_status(&dir), DeployStatus::Deployed);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn target_dlls_present_is_file_based() {
        let dir = std::env::temp_dir().join(format!("ost_dlls_present_{}", std::process::id()));
        assert!(!target_dlls_present(&dir));
        fs::create_dir_all(&dir).unwrap();
        assert!(!target_dlls_present(&dir));
        fs::write(dir.join(TARGET_DLLS[0]), b"x").unwrap();
        assert!(!target_dlls_present(&dir));
        for dll in TARGET_DLLS {
            fs::write(dir.join(dll), b"x").unwrap();
        }
        assert!(target_dlls_present(&dir));
        fs::write(dir.join(VERSION_FILE), b"9.9.9").unwrap();
        assert!(target_dlls_present(&dir));
        fs::remove_file(dir.join(TARGET_DLLS[0])).unwrap();
        assert!(!target_dlls_present(&dir));
        fs::remove_dir_all(&dir).ok();
    }
}
