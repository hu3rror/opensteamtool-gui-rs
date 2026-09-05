//! 共享小工具：原子文件写入。
//!
//! 两个编辑模块（`config_editor` 的 TOML、`onlinefix` 的 VDF）都需要
//! 「临时文件 + rename」的原子落盘，避免半截文件被读取方看到
//! （TOML 热重载 / Steam 配置回写）。本模块收敛这一重复实现。

use std::fs;
use std::io;
use std::path::Path;

/// 原子写入：先写同目录临时文件再 rename；rename 失败时清理临时文件。
/// 临时文件名带进程号，避免并发实例互相覆盖。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}
