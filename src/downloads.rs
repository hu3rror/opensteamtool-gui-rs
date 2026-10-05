//! 下载链「取字节 → 原子落盘」深模块(ADR-0021):URL 链逐一下载,404 为权威信号(续链不终止),
//! 其余错误记尾;超时档位按负载分档(镜像小文件 / 更新大 zip);落盘恒原子。

use std::io;
use std::path::Path;
use std::time::Duration;

use ureq::Agent;

/// 下载策略:超时档位与 body 上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// 镜像链签名 TOML(实测 4-6 KB):connect 10s / global 30s / body 30s;body 上限 ureq 默认已足。
    Small,
    /// GitHub 更新 zip:connect 10s / global 10min / body 10min;body 上限 512MB
    /// (GitHub 资产经 302 到 CDN,慢网络下 body 阶段可远超 30s;ureq 默认 10MB 必超)。
    Large,
}

const LARGE_BODY_LIMIT: u64 = 512 * 1024 * 1024;

/// 下载链错误:404 是上游「确无此文件」的确定性信号,区别于暂时性网络错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    NotFound404,
    Network(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::NotFound404 => write!(f, "HTTP 404"),
            DownloadError::Network(d) => write!(f, "network: {d}"),
        }
    }
}

/// 沿 URL 链取字节:首个 2xx 即返回;404 为权威信号(续链不终止),其余错误记尾续链;
/// 链耗尽时链上曾见 404 → 报 404,否则报最后一个错误;空链报 `no URLs`(保现状)。
pub fn first_match(urls: &[String], policy: Policy) -> Result<Vec<u8>, DownloadError> {
    let agent = agent_for(policy);
    first_match_with(urls, |url| get_bytes(&agent, url, policy))
}

/// 注入 GET 的链逻辑(测试 seam,与 `probe_all`/`probe_all_with` 同构):
/// 只测回退次序与错误选择,不触真实网络。
fn first_match_with(
    urls: &[String],
    mut get: impl FnMut(&str) -> Result<Vec<u8>, DownloadError>,
) -> Result<Vec<u8>, DownloadError> {
    let mut any_404 = false;
    let mut last_err: Option<DownloadError> = None;
    for url in urls {
        match get(url) {
            Ok(bytes) => return Ok(bytes),
            Err(DownloadError::NotFound404) => any_404 = true,
            Err(e @ DownloadError::Network(_)) => last_err = Some(e),
        }
    }
    if any_404 {
        Err(DownloadError::NotFound404)
    } else {
        Err(last_err.unwrap_or_else(|| DownloadError::Network("no URLs".into())))
    }
}

fn agent_for(policy: Policy) -> Agent {
    match policy {
        Policy::Small => Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(30)))
            .build()
            .into(),
        Policy::Large => Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(600)))
            .timeout_recv_body(Some(Duration::from_secs(600)))
            .build()
            .into(),
    }
}

fn map_call_err(e: ureq::Error) -> DownloadError {
    match e {
        ureq::Error::StatusCode(404) => DownloadError::NotFound404,
        ureq::Error::StatusCode(code) => DownloadError::Network(format!("HTTP {code}")),
        e => DownloadError::Network(e.to_string()),
    }
}

/// 单 URL GET→bytes:2xx 读 body(按策略 body 上限),非 2xx 由错误映射区分 404 与其余状态码。
fn get_bytes(agent: &Agent, url: &str, policy: Policy) -> Result<Vec<u8>, DownloadError> {
    let mut resp = agent.get(url).call().map_err(map_call_err)?;
    let read = match policy {
        Policy::Small => resp.into_body().read_to_vec(),
        Policy::Large => resp
            .body_mut()
            .with_config()
            .limit(LARGE_BODY_LIMIT)
            .read_to_vec(),
    };
    read.map_err(|e| DownloadError::Network(format!("read body: {e}")))
}

/// 原子落盘门面(复用 [`crate::fsutil::write_atomic`]):下载域持久化统一经此,避免半截文件
/// 被文件本位判据误判为完整。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    crate::fsutil::write_atomic(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_match_falls_back_to_second_url_on_404() {
        let urls = vec!["a".to_string(), "b".to_string()];
        let bytes = first_match_with(&urls, |u| match u {
            "a" => Err(DownloadError::NotFound404),
            _ => Ok(b"b-body".to_vec()),
        })
        .unwrap();
        assert_eq!(bytes, b"b-body");
    }

    #[test]
    fn first_match_short_circuits_on_first_success() {
        let urls = vec!["a".to_string(), "b".to_string()];
        let mut calls = 0;
        let bytes = first_match_with(&urls, |u| {
            calls += 1;
            assert_eq!(u, "a", "b must not be tried after success");
            Ok(b"a-body".to_vec())
        })
        .unwrap();
        assert_eq!(bytes, b"a-body");
        assert_eq!(calls, 1);
    }

    #[test]
    fn first_match_prefers_404_over_network_error() {
        let urls = vec!["a".to_string(), "b".to_string()];
        let err = first_match_with(&urls, |u| match u {
            "a" => Err(DownloadError::NotFound404),
            _ => Err(DownloadError::Network("timeout".into())),
        })
        .unwrap_err();
        assert_eq!(err, DownloadError::NotFound404);
    }

    #[test]
    fn first_match_prefers_404_when_seen_later() {
        let urls = vec!["a".to_string(), "b".to_string()];
        let err = first_match_with(&urls, |u| match u {
            "a" => Err(DownloadError::Network("500".into())),
            _ => Err(DownloadError::NotFound404),
        })
        .unwrap_err();
        assert_eq!(err, DownloadError::NotFound404);
    }

    #[test]
    fn first_match_reports_last_network_error_when_no_404() {
        let urls = vec!["a".to_string(), "b".to_string()];
        let err = first_match_with(&urls, |u| match u {
            "a" => Err(DownloadError::Network("first".into())),
            _ => Err(DownloadError::Network("last".into())),
        })
        .unwrap_err();
        assert_eq!(err, DownloadError::Network("last".into()));
    }

    #[test]
    fn first_match_empty_chain_is_no_urls() {
        let err = first_match_with(&[], |_| unreachable!("no urls to probe")).unwrap_err();
        assert_eq!(err, DownloadError::Network("no URLs".into()));
    }

    #[test]
    fn first_match_single_url_404() {
        let urls = vec!["a".to_string()];
        let err = first_match_with(&urls, |_| Err(DownloadError::NotFound404)).unwrap_err();
        assert_eq!(err, DownloadError::NotFound404);
    }

    #[test]
    fn write_atomic_round_trip() {
        let dir = std::env::temp_dir().join(format!("ost_downloads_w_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sig.toml");
        write_atomic(&path, b"v1").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"v1");
        write_atomic(&path, b"v2").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"v2");
        let leftover: Vec<String> = dir
            .read_dir()
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect();
        assert!(leftover.is_empty(), "temp files cleaned: {leftover:?}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
