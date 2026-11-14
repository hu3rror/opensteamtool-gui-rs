//! 「补丁源」与「源布局」：补丁 DLL 从哪个仓库取、取到的文件放在 Steam 根目录哪里。
//! 纯模块：无 IO、无网络、无 egui（词条见 GLOSSARY.md，决策见 ADR-0023）。

use std::fmt;

/// 源布局：一套 Steam 根目录文件表 + Lua 子目录。新增一个 fork 只需加一个枚举臂 + 一张表。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Layout {
    OpenSteamTool,
    SteamDaddy,
}

/// 布局中的一个 Steam 根目录文件：`rename_from` 是 zip 内的原始文件名（发布包用扩展名禁用代理）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootFile {
    pub name: &'static str,
    pub rename_from: Option<&'static str>,
}

const OST_ROOT_FILES: [RootFile; 3] = [
    RootFile {
        name: "OpenSteamTool.dll",
        rename_from: None,
    },
    RootFile {
        name: "dwmapi.dll",
        rename_from: None,
    },
    RootFile {
        name: "xinput1_4.dll",
        rename_from: None,
    },
];

const STEAMDADDY_ROOT_FILES: [RootFile; 3] = [
    RootFile {
        name: "SteamDaddy.dll",
        rename_from: None,
    },
    RootFile {
        name: "version.dll",
        rename_from: Some("version.dll.disable"),
    },
    RootFile {
        name: "xinput1_4.dll",
        rename_from: None,
    },
];

impl Layout {
    pub const ALL: [Layout; 2] = [Layout::OpenSteamTool, Layout::SteamDaddy];

    pub fn root_files(self) -> &'static [RootFile] {
        match self {
            Layout::OpenSteamTool => &OST_ROOT_FILES,
            Layout::SteamDaddy => &STEAMDADDY_ROOT_FILES,
        }
    }

    /// Lua 脚本子目录（相对 Steam 目录）。
    pub fn lua_subdir(self) -> &'static str {
        match self {
            Layout::OpenSteamTool => "config/lua",
            Layout::SteamDaddy => "config/stplug-in",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "opensteamtool" => Some(Layout::OpenSteamTool),
            "steamdaddy" => Some(Layout::SteamDaddy),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Layout::OpenSteamTool => "opensteamtool",
            Layout::SteamDaddy => "steamdaddy",
        }
    }
}

/// 补丁源：三个内置仓库 + 一个自定义 GitHub 仓库（自定义 URL 已归一化为 releases API 形式）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PatchSource {
    /// 上游 `OpenSteam001/OpenSteamTool`（已停更，界面标注 discontinued）。
    Upstream,
    /// `huanyuejue/OpenSteamTool`（推荐；新配置与向导默认值）。
    Huanyuejue,
    /// `Contrary-v2/SteamDaddy-Dll`。
    SteamDaddy,
    Custom { url: String, layout: Layout },
}

const UPSTREAM_ENDPOINT: &str =
    "https://api.github.com/repos/OpenSteam001/OpenSteamTool/releases/latest";
const EBOSVNALAE_ENDPOINT: &str =
    "https://api.github.com/repos/huanyuejue/OpenSteamTool/releases/latest";
const STEAMDADDY_ENDPOINT: &str =
    "https://api.github.com/repos/Contrary-v2/SteamDaddy-Dll/releases/latest";

/// 发布资产命名约定（选中条件的经验值；`-Debug` 变体永远不选）。
pub const ASSET_PATTERN: &str = "*-Release.zip";

impl PatchSource {
    pub const BUILTINS: [PatchSource; 3] = [PatchSource::Upstream, PatchSource::Huanyuejue, PatchSource::SteamDaddy];

