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
    crate::fsutil::exe_dir().join("dlls")
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

/// 文件事实快照：部署状态 + 本地版本 + DLL 齐全，单次探测产出（spec #43）。
/// 渲染只消费快照，刷新时机由调用方（事件驱动）决定，零每帧文件 IO。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeploymentFacts {
    pub status: DeployStatus,
    pub local_version: Option<String>,
    pub dlls_present: bool,
}

impl DeploymentFacts {
    /// 本地版本仅在补丁 DLL 齐全时视为已知（「补丁已下载」是文件本位判据，ADR-0011）。
    pub fn known_local_version(&self) -> Option<&str> {
        self.dlls_present
            .then_some(self.local_version.as_deref())
            .flatten()
    }
}

pub fn probe_facts(steam_dir: &Path, dll_dir: &Path) -> DeploymentFacts {
    DeploymentFacts {
        status: check_status(steam_dir),
        local_version: read_local_version(dll_dir),
        dlls_present: target_dlls_present(dll_dir),
    }
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

    #[test]
    fn probe_facts_reflects_empty_dirs() {
        let steam = std::env::temp_dir().join(format!("ost_facts_steam_{}", std::process::id()));
        let dlldir = std::env::temp_dir().join(format!("ost_facts_dlls_{}", std::process::id()));
        fs::create_dir_all(&steam).unwrap();
        fs::create_dir_all(&dlldir).unwrap();
        let facts = probe_facts(&steam, &dlldir);
        assert_eq!(facts.status, DeployStatus::NotDeployed);
        assert!(!facts.dlls_present);
        assert_eq!(facts.local_version, None);
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn probe_facts_sees_downloaded_patch() {
        let steam = std::env::temp_dir().join(format!("ost_facts_steam2_{}", std::process::id()));
        let dlldir = std::env::temp_dir().join(format!("ost_facts_dlls2_{}", std::process::id()));
        fs::create_dir_all(&steam).unwrap();
        fs::create_dir_all(&dlldir).unwrap();
        for dll in TARGET_DLLS {
            fs::write(dlldir.join(dll), b"x").unwrap();
        }
        fs::write(dlldir.join(VERSION_FILE), b"9.9.9").unwrap();
        let facts = probe_facts(&steam, &dlldir);
        assert!(facts.dlls_present);
        assert_eq!(facts.local_version.as_deref(), Some("9.9.9"));
        assert_eq!(facts.known_local_version(), Some("9.9.9"));
        assert_eq!(facts.status, DeployStatus::NotDeployed);
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn probe_facts_sees_deploy_status() {
        let steam = std::env::temp_dir().join(format!("ost_facts_steam3_{}", std::process::id()));
        let dlldir = std::env::temp_dir().join(format!("ost_facts_dlls3_{}", std::process::id()));
        fs::create_dir_all(&steam).unwrap();
        fs::create_dir_all(&dlldir).unwrap();
        for dll in TARGET_DLLS {
            fs::write(steam.join(dll), b"x").unwrap();
        }
        let facts = probe_facts(&steam, &dlldir);
        assert_eq!(facts.status, DeployStatus::Deployed);
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn probe_facts_invalid_steam_dir_is_invalid_path() {
        let dlldir = std::env::temp_dir().join(format!("ost_facts_dlls4_{}", std::process::id()));
        fs::create_dir_all(&dlldir).unwrap();
        let facts = probe_facts(Path::new("Z:/definitely/not/a/real/dir_12345"), &dlldir);
        assert_eq!(facts.status, DeployStatus::InvalidPath);
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn known_local_version_gates_on_dll_presence() {
        let facts = DeploymentFacts {
            status: DeployStatus::Deployed,
            local_version: Some("1.2.3".to_string()),
            dlls_present: false,
        };
        assert_eq!(facts.known_local_version(), None);
        let facts = DeploymentFacts {
            status: DeployStatus::NotDeployed,
            local_version: Some("1.2.3".to_string()),
            dlls_present: true,
        };
        assert_eq!(facts.known_local_version(), Some("1.2.3"));
        let facts = DeploymentFacts {
            status: DeployStatus::NotDeployed,
            local_version: None,
            dlls_present: true,
        };
        assert_eq!(facts.known_local_version(), None);
    }
}
