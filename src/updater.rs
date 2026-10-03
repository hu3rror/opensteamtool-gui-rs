//! 在线版本检查、下载并解压新版本。

use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use zip::ZipArchive;

use crate::dll::{TARGET_DLLS, VERSION_FILE};

const RELEASES_URL: &str =
    "https://api.github.com/repos/OpenSteam001/OpenSteamTool/releases/latest";
const APP_RELEASES_URL: &str =
    "https://api.github.com/repos/hu3rror/opensteamtool-gui-rs/releases/latest";
pub const APP_REPO_PAGE: &str = "https://github.com/hu3rror/opensteamtool-gui-rs";
pub const APP_RELEASES_PAGE: &str = "https://github.com/hu3rror/opensteamtool-gui-rs/releases";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36 OpenSteamTool-Manager";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(30);
/// 下载 zip 超时：连接 10s 快速失败；总时长 10min——GitHub 资产经 302 重定向到 CDN，慢网络下 body 阶段可能超过 30s。
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_GLOBAL_TIMEOUT: Duration = Duration::from_secs(600);
const DOWNLOAD_BODY_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug)]
pub struct OnlineInfo {
    pub version: String,
    pub zip_url: String,
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
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let json = api_response_to_json(resp)?;
    app_release_from_json(&json, env!("CARGO_PKG_VERSION"))
}

pub fn open_in_browser(url: &str) {
    #[cfg(windows)]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(not(windows))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = result {
        eprintln!("[opensteamtool-manager] open browser {url}: {e}");
    }
}

fn agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .build()
        .into()
}

