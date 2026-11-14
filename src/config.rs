//! 应用配置（App Config）：exe 同目录 `config.toml`，便携优先（ADR-0012）。
//! 只存用户选择，不存派生状态（ADR-0011 精神）。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::fsutil::{exe_dir, write_atomic};
use crate::i18n::{Lang, detect_system_lang};
use crate::patch_source::{self, Layout, PatchSource};

/// 当前配置格式版本（未来格式变更时递增并写迁移逻辑）。
pub const CONFIG_VERSION: u32 = 1;

pub const CONFIG_FILE: &str = "config.toml";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    Auto,
    Zh,
    En,
}

impl Language {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Language::Auto),
            "zh" => Some(Language::Zh),
            "en" => Some(Language::En),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Language::Auto => "auto",
            Language::Zh => "zh",
            Language::En => "en",
        }
    }

    pub fn effective(self) -> Lang {
        match self {
            Language::Auto => detect_system_lang(),
            Language::Zh => Lang::Zh,
            Language::En => Lang::En,
        }
    }
}

/// 深浅主题偏好：`system` 跟随系统（实际深浅解析由 egui 承担，见 ADR-0016）；
/// 本枚举只负责 config.toml 的持久化与三态映射，不复制系统检测逻辑。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemePreference {
    System,
    Dark,
    Light,
}

impl ThemePreference {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(ThemePreference::System),
            "dark" => Some(ThemePreference::Dark),
            "light" => Some(ThemePreference::Light),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemePreference::System => "system",
            ThemePreference::Dark => "dark",
            ThemePreference::Light => "light",
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    /// 文件系统错误（缺文件除外——缺文件是「未配置」的合法形态，走默认值）。
    Io(io::Error),
    Parse(String),
    UnsupportedVersion(u32),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config io error: {e}"),
            ConfigError::Parse(detail) => write!(f, "config parse error: {detail}"),
            ConfigError::UnsupportedVersion(v) => write!(f, "unsupported config version: {v}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub version: u32,
    /// Steam 安装路径；空串 = 未设置（启动时回退注册表检测）。
    pub steam_path: String,
    pub language: Language,
    /// 最小化时是否自动隐藏到托盘（缺省启用；字段缺失 = true，老配置无迁移）。
    pub minimize_to_tray: bool,
    /// 深浅主题偏好（缺省跟随系统；字段缺失/非法 = System，老配置无迁移，ADR-0016）。
    pub theme: ThemePreference,
    /// 补丁源（ADR-0023）。缺字段 = 上游（旧行为）；新配置默认推荐源。
    pub patch_source: PatchSource,
    /// 上次部署时记录的客户端构建号（`package/steam_client_win32.manifest` 的 version）。
    pub last_client_build: Option<String>,
}

impl Config {
    pub fn defaults() -> Self {
        Self {
            version: CONFIG_VERSION,
            steam_path: String::new(),
            language: Language::Auto,
            minimize_to_tray: true,
            theme: ThemePreference::System,
            patch_source: PatchSource::Huanyuejue,
            last_client_build: None,
        }
    }
}

pub fn config_path() -> PathBuf {
    exe_dir().join(CONFIG_FILE)
}

/// 读取配置：文件缺失 → 默认值；存在但非法 → 类型化错误（不 panic）。
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::defaults()),
        Err(e) => Err(ConfigError::Io(e)),
    }
}

/// 原子落盘（write_atomic）：任何时刻不会出现半截 `config.toml`；父目录缺失时先创建。
pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    let text = serialize(config);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(ConfigError::Io)?;
    }
    write_atomic(path, text.as_bytes()).map_err(ConfigError::Io)
}

