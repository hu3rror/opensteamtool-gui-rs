//! 共享小工具：原子文件写入。
//!
//! 「临时文件 + rename」的原子落盘，避免半截文件被读取方看到
//! （补丁部署、缓存预热、验证缓存回写等）。本模块收敛这一重复实现。

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