/// 下载专用 agent：连接快速失败，body 读取给足时间。
pub(crate) fn download_agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(DOWNLOAD_CONNECT_TIMEOUT))
        .timeout_global(Some(DOWNLOAD_GLOBAL_TIMEOUT))
        .timeout_recv_body(Some(DOWNLOAD_BODY_TIMEOUT))
        .build()
        .into()
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateError {
    Network(String),
    NoZip,
    Parse(String),
    NoTargetDll,
    Io(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Network(d) => write!(f, "network: {d}"),
            UpdateError::NoZip => write!(f, "no .zip asset"),
            UpdateError::Parse(d) => write!(f, "parse: {d}"),
            UpdateError::NoTargetDll => write!(f, "no target DLL"),
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

pub fn check_update() -> Result<OnlineInfo, UpdateError> {
    let resp = agent()
        .get(RELEASES_URL)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let json = api_response_to_json(resp)?;

    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or(UpdateError::Parse("missing tag_name".into()))?;
    let version = tag.trim_start_matches('v').to_string();

    let zip_url = json
        .get("assets")
        .and_then(|v| v.as_array())
        .and_then(|assets| {
            assets.iter().find(|a| {
                a.get("name")
                    .and_then(|n| n.as_str())
                    .is_some_and(|n| n.ends_with(".zip"))
            })
        })
        .and_then(|a| a.get("browser_download_url"))
        .and_then(|u| u.as_str())
        .ok_or(UpdateError::NoZip)?;

    Ok(OnlineInfo {
        version,
        zip_url: zip_url.to_string(),
    })
}

pub fn download_and_extract(info: &OnlineInfo, dll_dir: &Path) -> Result<(), UpdateError> {
    let mut resp = download_agent()
        .get(&info.zip_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(UpdateError::Network(format!("download HTTP {status}")));
    }

    // ureq 默认 body 上限 10MB，OpenSteamTool 的 zip 可能超过，显式放宽到 512MB。
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| UpdateError::Network(format!("read body: {e}")))?;

    extract_update(&bytes, dll_dir, &info.version)
}

/// 从内存 zip 中仅提取目标 DLL 集合成员写入 `dll_dir`，成功后写 `version.txt`。
fn extract_update(bytes: &[u8], dll_dir: &Path, version: &str) -> Result<(), UpdateError> {
    // 便携版可能没有 dlls/ 目录（只拷了 exe），写入前确保存在。
    std::fs::create_dir_all(dll_dir).map_err(|e| UpdateError::Io(format!("create dir: {e}")))?;

    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| UpdateError::Parse(format!("open zip: {e}")))?;

    let mut extracted: Vec<String> = Vec::new();
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

        if TARGET_DLLS.contains(&file_name.as_str()) {
            let mut buf = Vec::new();
            use std::io::Read;
            file.read_to_end(&mut buf)
                .map_err(|e| UpdateError::Parse(format!("extract {file_name}: {e}")))?;
            std::fs::write(dll_dir.join(&file_name), buf)
                .map_err(|e| UpdateError::Io(format!("write {file_name}: {e}")))?;
            extracted.push(file_name.to_string());
        }
    }

    if extracted.is_empty() {
        return Err(UpdateError::NoTargetDll);
    }

    std::fs::write(dll_dir.join(VERSION_FILE), version)
        .map_err(|e| UpdateError::Io(format!("write version.txt: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_release_json() {
        let json: Value = serde_json::from_str(
            r#"{
                "tag_name": "v1.4.8",
                "assets": [
                    {"name": "readme.md", "browser_download_url": "https://x/readme.md"},
                    {"name": "OpenSteamTool-1.4.8.zip", "browser_download_url": "https://x/ost.zip"},
                    {"name": "installer.exe", "browser_download_url": "https://x/setup.exe"}
                ]
            }"#,
        )
        .unwrap();
        let tag = json.get("tag_name").and_then(|v| v.as_str()).unwrap();
        assert_eq!(tag.trim_start_matches('v'), "1.4.8");
        let zip_url = json
            .get("assets")
            .and_then(|v| v.as_array())
            .and_then(|assets| {
                assets.iter().find(|a| {
                    a.get("name")
                        .and_then(|n| n.as_str())
                        .is_some_and(|n| n.ends_with(".zip"))
                })
            })
            .and_then(|a| a.get("browser_download_url"))
            .and_then(|u| u.as_str())
            .unwrap();
        assert_eq!(zip_url, "https://x/ost.zip");
    }

    #[test]
    fn no_zip_asset_is_error() {
        let json: Value = serde_json::from_str(
            r#"{"tag_name":"v1.0.0","assets":[{"name":"a.exe","browser_download_url":"https://x/a"}]}"#,
        )
        .unwrap();
        let zip_url = json
            .get("assets")
            .and_then(|v| v.as_array())
            .and_then(|assets| {
                assets.iter().find(|a| {
                    a.get("name")
                        .and_then(|n| n.as_str())
                        .is_some_and(|n| n.ends_with(".zip"))
                })
            })
            .and_then(|a| a.get("browser_download_url"))
            .and_then(|u| u.as_str());
        assert!(zip_url.is_none());
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

    #[test]
    fn extract_picks_only_target_dlls() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for name in [
                "OpenSteamTool.dll",
                "dwmapi.dll",
                "xinput1_4.dll",
                "readme.txt",
            ] {
                zw.start_file(name, opts).unwrap();
                zw.write_all(b"x").unwrap();
            }
            zw.finish().unwrap();
        }

        let dir = std::env::temp_dir().join(format!("ost_extract_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let res = extract_update(&buf, &dir, "1.4.8");
        assert!(res.is_ok(), "extract failed: {res:?}");

        for dll in TARGET_DLLS {
            assert!(dir.join(dll).is_file(), "missing {dll}");
        }
        assert!(
            !dir.join("readme.txt").exists(),
            "readme.txt should not be extracted"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap(),
            "1.4.8"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extract_creates_missing_dll_dir() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for dll in TARGET_DLLS {
                zw.start_file(dll, opts).unwrap();
                zw.write_all(b"x").unwrap();
            }
            zw.finish().unwrap();
        }

        let dir = std::env::temp_dir().join(format!("ost_missing_dlls_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!dir.exists(), "precondition: dir must not exist");

        let res = extract_update(&buf, &dir, "1.4.8");
        assert!(res.is_ok(), "extract to missing dir failed: {res:?}");

        for dll in TARGET_DLLS {
            assert!(dir.join(dll).is_file(), "missing {dll}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extract_without_target_dll_errors() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zw.start_file("readme.txt", opts).unwrap();
            zw.write_all(b"hi").unwrap();
            zw.finish().unwrap();
        }

        let dir = std::env::temp_dir().join(format!("ost_extract_none_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = extract_update(&buf, &dir, "1.4.8").unwrap_err();
        assert!(matches!(err, UpdateError::NoTargetDll), "err: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[ignore = "requires network"]
    fn e2e_check_and_download() {
        let info = check_update().expect("check_update should succeed");
        assert!(!info.version.is_empty());
        assert!(!info.zip_url.is_empty());

        let dir = std::env::temp_dir().join(format!("ost_e2e_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let res = download_and_extract(&info, &dir);
        if let Err(e) = &res {
            panic!("download_and_extract failed: {e}");
        }
        for dll in TARGET_DLLS {
            assert!(dir.join(dll).is_file(), "missing {dll}");
        }
        assert_eq!(
            crate::dll::read_local_version(&dir).as_deref(),
            Some(info.version.as_str())
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