/// TOML 解析严格而非宽容（宽容会悄悄吞掉写错的文件，严格让调用方显式降级）。
/// 唯一例外：`minimize_to_tray` 字段级宽容（缺失/类型非法均降级默认启用）。
fn parse(text: &str) -> Result<Config, ConfigError> {
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ConfigError::Parse(e.to_string()))?;

    let version = doc
        .get("version")
        .and_then(|v| v.as_integer())
        .ok_or_else(|| ConfigError::Parse("missing or non-integer version".into()))?;
    let version =
        u32::try_from(version).map_err(|_| ConfigError::Parse("version out of range".into()))?;
    if version != CONFIG_VERSION {
        return Err(ConfigError::UnsupportedVersion(version));
    }

    let steam_path = doc
        .get("steam_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConfigError::Parse("missing steam_path".into()))?
        .trim()
        .to_string();

    let language = doc
        .get("language")
        .and_then(|v| v.as_str())
        .and_then(Language::parse)
        .ok_or_else(|| ConfigError::Parse("missing or invalid language".into()))?;

    let minimize_to_tray = match doc.get("minimize_to_tray") {
        None => true,
        Some(v) => v.as_bool().unwrap_or(true),
    };

    let theme = match doc.get("theme") {
        None => ThemePreference::System,
        Some(v) => v
            .as_str()
            .and_then(ThemePreference::parse)
            .unwrap_or(ThemePreference::System),
    };

    let patch_source = doc
        .get("patch_source")
        .and_then(parse_patch_source)
        .unwrap_or(PatchSource::Upstream);

    let last_client_build = doc
        .get("last_client_build")
        .and_then(|v| v.as_str())
        .and_then(parse_build_id);

    Ok(Config {
        version: CONFIG_VERSION,
        steam_path,
        language,
        minimize_to_tray,
        theme,
        patch_source,
        last_client_build,
    })
}

/// TOML 序列化（手动构造 DocumentMut：三字段小 schema，避免引入 serde derive 依赖，
/// 见 #26「零新依赖」决策）。
fn serialize(config: &Config) -> String {
    let mut doc = toml_edit::DocumentMut::new();
    doc["version"] = toml_edit::value(config.version as i64);
    doc["steam_path"] = toml_edit::value(config.steam_path.trim().to_owned());
    doc["language"] = toml_edit::value(config.language.as_str());
    doc["minimize_to_tray"] = toml_edit::value(config.minimize_to_tray);
    doc["theme"] = toml_edit::value(config.theme.as_str());
    let mut patch_source = toml_edit::Table::new();
    patch_source["kind"] = toml_edit::value(config.patch_source.kind());
    if let PatchSource::Custom { url, layout } = &config.patch_source {
        patch_source["url"] = toml_edit::value(url.as_str());
        patch_source["layout"] = toml_edit::value(layout.as_str());
    }
    doc["patch_source"] = toml_edit::Item::Table(patch_source);
    if let Some(build) = &config.last_client_build {
        doc["last_client_build"] = toml_edit::value(build.as_str());
    }
    doc.to_string()
}

fn parse_patch_source(item: &toml_edit::Item) -> Option<PatchSource> {
    let table = item.as_table()?;
    let kind = table.get("kind")?.as_str()?;
    if kind != "custom" {
        return PatchSource::from_kind(kind);
    }
    let url = table.get("url")?.as_str()?;
    let layout = Layout::parse(table.get("layout")?.as_str()?)?;
    patch_source::custom_source(url, layout).ok()
}

