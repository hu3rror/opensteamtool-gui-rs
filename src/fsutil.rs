//! 原子文件写入（临时文件 + rename）与「工具目录」（exe 同目录）路径解析。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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

/// 工具目录 = exe 所在目录（便携布局根，GLOSSARY「工具目录」，ADR-0012「随目录拷贝即迁移」）。
/// `current_exe()` 失败（罕见）回退当前工作目录。
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_dir_resolves_to_binary_parent() {
        let parent = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        assert_eq!(exe_dir(), parent);
        assert_ne!(exe_dir(), PathBuf::from("."));
    }
}
