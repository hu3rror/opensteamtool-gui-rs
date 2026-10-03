//! 应用配置（App Config）：exe 同目录 `config.toml`，便携优先（ADR-0012）。
//! 只存用户选择，不存派生状态（ADR-0011 精神）。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::fsutil::write_atomic;
use crate::i18n::{Lang, detect_system_lang};

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
}

impl Config {
    pub fn defaults() -> Self {
        Self {
            version: CONFIG_VERSION,
            steam_path: String::new(),
            language: Language::Auto,
            minimize_to_tray: true,
            theme: ThemePreference::System,
        }
    }
}

pub fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONFIG_FILE)
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

    // 最小化隐身：字段缺失 = 默认启用（老配置无该字段，不迁移不 bump 版本）；
    let minimize_to_tray = match doc.get("minimize_to_tray") {
        None => true,
        Some(v) => v.as_bool().unwrap_or(true),
    };

    // 主题偏好：字段缺失/非法 = 默认跟随系统（老配置无该字段，不迁移不 bump 版本）。
    let theme = match doc.get("theme") {
        None => ThemePreference::System,
        Some(v) => v
            .as_str()
            .and_then(ThemePreference::parse)
            .unwrap_or(ThemePreference::System),
    };

    Ok(Config {
        version: CONFIG_VERSION,
        steam_path,
        language,
        minimize_to_tray,
        theme,
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
    doc.to_string()
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
}
