//! 补丁文件表与部署状态：布局决定文件名，文件齐全决定「已下载」。

use std::fs;
use std::path::{Path, PathBuf};

use crate::fsutil::write_atomic;
use crate::patch_source::Layout;

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

/// 「补丁已下载」是文件本位判据：`version.txt` 记录不作数（ADR-0011）。
pub fn target_dlls_present(dir: &Path, layout: Layout) -> bool {
    layout
        .root_files()
        .iter()
        .all(|f| dir.join(f.name).is_file())
}

/// 渲染只消费快照，刷新时机由调用方（事件驱动）决定，零每帧文件 IO。
/// 单次探测的文件事实；渲染只消费快照，刷新由事件驱动，零每帧 IO。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeploymentFacts {
    pub status: DeployStatus,
    pub local_version: Option<String>,
    pub dlls_present: bool,
}

impl DeploymentFacts {
    /// 本地版本仅在目标文件齐全时视为已知（「补丁已下载」是文件本位判据，ADR-0011）。
    pub fn known_local_version(&self) -> Option<&str> {
        self.dlls_present
            .then_some(self.local_version.as_deref())
            .flatten()
    }
}

pub fn probe_facts(steam_dir: &Path, dll_dir: &Path, layout: Layout) -> DeploymentFacts {
    DeploymentFacts {
        status: check_status(steam_dir, layout),
        local_version: read_local_version(dll_dir),
        dlls_present: target_dlls_present(dll_dir, layout),
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

pub fn check_status(steam_dir: &Path, layout: Layout) -> DeployStatus {
    if !is_valid_steam_dir(&steam_dir.to_string_lossy()) {
        return DeployStatus::InvalidPath;
    }
    if layout
        .root_files()
        .iter()
        .all(|f| steam_dir.join(f.name).is_file())
    {
        DeployStatus::Deployed
    } else {
        DeployStatus::NotDeployed
    }
}

pub fn lua_dir(steam_dir: &Path, layout: Layout) -> PathBuf {
    steam_dir.join(layout.lua_subdir())
}

pub fn ensure_lua_dir(steam_dir: &Path, layout: Layout) -> Result<(), String> {
    fs::create_dir_all(lua_dir(steam_dir, layout))
        .map_err(|e| format!("create {}: {e}", layout.lua_subdir()))
}

/// 任一内置布局声明的根目录文件名（部署前清掉「另一个布局」的残留用）。
fn foreign_root_files(layout: Layout) -> impl Iterator<Item = &'static str> {
    Layout::ALL
        .into_iter()
        .filter(move |l| *l != layout)
        .flat_map(|l| l.root_files().iter())
        .map(|f| f.name)
        .filter(move |name| !layout.root_files().iter().any(|f| f.name == *name))
}

pub fn deploy(dll_dir: &Path, steam_dir: &Path, layout: Layout) -> Result<(), String> {
    ensure_lua_dir(steam_dir, layout)?;

    for name in foreign_root_files(layout) {
        let stale = steam_dir.join(name);
        if stale.exists() {
            fs::remove_file(&stale).map_err(|e| format!("remove stale {name}: {e}"))?;
        }
    }

    for file in layout.root_files() {
        let source_name = file.rename_from.unwrap_or(file.name);
        let bytes =
            fs::read(dll_dir.join(source_name)).map_err(|e| format!("read {source_name}: {e}"))?;
        write_atomic(&steam_dir.join(file.name), &bytes)
            .map_err(|e| format!("write {}: {e}", file.name))?;
    }
    Ok(())
}

pub fn uninstall(steam_dir: &Path, layout: Layout) -> Result<(), String> {
    for file in layout.root_files() {
        let target = steam_dir.join(file.name);
        if target.exists() {
            fs::remove_file(&target).map_err(|e| format!("remove {}: {e}", file.name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steam_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 目标形态（Steam 目录 / dlls 解压后目录）：按最终文件名落盘。
    fn seed_files(dir: &Path, layout: Layout) {
        for file in layout.root_files() {
            fs::write(dir.join(file.name), b"x").unwrap();
        }
    }

    /// 发布形态（解压前的 dlls/）：改名规则取源文件名。
    fn seed_release(dir: &Path, layout: Layout) {
        for file in layout.root_files() {
            fs::write(dir.join(file.rename_from.unwrap_or(file.name)), b"x").unwrap();
        }
    }

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
        assert_eq!(
            check_status(Path::new(""), Layout::OpenSteamTool),
            DeployStatus::InvalidPath
        );
        assert_eq!(
            check_status(Path::new(""), Layout::SteamDaddy),
            DeployStatus::InvalidPath
        );
    }

    #[test]
    fn nonexistent_dir_is_invalid() {
        assert_eq!(
            check_status(
                Path::new("Z:/definitely/not/a/real/dir_12345"),
                Layout::OpenSteamTool
            ),
            DeployStatus::InvalidPath
        );
    }

    #[test]
    fn empty_dir_is_not_deployed() {
        let dir = steam_dir("empty_dir");
        assert_eq!(
            check_status(&dir, Layout::OpenSteamTool),
            DeployStatus::NotDeployed
        );
        assert_eq!(
            check_status(&dir, Layout::SteamDaddy),
            DeployStatus::NotDeployed
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deploy_status_is_layout_sensitive() {
        let dir = steam_dir("layout_status");
        seed_files(&dir, Layout::SteamDaddy);
        assert_eq!(
            check_status(&dir, Layout::SteamDaddy),
            DeployStatus::Deployed
        );
        assert_eq!(
            check_status(&dir, Layout::OpenSteamTool),
            DeployStatus::NotDeployed,
            "SteamDaddy 的文件表不算 OpenSteamTool 已部署"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn target_dlls_present_is_file_based() {
        let dir = steam_dir("dlls_present");
        assert!(!target_dlls_present(&dir, Layout::OpenSteamTool));
        fs::write(dir.join("OpenSteamTool.dll"), b"x").unwrap();
        assert!(!target_dlls_present(&dir, Layout::OpenSteamTool));
        seed_files(&dir, Layout::OpenSteamTool);
        assert!(target_dlls_present(&dir, Layout::OpenSteamTool));
        fs::write(dir.join(VERSION_FILE), b"9.9.9").unwrap();
        assert!(target_dlls_present(&dir, Layout::OpenSteamTool));
        fs::remove_file(dir.join("dwmapi.dll")).unwrap();
        assert!(!target_dlls_present(&dir, Layout::OpenSteamTool));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn target_dlls_present_rejects_other_layout() {
        let dir = steam_dir("dlls_present_layout");
        seed_files(&dir, Layout::OpenSteamTool);
        assert!(!target_dlls_present(&dir, Layout::SteamDaddy));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn probe_facts_reflects_empty_dirs() {
        let steam = steam_dir("facts_steam");
        let dlldir = steam_dir("facts_dlls");
        let facts = probe_facts(&steam, &dlldir, Layout::OpenSteamTool);
        assert_eq!(facts.status, DeployStatus::NotDeployed);
        assert!(!facts.dlls_present);
        assert_eq!(facts.local_version, None);
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn probe_facts_sees_downloaded_patch() {
        let steam = steam_dir("facts_steam2");
        let dlldir = steam_dir("facts_dlls2");
        seed_files(&dlldir, Layout::OpenSteamTool);
        fs::write(dlldir.join(VERSION_FILE), b"9.9.9").unwrap();
        let facts = probe_facts(&steam, &dlldir, Layout::OpenSteamTool);
        assert!(facts.dlls_present);
        assert_eq!(facts.local_version.as_deref(), Some("9.9.9"));
        assert_eq!(facts.known_local_version(), Some("9.9.9"));
        assert_eq!(facts.status, DeployStatus::NotDeployed);
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn probe_facts_sees_deploy_status() {
        let steam = steam_dir("facts_steam3");
        let dlldir = steam_dir("facts_dlls3");
        seed_files(&steam, Layout::SteamDaddy);
        let facts = probe_facts(&steam, &dlldir, Layout::SteamDaddy);
        assert_eq!(facts.status, DeployStatus::Deployed);
        assert!(!facts.dlls_present);
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn probe_facts_invalid_steam_dir_is_invalid_path() {
        let dlldir = steam_dir("facts_dlls4");
        let facts = probe_facts(
            Path::new("Z:/definitely/not/a/real/dir_12345"),
            &dlldir,
            Layout::OpenSteamTool,
        );
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

    #[test]
    fn lua_dir_follows_layout() {
        let steam = steam_dir("lua_dir");
        assert_eq!(
            lua_dir(&steam, Layout::OpenSteamTool),
            steam.join("config").join("lua")
        );
        assert_eq!(
            lua_dir(&steam, Layout::SteamDaddy),
            steam.join("config").join("stplug-in")
        );
        fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn ensure_lua_dir_creates_missing_dir() {
        let steam = steam_dir("lua_create");
        assert!(!steam.join("config").join("stplug-in").exists());
        ensure_lua_dir(&steam, Layout::SteamDaddy).unwrap();
        assert!(steam.join("config").join("stplug-in").is_dir());
        fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn ensure_lua_dir_is_idempotent() {
        let steam = steam_dir("lua_idem");
        ensure_lua_dir(&steam, Layout::OpenSteamTool).unwrap();
        ensure_lua_dir(&steam, Layout::OpenSteamTool).unwrap();
        ensure_lua_dir(&steam, Layout::OpenSteamTool).unwrap();
        fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn deploy_applies_rename_rules() {
        let steam = steam_dir("deploy_rename");
        let dlldir = steam_dir("deploy_rename_src");
        fs::write(dlldir.join("SteamDaddy.dll"), b"sd").unwrap();
        fs::write(dlldir.join("version.dll.disable"), b"vd").unwrap();
        fs::write(dlldir.join("xinput1_4.dll"), b"xi").unwrap();

        deploy(&dlldir, &steam, Layout::SteamDaddy).unwrap();

        assert_eq!(fs::read(steam.join("version.dll")).unwrap(), b"vd");
        assert!(steam.join("SteamDaddy.dll").is_file());
        assert!(steam.join("config").join("stplug-in").is_dir());
        assert_eq!(
            check_status(&steam, Layout::SteamDaddy),
            DeployStatus::Deployed
        );
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn deploy_removes_files_of_the_other_layout() {
        let steam = steam_dir("deploy_foreign");
        let dlldir = steam_dir("deploy_foreign_src");
        seed_files(&steam, Layout::OpenSteamTool);
        fs::write(steam.join("steam.exe"), b"keep").unwrap();
        seed_release(&dlldir, Layout::SteamDaddy);

        deploy(&dlldir, &steam, Layout::SteamDaddy).unwrap();

        assert!(!steam.join("OpenSteamTool.dll").exists());
        assert!(!steam.join("dwmapi.dll").exists());
        assert!(steam.join("SteamDaddy.dll").is_file());
        assert!(steam.join("steam.exe").is_file(), "非补丁文件不受影响");
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn deploy_fails_when_a_member_is_missing() {
        let steam = steam_dir("deploy_missing");
        let dlldir = steam_dir("deploy_missing_src");
        fs::write(dlldir.join("SteamDaddy.dll"), b"sd").unwrap();
        let err = deploy(&dlldir, &steam, Layout::SteamDaddy).unwrap_err();
        assert!(err.contains("version.dll.disable"), "{err}");
        fs::remove_dir_all(&steam).ok();
        fs::remove_dir_all(&dlldir).ok();
    }

    #[test]
    fn uninstall_removes_only_the_layout_files() {
        let steam = steam_dir("uninstall");
        seed_files(&steam, Layout::SteamDaddy);
        fs::write(steam.join("steam.exe"), b"keep").unwrap();
        fs::write(steam.join("OpenSteamTool.dll"), b"other").unwrap();

        uninstall(&steam, Layout::SteamDaddy).unwrap();

        assert!(!steam.join("SteamDaddy.dll").exists());
        assert!(!steam.join("version.dll").exists());
        assert!(!steam.join("xinput1_4.dll").exists());
        assert!(steam.join("steam.exe").is_file());
        assert!(steam.join("OpenSteamTool.dll").is_file());
        fs::remove_dir_all(&steam).ok();
    }

    #[test]
    fn uninstall_is_idempotent() {
        let steam = steam_dir("uninstall_idem");
        uninstall(&steam, Layout::SteamDaddy).unwrap();
        uninstall(&steam, Layout::SteamDaddy).unwrap();
        fs::remove_dir_all(&steam).ok();
    }
}
