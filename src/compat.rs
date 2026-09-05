//! Steam 核心版本健康度体检：核心 DLL 哈希、Pattern/IPC 通道映射与本地缓存检查。
//!
//! 上游 OpenSteamTool 按通道（`{channel}` = 上游仓库分支名 `pattern`/`ipc`）从
//! `OpenSteam001/steam-monitor` 拉取匹配的 TOML（特征码 / IPC 规约），并按
//! `<Steam>/opensteamtool/{channel}/{component}/<sha256>.toml` 落盘缓存。
//! 本模块实现本地部分（类型、哈希、路径映射）与远程探针链路（镜像链 HEAD 判定）；
//! 缓存预热下载（T4）与 UI 集成（T5）在后续增量中实现。

use std::fs::{self, File};
use std::io::{self, Read};
use std::time::Duration;

use ureq::Agent;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 探测目标：本地 Steam 核心 DLL 的通道/组件映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeTarget {
    /// steamclient64.dll 的特征码（pattern 通道）。
    PatternSteamClient,
    /// steamui.dll 的特征码（pattern 通道）。
    PatternSteamUi,
    /// steamclient64.dll 的 IPC 规约（ipc 通道）。
    IpcSteamClient,
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeStatus {
    /// 体检进行中（骨架态，不阻塞 UI）。
    ///
    /// T5（UI 集成）初始化时构造；T3 探测路径不产出该状态。
    #[allow(dead_code)]
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
/// 字段由 T5（UI 集成）读取（详情弹窗展示）。
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ProbeReport {
    pub target: ProbeTarget,
    pub sha256: Option<String>,
    pub status: ProbeStatus,
    pub cache_path: PathBuf,
}

/// 三项探针的综合健康度报告。
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
pub fn cache_dir(steam_dir: &Path, target: ProbeTarget) -> PathBuf {
    steam_dir
        .join("opensteamtool")
        .join(target.channel())
        .join(target.component())
}

/// 缓存文件路径：`<Steam>/opensteamtool/{channel}/{component}/<sha256>.toml`。
pub fn cache_path(steam_dir: &Path, target: ProbeTarget, sha256: &str) -> PathBuf {
    cache_dir(steam_dir, target).join(format!("{sha256}.toml"))
}

/// 本地缓存是否已就绪（对应哈希的 TOML 文件已存在）。
pub fn is_cached(steam_dir: &Path, target: ProbeTarget, sha256: &str) -> bool {
    cache_path(steam_dir, target, sha256).is_file()
}

/// 远程探针结果（网络层内部枚举，与 `ProbeStatus` 的对应见 `decide`）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum RemoteOutcome {
    /// 任一镜像返回 2xx。
    Found,
    /// 所有可达镜像均 404（服务器明确无此文件）。
    NotFound404,
    /// 网络错误/非 404 状态码（结果不确定）。
    Error(String),
}

/// 构造探测 URL 链：自定义模板存在时**替代**内置源（仅返回自定义 URL）；
/// 否则 GitHub Raw → jsDelivr 两链（SPEC.md §7.4）。
///
/// 占位符替换：`{channel}` / `{component}` / `{sha256}`。
fn build_urls(template: Option<&str>, target: ProbeTarget, sha256: &str) -> Vec<String> {
    let (channel, component) = (target.channel(), target.component());
    let substitute = |t: &str| {
        t.replace("{channel}", channel)
            .replace("{component}", component)
            .replace("{sha256}", sha256)
    };
    match template {
        Some(t) => vec![substitute(t)],
        None => vec![
            substitute("https://raw.githubusercontent.com/OpenSteam001/steam-monitor/{channel}/{component}/{sha256}.toml"),
            substitute("https://fast.jsdelivr.net/gh/OpenSteam001/steam-monitor@{channel}/{component}/{sha256}.toml"),
        ],
    }
}

/// 决策矩阵（SPEC.md §7.5）：本地缓存状态 × 远程结果 → 探针状态。
///
/// | 远程 \ 本地 | cached | 无缓存 |
/// |---|---|---|
/// | Found | RemoteAvailable{cached} | RemoteAvailable{cached} |
/// | 404 | CompatibleOffline | IncompatiblePending |
/// | Error | CompatibleOffline | NetworkError |
fn decide(cached: bool, remote: RemoteOutcome) -> ProbeStatus {
    match remote {
        RemoteOutcome::Found => ProbeStatus::RemoteAvailable { cached },
        RemoteOutcome::NotFound404 if cached => ProbeStatus::CompatibleOffline,
        RemoteOutcome::NotFound404 => ProbeStatus::IncompatiblePending,
        RemoteOutcome::Error(_) if cached => ProbeStatus::CompatibleOffline,
        RemoteOutcome::Error(msg) => ProbeStatus::NetworkError(msg),
    }
}

/// 探针专用 agent：连接/总超时各 2.5s 快速失败（updater 的 10s/30s 不适合探针；
/// 2.5s 为无网络时的等待上限，配合快速体检零网络语义）。
fn probe_agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(Duration::from_millis(2500)))
        .timeout_global(Some(Duration::from_millis(2500)))
        .build()
        .into()
}