    pub fn kind(&self) -> &'static str {
        match self {
            PatchSource::Upstream => "upstream",
            PatchSource::Huanyuejue => "ebosvnalae",
            PatchSource::SteamDaddy => "steamdaddy",
            PatchSource::Custom { .. } => "custom",
        }
    }

    /// 内置源按 kind 解析；`custom` 返回 `None`（自定义源需要额外字段）。
    pub fn from_kind(kind: &str) -> Option<Self> {
        match kind {
            "upstream" => Some(PatchSource::Upstream),
            "ebosvnalae" => Some(PatchSource::Huanyuejue),
            "steamdaddy" => Some(PatchSource::SteamDaddy),
            _ => None,
        }
    }

    pub fn layout(&self) -> Layout {
        match self {
            PatchSource::Upstream | PatchSource::Huanyuejue => Layout::OpenSteamTool,
            PatchSource::SteamDaddy => Layout::SteamDaddy,
            PatchSource::Custom { layout, .. } => *layout,
        }
    }

    pub fn recommended(&self) -> bool {
        matches!(self, PatchSource::Huanyuejue)
    }

    pub fn discontinued(&self) -> bool {
        matches!(self, PatchSource::Upstream)
    }

    /// 仓库名（`owner/repo`），用于界面来源行。
    pub fn repo_slug(&self) -> &str {
        match self {
            PatchSource::Upstream => "OpenSteam001/OpenSteamTool",
            PatchSource::Huanyuejue => "huanyuejue/OpenSteamTool",
            PatchSource::SteamDaddy => "Contrary-v2/SteamDaddy-Dll",
            PatchSource::Custom { url, .. } => slug_of_endpoint(url).unwrap_or(url),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub endpoint: String,
    pub layout: Layout,
}

/// 配置 → 端点 + 布局的唯一映射点（所有调用方都消费 `Resolved`，不再各自派生）。
pub fn resolve(source: &PatchSource) -> Resolved {
    Resolved {
        endpoint: match source {
            PatchSource::Upstream => UPSTREAM_ENDPOINT.to_string(),
            PatchSource::Huanyuejue => EBOSVNALAE_ENDPOINT.to_string(),
            PatchSource::SteamDaddy => STEAMDADDY_ENDPOINT.to_string(),
            PatchSource::Custom { url, .. } => url.clone(),
        },
        layout: source.layout(),
    }
}

/// 自定义源头错误的裁剪语义（界面按此映射文案）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UrlError {
    /// 非 https。
    NotHttps,
    /// 主机不是 GitHub（只支持 GitHub releases）。
    UnsupportedHost,
    /// 形状不可识别（缺 owner/repo、路径多段等）。
    Shape,
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrlError::NotHttps => write!(f, "only https URLs are supported"),
            UrlError::UnsupportedHost => write!(f, "only GitHub releases are supported"),
            UrlError::Shape => write!(f, "expected a GitHub repo URL"),
        }
    }
}

/// 归一化自定义源 URL：接受仓库页 / releases 页 / tag 页 / releases API 形式，统一为 API 形式。
pub fn normalize_url(raw: &str) -> Result<String, UrlError> {
    let s = raw.trim();
    let Some(rest) = s.strip_prefix("https://") else {
        return Err(if s.starts_with("http://") {
            UrlError::NotHttps
        } else {
            UrlError::Shape
        });
    };
    let (host, path) = rest.split_once('/').ok_or(UrlError::Shape)?;
    let host = host.to_ascii_lowercase();
    let segments: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();

    let (owner, repo) = match host.as_str() {
        "github.com" | "www.github.com" => repo_page_slug(&segments)?,
        "api.github.com" => api_slug(&segments)?,
        _ => return Err(UrlError::UnsupportedHost),
    };
    Ok(format!(
        "https://api.github.com/repos/{owner}/{repo}/releases/latest"
    ))
}

fn repo_page_slug<'a>(segments: &'a [&'a str]) -> Result<(&'a str, &'a str), UrlError> {
    let [owner, repo, rest @ ..] = segments else {
        return Err(UrlError::Shape);
    };
    let ok_rest = rest.is_empty()
        || rest == ["releases"]
        || (rest.len() == 3 && rest[0] == "releases" && rest[1] == "tag");
    if !ok_rest {
        return Err(UrlError::Shape);
    }
    slug(owner, repo)
}

fn api_slug<'a>(segments: &'a [&'a str]) -> Result<(&'a str, &'a str), UrlError> {
    let ["repos", owner, repo, rest @ ..] = segments else {
        return Err(UrlError::Shape);
    };
    let ok_rest = rest.is_empty() || rest == ["releases"] || rest == ["releases", "latest"];
    if !ok_rest {
        return Err(UrlError::Shape);
    }
    slug(owner, repo)
}


fn slug<'a>(owner: &'a str, repo: &'a str) -> Result<(&'a str, &'a str), UrlError> {
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if !valid_segment(owner) || !valid_segment(repo) {
        return Err(UrlError::Shape);
    }
    Ok((owner, repo))
}

fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// 归一化后的 URL 若等于某个内置仓库，则落回内置变体（下拉框不出现非选项值）。
pub fn collapse(url: &str) -> Option<PatchSource> {
    match url {
        UPSTREAM_ENDPOINT => Some(PatchSource::Upstream),
        EBOSVNALAE_ENDPOINT => Some(PatchSource::Huanyuejue),
        STEAMDADDY_ENDPOINT => Some(PatchSource::SteamDaddy),
        _ => None,
    }
}

/// 自定义源的构造入口：归一化 → 内置折叠（内置源的布局优先）。
pub fn custom_source(raw: &str, layout: Layout) -> Result<PatchSource, UrlError> {
    let url = normalize_url(raw)?;
    Ok(collapse(&url).unwrap_or(PatchSource::Custom { url, layout }))
}

fn slug_of_endpoint(endpoint: &str) -> Option<&str> {
    let rest = endpoint.strip_prefix("https://api.github.com/repos/")?;
    rest.strip_suffix("/releases/latest")
}

/// 发布资产选择失败的原因（文案由界面映射，均带源名）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AssetError {
    /// 没有任何匹配资产（命名约定见 [`ASSET_PATTERN`]）。
    NoMatch,
    /// 只有非 zip 归档（`.7z` / `.rar` / `.tar.gz` …）。
    UnsupportedArchive,
}

/// 从发布资产名列表中选择下载项：优先 `-Release.zip`，再退到任意非 `-Debug` 的 zip，
/// 最后接受唯一的 Debug zip（有总比没有强）。
pub fn select_asset(names: &[&str]) -> Result<usize, AssetError> {
    let release = names
        .iter()
        .position(|n| is_zip(n) && n.to_ascii_lowercase().ends_with("-release.zip"));
    if let Some(i) = release {
        return Ok(i);
    }
    let other = names
        .iter()
        .position(|n| is_zip(n) && !n.to_ascii_lowercase().ends_with("-debug.zip"));
    if let Some(i) = other {
        return Ok(i);
    }
    let zip = names.iter().position(|n| is_zip(n));
    if let Some(i) = zip {
        return Ok(i);
    }
    if names.iter().any(|n| is_archive(n)) {
        return Err(AssetError::UnsupportedArchive);
    }
    Err(AssetError::NoMatch)
}

