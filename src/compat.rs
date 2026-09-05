//! Steam 核心版本健康度体检：核心 DLL 哈希、Pattern/IPC 通道映射与本地缓存检查。
//!
//! 上游 OpenSteamTool 按通道（`{channel}` = 上游仓库分支名 `pattern`/`ipc`）从
//! `OpenSteam001/steam-monitor` 拉取匹配的 TOML（特征码 / IPC 规约），并按
//! `<Steam>/opensteamtool/{channel}/{component}/<sha256>.toml` 落盘缓存。
//! 本模块先落地本地部分（类型、哈希、路径映射）；网络探针与下载在后续增量中实现。

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 探测目标：本地 Steam 核心 DLL 的通道/组件映射。
///
/// T1 仅落地类型骨架，由 T3（探测链路）构造消费。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeTarget {
    /// steamclient64.dll 的特征码（pattern 通道）。
    PatternSteamClient,
    /// steamui.dll 的特征码（pattern 通道）。
    PatternSteamUi,
    /// steamclient64.dll 的 IPC 规约（ipc 通道）。
    IpcSteamClient,
}

/// 骨架 API，由 T3（探测链路）消费。
#[allow(dead_code)]
impl ProbeTarget {
    /// 通道名，即上游 `steam-monitor` 仓库分支名（`pattern` / `ipc`）。
    pub fn channel(self) -> &'static str {
        match self {
            Self::PatternSteamClient | Self::PatternSteamUi => "pattern",
            Self::IpcSteamClient => "ipc",
        }
    }

    /// 组件名（上游仓库一级目录）。
    pub fn component(self) -> &'static str {
        match self {
            Self::PatternSteamClient | Self::IpcSteamClient => "steamclient",
            Self::PatternSteamUi => "steamui",
        }
    }

    /// Steam 目录下的相对 DLL 路径。
    pub fn relative_dll(self) -> &'static str {
        match self {
            Self::PatternSteamClient | Self::IpcSteamClient => "steamclient64.dll",
            Self::PatternSteamUi => "steamui.dll",
        }
    }
}

/// 单项目探针状态。
///
/// T1 仅落地类型骨架，由 T3（探测链路）构造消费。
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeStatus {
    /// 体检进行中（骨架态，不阻塞 UI）。
    Checking,
    /// 上游已适配；`cached` 表示本地缓存是否已就绪。
    RemoteAvailable { cached: bool },
    /// 上游不可达/未适配，但本地缓存可用（离线模式就绪）。
    CompatibleOffline,
    /// 上游尚未适配且本地无缓存。
    IncompatiblePending,
    /// 网络超时/连接错误（含 URL 解析失败）。
    NetworkError(String),
    /// 本地核心 DLL 缺失。
    FileNotFound,
}

/// 单项目探针报告。
///
/// T1 仅落地类型骨架，由 T3（探测链路）构造消费。
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ProbeReport {
    pub target: ProbeTarget,
    pub sha256: Option<String>,
    pub status: ProbeStatus,
    pub cache_path: PathBuf,
}

/// 三项探针的综合健康度报告。
///
/// T1 仅落地类型骨架，由 T3（探测链路）构造消费。
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OverallHealthReport {
    pub steamclient_pattern: ProbeReport,
    pub steamui_pattern: ProbeReport,
    pub steamclient_ipc: ProbeReport,
    /// 三项均适配且本地缓存齐全。
    pub is_all_compatible: bool,
    /// 存在「上游已适配但本地未缓存」的项（提示预热）。
    pub has_missing_cache: bool,
}

/// 计算文件的 SHA-256（64 位小写十六进制），64KB 缓冲分块流式读取，
///
/// 骨架 API，由 T3（探测链路）消费。
#[allow(dead_code)]
/// 避免大 DLL 一次性载入内存。
pub fn sha256_of_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 缓存目录：`<Steam>/opensteamtool/{channel}/{component}/`。
///
/// 骨架 API，由 T3（探测链路）消费。
#[allow(dead_code)]
pub fn cache_dir(steam_dir: &Path, target: ProbeTarget) -> PathBuf {
    steam_dir
        .join("opensteamtool")
        .join(target.channel())
        .join(target.component())
}

/// 缓存文件路径：`<Steam>/opensteamtool/{channel}/{component}/<sha256>.toml`。
///
/// 骨架 API，由 T3（探测链路）消费。
#[allow(dead_code)]
pub fn cache_path(steam_dir: &Path, target: ProbeTarget, sha256: &str) -> PathBuf {
    cache_dir(steam_dir, target).join(format!("{sha256}.toml"))
}

