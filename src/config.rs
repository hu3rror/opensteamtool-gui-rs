//! 应用配置（App Config）：exe 同目录 `config.toml`，便携优先。
//!
//! 与 `dlls/`、`cache/` 同一便携思路——整个目录拷贝即带走设置，不落 `%APPDATA%`
//! （取舍见 ADR-0012）。只存「用户选择」（语言偏好、Steam 路径），不存任何派生
//! 状态：补丁是否已下载是文件系统事实（`dlls/` 三个目标 DLL 是否齐全），永远
//! 不进配置，避免第二事实源（ADR-0011 精神）。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::fsutil::write_atomic;
use crate::i18n::{Lang, detect_system_lang};

/// 当前配置格式版本（未来格式变更时递增并写迁移逻辑）。
pub const CONFIG_VERSION: u32 = 1;

/// 配置文件名称（位于 exe 同目录）。
pub const CONFIG_FILE: &str = "config.toml";

/// 语言偏好三态：「跟随系统 / 中文 / English」。
/// 「跟随系统」仅在解析为实际语言时才查询系统（`effective`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    Auto,
    Zh,
    En,
}

impl Language {
    /// 配置字符串解析：`"auto" | "zh" | "en"`；其它取值返回 None。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Language::Auto),
            "zh" => Some(Language::Zh),
            "en" => Some(Language::En),
            _ => None,
        }
    }

    /// 序列化字符串（与 schema 一一对应）。
    pub fn as_str(self) -> &'static str {
        match self {
            Language::Auto => "auto",
            Language::Zh => "zh",
            Language::En => "en",
        }
    }

    /// 实际生效语言：`auto` 跟随系统检测，`zh`/`en` 固定。
    pub fn effective(self) -> Lang {
        match self {
            Language::Auto => detect_system_lang(),
            Language::Zh => Lang::Zh,
            Language::En => Lang::En,
        }
    }

    /// 顶栏切换的目标偏好：`zh`↔`en` 互切；`auto` 时切到当前生效语言的另一侧
    /// （「跟随系统」不可从顶栏选回——三态选择是 Settings — General 的选项，见 #26）。
    pub fn toggled(self, effective: Lang) -> Self {
        match self {
            Language::Auto => match effective {
                Lang::Zh => Language::En,
                Lang::En => Language::Zh,
            },
            Language::Zh => Language::En,
            Language::En => Language::Zh,
        }
    }
}

/// 配置读取错误（类型化；调用方决定降级策略，启动路径一律降级默认值、不 panic）。
#[derive(Debug)]
pub enum ConfigError {
    /// 文件系统错误（缺文件除外——缺文件是「未配置」的合法形态，走默认值）。
    Io(io::Error),
    /// TOML 语法错误或字段缺失/类型非法/取值非法。
    Parse(String),
    /// 配置版本不受支持（未来格式迁移时启用）。
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

/// 应用配置内存形态（与 `config.toml` 一一对应）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// 格式版本（恒为 `CONFIG_VERSION`，load 校验后归一）。
    pub version: u32,
    /// Steam 安装路径；空串 = 未设置（启动时回退注册表检测）。
    pub steam_path: String,
    /// 语言偏好（三态）。
    pub language: Language,
}

impl Config {
    /// 缺文件/内容非法时的兜底默认值：未设置路径、跟随系统。
    pub fn defaults() -> Self {
        Self {
            version: CONFIG_VERSION,
            steam_path: String::new(),
            language: Language::Auto,
        }
    }
}

/// `config.toml` 完整路径：exe 同目录（便携版；`current_exe` 不可用时回退当前目录）。
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

/// 原子落盘：TOML 序列化 → `write_atomic`（同目录临时文件 + rename），任何时刻
/// 都不会出现半截 `config.toml`。父目录缺失时先创建（测试与异常场景兜底）。
pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    let text = serialize(config);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(ConfigError::Io)?;
    }
    write_atomic(path, text.as_bytes()).map_err(ConfigError::Io)
}

/// TOML 解析（三字段小 schema 手动映射；任何非法一律类型化错误，严格而非宽容——
/// 宽容会悄悄吞掉写错的文件，严格让调用方显式降级）。
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

    Ok(Config {
        version: CONFIG_VERSION,
        steam_path,
        language,
    })
}

