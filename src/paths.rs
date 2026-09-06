//! 双模路径解析：便携模式 vs 安装模式（SPEC §8.3 / §8.6 AC7）。
//!
//! 存储模式在首次路径查询时惰性判定（`resolver()`，探测一次）；测试直接构造
//! `FsPathResolver` + mock probe 断言路径跳转，不依赖全局状态。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 运行环境存储策略：便携模式 vs 系统规范模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    /// 便携模式：exe 同级目录可写，配置/缓存/DLL 资产均收拢于 exe 同级。
    Portable,
    /// 安装模式：exe 同级只读（如 Program Files），配置/缓存回退至 AppData。
    Installed,
}

/// 环境只读探测接缝：解耦物理文件系统权限判定（SPEC §8.5）。
pub trait EnvironmentProbe: Send + Sync {
    fn is_directory_writable(&self, path: &Path) -> bool;
    fn get_appdata_dir(&self) -> Option<PathBuf>;
    fn get_local_appdata_dir(&self) -> Option<PathBuf>;
    fn get_exe_dir(&self) -> Option<PathBuf>;
}

/// 真实环境探测：`%APPDATA%` / `%LOCALAPPDATA%` 环境变量 + 当前 exe 目录 + 写探测。
pub struct RealEnvironmentProbe;

impl EnvironmentProbe for RealEnvironmentProbe {
    fn is_directory_writable(&self, path: &Path) -> bool {
        // 尝试在目录下创建临时文件再删除；任何失败即视为不可写。
        let probe = path.join(format!(".ost_wtest_{}", std::process::id()));
        match std::fs::File::create(&probe) {
            Ok(f) => {
                drop(f);
                let _ = std::fs::remove_file(&probe);
                true
            }
            Err(_) => false,
        }
    }

    fn get_appdata_dir(&self) -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }

    fn get_local_appdata_dir(&self) -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }

    fn get_exe_dir(&self) -> Option<PathBuf> {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }
}

/// 集中式路径解析器契约（SPEC §8.3）。
pub trait PathResolver: Send + Sync {
    /// T2（GuiConfig 持久化）起消费；此前无 bin 调用者。
    #[allow(dead_code)]
    fn mode(&self) -> StorageMode;
    /// GUI 用户偏好配置文件（统一命名 gui_config.toml）。
    fn config_path(&self) -> PathBuf;
    /// 临时验证缓存目录（verified.toml 所在目录）。
    fn cache_dir(&self) -> PathBuf;
    /// 打包自带只读 DLL 目录（`<exe>/dlls`）。
    fn bundled_dll_dir(&self) -> PathBuf;
    /// 在线更新 DLL 写入目标（Portable → `<exe>/dlls`；Installed → `%LOCALAPPDATA%/OpenSteamTool/dlls`）。
    fn update_target_dll_dir(&self) -> PathBuf;
    /// 生效 DLL 目录：更新目录三个目标 DLL 全齐则优先，否则回退自带目录（SPEC §8.2 R6）。
    fn effective_dll_dir(&self) -> PathBuf;
}

/// 基于真实/模拟探测的文件系统路径解析器。
pub struct FsPathResolver {
    mode: StorageMode,
    exe_dir: PathBuf,
    probe: Box<dyn EnvironmentProbe>,
}

impl FsPathResolver {
    /// 按探测结果判定存储模式：exe 同级可写 → Portable，否则 Installed。
    pub fn new(probe: Box<dyn EnvironmentProbe>) -> Self {
        let exe_dir = probe.get_exe_dir().unwrap_or_else(|| PathBuf::from("."));
        let mode = if probe.is_directory_writable(&exe_dir) {
            StorageMode::Portable
        } else {
            StorageMode::Installed
        };
        Self {
            mode,
            exe_dir,
            probe,
        }
    }
}

impl PathResolver for FsPathResolver {
    fn mode(&self) -> StorageMode {
        self.mode
    }

    fn config_path(&self) -> PathBuf {
        match self.mode {
            StorageMode::Portable => self.exe_dir.join("gui_config.toml"),
            StorageMode::Installed => self
                .probe
                .get_appdata_dir()
                .map(|d| d.join("OpenSteamTool").join("gui_config.toml"))
                .unwrap_or_else(|| self.exe_dir.join("gui_config.toml")),
        }
    }