/// 本地缓存是否已就绪（对应哈希的 TOML 文件已存在）。
///
/// 骨架 API，由 T3（探测链路）消费。
#[allow(dead_code)]
pub fn is_cached(steam_dir: &Path, target: ProbeTarget, sha256: &str) -> bool {
    cache_path(steam_dir, target, sha256).is_file()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// 探测目标三方法映射（对齐 SPEC.md §7.3 通道映射表）。
    #[test]
    fn probe_target_maps_channel_component_dll() {
        assert_eq!(ProbeTarget::PatternSteamClient.channel(), "pattern");
        assert_eq!(ProbeTarget::PatternSteamUi.channel(), "pattern");
        assert_eq!(ProbeTarget::IpcSteamClient.channel(), "ipc");
        assert_eq!(ProbeTarget::PatternSteamClient.component(), "steamclient");
        assert_eq!(ProbeTarget::PatternSteamUi.component(), "steamui");
        assert_eq!(ProbeTarget::IpcSteamClient.component(), "steamclient");
        assert_eq!(
            ProbeTarget::PatternSteamClient.relative_dll(),
            "steamclient64.dll"
        );
        assert_eq!(ProbeTarget::PatternSteamUi.relative_dll(), "steamui.dll");
        assert_eq!(ProbeTarget::IpcSteamClient.relative_dll(), "steamclient64.dll");
    }

    /// 标准向量验证：SHA-256("abc")。
    #[test]
    fn sha256_of_file_matches_known_vector() {
        let dir = std::env::temp_dir().join(format!("ost_compat_vec_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dll.bin");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_of_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_dir_all(&dir).ok();
    }

    /// 跨 64KB 缓冲块的大文件流式哈希（期望值一次性算出，验证分块拼接逻辑）。
    #[test]
    fn sha256_of_file_streams_large_input() {
        let dir = std::env::temp_dir().join(format!("ost_compat_stream_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.dll");
        let content = vec![0xABu8; 200 * 1024];
        fs::write(&path, &content).unwrap();
        let expected = format!("{:x}", Sha256::digest(&content));
        assert_eq!(sha256_of_file(&path).unwrap(), expected);
        fs::remove_dir_all(&dir).ok();
    }

    /// 缺失文件报错。
    #[test]
    fn sha256_of_file_missing_errors() {
        assert!(sha256_of_file(Path::new("Z:/no/such/file_12345.bin")).is_err());
    }

    /// 缓存路径布局（对齐 SPEC.md §7.3 本地缓存持久化路径）。
    #[test]
    fn cache_path_layout_matches_spec() {
        let steam = Path::new("F:/Steam");
        let expect = |target: ProbeTarget, sha: &str| {
            steam
                .join("opensteamtool")
                .join(target.channel())
                .join(target.component())
                .join(format!("{sha}.toml"))
        };
        assert_eq!(
            cache_path(steam, ProbeTarget::PatternSteamClient, "abc"),
            expect(ProbeTarget::PatternSteamClient, "abc")
        );
        assert_eq!(
            cache_path(steam, ProbeTarget::PatternSteamUi, "abc"),
            expect(ProbeTarget::PatternSteamUi, "abc")
        );
        assert_eq!(
            cache_path(steam, ProbeTarget::IpcSteamClient, "abc"),
            expect(ProbeTarget::IpcSteamClient, "abc")
        );
        // 文件名形态固定为 <sha256>.toml。
        assert_eq!(
            cache_path(steam, ProbeTarget::PatternSteamClient, "abc")
                .file_name()
                .unwrap(),
            "abc.toml"
        );
    }

    /// 缓存目录形态：`<Steam>/opensteamtool/{channel}/{component}`。
    #[test]
    fn cache_dir_matches_layout() {
        let steam = Path::new("F:/Steam");
        assert_eq!(
            cache_dir(steam, ProbeTarget::IpcSteamClient),
            steam.join("opensteamtool").join("ipc").join("steamclient")
        );
    }

    /// 已存在的缓存文件命中。
    #[test]
    fn cache_hit_detects_existing_file() {
        let dir = std::env::temp_dir().join(format!("ost_compat_hit_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = ProbeTarget::PatternSteamClient;
        let path = cache_path(&dir, target, "deadbeef");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"[patterns]").unwrap();
        assert!(is_cached(&dir, target, "deadbeef"));
        fs::remove_dir_all(&dir).ok();
    }

    /// 缺失缓存未命中（含目录不存在的情形）。
    #[test]
    fn cache_miss_when_missing() {
        let dir = std::env::temp_dir().join(format!("ost_compat_miss_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert!(!is_cached(&dir, ProbeTarget::PatternSteamClient, "deadbeef"));
        assert!(!is_cached(&dir, ProbeTarget::PatternSteamUi, "deadbeef"));
        assert!(!is_cached(&dir, ProbeTarget::IpcSteamClient, "deadbeef"));
        fs::remove_dir_all(&dir).ok();
    }
}