/// 单 URL HEAD 探针：2xx → Found；404 → NotFound404；其余状态码/传输错误 → Error。
fn head_probe(agent: &Agent, url: &str) -> RemoteOutcome {
    match agent.head(url).call() {
        Ok(_) => RemoteOutcome::Found,
        Err(ureq::Error::StatusCode(404)) => RemoteOutcome::NotFound404,
        Err(ureq::Error::StatusCode(code)) => RemoteOutcome::Error(format!("HTTP {code}")),
        Err(e) => RemoteOutcome::Error(e.to_string()),
    }
}

/// 沿 URL 链探测：命中首个 2xx 即返回；404 为权威信号（优先于网络错误）；
/// 全部不可达 → Error / 全 404 → NotFound404。
fn probe_urls(head: impl Fn(&str) -> RemoteOutcome, urls: &[String]) -> RemoteOutcome {
    let mut any_404 = false;
    let mut last_err: Option<String> = None;
    for url in urls {
        match head(url) {
            RemoteOutcome::Found => return RemoteOutcome::Found,
            RemoteOutcome::NotFound404 => any_404 = true,
            RemoteOutcome::Error(msg) => last_err = Some(msg),
        }
    }
    if any_404 {
        RemoteOutcome::NotFound404
    } else {
        last_err
            .map(RemoteOutcome::Error)
            .unwrap_or(RemoteOutcome::NotFound404)
    }
}

/// 单项目探针：本地（哈希 → 缓存命中）→ 远程（镜像链 HEAD）→ 报告。
/// `sha` 由调用方预计算（`probe_all_with` 对 steamclient64.dll 只算一次，Pattern/IPC 复用）。
/// `network=false`（快速体检）时**完全零网络**，优先级：签名缓存命中 → `CompatibleOffline`
/// （绿，离线可用）；验证缓存命中 → `RemoteAvailable{cached:true}`（绿，上次已验证适配）；
/// 均未命中 → 乐观 `RemoteAvailable{cached:false}`（琥珀可预热）。网络适配状态由
/// `probe_all_refresh` 后台补齐并写入验证缓存（SPEC.md §7.5 增强）。
fn probe_one(
    steam_dir: &Path,
    target: ProbeTarget,
    template: Option<&str>,
    head: impl Fn(&str) -> RemoteOutcome,
    network: bool,
    sha: Option<String>,
    verified: &std::collections::HashMap<ProbeTarget, String>,
) -> ProbeReport {
    let Some(sha) = sha else {
        return ProbeReport {
            target,
            sha256: None,
            status: ProbeStatus::FileNotFound,
            cache_path: cache_dir(steam_dir, target),
        };
    };
    let cached = is_cached(steam_dir, target, &sha);
    let urls = build_urls(template, target, &sha);
    let status = if network {
        decide(cached, probe_urls(&head, &urls))
    } else if cached {
        // 签名缓存命中：本地齐全，离线可用。
        ProbeStatus::CompatibleOffline
    } else if verified.get(&target).is_some_and(|v| v == &sha) {
        // 验证缓存命中：上次已确认上游适配（哈希未变，结论仍成立）。
        ProbeStatus::RemoteAvailable { cached: true }
    } else {
        // 均未命中：乐观假定「上游已适配 (未缓存)」，后台刷新确认后写入验证缓存。
        ProbeStatus::RemoteAvailable { cached: false }
    };
    ProbeReport {
        target,
        sha256: Some(sha.clone()),
        status,
        cache_path: cache_path(steam_dir, target, &sha),
    }
}