    fn cache_dir(&self) -> PathBuf {
        match self.mode {
            StorageMode::Portable => self.exe_dir.join("cache"),
            StorageMode::Installed => self
                .probe
                .get_local_appdata_dir()
                .map(|d| d.join("OpenSteamTool").join("cache"))
                .unwrap_or_else(|| self.exe_dir.join("cache")),
        }
    }

    fn bundled_dll_dir(&self) -> PathBuf {
        self.exe_dir.join("dlls")
    }

    fn update_target_dll_dir(&self) -> PathBuf {
        match self.mode {
            StorageMode::Portable => self.exe_dir.join("dlls"),
            StorageMode::Installed => self
                .probe
                .get_local_appdata_dir()
                .map(|d| d.join("OpenSteamTool").join("dlls"))
                .unwrap_or_else(|| self.exe_dir.join("dlls")),
        }
    }

    fn effective_dll_dir(&self) -> PathBuf {
        let target = self.update_target_dll_dir();
        if crate::dll::TARGET_DLLS
            .iter()
            .all(|d| target.join(d).is_file())
        {
            target
        } else {
            self.bundled_dll_dir()
        }
    }
}

// 全局装配：首次路径查询时惰性初始化（一次探测）；`init` 显式预置（幂等）。
static RESOLVER: OnceLock<Box<dyn PathResolver>> = OnceLock::new();

/// 显式初始化全局解析器（App 启动时调用；不调用则由 `resolver()` 惰性兜底）。
pub fn init(probe: Box<dyn EnvironmentProbe>) {
    let _ = RESOLVER.set(Box::new(FsPathResolver::new(probe)));
}

