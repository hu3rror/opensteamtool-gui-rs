//! 外部系统接缝（SPEC §8.5 #3）：默认浏览器调起等系统级操作。
//!
//! T4（#10）：GitHub 入口经此分派器以 `cmd /c start` 调起默认浏览器
//! （零新增依赖，SPEC §8.4 Non-goals「新增依赖最小化」）。

use std::process::Command;

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
    fn repo_url_is_the_github_rewrite_target() {
        assert_eq!(
            GITHUB_REPO_URL,
            "https://github.com/hu3rror/opensteamtool-gui-rs"
        );
    }
}
