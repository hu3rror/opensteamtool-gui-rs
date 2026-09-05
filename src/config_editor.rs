//! 「配置编辑器」模块：`opensteamtool.toml` 的读取、校验与原子写入。
//!
//! 目标文件为 `<Steam>/opensteamtool.toml`（上游 OpenSteamTool 的配置文件，
//! 无此文件时上游使用内置默认值并热重载）。
//!
//! 只做无损文本编辑 + 语法校验，不维护字段 schema：上游字段持续演进，
//! 结构化表单会失同步（见 issue #1 决策）。全文编辑由 `toml_edit` 保证
//! 往返保留注释与格式；本模块不修改内容，仅校验与落盘。

use std::io;
use std::path::{Path, PathBuf};

/// 校验失败：TOML 语法错误（行列 1-based）与消息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    /// 出错行（1-based）。
    pub line: usize,
    /// 出错列（1-based）。
    pub col: usize,
    /// 解析器给出的错误消息（英文，来自 toml_edit）。
    pub message: String,
}

/// 上游示例配置（`opensteamtool.example.toml`）快照，供「从示例模板创建」。
pub const EXAMPLE_TEMPLATE: &str = r#"# opensteamtool.toml — OpenSteamTool configuration
# Place at: <Steam>/opensteamtool.toml
# This file is loaded at startup and hot-reloaded after changes.

[log]
# Log verbosity for all log files (Debug build only).
# Valid: trace, debug, info, warn, error
level = "debug"

[manifest]
# Which upstream API to query for depot manifest request codes.
#   "opensteamtool" → https://manifest.opensteamtool.com/{gid}   (default)
#   "wudrm"         → http://gmrc.wudrm.com/manifest/{gid} (recommended for China users)
#   "steamrun"      → https://manifest.steam.run/api/manifest/{gid}
# If <Steam>/config/lua/manifest.lua defines fetch_manifest_code(gid) or
# fetch_manifest_code_ex(app_id, depot_id, gid), those Lua functions take
# priority over the url setting below.
#
# --- manifest.lua reference ---
# The C++ side provides http_get(url [, headers]) and
# http_post(url, body [, headers]) to Lua scripts.  Both return
# (body_string, status_code) on success, or (nil, error_msg) on failure.
#
# -- Example: try wudrm (plain-text uint64), fall back to steamrun (JSON).
# -- Return a digit *string* to avoid double-precision loss > 2^53.
# function fetch_manifest_code(gid)
#     -- wudrm responds with e.g. "10570517747114638659"
#     local body, st = http_get("http://gmrc.wudrm.com/manifest/" .. gid)
#     if st == 200 and body then return body end
#     -- steamrun responds with {"content":"1666836470726104466"}
#     body, st = http_get("https://manifest.steam.run/api/manifest/" .. gid)
#     if st == 200 and body then
#         local code = body:match('"content":"(%d+)"')
#         if code then return code end
#     end
#     return nil
# end
#
# -- Extended variant with app_id and depot_id:
# function fetch_manifest_code_ex(app_id, depot_id, gid)
#     local body, st = http_get("https://your-api.com/manifest-code/" .. app_id .. "/" .. gid)
#     if st == 200 and body and body:match("^%d+$") then
#         return body
#     end
#     return nil
# end
url = "opensteamtool"

# HTTP timeouts for manifest requests (milliseconds).
# timeout_resolve_ms  — DNS resolution        (default: 5000)
# timeout_connect_ms  — TCP handshake          (default: 5000)
# timeout_send_ms     — request transmission   (default: 10000)
# timeout_recv_ms     — response body download (default: 10000)
timeout_resolve_ms = 5000
timeout_connect_ms = 5000
timeout_send_ms    = 10000
timeout_recv_ms    = 10000

[stats]
# Automatically query https://stats.opensteamtool.com/{appid} for a recommended
# SteamID when no Lua setStat(appid, "steamid") override exists.
# Priority: setStat > stats API when enabled and valid > hardcoded preset SteamID.
enable_api = true

# Additional Lua config directories (optional).
# Files are loaded after the default <Steam>/config/lua folder.
# The default folder is always loaded last so user files take priority.
# Example — load a custom directory on another drive:
#   [lua]
#   paths = ["D:/my-steam-config/lua"]
[lua]
# paths = []

[inject]
# Optional library injection into game processes.
# The injected library must match the target process architecture.
enabled = false
# library_x64 = "OpenSteamTool.GameHook.x64.dll"
# library_x86 = "OpenSteamTool.GameHook.x86.dll"

[cloud]
# Optional Steam Cloud save redirection for unlocked ("lua") games, powered by
# CloudRedirect (https://github.com/Selectively11/CloudRedirect).
# When enabled, OpenSteamTool loads cloud_redirect.dll inside Steam, registers
# every addappid() game as a redirected app, and routes their Steam Cloud RPCs
# through CloudRedirect's cloud-save engine.
#
# Provider sign-in (Google Drive / OneDrive / local folder) is still done through
# CloudRedirect's own companion app — OpenSteamTool only hosts the DLL.
enabled = false
# Path to cloud_redirect.dll. Absolute, or relative to the Steam root directory.
# Defaults to "<Steam>/cloud_redirect.dll" when unset.
# library = "cloud_redirect.dll"