fn is_zip(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

fn is_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".7z", ".rar", ".tar.gz", ".tgz", ".tar", ".gz", ".xz", ".bz2"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_resolve_to_endpoint_and_layout() {
        let cases = [
            (
                PatchSource::Upstream,
                "https://api.github.com/repos/OpenSteam001/OpenSteamTool/releases/latest",
                Layout::OpenSteamTool,
            ),
            (
                PatchSource::Huanyuejue,
                "https://api.github.com/repos/huanyuejue/OpenSteamTool/releases/latest",
                Layout::OpenSteamTool,
            ),
            (
                PatchSource::SteamDaddy,
                "https://api.github.com/repos/Contrary-v2/SteamDaddy-Dll/releases/latest",
                Layout::SteamDaddy,
            ),
        ];
        for (source, endpoint, layout) in cases {
            let r = resolve(&source);
            assert_eq!(r.endpoint.as_str(), endpoint, "{source:?}");
            assert_eq!(r.layout, layout, "{source:?}");
        }
    }

    #[test]
    fn custom_resolves_to_its_normalized_endpoint_and_layout() {
        let source = PatchSource::Custom {
            url: normalize_url("https://github.com/Some-One/Some.Repo").unwrap(),
            layout: Layout::SteamDaddy,
        };
        let r = resolve(&source);
        assert_eq!(
            r.endpoint.as_str(),
            "https://api.github.com/repos/Some-One/Some.Repo/releases/latest"
        );
        assert_eq!(r.layout, Layout::SteamDaddy);
    }

    #[test]
    fn custom_never_holds_a_builtin_repo() {
        for builtin in [
            PatchSource::Upstream,
            PatchSource::Huanyuejue,
            PatchSource::SteamDaddy,
        ] {
            let url = normalize_url(resolve(&builtin).endpoint.as_str()).unwrap();
            assert_eq!(collapse(&url), Some(builtin));
        }
        assert_eq!(
            collapse(&normalize_url("https://github.com/other/repo").unwrap()),
            None
        );
    }

    #[test]
    fn normalize_accepts_common_github_forms() {
        let want = "https://api.github.com/repos/o/r/releases/latest";
        for raw in [
            "https://github.com/o/r",
            "https://github.com/o/r/",
            "https://github.com/o/r.git",
            "https://github.com/o/r/releases",
            "https://github.com/o/r/releases/",
            "https://github.com/o/r/releases/tag/v1.2.3",
            "https://api.github.com/repos/o/r/releases",
            "https://api.github.com/repos/o/r/releases/",
            "https://api.github.com/repos/o/r/releases/latest",
            "https://api.github.com/repos/o/r/releases/latest/",
            "  https://github.com/o/r  ",
        ] {
            assert_eq!(normalize_url(raw).unwrap(), want, "input {raw:?}");
        }
    }

    #[test]
    fn normalize_rejects_unsupported_input() {
        for raw in [
            "",
            "   ",
            "github.com/o/r",
            "http://github.com/o/r",
            "https://gitlab.com/o/r",
            "https://github.com/o",
            "https://github.com/o/r/tree/main",
            "https://api.github.com/repos/o/r/tags",
            "https://github.com/o r/repo",
            "ftp://github.com/o/r",
        ] {
            assert!(normalize_url(raw).is_err(), "input {raw:?}");
        }
    }

    #[test]
    fn normalize_rejects_extra_path_segments() {
        assert!(normalize_url("https://api.github.com/repos/o/r/releases/latest/x").is_err());
        assert!(normalize_url("https://github.com/o/r/releases/tag").is_err());
    }

    #[test]
    fn select_prefers_release_over_debug() {
        let names = ["checksums.txt", "OpenSteamTool-1.4.8-Debug.zip", "OpenSteamTool-1.4.8-Release.zip"];
        assert_eq!(select_asset(&names), Ok(2));
    }

    #[test]
    fn select_accepts_release_only() {
        let names = ["SteamDaddy.Dll-v4.0.7-Release.zip"];
        assert_eq!(select_asset(&names), Ok(0));
    }

    #[test]
    fn select_accepts_single_zip_without_variant_suffix() {
        let names = ["1.5.5.4.zip"];
        assert_eq!(select_asset(&names), Ok(0));
    }

    #[test]
    fn select_skips_debug_only_zip() {
        let names = ["OpenSteamTool-1.4.8-Debug.zip"];
        assert_eq!(select_asset(&names), Ok(0));
    }

    #[test]
    fn select_reports_unsupported_archive_when_only_non_zip() {
        for name in ["Patch-1.0.7z", "Patch-1.0.rar", "Patch-1.0.tar.gz"] {
            assert_eq!(select_asset(&[name]), Err(AssetError::UnsupportedArchive), "{name}");
        }
    }

    #[test]
    fn select_reports_no_match_when_nothing_fits() {
        assert_eq!(select_asset(&[]), Err(AssetError::NoMatch));
        assert_eq!(select_asset(&["readme.md", "notes.txt"]), Err(AssetError::NoMatch));
    }

    #[test]
    fn layouts_declare_their_file_tables() {
        let ost = Layout::OpenSteamTool.root_files();
        assert_eq!(ost.len(), 3);
        assert_eq!(ost[0].name, "OpenSteamTool.dll");
        assert_eq!(ost[0].rename_from, None);
        assert_eq!(ost[1].name, "dwmapi.dll");
        assert_eq!(ost[2].name, "xinput1_4.dll");

        let sd = Layout::SteamDaddy.root_files();
        assert_eq!(sd.len(), 3);
        assert_eq!(sd[0].name, "SteamDaddy.dll");
        assert_eq!(sd[1].name, "version.dll");
        assert_eq!(sd[1].rename_from, Some("version.dll.disable"));
        assert_eq!(sd[2].name, "xinput1_4.dll");
    }

    #[test]
    fn layouts_declare_their_lua_subdir() {
        assert_eq!(Layout::OpenSteamTool.lua_subdir(), "config/lua");
        assert_eq!(Layout::SteamDaddy.lua_subdir(), "config/stplug-in");
    }

    #[test]
    fn layout_strings_round_trip_and_reject_unknown() {
        for layout in [Layout::OpenSteamTool, Layout::SteamDaddy] {
            assert_eq!(Layout::parse(layout.as_str()), Some(layout));
        }
        assert_eq!(Layout::parse(""), None);
        assert_eq!(Layout::parse("OPENSTEAMTOOL"), None);
    }
}