/// 获取全局路径解析器（首次调用触发一次存储模式判定）。
pub fn resolver() -> &'static dyn PathResolver {
    RESOLVER
        .get_or_init(|| Box::new(FsPathResolver::new(Box::new(RealEnvironmentProbe))))
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试桩：可控环境探测（SPEC §8.5 MockEnvironmentProbe）。
    struct MockProbe {
        writable: bool,
        appdata: Option<PathBuf>,
        local_appdata: Option<PathBuf>,
        exe_dir: Option<PathBuf>,
    }

    impl MockProbe {
        fn portable(exe_dir: PathBuf) -> Self {
            Self {
                writable: true,
                appdata: Some(PathBuf::from("C:/AppData/Roaming")),
                local_appdata: Some(PathBuf::from("C:/AppData/Local")),
                exe_dir: Some(exe_dir),
            }
        }

        fn installed(exe_dir: PathBuf) -> Self {
            Self {
                writable: false,
                appdata: Some(PathBuf::from("C:/AppData/Roaming")),
                local_appdata: Some(PathBuf::from("C:/AppData/Local")),
                exe_dir: Some(exe_dir),
            }
        }
    }

    impl EnvironmentProbe for MockProbe {
        fn is_directory_writable(&self, _path: &Path) -> bool {
            self.writable
        }
        fn get_appdata_dir(&self) -> Option<PathBuf> {
            self.appdata.clone()
        }
        fn get_local_appdata_dir(&self) -> Option<PathBuf> {
            self.local_appdata.clone()
        }
        fn get_exe_dir(&self) -> Option<PathBuf> {
            self.exe_dir.clone()
        }
    }

    #[test]
    fn mode_jumps_between_portable_and_installed() {
        let exe = PathBuf::from("C:/Tool");
        let portable = FsPathResolver::new(Box::new(MockProbe::portable(exe.clone())));
        assert_eq!(portable.mode(), StorageMode::Portable);
        let installed = FsPathResolver::new(Box::new(MockProbe::installed(exe)));
        assert_eq!(installed.mode(), StorageMode::Installed);
    }

    #[test]
    fn portable_mode_paths_stay_next_to_exe() {
        let exe = PathBuf::from("C:/Tool");
        let r = FsPathResolver::new(Box::new(MockProbe::portable(exe.clone())));
        assert_eq!(r.config_path(), PathBuf::from("C:/Tool/gui_config.toml"));
        assert_eq!(r.cache_dir(), PathBuf::from("C:/Tool/cache"));
        assert_eq!(r.bundled_dll_dir(), PathBuf::from("C:/Tool/dlls"));
        assert_eq!(r.update_target_dll_dir(), PathBuf::from("C:/Tool/dlls"));
    }

    #[test]
    fn installed_mode_paths_fall_back_to_appdata() {
        let exe = PathBuf::from("C:/Program Files/OpenSteamTool");
        let r = FsPathResolver::new(Box::new(MockProbe::installed(exe.clone())));
        assert_eq!(
            r.config_path(),
            PathBuf::from("C:/AppData/Roaming/OpenSteamTool/gui_config.toml")
        );
        assert_eq!(
            r.cache_dir(),
            PathBuf::from("C:/AppData/Local/OpenSteamTool/cache")
        );
        assert_eq!(
            r.bundled_dll_dir(),
            PathBuf::from("C:/Program Files/OpenSteamTool/dlls")
        );
        assert_eq!(
            r.update_target_dll_dir(),
            PathBuf::from("C:/AppData/Local/OpenSteamTool/dlls")
        );
    }

    #[test]
    fn installed_mode_without_appdata_falls_back_to_exe() {
        // AppData 缺失（极罕见）：Installed 语义下兜底 exe 同级，避免 panic。
        let exe = PathBuf::from("C:/Program Files/OpenSteamTool");
        let probe = MockProbe {
            writable: false,
            appdata: None,
            local_appdata: None,
            exe_dir: Some(exe.clone()),
        };
        let r = FsPathResolver::new(Box::new(probe));
        assert_eq!(
            r.config_path(),
            PathBuf::from("C:/Program Files/OpenSteamTool/gui_config.toml")
        );
        assert_eq!(
            r.cache_dir(),
            PathBuf::from("C:/Program Files/OpenSteamTool/cache")
        );
        assert_eq!(
            r.update_target_dll_dir(),
            PathBuf::from("C:/Program Files/OpenSteamTool/dlls")
        );
    }

    #[test]
    fn effective_prefers_update_dir_when_all_target_dlls_present() {
        // Installed：更新目录三 DLL 全齐 → effective = 更新目录。
        let exe = std::env::temp_dir().join(format!("ost_paths_exe_{}", std::process::id()));
        let local = std::env::temp_dir().join(format!("ost_paths_local_{}", std::process::id()));
        std::fs::create_dir_all(&exe).unwrap();
        std::fs::create_dir_all(local.join("OpenSteamTool").join("dlls")).unwrap();
        let probe = MockProbe {
            writable: false,
            appdata: None,
            local_appdata: Some(local.clone()),
            exe_dir: Some(exe),
        };
        let r = FsPathResolver::new(Box::new(probe));
        let target = local.join("OpenSteamTool").join("dlls");
        for d in crate::dll::TARGET_DLLS {
            std::fs::write(target.join(d), b"x").unwrap();
        }
        assert_eq!(r.effective_dll_dir(), target);
        std::fs::remove_dir_all(&local).ok();
    }

    #[test]
    fn effective_falls_back_to_bundled_when_update_incomplete() {
        // Installed：更新目录仅 1 个 DLL → 不完整 → 回退自带目录。
        let exe = std::env::temp_dir().join(format!("ost_paths_exe2_{}", std::process::id()));
        let local = std::env::temp_dir().join(format!("ost_paths_local2_{}", std::process::id()));
        std::fs::create_dir_all(&exe).unwrap();
        std::fs::create_dir_all(&exe.join("dlls")).unwrap();
        std::fs::create_dir_all(local.join("OpenSteamTool").join("dlls")).unwrap();
        std::fs::write(
            local
                .join("OpenSteamTool")
                .join("dlls")
                .join(crate::dll::TARGET_DLLS[0]),
            b"x",
        )
        .unwrap();
        let probe = MockProbe {
            writable: false,
            appdata: None,
            local_appdata: Some(local),
            exe_dir: Some(exe.clone()),
        };
        let r = FsPathResolver::new(Box::new(probe));
        assert_eq!(r.effective_dll_dir(), exe.join("dlls"));
        std::fs::remove_dir_all(&exe).ok();
    }

    #[test]
    fn real_probe_detects_writable_vs_missing_dir() {
        let writable = std::env::temp_dir().join(format!("ost_probe_w_{}", std::process::id()));
        std::fs::create_dir_all(&writable).unwrap();
        assert!(RealEnvironmentProbe.is_directory_writable(&writable));
        let missing = std::env::temp_dir().join(format!("ost_probe_m_{}", std::process::id()));
        std::fs::remove_dir_all(&missing).ok();
        assert!(!RealEnvironmentProbe.is_directory_writable(&missing));
        std::fs::remove_dir_all(&writable).ok();
    }
}