[remote]
# Optional metadata mirror. Leave unset to use GitHub with jsDelivr fallback.
# A custom mirror replaces the built-in remote sources and must include all
# three placeholders: {channel}, {component}, and {sha256}.
#
# url_template = "https://your.server/{channel}/{component}/{sha256}.toml"
# url_template = "https://fast.jsdelivr.net/gh/OpenSteam001/steam-monitor@{channel}/{component}/{sha256}.toml"
"#;

/// 配置目标文件路径：`<Steam>/opensteamtool.toml`。
pub fn target_path(steam_dir: &Path) -> PathBuf {
    steam_dir.join("opensteamtool.toml")
}

/// 校验 TOML 文本；语法错误返回带行列定位的 `ConfigError`。
pub fn validate(text: &str) -> Result<(), ConfigError> {
    match text.parse::<toml_edit::DocumentMut>() {
        Ok(_) => Ok(()),
        Err(e) => {
            let message = e.message().to_owned();
            // span 起点对应首个语法错误位置；无 span 时退化为 (1, 1)。
            let (line, col) = match e.span() {
                Some(span) => line_col(text, span.start),
                None => (1, 1),
            };
            Err(ConfigError { line, col, message })
        }
    }
}

/// span 字节偏移 → (行, 列)，均 1-based。
fn line_col(text: &str, offset: usize) -> (usize, usize) {
    // 收敛到字符边界，避免 mid-char 偏移切片 panic。
    let offset = text.floor_char_boundary(offset.min(text.len()));
    let before = &text[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let last_nl = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = before[last_nl..].chars().count() + 1;
    (line, col)
}

/// 原子写入：先写同目录临时文件再 rename，避免半截文件被上游热重载读到。
/// rename 失败时清理临时文件并返回错误。
pub fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    crate::fsutil::write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use super::*;

    /// 示例模板本身必须能通过校验（与上游快照保持一致）。
    #[test]
    fn example_template_is_valid_toml() {
        validate(EXAMPLE_TEMPLATE).expect("template must parse");
    }

    #[test]
    fn empty_text_is_valid() {
        assert!(validate("").is_ok());
        assert!(validate("\n\n").is_ok());
    }

    #[test]
    fn valid_toml_passes() {
        assert!(validate("[log]\nlevel = \"info\"\n").is_ok());
    }

    /// 语法错误带行列定位：三类错误（非法字面量/未闭合字符串/重复键）均定位到第 2 行。
    #[test]
    fn syntax_error_reports_line_col() {
        let cases = [
            ("x = 1\nbad = {{}\nz = 3\n", 2),
            ("x = 1\ns = \"abc\nz = 3\n", 2),
            ("a = 1\na = 2\n", 2),
        ];
        for (input, want_line) in cases {
            let err = validate(input).unwrap_err();
            assert_eq!(err.line, want_line, "input: {input:?}");
            assert!(err.col >= 1);
            assert!(!err.message.is_empty());
        }
    }

    /// 头部未闭合的表格报错（消息非空、行列合理）。
    #[test]
    fn unclosed_table_reports_error() {
        let err = validate("a = 1\n[broken\nb = 2\n").unwrap_err();
        assert!(!err.message.is_empty());
        assert!(err.line >= 1);
        assert!(err.col >= 1);
    }

    #[test]
    fn line_col_helper() {
        // "ab\ncd": a(0) b(1) \n(2) c(3) d(4)
        assert_eq!(line_col("ab\ncd", 0), (1, 1));
        assert_eq!(line_col("ab\ncd", 1), (1, 2));
        assert_eq!(line_col("ab\ncd", 3), (2, 1));
        assert_eq!(line_col("ab\ncd", 4), (2, 2));
        // 越界偏移收敛到文本末尾。
        assert_eq!(line_col("ab\ncd", 99), (2, 3));
        // 多字节字符：列按字符计数（非字节）；偏移在字内时收敛到字首。
        assert_eq!(line_col("中文\nx", 7), (2, 1)); // offset 7 落在换行之后
        assert_eq!(line_col("中文", 6), (1, 3));
        assert_eq!(line_col("中文", 4), (1, 2)); // 4 落在「文」字内 → floor 到字首 3 → 列 2
    }

    /// 写入创建文件并可覆盖。
    #[test]
    fn write_atomic_creates_and_overwrites() {
        let dir = std::env::temp_dir().join(format!("ost-ce-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opensteamtool.toml");

        write_atomic(&path, "[log]\nlevel = \"debug\"\n").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[log]\nlevel = \"debug\"\n"
        );

        write_atomic(&path, "url = \"opensteamtool\"\n").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "url = \"opensteamtool\"\n"
        );

        // 清理临时目录。
        let _ = fs::remove_dir_all(&dir);
    }

    /// write_atomic 不留下临时文件残片。
    #[test]
    fn write_atomic_leaves_no_temp_files() {
        let dir = std::env::temp_dir().join(format!("ost-ce-tmp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opensteamtool.toml");
        write_atomic(&path, "a = 1\n").unwrap();
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "opensteamtool.toml")
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn target_path_joins_steam_dir() {
        let p = target_path(Path::new("C:/Steam"));
        assert_eq!(p, PathBuf::from("C:/Steam/opensteamtool.toml"));
        assert_eq!(p.parent(), Some(Path::new("C:/Steam")));
    }
}
