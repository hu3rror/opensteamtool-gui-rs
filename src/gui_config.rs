//! GUI 全局偏好持久化（`gui_config.toml`，SPEC §8.3 / §8.6 AC4）。
//!
//! 偏好文件由全局路径解析器定位（`paths::resolver().config_path()`，统一命名
//! `gui_config.toml`，便携/安装模式仅目录不同）。缺失/损坏/未知值静默回退默认
//! （`Auto` + 勾选）；写盘走原子写（`fsutil`）。

use std::path::Path;

use crate::fsutil;
use crate::i18n::LanguagePreference;

/// GUI 用户全局偏好。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiConfig {
    pub language: LanguagePreference,
    pub minimize_to_tray: bool,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            language: LanguagePreference::Auto,
            minimize_to_tray: true,
        }
    }
}

impl GuiConfig {
    /// 从配置文件加载；缺失/损坏/缺字段 → 对应默认值，绝不 panic。
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
            return Self::default();
        };
        let mut cfg = Self::default();
        if let Some(lang) = doc.get("language").and_then(|v| v.as_str()) {
            cfg.language = LanguagePreference::parse(lang);
        }
        if let Some(m) = doc.get("minimize_to_tray").and_then(|v| v.as_bool()) {
            cfg.minimize_to_tray = m;
        }
        cfg
    }

    /// 原子写盘；失败返回错误信息（调用方决定是否提示——写偏好失败不阻塞交互）。
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut doc = toml_edit::DocumentMut::new();
        doc["language"] = toml_edit::value(self.language.as_str());
        doc["minimize_to_tray"] = toml_edit::value(self.minimize_to_tray);
        fsutil::write_atomic(path, doc.to_string().as_bytes()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_cfg(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ost_gui_{}_{}", name, std::process::id()))
    }

    #[test]
    fn load_missing_returns_default() {
        let path = tmp_cfg("missing");
        std::fs::remove_file(&path).ok();
        assert_eq!(GuiConfig::load(&path), GuiConfig::default());
    }

    #[test]
    fn load_corrupt_returns_default() {
        let path = tmp_cfg("corrupt");
        std::fs::write(&path, "not [valid toml ===").unwrap();
        assert_eq!(GuiConfig::load(&path), GuiConfig::default());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_valid_parses_fields() {
        let path = tmp_cfg("valid");
        std::fs::write(&path, "language = \"zh\"\nminimize_to_tray = false\n").unwrap();
        let cfg = GuiConfig::load(&path);
        assert_eq!(cfg.language, LanguagePreference::Zh);
        assert!(!cfg.minimize_to_tray);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_unknown_language_falls_back_auto() {
        let path = tmp_cfg("unknown_lang");
        std::fs::write(&path, "language = \"fr\"\nminimize_to_tray = false\n").unwrap();
        let cfg = GuiConfig::load(&path);
        assert_eq!(cfg.language, LanguagePreference::Auto);
        assert!(!cfg.minimize_to_tray);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_missing_fields_use_defaults() {
        // 只写了 language：minimize_to_tray 保持默认 true。
        let path = tmp_cfg("partial");
        std::fs::write(&path, "language = \"en\"\n").unwrap();
        let cfg = GuiConfig::load(&path);
        assert_eq!(cfg.language, LanguagePreference::En);
        assert!(cfg.minimize_to_tray);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_writes_toml_file() {
        let path = tmp_cfg("save");
        std::fs::remove_file(&path).ok();
        let cfg = GuiConfig {
            language: LanguagePreference::En,
            minimize_to_tray: false,
        };
        cfg.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("language = \"en\""));
        assert!(text.contains("minimize_to_tray = false"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_then_load_roundtrip_restores_preference() {
        // 重启还原：save 后 load 得到同一偏好（模拟冷启动）。
        let path = tmp_cfg("roundtrip");
        std::fs::remove_file(&path).ok();
        let cfg = GuiConfig {
            language: LanguagePreference::Zh,
            minimize_to_tray: false,
        };
        cfg.save(&path).unwrap();
        assert_eq!(GuiConfig::load(&path), cfg);
        std::fs::remove_file(&path).ok();
    }
}
