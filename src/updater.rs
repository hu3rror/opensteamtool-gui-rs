//! 在线版本检查、下载并解压新版本。

use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use zip::ZipArchive;

use crate::dll::VERSION_FILE;
use crate::patch_source::{self, AssetError, Layout, PatchSource, Resolved};
const APP_RELEASES_URL: &str =
    "https://api.github.com/repos/hu3rror/opensteamtool-gui-rs/releases/latest";
pub const APP_REPO_PAGE: &str = "https://github.com/hu3rror/opensteamtool-gui-rs";
pub const APP_RELEASES_PAGE: &str = "https://github.com/hu3rror/opensteamtool-gui-rs/releases";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq)]
pub struct OnlineInfo {
    pub version: String,
    pub zip_url: String,
    pub source: PatchSource,
}

#[derive(Clone, Debug)]
pub struct AppUpdateCheckResult {
    pub latest_version: String,
    pub newer: bool,
}

/// 数值语义的版本比较（点分数字段；缺段按 0 补）：`latest` 是否严格新于 `current`。
/// App 更新检查专用——补丁版本比较不在 UI 出现（仍是字符串相等）。
pub(crate) fn is_newer_version(latest: &str, current: &str) -> bool {
    let latest: Vec<u64> = latest.split('.').map(|s| s.parse().unwrap_or(0)).collect();
    let current: Vec<u64> = current.split('.').map(|s| s.parse().unwrap_or(0)).collect();
    for i in 0..latest.len().max(current.len()) {
        let a = latest.get(i).copied().unwrap_or(0);
        let b = current.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

fn app_release_from_json(json: &Value, current: &str) -> Result<AppUpdateCheckResult, UpdateError> {
    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or(UpdateError::Parse("missing tag_name".into()))?;
    let version = tag.trim_start_matches('v').to_string();
    Ok(AppUpdateCheckResult {
        newer: is_newer_version(&version, current),
        latest_version: version,
    })
}

/// 查询本工具仓库最新发布并与当前程序版本（crate 版本）比较；只检查，不下载、不自替换（明确非目标，#26）。
pub fn check_app_update() -> Result<AppUpdateCheckResult, UpdateError> {
    let resp = agent()
        .get(APP_RELEASES_URL)
        .header("User-Agent", crate::downloads::USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let json = api_response_to_json(resp)?;
    app_release_from_json(&json, env!("CARGO_PKG_VERSION"))
}

fn open_target(target: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn()?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

pub fn open_in_browser(url: &str) {
    if let Err(e) = open_target(url) {
        eprintln!("[opensteamtool-manager] open browser {url}: {e}");
    }
}

pub fn open_path(path: &Path) {
    let target = path.to_string_lossy();
    if let Err(e) = open_target(&target) {
        eprintln!("[opensteamtool-manager] open folder {target}: {e}");
    }
}

fn agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .build()
        .into()
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateError {
    Network(String),
    /// 发布资产里没有可用的 zip（命名约定 `*-Release.zip`，`-Debug` 永不选）。
    NoAsset {
        source: String,
    },
    UnsupportedArchive {
        source: String,
    },
    /// 布局要求的文件在 zip 里缺失（不进 `dlls/`，也不写 version.txt）。
    MissingMember {
        member: String,
        source: String,
    },
    Parse(String),
    Io(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Network(d) => write!(f, "network: {d}"),
            UpdateError::NoAsset { source } => write!(f, "no release asset for {source}"),
            UpdateError::Parse(d) => write!(f, "parse: {d}"),
            UpdateError::UnsupportedArchive { source } => {
                write!(f, "no .zip asset for {source} (only archives)")
            }
            UpdateError::MissingMember { member, source } => {
                write!(f, "missing {member} in {source} release")
            }
            UpdateError::Io(d) => write!(f, "io: {d}"),
        }
    }
}

fn api_response_to_json(resp: ureq::http::Response<ureq::Body>) -> Result<Value, UpdateError> {
    let status = resp.status();
    if !status.is_success() {
        return Err(UpdateError::Network(format!("HTTP {status}")));
    }
    resp.into_body()
        .read_json::<Value>()
        .map_err(|e| UpdateError::Parse(format!("JSON: {e}")))
}

pub fn check_update(resolved: &Resolved) -> Result<OnlineInfo, UpdateError> {
    let resp = agent()
        .get(&resolved.endpoint)
        .header("User-Agent", crate::downloads::USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let json = api_response_to_json(resp)?;
    online_info_from_json(&json, resolved)
}

fn online_info_from_json(json: &Value, resolved: &Resolved) -> Result<OnlineInfo, UpdateError> {
    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or(UpdateError::Parse("missing tag_name".into()))?;
    let version = tag.trim_start_matches('v').to_string();

    let assets: Vec<(String, String)> = json
        .get("assets")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let name = a.get("name")?.as_str()?;
                    let url = a.get("browser_download_url")?.as_str()?;
                    Some((name.to_string(), url.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

    let source = resolved.source.repo_slug().to_string();
    let names: Vec<&str> = assets.iter().map(|(n, _)| n.as_str()).collect();
    let index = patch_source::select_asset(&names).map_err(|e| match e {
        AssetError::NoMatch => UpdateError::NoAsset {
            source: source.clone(),
        },
        AssetError::UnsupportedArchive => UpdateError::UnsupportedArchive {
            source: source.clone(),
        },
    })?;

    Ok(OnlineInfo {
        version,
        zip_url: assets[index].1.clone(),
        source: resolved.source.clone(),
    })
}

pub fn download_and_extract(
    info: &OnlineInfo,
    dll_dir: &Path,
    layout: Layout,
) -> Result<(), UpdateError> {
    // 下载语义收编至 downloads 模块（ADR-0021）：单 URL 也是长度为 1 的链；
    // zip 走 Large 档（含 512MB body 上限，updater 侧不再自设）。
    let urls = vec![info.zip_url.clone()];
    let bytes = crate::downloads::first_match(&urls, crate::downloads::Policy::Large)
        .map_err(download_error_to_update)?;
    extract_update(&bytes, dll_dir, &info.version, layout)
}

fn download_error_to_update(e: crate::downloads::DownloadError) -> UpdateError {
    match e {
        crate::downloads::DownloadError::NotFound404 => UpdateError::Network("HTTP 404".into()),
        crate::downloads::DownloadError::Network(d) => UpdateError::Network(d),
    }
}

fn extract_update(
    bytes: &[u8],
    dll_dir: &Path,
    version: &str,
    layout: Layout,
) -> Result<(), UpdateError> {
    use std::io::Read;

    let source = layout_source_name(layout);
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| UpdateError::Parse(format!("open zip: {e}")))?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| UpdateError::Parse(format!("read zip entry {i}: {e}")))?;
        let file_name = file
            .name()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_string();
        let needed = layout.root_files().iter().any(|f| {
            f.rename_from
                .unwrap_or(f.name)
                .eq_ignore_ascii_case(&file_name)
        });
        if needed {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .map_err(|e| UpdateError::Parse(format!("extract {file_name}: {e}")))?;
            entries.push((file_name, buf));
        }
    }

    let mut planned: Vec<(&'static str, &[u8])> = Vec::new();
    for file in layout.root_files() {
        let source_name = file.rename_from.unwrap_or(file.name);
        let found = entries
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(source_name))
            .map(|(_, data)| data.as_slice());
        match found {
            Some(data) => planned.push((file.name, data)),
            None => {
                return Err(UpdateError::MissingMember {
                    member: source_name.to_string(),
                    source: source.to_string(),
                });
            }
        }
    }

    std::fs::create_dir_all(dll_dir).map_err(|e| UpdateError::Io(format!("create dir: {e}")))?;
    for (dest_name, data) in planned {
        crate::downloads::write_atomic(&dll_dir.join(dest_name), data)
            .map_err(|e| UpdateError::Io(format!("write {dest_name}: {e}")))?;
    }
    crate::downloads::write_atomic(&dll_dir.join(VERSION_FILE), version.as_bytes())
        .map_err(|e| UpdateError::Io(format!("write version.txt: {e}")))?;

    Ok(())
}

fn layout_source_name(layout: Layout) -> &'static str {
    match layout {
        Layout::OpenSteamTool => "OpenSteamTool",
        Layout::SteamDaddy => "SteamDaddy",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(source: PatchSource, layout: Layout) -> Resolved {
        Resolved {
            source,
            endpoint: "https://api.github.com/repos/o/r/releases/latest".into(),
            layout,
        }
    }

    #[test]
    fn release_json_prefers_release_zip() {
        let json: Value = serde_json::from_str(
            r#"{
                "tag_name": "v1.5.5.4",
                "assets": [
                    {"name": "readme.md", "browser_download_url": "https://x/readme.md"},
                    {"name": "OpenSteamTool-1.5.5.4-Debug.zip", "browser_download_url": "https://x/debug.zip"},
                    {"name": "OpenSteamTool-1.5.5.4-Release.zip", "browser_download_url": "https://x/release.zip"}
                ]
            }"#,
        )
        .unwrap();
        let info = online_info_from_json(
            &json,
            &resolved(PatchSource::Huanyuejue, Layout::OpenSteamTool),
        )
        .unwrap();
        assert_eq!(info.version, "1.5.5.4");
        assert_eq!(info.zip_url, "https://x/release.zip");
        assert_eq!(info.source, PatchSource::Huanyuejue);
    }

    #[test]
    fn release_json_without_usable_asset_names_the_source() {
        let json: Value = serde_json::from_str(
            r#"{"tag_name":"v1.0.0","assets":[{"name":"a.exe","browser_download_url":"https://x/a"}]}"#,
        )
        .unwrap();
        let err = online_info_from_json(
            &json,
            &resolved(PatchSource::Upstream, Layout::OpenSteamTool),
        )
        .unwrap_err();
        match err {
            UpdateError::NoAsset { source } => assert_eq!(source, "OpenSteam001/OpenSteamTool"),
            other => panic!("expected NoAsset, got {other:?}"),
        }
    }

    #[test]
    fn release_json_archive_only_names_the_source() {
        let json: Value = serde_json::from_str(
            r#"{"tag_name":"v1.0.0","assets":[{"name":"Patch-1.0.7z","browser_download_url":"https://x/a"}]}"#,
        )
        .unwrap();
        let err = online_info_from_json(
            &json,
            &resolved(PatchSource::SteamDaddy, Layout::SteamDaddy),
        )
        .unwrap_err();
        match err {
            UpdateError::UnsupportedArchive { source } => {
                assert_eq!(source, "Contrary-v2/SteamDaddy-Dll")
            }
            other => panic!("expected UnsupportedArchive, got {other:?}"),
        }
    }

    #[test]
    fn release_json_missing_tag_is_parse_error() {
        let json: Value = serde_json::from_str(r#"{"assets":[]}"#).unwrap();
        assert!(matches!(
            online_info_from_json(
                &json,
                &resolved(PatchSource::Upstream, Layout::OpenSteamTool)
            ),
            Err(UpdateError::Parse(_))
        ));
    }

    #[test]
    fn is_newer_version_table() {
        assert!(!is_newer_version("0.2.4", "0.2.4"));
        assert!(is_newer_version("0.6.3", "0.2.4"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(is_newer_version("0.10.0", "0.9.0"));
        assert!(!is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("1.2.1", "1.2"));
        assert!(!is_newer_version("1.2", "1.2.1"));
        assert!(!is_newer_version("1.2.0", "1.2"));
        assert!(!is_newer_version("beta", "0.1.0"));
    }

    #[test]
    fn app_release_from_json_extracts_latest() {
        let json: Value = serde_json::from_str(r#"{"tag_name":"v0.6.3","assets":[]}"#).unwrap();
        let r = app_release_from_json(&json, "0.2.4").unwrap();
        assert_eq!(r.latest_version, "0.6.3");
        assert!(r.newer, "0.6.3 应新于 0.2.4");
        let r = app_release_from_json(&json, "0.6.3").unwrap();
        assert!(!r.newer);
        let empty: Value = serde_json::from_str(r#"{}"#).unwrap();
        assert!(matches!(
            app_release_from_json(&empty, "0.2.4"),
            Err(UpdateError::Parse(_))
        ));
    }

    fn zip_with(names: &[&str]) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for name in names {
                zw.start_file(*name, opts).unwrap();
                zw.write_all(b"x").unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extract_keeps_only_layout_members() {
        let buf = zip_with(&[
            "OpenSteamTool.dll",
            "dwmapi.dll",
            "xinput1_4.dll",
            "readme.txt",
        ]);
        let dir = tmp_dir("extract_members");

        extract_update(&buf, &dir, "1.5.5.4", Layout::OpenSteamTool).unwrap();

        for file in Layout::OpenSteamTool.root_files() {
            assert!(dir.join(file.name).is_file(), "missing {}", file.name);
        }
        assert!(!dir.join("readme.txt").exists(), "readme 不应被解包");
        assert_eq!(
            std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap(),
            "1.5.5.4"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extract_applies_rename_rules() {
        let buf = zip_with(&[
            "SteamDaddy.dll",
            "version.dll.disable",
            "xinput1_4.dll",
            "steamdaddy.toml",
        ]);
        let dir = tmp_dir("extract_rename");

        extract_update(&buf, &dir, "4.0.7", Layout::SteamDaddy).unwrap();

        assert!(dir.join("SteamDaddy.dll").is_file());
        assert!(
            dir.join("version.dll").is_file(),
            "version.dll.disable 应改名"
        );
        assert!(!dir.join("version.dll.disable").exists());
        assert!(dir.join("xinput1_4.dll").is_file());
        assert!(
            !dir.join("steamdaddy.toml").exists(),
            "配置模板不属补丁文件"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extract_creates_missing_dll_dir() {
        let buf = zip_with(&["SteamDaddy.dll", "version.dll.disable", "xinput1_4.dll"]);
        let dir = std::env::temp_dir().join(format!("ost_missing_dlls_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!dir.exists(), "precondition: dir must not exist");

        let res = extract_update(&buf, &dir, "4.0.7", Layout::SteamDaddy);
        assert!(res.is_ok(), "extract to missing dir failed: {res:?}");
        for file in Layout::SteamDaddy.root_files() {
            assert!(dir.join(file.name).is_file(), "missing {}", file.name);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extract_reports_missing_member_without_writing() {
        let buf = zip_with(&["SteamDaddy.dll", "xinput1_4.dll"]);
        let dir = tmp_dir("extract_missing");

        let err = extract_update(&buf, &dir, "4.0.7", Layout::SteamDaddy).unwrap_err();
        match err {
            UpdateError::MissingMember { member, source } => {
                assert_eq!(member, "version.dll.disable");
                assert_eq!(source, "SteamDaddy");
            }
            other => panic!("expected MissingMember, got {other:?}"),
        }
        assert!(!dir.join("SteamDaddy.dll").exists(), "缺成员时不落任何文件");
        assert!(!dir.join(VERSION_FILE).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extract_rejects_zip_without_layout_members() {
        let buf = zip_with(&["readme.txt"]);
        let dir = tmp_dir("extract_none");
        let err = extract_update(&buf, &dir, "1.5.5.4", Layout::OpenSteamTool).unwrap_err();
        assert!(
            matches!(err, UpdateError::MissingMember { .. }),
            "err: {err}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[ignore = "requires network"]
    fn e2e_check_and_download() {
        let source = patch_source::resolve(&PatchSource::Huanyuejue);
        let info = check_update(&source).expect("check_update should succeed");
        assert!(!info.version.is_empty());
        assert!(!info.zip_url.is_empty());

        let dir = std::env::temp_dir().join(format!("ost_e2e_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let res = download_and_extract(&info, &dir, source.layout);
        if let Err(e) = &res {
            panic!("download_and_extract failed: {e}");
        }
        for file in source.layout.root_files() {
            assert!(dir.join(file.name).is_file(), "missing {}", file.name);
        }
        assert_eq!(
            crate::dll::read_local_version(&dir).as_deref(),
            Some(info.version.as_str())
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