fn parse_build_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty() && trimmed.len() <= 12 && trimmed.chars().all(|c| c.is_ascii_digit()))
        .then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_cfg_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_all_fields() {
        let dir = tmp_dir("roundtrip");
        let path = dir.join(CONFIG_FILE);
        for lang in [Language::Auto, Language::Zh, Language::En] {
            for theme in [
                ThemePreference::System,
                ThemePreference::Dark,
                ThemePreference::Light,
            ] {
                for steam_path in ["C:/Program Files (x86)/Steam", ""] {
                    for minimize_to_tray in [true, false] {
                        let cfg = Config {
                            version: CONFIG_VERSION,
                            steam_path: steam_path.into(),
                            language: lang,
                            minimize_to_tray,
                            theme,
                            ..Config::defaults()
                        };
                        save(&path, &cfg).unwrap();
                        assert_eq!(
                            load(&path).unwrap(),
                            cfg,
                            "round-trip {lang:?} {theme:?} {steam_path:?} {minimize_to_tray}"
                        );
                    }
                }
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_theme_defaults_to_system() {
        let dir = tmp_dir("oldtheme");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(
            &path,
            "version = 1\nsteam_path = \"C:/S\"\nlanguage = \"zh\"",
        )
        .unwrap();
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.language, Language::Zh);
        assert_eq!(cfg.theme, ThemePreference::System, "字段缺失应默认跟随系统");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn theme_invalid_is_tolerated() {
        let dir = tmp_dir("themetol");
        for bad in ["\"blue\"", "\"auto\"", "42"] {
            let path = dir.join(CONFIG_FILE);
            std::fs::write(
                &path,
                format!("version = 1\nsteam_path = \"C:/S\"\nlanguage = \"zh\"\ntheme = {bad}"),
            )
            .unwrap();
            let cfg = load(&path).unwrap();
            assert_eq!(
                cfg.theme,
                ThemePreference::System,
                "theme = {bad} 应宽容降级默认跟随系统"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_defaults() {
        let dir = tmp_dir("missing");
        let cfg = load(&dir.join("nope.toml")).unwrap();
        assert_eq!(cfg, Config::defaults());
        assert_eq!(cfg.steam_path, "");
        assert_eq!(cfg.language, Language::Auto);
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert!(cfg.minimize_to_tray, "缺省应启用托盘隐身");
        assert_eq!(cfg.patch_source, PatchSource::Huanyuejue, "新装默认推荐源");
        assert_eq!(cfg.last_client_build, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_minimize_to_tray_defaults_enabled() {
        let dir = tmp_dir("oldcfg");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(
            &path,
            "version = 1\nsteam_path = \"C:/S\"\nlanguage = \"zh\"",
        )
        .unwrap();
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.language, Language::Zh);
        assert_eq!(cfg.steam_path, "C:/S");
        assert!(cfg.minimize_to_tray, "字段缺失应默认启用");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_content_is_typed_error() {
        let dir = tmp_dir("malformed");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(&path, "version = 1\nsteam_path = ").unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        std::fs::write(
            &path,
            "version = \"one\"\nsteam_path = \"C:/S\"\nlanguage = \"zh\"",
        )
        .unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        std::fs::write(&path, "version = 1\nsteam_path = \"C:/S\"").unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        std::fs::write(
            &path,
            "version = 1\nsteam_path = \"C:/S\"\nlanguage = \"fr\"",
        )
        .unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn minimize_to_tray_non_bool_is_tolerated() {
        let dir = tmp_dir("traytol");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(
            &path,
            "version = 1\nsteam_path = \"C:/S\"\nlanguage = \"zh\"\nminimize_to_tray = \"yes\"",
        )
        .unwrap();
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.steam_path, "C:/S");
        assert_eq!(cfg.language, Language::Zh);
        assert!(cfg.minimize_to_tray, "非布尔托盘值应宽容降级默认启用");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsupported_version_is_typed_error() {
        let dir = tmp_dir("version");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(
            &path,
            "version = 2\nsteam_path = \"C:/S\"\nlanguage = \"zh\"",
        )
        .unwrap();
        assert!(matches!(
            load(&path),
            Err(ConfigError::UnsupportedVersion(2))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_leaves_no_partial_file() {
        let dir = tmp_dir("atomic");
        let path = dir.join(CONFIG_FILE);
        save(&path, &Config::defaults()).unwrap();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            assert!(!name.contains(".tmp-"), "残留临时文件: {name}");
        }
        assert_eq!(load(&path).unwrap(), Config::defaults());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_creates_missing_parent_dir() {
        let dir = tmp_dir("parent");
        let path = dir.join("nested").join("deep").join(CONFIG_FILE);
        save(&path, &Config::defaults()).unwrap();
        assert!(path.is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn theme_pref_parse_table() {
        assert_eq!(
            ThemePreference::parse("system"),
            Some(ThemePreference::System)
        );
        assert_eq!(ThemePreference::parse("dark"), Some(ThemePreference::Dark));
        assert_eq!(
            ThemePreference::parse("light"),
            Some(ThemePreference::Light)
        );
        assert_eq!(ThemePreference::parse("auto"), None);
        assert_eq!(ThemePreference::parse(""), None);
        assert_eq!(ThemePreference::parse("SYSTEM"), None);
    }

    #[test]
    fn theme_pref_as_str_round_trips() {
        for t in [
            ThemePreference::System,
            ThemePreference::Dark,
            ThemePreference::Light,
        ] {
            assert_eq!(ThemePreference::parse(t.as_str()), Some(t));
        }
    }

    #[test]
    fn language_parse_table() {
        assert_eq!(Language::parse("auto"), Some(Language::Auto));
        assert_eq!(Language::parse("zh"), Some(Language::Zh));
        assert_eq!(Language::parse("en"), Some(Language::En));
        assert_eq!(Language::parse("fr"), None);
        assert_eq!(Language::parse(""), None);
        assert_eq!(Language::parse("AUTO"), None);
    }

    #[test]
    fn language_as_str_round_trips() {
        for l in [Language::Auto, Language::Zh, Language::En] {
            assert_eq!(Language::parse(l.as_str()), Some(l));
        }
    }

    #[test]
    fn language_effective_pins() {
        assert_eq!(Language::Zh.effective(), Lang::Zh);
        assert_eq!(Language::En.effective(), Lang::En);
        assert_eq!(Language::Auto.effective(), detect_system_lang());
    }

    #[test]
    fn round_trip_patch_source_variants() {
        let dir = tmp_dir("patchsrc");
        let path = dir.join(CONFIG_FILE);
        for source in [
            PatchSource::Upstream,
            PatchSource::Huanyuejue,
            PatchSource::SteamDaddy,
            PatchSource::Custom {
                url: "https://api.github.com/repos/someone/fork/releases/latest".into(),
                layout: Layout::OpenSteamTool,
            },
            PatchSource::Custom {
                url: "https://api.github.com/repos/someone/other/releases/latest".into(),
                layout: Layout::SteamDaddy,
            },
        ] {
            let cfg = Config {
                patch_source: source.clone(),
                last_client_build: Some("1769731672".into()),
                ..Config::defaults()
            };
            save(&path, &cfg).unwrap();
            assert_eq!(load(&path).unwrap(), cfg, "round-trip {source:?}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_patch_source_reads_as_upstream() {
        let dir = tmp_dir("oldpatch");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(
            &path,
            "version = 1
steam_path = \"C:/S\"
language = \"zh\"
",
        )
        .unwrap();
        let cfg = load(&path).unwrap();
        assert_eq!(
            cfg.patch_source,
            PatchSource::Upstream,
            "老配置缺 patch_source 应保持旧行为"
        );
        assert_eq!(cfg.last_client_build, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_custom_source_degrades_to_upstream() {
        let dir = tmp_dir("badcustom");
        let path = dir.join(CONFIG_FILE);
        for section in [
            "[patch_source]
kind = \"custom\"
url = \"http://github.com/o/r\"
layout = \"opensteamtool\"
",
            "[patch_source]
kind = \"custom\"
url = \"https://gitlab.com/o/r\"
layout = \"opensteamtool\"
",
            "[patch_source]
kind = \"custom\"
layout = \"opensteamtool\"
",
            "[patch_source]
kind = \"custom\"
url = \"https://github.com/o/r\"
layout = \"unknown\"
",
            "[patch_source]
kind = \"nonsense\"
",
        ] {
            std::fs::write(
                &path,
                format!(
                    "version = 1
steam_path = \"C:/S\"
language = \"zh\"
{section}"
                ),
            )
            .unwrap();
            let cfg = load(&path).unwrap();
            assert_eq!(
                cfg.patch_source,
                PatchSource::Upstream,
                "非法补丁源应降级上游：{section}"
            );
            assert_eq!(cfg.steam_path, "C:/S", "降级不应丢其余配置：{section}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn custom_source_matching_builtin_collapses_on_load() {
        let dir = tmp_dir("collapse");
        let path = dir.join(CONFIG_FILE);
        std::fs::write(
            &path,
            "version = 1
steam_path = \"C:/S\"
language = \"zh\"
             [patch_source]
kind = \"custom\"
             url = \"https://github.com/Contrary-v2/SteamDaddy-Dll/releases\"
             layout = \"opensteamtool\"
",
        )
        .unwrap();
        let cfg = load(&path).unwrap();
        assert_eq!(
            cfg.patch_source,
            PatchSource::SteamDaddy,
            "自定义 URL 等于内置仓库时应折叠为内置源"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_last_client_build_is_none() {
        let dir = tmp_dir("buildtol");
        let path = dir.join(CONFIG_FILE);
        for raw in ["\"\"", "\"abc\"", "\"1769-731672\"", "42", "true"] {
            std::fs::write(
                &path,
                format!(
                    "version = 1
steam_path = \"C:/S\"
language = \"zh\"
last_client_build = {raw}"
                ),
            )
            .unwrap();
            let cfg = load(&path).unwrap();
            assert_eq!(cfg.last_client_build, None, "last_client_build = {raw}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