/// 三探针聚合（注入 head，供测试离线覆盖决策矩阵）。
fn probe_all_with(
    steam_dir: &Path,
    template: Option<&str>,
    head: impl Fn(&str) -> RemoteOutcome,
    network: bool,
    verified: &std::collections::HashMap<ProbeTarget, String>,
) -> OverallHealthReport {
    // 哈希去重：steamclient64.dll 由 Pattern 与 IPC 两项共享，只算一次（~25MB×2 → ×1）。
    let sc_sha = sha256_of_file(&steam_dir.join(ProbeTarget::PatternSteamClient.relative_dll())).ok();
    let ui_sha = sha256_of_file(&steam_dir.join(ProbeTarget::PatternSteamUi.relative_dll())).ok();
    let steamclient_pattern = probe_one(steam_dir, ProbeTarget::PatternSteamClient, template, &head, network, sc_sha.clone(), verified);
    let steamui_pattern = probe_one(steam_dir, ProbeTarget::PatternSteamUi, template, &head, network, ui_sha.clone(), verified);
    let steamclient_ipc = probe_one(steam_dir, ProbeTarget::IpcSteamClient, template, &head, network, sc_sha.clone(), verified);
    let reports = [&steamclient_pattern, &steamui_pattern, &steamclient_ipc];
    // Fully Compatible：每项均已适配且本地缓存齐全（SPEC.md §7.5）。
    let is_all_compatible = reports.iter().all(|r| {
        matches!(
            r.status,
            ProbeStatus::RemoteAvailable { cached: true } | ProbeStatus::CompatibleOffline
        )
    });
    // 存在「上游已适配但本地未缓存」的项 → 提示预热。
    let has_missing_cache = reports
        .iter()
        .any(|r| matches!(r.status, ProbeStatus::RemoteAvailable { cached: false }));
    OverallHealthReport {
        steamclient_pattern,
        steamui_pattern,
        steamclient_ipc,
        is_all_compatible,
        has_missing_cache,
    }
}

/// 快速体检（启动/路径变更入口）：签名缓存/验证缓存命中项零网络，立即出绿；
/// 均未命中项乐观琥珀，后台刷新确认后写入验证缓存。
pub fn probe_all(steam_dir: &Path) -> OverallHealthReport {
    let template = crate::config_editor::remote_url_template(steam_dir);
    let agent = probe_agent();
    let verified = read_verified(&tool_cache_dir());
    probe_all_with(steam_dir, template.as_deref(), |url| head_probe(&agent, url), false, &verified)
}

/// 全量体检（后台网络刷新）：补查镜像链 HEAD，确认适配的项写入验证缓存（下次启动直接绿）。
pub fn probe_all_refresh(steam_dir: &Path) -> OverallHealthReport {
    let template = crate::config_editor::remote_url_template(steam_dir);
    let agent = probe_agent();
    let verified = read_verified(&tool_cache_dir());
    let report = probe_all_with(steam_dir, template.as_deref(), |url| head_probe(&agent, url), true, &verified);
    // 网络确认 Found 的项持久化为验证缓存；写失败静默（不阻塞 UI 刷新）。
    let entries = verified_from_report(&report);
    let _ = write_verified(&tool_cache_dir(), &entries);
    report
}

/// 工具自身目录（exe 旁）：`dlls/` 同级，验证缓存存放于此（`cache/` 子目录，
/// 后续工具状态类文件可复用同一目录）。
fn tool_cache_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cache")
}

