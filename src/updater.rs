//! 在线版本检查、下载并解压新版本。

use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use zip::ZipArchive;

use crate::dll::{TARGET_DLLS, VERSION_FILE};

/// GitHub 线上最新发布 API。
const RELEASES_URL: &str =
    "https://api.github.com/repos/OpenSteam001/OpenSteamTool/releases/latest";
/// 浏览器标识 User-Agent（GitHub API 要求）。
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36 OpenSteamTool-Manager";

/// 检查更新请求超时（连接 10s，总体 30s——API 响应小，快超快速失败）。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(30);
/// 下载 zip 超时（连接 10s 快速失败；总时长放宽到 10min，
/// 因 GitHub 资产经 302 重定向到 CDN，慢网络下 body 阶段可能超过 30s。
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_GLOBAL_TIMEOUT: Duration = Duration::from_secs(600);
const DOWNLOAD_BODY_TIMEOUT: Duration = Duration::from_secs(600);

/// 线上更新信息。
#[derive(Clone, Debug)]
pub struct OnlineInfo {
    pub version: String,
    pub zip_url: String,
}

fn agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .build()
        .into()
}

/// 下载专用 agent：连接快速失败，body 读取给足时间。
fn download_agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(DOWNLOAD_CONNECT_TIMEOUT))
        .timeout_global(Some(DOWNLOAD_GLOBAL_TIMEOUT))
        .timeout_recv_body(Some(DOWNLOAD_BODY_TIMEOUT))
        .build()
        .into()
}


/// 更新相关错误，UI 层据此映射双语文案。
#[derive(Clone, Debug)]
pub enum UpdateError {
    /// 网络请求失败（HTTP 非成功/传输错误）。
    Network(String),
    /// 发布包中没有 .zip 资产。
    NoZip,
    /// 解析发布信息失败。
    Parse(String),
    /// 压缩包内没有目标 DLL。
    NoTargetDll,
    /// 本地文件操作失败。
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

/// 查询线上最新版本：解析 `tag_name`（去 `v` 前缀），在 assets 中找第一个 `.zip`。
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
            assets
                .iter()
                .find(|a| {
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

/// 下载 zip 到内存，仅提取文件名属于目标 DLL 集合的成员写入 `dlls/`，成功后写 `version.txt`。
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
    std::fs::create_dir_all(dll_dir)
        .map_err(|e| UpdateError::Io(format!("create dir: {e}")))?;

    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| UpdateError::Parse(format!("open zip: {e}")))?;

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
                assets
                    .iter()
                    .find(|a| {
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
                assets
                    .iter()
                    .find(|a| {
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
    fn extract_picks_only_target_dlls() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        // 构造一个含 3 个目标 DLL + 1 个无关文件的 zip。
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
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
        assert!(!dir.join("readme.txt").exists(), "readme.txt should not be extracted");
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
            let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for dll in TARGET_DLLS {
                zw.start_file(dll, opts).unwrap();
                zw.write_all(b"x").unwrap();
            }
            zw.finish().unwrap();
        }

        // 用户场景：exe 旁边没有 dlls/ 目录（便携版未解压完整/目录被删）。
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
            let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
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

    /// 端到端：真实请求 GitHub 检查/下载最新发布并解压（需网络）。
    /// 手动运行：cargo test --release -- --ignored
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
