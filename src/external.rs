//! 外部系统接缝（SPEC §8.5 #3）：默认浏览器调起等系统级操作。
//!
//! T4（#10）：GitHub 入口经此分派器以 `cmd /c start` 调起默认浏览器
//! （零新增依赖，SPEC §8.4 Non-goals「新增依赖最小化」）。

use std::fs;
use std::path::Path;
use std::process::Command;
/// 本地清单读取接缝：解耦物理磁盘 ACF 文件 IO（SPEC §8.5 #2）。
/// T7（#11）：`resolve_game_name` 经此读取 `appmanifest_<appid>.acf`；
/// 测试注入 mock 断言合规/畸变/缺失，不真正触盘。
pub trait ManifestAccessor: Send + Sync {
    /// 读取 `<steam_dir>/steamapps/appmanifest_<appid>.acf` 全文；缺失/读失败 → None。
    fn read_manifest_content(&self, steam_dir: &Path, appid: u32) -> Option<String>;
}

/// 真实实现：直接读盘（Steam 本地清单为 UTF-8 文本，lossy 不必要）。
pub struct FsManifestAccessor;

impl ManifestAccessor for FsManifestAccessor {
    fn read_manifest_content(&self, steam_dir: &Path, appid: u32) -> Option<String> {
        let path = steam_dir
            .join("steamapps")
            .join(format!("appmanifest_{appid}.acf"));
        fs::read_to_string(path).ok()
    }
}

/// 仓库主页（T4 AC1：GitHub 按钮目标 URL）。
pub const GITHUB_REPO_URL: &str = "https://github.com/hu3rror/opensteamtool-gui-rs";

/// 外部进程与命令分派接缝：默认浏览器调起（`cmd /c start` 实现）。
/// 测试注入 mock 记录调用而不真正拉起浏览器。
pub trait ExternalSystemDispatcher: Send + Sync {
    /// 以系统默认浏览器打开 URL；失败返回错误描述（不 panic）。
    fn open_browser_url(&self, url: &str) -> Result<(), String>;
}

/// 构造 `cmd /c start "" <url>` 命令（纯函数：测试断言 argv 而不真正拉起浏览器）。
fn cmd_start_command(url: &str) -> Command {
    let mut cmd = Command::new("cmd");
    // 空标题参数：URL 以引号开头时会被 start 误认为窗口标题（防御性，当前 URL 不含引号）。
    cmd.args(["/c", "start", "", url]);
    cmd
}

/// 真实实现：`cmd /c start` 调起默认浏览器（Windows 内置，零依赖）。
pub struct CmdStartDispatcher;

impl ExternalSystemDispatcher for CmdStartDispatcher {
    fn open_browser_url(&self, url: &str) -> Result<(), String> {
        cmd_start_command(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("failed to launch browser via cmd: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_start_command_targets_default_browser() {
        let cmd = cmd_start_command("https://example.com/a?b=1");
        assert_eq!(cmd.get_program(), "cmd");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["/c", "start", "", "https://example.com/a?b=1"]);
    }

    #[test]
    fn fs_manifest_reads_existing_acf_and_missing_is_none() {
        let dir = std::env::temp_dir().join(format!("ost-manifest-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("steamapps")).unwrap();
        std::fs::write(
            dir.join("steamapps").join("appmanifest_367520.acf"),
            "\"AppState\"\n{\n\t\"name\"\t\t\"Hollow Knight\"\n}\n",
        )
        .unwrap();
        let acc = FsManifestAccessor;
        let content = acc.read_manifest_content(&dir, 367520).unwrap();
        assert!(content.contains("Hollow Knight"));
        // 缺失 AppID → None。
        assert_eq!(acc.read_manifest_content(&dir, 9999), None);
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn repo_url_is_the_github_rewrite_target() {
        assert_eq!(
            GITHUB_REPO_URL,
            "https://github.com/hu3rror/opensteamtool-gui-rs"
        );
    }
}