/// 验证缓存文件路径：`<exe>/cache/verified.toml`。
fn verified_cache_path(tool_dir: &Path) -> PathBuf {
    tool_dir.join("verified.toml")
}

/// 读取验证缓存（{target → 已验证适配的 sha256}）；缺失/损坏 → 空表（视为从未验证）。
fn read_verified(tool_dir: &Path) -> std::collections::HashMap<ProbeTarget, String> {
    use std::collections::HashMap;
    let Ok(text) = std::fs::read_to_string(verified_cache_path(tool_dir)) else {
        return HashMap::new();
    };
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for target in [ProbeTarget::PatternSteamClient, ProbeTarget::PatternSteamUi, ProbeTarget::IpcSteamClient] {
        if let Some(sha) = doc
            .get("verified")
            .and_then(|t| t.get(target_key(target)))
            .and_then(|v| v.as_str())
        {
            map.insert(target, sha.to_string());
        }
    }
    map
}

/// 从全量报告提取「网络确认适配」的项（RemoteAvailable 无论 cached 均视为确认）。
fn verified_from_report(report: &OverallHealthReport) -> Vec<(ProbeTarget, String)> {
    [
        &report.steamclient_pattern,
        &report.steamui_pattern,
        &report.steamclient_ipc,
    ]
    .iter()
    .filter_map(|r| match &r.status {
        ProbeStatus::RemoteAvailable { .. } => r.sha256.clone().map(|sha| (r.target, sha)),
        _ => None,
    })
    .collect()
}

/// 写入验证缓存（原子写）；目标键名与 SPEC 通道映射一致。
fn write_verified(tool_dir: &Path, entries: &[(ProbeTarget, String)]) -> Result<(), CompatError> {
    if entries.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(tool_dir).map_err(|e| CompatError::Io(format!("create cache dir: {e}")))?;
    let mut text = String::from("[verified]\n");
    for (target, sha) in entries {
        text.push_str(&format!("{} = \"{sha}\"\n", target_key(*target)));
    }
    crate::fsutil::write_atomic(&verified_cache_path(tool_dir), text.as_bytes())
        .map_err(|e| CompatError::Io(format!("write verified: {e}")))?;
    Ok(())
}

/// TOML 键名（= target 枚举名的小写蛇形，SPEC §7.3 通道映射）。
fn target_key(target: ProbeTarget) -> &'static str {
    match target {
        ProbeTarget::PatternSteamClient => "steamclient_pattern",
        ProbeTarget::PatternSteamUi => "steamui_pattern",
        ProbeTarget::IpcSteamClient => "steamclient_ipc",
    }
}

/// 预热下载错误，UI 层据此映射双语文案。
#[derive(Clone, Debug)]
pub enum CompatError {
    /// 网络请求失败（HTTP 非 2xx / 传输错误）。
    Network(String),
    /// 本地文件操作失败。
    Io(String),
}

impl std::fmt::Display for CompatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompatError::Network(d) => write!(f, "network: {d}"),
            CompatError::Io(d) => write!(f, "io: {d}"),
        }
    }
}

/// 沿镜像链下载首个 2xx 的 TOML 内容（自定义模板替代语义与探测一致）。
fn download_first(urls: &[String]) -> Result<Vec<u8>, CompatError> {
    let agent = crate::updater::download_agent();
    let mut last_err: Option<CompatError> = None;
    for url in urls {
        match agent.get(url).call() {
            Ok(resp) => {
                let body = resp.into_body().read_to_vec()
                    .map_err(|e| CompatError::Network(format!("read body: {e}")))?;
                return Ok(body);
            }
            Err(ureq::Error::StatusCode(404)) => {
                last_err = Some(CompatError::Network(format!("HTTP 404: {url}")));
            }
            Err(e) => last_err = Some(CompatError::Network(e.to_string())),
        }
    }
    Err(last_err
        .unwrap_or_else(|| CompatError::Network("no URLs".into())))
}