/// TOML 序列化（手动构造 DocumentMut：三字段小 schema，避免引入 serde derive 依赖，
/// 见 #26「零新依赖」决策）。
fn serialize(config: &Config) -> String {
    let mut doc = toml_edit::DocumentMut::new();
    doc["version"] = toml_edit::value(config.version as i64);
    doc["steam_path"] = toml_edit::value(config.steam_path.trim().to_owned());
    doc["language"] = toml_edit::value(config.language.as_str());
    doc.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 唯一临时目录（测试并行时避免互踩；沿用本仓库纯状态模块测试的惯例）。
    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ost_cfg_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 三语言 × 有/无路径的完整往返：save → load 恒等。
    #[test]
    fn round_trip_all_fields() {
        let dir = tmp_dir("roundtrip");
        let path = dir.join(CONFIG_FILE);
        for lang in [Language::Auto, Language::Zh, Language::En] {
            for steam_path in ["C:/Program Files (x86)/Steam", ""] {
                let cfg = Config {
                    version: CONFIG_VERSION,
                    steam_path: steam_path.into(),
                    language: lang,
                };
                save(&path, &cfg).unwrap();
                assert_eq!(
                    load(&path).unwrap(),
                    cfg,
                    "round-trip {lang:?} {steam_path:?}"
                );
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 缺失文件 = 未配置（默认值），不报错。
    #[test]
    fn missing_file_is_defaults() {
        let dir = tmp_dir("missing");
        let cfg = load(&dir.join("nope.toml")).unwrap();
        assert_eq!(cfg, Config::defaults());
        assert_eq!(cfg.steam_path, "");
        assert_eq!(cfg.language, Language::Auto);
        assert_eq!(cfg.version, CONFIG_VERSION);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 各类非法内容 → 类型化 Parse 错误（不 panic；调用方负责降级）。
    #[test]
    fn malformed_content_is_typed_error() {
        let dir = tmp_dir("malformed");
        let path = dir.join(CONFIG_FILE);
        // TOML 语法错误。
        std::fs::write(&path, "version = 1\nsteam_path = ").unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        // 字段类型错误（version 为字符串）。
        std::fs::write(
            &path,
            "version = \"one\"\nsteam_path = \"C:/S\"\nlanguage = \"zh\"",
        )
        .unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        // 字段缺失。
        std::fs::write(&path, "version = 1\nsteam_path = \"C:/S\"").unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        // language 取值非法。
        std::fs::write(
            &path,
            "version = 1\nsteam_path = \"C:/S\"\nlanguage = \"fr\"",
        )
        .unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 版本不符 → 类型化 UnsupportedVersion（未来迁移的入口判据）。
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

    /// 原子写：保存后目录内无残留临时文件，且落盘内容独立可读回（不依赖内存态）。
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

    /// 父目录缺失时 save 自动创建（便携目录恒存在，此处兜底）。
    #[test]
    fn save_creates_missing_parent_dir() {
        let dir = tmp_dir("parent");
        let path = dir.join("nested").join("deep").join(CONFIG_FILE);
        save(&path, &Config::defaults()).unwrap();
        assert!(path.is_file());
        std::fs::remove_dir_all(&dir).ok();
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

    /// 生效语言：zh/en 固定（不随系统变）；auto = 系统检测结果。
    #[test]
    fn language_effective_pins() {
        assert_eq!(Language::Zh.effective(), Lang::Zh);
        assert_eq!(Language::En.effective(), Lang::En);
        assert_eq!(Language::Auto.effective(), detect_system_lang());
    }

    /// 顶栏切换表：zh↔en 互切；auto 切到当前生效语言的另一侧。
    #[test]
    fn language_toggled_table() {
        assert_eq!(Language::Zh.toggled(Lang::Zh), Language::En);
        assert_eq!(Language::En.toggled(Lang::En), Language::Zh);
        assert_eq!(Language::Auto.toggled(Lang::Zh), Language::En);
        assert_eq!(Language::Auto.toggled(Lang::En), Language::Zh);
    }
}