/// 持久化 TOML 到本地缓存：建目录 + 原子写（避免半截文件被上游热重载读到）。
fn write_cache_file(
    steam_dir: &Path,
    target: ProbeTarget,
    sha256: &str,
    body: &[u8],
) -> Result<(), CompatError> {
    let path = cache_path(steam_dir, target, sha256);
    let dir = path
        .parent()
        .ok_or_else(|| CompatError::Io("no parent dir".into()))?;
    fs::create_dir_all(dir).map_err(|e| CompatError::Io(format!("create dir: {e}")))?;
    crate::fsutil::write_atomic(&path, body)
        .map_err(|e| CompatError::Io(format!("write {}: {e}", path.display())))?;
    Ok(())
}

/// 预热单个目标的签名缓存：下载 → 持久化。
///
/// T4 暴露给 T5（UI 集成）的入口，T5 接入前保持 allow。
#[allow(dead_code)]
pub fn precache(steam_dir: &Path, target: ProbeTarget, sha256: &str) -> Result<(), CompatError> {
    let template = crate::config_editor::remote_url_template(steam_dir);
    let urls = build_urls(template.as_deref(), target, sha256);
    let body = download_first(&urls)?;
    write_cache_file(steam_dir, target, sha256, &body)
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
    /// 伪造 Steam 目录：写两个核心 DLL，返回 (dir, steamclient64 sha, steamui sha)。
    fn fake_steam_dir(tag: &str) -> (PathBuf, String, String) {
        let dir = std::env::temp_dir().join(format!("ost_compat_probe_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("steamclient64.dll"), b"client-bytes").unwrap();
        fs::write(dir.join("steamui.dll"), b"ui-bytes").unwrap();
        let sc_sha = sha256_of_file(&dir.join("steamclient64.dll")).unwrap();
        let ui_sha = sha256_of_file(&dir.join("steamui.dll")).unwrap();
        (dir, sc_sha, ui_sha)
    }

    /// 写缓存文件（含父目录）。
    fn write_cache(dir: &Path, target: ProbeTarget, sha: &str) {
        let path = cache_path(dir, target, sha);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"[x]").unwrap();
    }

    /// 无模板：GitHub Raw → jsDelivr 两链（SPEC.md §7.4 官方默认链路）。
    #[test]
    fn build_urls_default_chain() {
        let urls = build_urls(None, ProbeTarget::PatternSteamClient, "abc123");
        assert_eq!(urls.len(), 2);
        assert_eq!(
            urls[0],
            "https://raw.githubusercontent.com/OpenSteam001/steam-monitor/pattern/steamclient/abc123.toml"
        );
        assert_eq!(
            urls[1],
            "https://fast.jsdelivr.net/gh/OpenSteam001/steam-monitor@pattern/steamclient/abc123.toml"
        );
    }

    /// 自定义模板：替代内置源（仅返回自定义 URL，不回退 GitHub/jsDelivr）。
    #[test]
    fn build_urls_custom_template_replaces_sources() {
        let urls = build_urls(
            Some("https://my.mirror/{channel}/{component}/{sha256}.toml"),
            ProbeTarget::IpcSteamClient,
            "def456",
        );
        assert_eq!(
            urls,
            vec!["https://my.mirror/ipc/steamclient/def456.toml".to_string()]
        );
    }

    /// 决策矩阵全分支（SPEC.md §7.5）。
    #[test]
    fn decide_matrix_covers_all_branches() {
        // Found → RemoteAvailable（cached 透传）。
        assert_eq!(
            decide(true, RemoteOutcome::Found),
            ProbeStatus::RemoteAvailable { cached: true }
        );
        assert_eq!(
            decide(false, RemoteOutcome::Found),
            ProbeStatus::RemoteAvailable { cached: false }
        );
        // 404 + cached → CompatibleOffline；404 + 无缓存 → IncompatiblePending。
        assert_eq!(
            decide(true, RemoteOutcome::NotFound404),
            ProbeStatus::CompatibleOffline
        );
        assert_eq!(
            decide(false, RemoteOutcome::NotFound404),
            ProbeStatus::IncompatiblePending
        );
        // Error + cached → CompatibleOffline；Error + 无缓存 → NetworkError。
        assert_eq!(
            decide(true, RemoteOutcome::Error("boom".into())),
            ProbeStatus::CompatibleOffline
        );
        assert_eq!(
            decide(false, RemoteOutcome::Error("boom".into())),
            ProbeStatus::NetworkError("boom".into())
        );
    }

    /// 镜像链 404 权威性：404 + Error 混合 → 404（服务器可达即权威）。
    #[test]
    fn probe_urls_prefers_404_over_error() {
        let urls = vec!["a".to_string(), "b".to_string()];
        let outcome = probe_urls(
            |u| match u {
                "a" => RemoteOutcome::NotFound404,
                _ => RemoteOutcome::Error("timeout".into()),
            },
            &urls,
        );
        assert_eq!(outcome, RemoteOutcome::NotFound404);
    }

    /// 缺失 DLL → 三 FileNotFound（聚合指标为 false）。
    #[test]
    fn probe_missing_dlls_yields_file_not_found() {
        let dir = std::env::temp_dir().join(format!("ost_compat_nodll_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let report = probe_all_with(&dir, None, |_| RemoteOutcome::Found, true, &std::collections::HashMap::new());
        assert_eq!(report.steamclient_pattern.status, ProbeStatus::FileNotFound);
        assert_eq!(report.steamui_pattern.status, ProbeStatus::FileNotFound);
        assert_eq!(report.steamclient_ipc.status, ProbeStatus::FileNotFound);
        assert!(!report.is_all_compatible);
        assert!(!report.has_missing_cache);
        let _ = fs::remove_dir_all(&dir);
    }

    /// DLL + 全缓存 + 404 → CompatibleOffline，is_all_compatible = true。
    #[test]
    fn probe_offline_compatible_when_all_cached() {
        let (dir, sc_sha, ui_sha) = fake_steam_dir("offline");
        write_cache(&dir, ProbeTarget::PatternSteamClient, &sc_sha);
        write_cache(&dir, ProbeTarget::PatternSteamUi, &ui_sha);
        write_cache(&dir, ProbeTarget::IpcSteamClient, &sc_sha);
        let report = probe_all_with(&dir, None, |_| RemoteOutcome::NotFound404, true, &std::collections::HashMap::new());
        assert_eq!(report.steamclient_pattern.status, ProbeStatus::CompatibleOffline);
        assert_eq!(report.steamui_pattern.status, ProbeStatus::CompatibleOffline);
        assert_eq!(report.steamclient_ipc.status, ProbeStatus::CompatibleOffline);
        assert!(report.is_all_compatible);
        assert!(!report.has_missing_cache);
        let _ = fs::remove_dir_all(&dir);
    }

    /// DLL 无缓存 + Found → RemoteAvailable{cached:false}，has_missing_cache = true。
    #[test]
    fn probe_available_online_with_missing_cache() {
        let (dir, _, _) = fake_steam_dir("online");
        let report = probe_all_with(&dir, None, |_| RemoteOutcome::Found, true, &std::collections::HashMap::new());
        assert_eq!(
            report.steamclient_pattern.status,
            ProbeStatus::RemoteAvailable { cached: false }
        );
        assert!(report.has_missing_cache);
        assert!(!report.is_all_compatible);
        let _ = fs::remove_dir_all(&dir);
    }

    /// DLL 无缓存 + Error → NetworkError（聚合不报 compatible）。
    #[test]
    fn probe_network_error_when_not_cached() {
        let (dir, _, _) = fake_steam_dir("nerr");
        let report = probe_all_with(&dir, None, |_| RemoteOutcome::Error("timeout".into()), true, &std::collections::HashMap::new());
        assert!(matches!(
            report.steamclient_pattern.status,
            ProbeStatus::NetworkError(_)
        ));
        assert!(!report.is_all_compatible);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 快速体检短路：全缓存 + network=false → 三 CompatibleOffline，head 不被调用（panic 闭包验证）。
    #[test]
    fn probe_shortcut_skips_network_when_cached() {
        let (dir, sc_sha, ui_sha) = fake_steam_dir("shortcut");
        write_cache(&dir, ProbeTarget::PatternSteamClient, &sc_sha);
        write_cache(&dir, ProbeTarget::PatternSteamUi, &ui_sha);
        write_cache(&dir, ProbeTarget::IpcSteamClient, &sc_sha);
        let report = probe_all_with(&dir, None, |_| panic!("head must not be called"), false, &std::collections::HashMap::new());
        assert_eq!(report.steamclient_pattern.status, ProbeStatus::CompatibleOffline);
        assert_eq!(report.steamui_pattern.status, ProbeStatus::CompatibleOffline);
        assert_eq!(report.steamclient_ipc.status, ProbeStatus::CompatibleOffline);
        assert!(report.is_all_compatible);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 快速体检乐观假定：无缓存 + network=false → RemoteAvailable{cached:false}（零网络，head 不被调用）。
    #[test]
    fn probe_optimistic_when_uncached_and_offline() {
        let (dir, _, _) = fake_steam_dir("optimistic");
        let report = probe_all_with(&dir, None, |_| panic!("head must not be called"), false, &std::collections::HashMap::new());
        assert_eq!(
            report.steamclient_pattern.status,
            ProbeStatus::RemoteAvailable { cached: false }
        );
        assert_eq!(
            report.steamui_pattern.status,
            ProbeStatus::RemoteAvailable { cached: false }
        );
        assert!(report.has_missing_cache);
        assert!(!report.is_all_compatible);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 后台刷新（network=true）：全缓存 + Found → 短路态升级为 RemoteAvailable{cached:true}。
    #[test]
    fn probe_refresh_upgrades_shortcut_to_available() {
        let (dir, sc_sha, ui_sha) = fake_steam_dir("refresh");
        write_cache(&dir, ProbeTarget::PatternSteamClient, &sc_sha);
        write_cache(&dir, ProbeTarget::PatternSteamUi, &ui_sha);
        write_cache(&dir, ProbeTarget::IpcSteamClient, &sc_sha);
        let report = probe_all_with(&dir, None, |_| RemoteOutcome::Found, true, &std::collections::HashMap::new());
        assert_eq!(
            report.steamclient_pattern.status,
            ProbeStatus::RemoteAvailable { cached: true }
        );
        assert!(report.is_all_compatible);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 持久化：建目录 + 写入后文件存在且内容一致（cache_path 目标正确）。
    #[test]
    fn write_cache_file_creates_dir_and_file() {
        let dir = std::env::temp_dir().join(format!("ost_compat_pre_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = ProbeTarget::PatternSteamClient;
        let body = b"[patterns]\nkey = 1\n";
        write_cache_file(&dir, target, "deadbeef", body).unwrap();
        let path = cache_path(&dir, target, "deadbeef");
        assert!(path.is_file());
        assert_eq!(fs::read(&path).unwrap(), body);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 覆盖写入：同路径二次写入替换内容（原子写语义）。
    #[test]
    fn write_cache_file_overwrites_existing() {
        let dir = std::env::temp_dir().join(format!("ost_compat_over_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = ProbeTarget::PatternSteamClient;
        write_cache_file(&dir, target, "sha", b"v1").unwrap();
        write_cache_file(&dir, target, "sha", b"v2").unwrap();
        assert_eq!(fs::read(cache_path(&dir, target, "sha")).unwrap(), b"v2");
        let _ = fs::remove_dir_all(&dir);
    }

    /// CompatError Display 文案（对齐 UpdateError 风格）。
    #[test]
    fn compat_error_display() {
        assert_eq!(CompatError::Network("x".into()).to_string(), "network: x");
        assert_eq!(CompatError::Io("y".into()).to_string(), "io: y");
    }

    /// e2e（requires network，默认忽略）：拉取线上已知哈希并落盘。
    #[test]
    #[ignore = "requires network"]
    fn precache_e2e_known_hash() {
        let dir = std::env::temp_dir().join(format!("ost_compat_e2e_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let sha = "3f864358fcf50e49e0a8c6bb8e1bf175e381f5628e4cd4997a59ca5e3976afe5";
        precache(&dir, ProbeTarget::PatternSteamClient, sha).unwrap();
        let path = cache_path(&dir, ProbeTarget::PatternSteamClient, sha);
        assert!(path.is_file());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("["), "signature TOML: {content}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 验证缓存写读往返（临时目录）。
    #[test]
    fn verified_cache_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ost_compat_ver_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let entries = [
            (ProbeTarget::PatternSteamClient, "abc".to_string()),
            (ProbeTarget::IpcSteamClient, "def".to_string()),
        ];
        write_verified(&dir, &entries).unwrap();
        let map = read_verified(&dir);
        assert_eq!(map.get(&ProbeTarget::PatternSteamClient).unwrap(), "abc");
        assert_eq!(map.get(&ProbeTarget::IpcSteamClient).unwrap(), "def");
        assert!(!map.contains_key(&ProbeTarget::PatternSteamUi));
        let _ = fs::remove_dir_all(&dir);
    }

    /// 缺失/损坏的验证缓存 → 空表（视为从未验证）。
    #[test]
    fn verified_cache_missing_or_corrupt_is_empty() {
        let dir = std::env::temp_dir().join(format!("ost_compat_verbad_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(read_verified(&dir).is_empty());
        fs::write(dir.join("verified.toml"), "not [valid toml ===").unwrap();
        assert!(read_verified(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    /// verified_from_report：网络确认适配（RemoteAvailable，无论 cached）均被提取。
    #[test]
    fn verified_from_report_picks_available() {
        let (dir, _, _) = fake_steam_dir("verpick");
        let report = probe_all_with(&dir, None, |_| RemoteOutcome::Found, true, &std::collections::HashMap::new());
        let entries = verified_from_report(&report);
        assert_eq!(entries.len(), 3);
        let targets: Vec<_> = entries.iter().map(|(t, _)| *t).collect();
        assert!(targets.contains(&ProbeTarget::PatternSteamClient));
        assert!(targets.contains(&ProbeTarget::PatternSteamUi));
        assert!(targets.contains(&ProbeTarget::IpcSteamClient));
        let _ = fs::remove_dir_all(&dir);
    }

    /// 快速体检：验证缓存命中（无签名缓存）→ RemoteAvailable{cached:true}（绿），零网络。
    #[test]
    fn probe_verified_cache_hit_turns_green() {
        let (dir, sc_sha, _) = fake_steam_dir("verhit");
        let mut verified = std::collections::HashMap::new();
        verified.insert(ProbeTarget::PatternSteamClient, sc_sha.clone());
        let report = probe_all_with(&dir, None, |_| panic!("head must not be called"), false, &verified);
        assert_eq!(
            report.steamclient_pattern.status,
            ProbeStatus::RemoteAvailable { cached: true }
        );
        // 未命中验证缓存的项维持乐观琥珀。
        assert_eq!(
            report.steamui_pattern.status,
            ProbeStatus::RemoteAvailable { cached: false }
        );
        assert_eq!(
            report.steamclient_ipc.status,
            ProbeStatus::RemoteAvailable { cached: false }
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// 签名缓存优先于验证缓存（两者都有 → 离线可用 CompatibleOffline）。
    #[test]
    fn probe_signature_cache_beats_verified() {
        let (dir, sc_sha, _) = fake_steam_dir("sigbeats");
        write_cache(&dir, ProbeTarget::PatternSteamClient, &sc_sha);
        let mut verified = std::collections::HashMap::new();
        verified.insert(ProbeTarget::PatternSteamClient, sc_sha.clone());
        let report = probe_all_with(&dir, None, |_| panic!("head must not be called"), false, &verified);
        assert_eq!(
            report.steamclient_pattern.status,
            ProbeStatus::CompatibleOffline
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
