//! Steam 运行状态模块：全应用共享的进程表与三种判定口径。
//!
//! 深模块——小接口（`alive` / `group_running` / `kill`），大实现（共享 `System`
//! 生命周期、两种判定口径、进程组终止与轮询预算）。缓存职责在调用层
//! （`process::SteamMonitor` 承担 2s 节流与运行态缓存）；本模块每次查询
//! 都在锁内「刷新 + 判定」，语义实时。

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sysinfo::{ProcessesToUpdate, System};

/// 主进程名（alive 口径与组判定回退）。
const STEAM_PROC: &str = "steam.exe";
/// 进程组判定中排除的服务进程：以服务形式高权限运行，普通权限杀不掉，且不阻塞 `steam.exe` 启动。
const STEAM_EXCLUDED_PROC: &str = "steamservice.exe";
/// 关闭时轮询等待进程组全部消失的总预算（慢磁盘/高负载也大概率够，见 ADR-0004）。
const KILL_POLL_BUDGET: Duration = Duration::from_secs(5);
/// 关闭轮询间隔。
const KILL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Steam 运行状态：唯一的进程表入口。
///
/// 三种查询对应两种口径（ADR-0004 有意为之）：
/// - [`Self::alive`]：只看 `steam.exe`，快而便宜（UI 轮询 / 启动验证）。
/// - [`Self::group_running`]：进程组判定（路径前缀，彻底），写 `localconfig.vdf`
///   等需要「Steam 彻底未运行」的门闩用。
/// - [`Self::kill`]：终止整个进程组并轮询等待全部消失。
///
/// `System` 跨线程共享（`Send + Sync`，sysinfo 0.39），内部以 `Mutex` 串行化
/// 刷新与判定；kill 用多段锁——扫描/判空短暂持锁，等待期不阻塞其他查询。
#[derive(Debug)]
pub struct SteamState {
    sys: Mutex<System>,
}

impl SteamState {
    /// 创建空进程表（首次查询时才真正刷新）。
    pub fn new() -> Self {
        Self {
            sys: Mutex::new(System::new()),
        }
    }

    /// 当前是否有 `steam.exe` 在运行（启动验证口径，同 ADR-0004「Steam 正在运行」）。
    pub fn alive(&self) -> bool {
        let mut sys = self.sys.lock().unwrap();
        refresh(&mut sys);
        sys.processes()
            .values()
            .any(|p| p.name().eq_ignore_ascii_case(STEAM_PROC))
    }

    /// 进程组内是否有进程在运行（关闭 Steam 的「在运行」口径）。
    ///
    /// 供写 `localconfig.vdf` 等需要「Steam 彻底未运行」的门闩使用：每次调用
    /// 全量扫描，比 [`Self::alive`]（仅看 steam.exe、2s 缓存）彻底——steam.exe
    /// 退出后残留的 steamwebhelper 等孤儿进程也会被计入。
    pub fn group_running(&self, dir: &Path) -> bool {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        let mut sys = self.sys.lock().unwrap();
        refresh(&mut sys);
        any_group_member(&sys, &dir)
    }

    /// 终止 Steam 进程组内全部进程并轮询等待全部退出（预算 5s、间隔 100ms）。
    /// 已无进程组在运行时立即成功。
    ///
    /// 多段锁：扫描快照 + 发送终止信号在一个锁块内（kill 为系统调用，毫秒级）；
    /// 随后轮询判空循环中 sleep 在锁外，等待期间不阻塞其他查询。
    pub fn kill(&self, dir: &Path) -> Result<(), String> {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        let self_pid = sysinfo::Pid::from_u32(std::process::id());
        {
            let mut sys = self.sys.lock().unwrap();
            refresh(&mut sys);
            if !any_group_member(&sys, &dir) {
                return Ok(()); // 本来就没在运行
            }
            let pids: Vec<sysinfo::Pid> = sys
                .processes()
                .iter()
                .filter(|(pid, p)| **pid != self_pid && is_group_member(p.name(), p.exe(), &dir))
                .map(|(pid, _)| *pid)
                .collect();
            for pid in pids {
                if let Some(proc) = sys.process(pid) {
                    proc.kill();
                }
            }
        }
        let deadline = Instant::now() + KILL_POLL_BUDGET;
        loop {
            {
                let mut sys = self.sys.lock().unwrap();
                refresh(&mut sys);
                if !any_group_member(&sys, &dir) {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err("Steam processes did not exit in time".into());
            }
            std::thread::sleep(KILL_POLL_INTERVAL);
        }
    }
}

fn refresh(sys: &mut System) {
    sys.refresh_processes(ProcessesToUpdate::All, true);
}

/// 进程组内是否有进程在运行（关闭流程的「Steam 在运行」口径）。
fn any_group_member(sys: &System, dir: &Path) -> bool {
    sys.processes()
        .values()
        .any(|p| is_group_member(p.name(), p.exe(), dir))
}

/// 是否属于 Steam 进程组：exe 路径位于 Steam 目录下的进程，排除 steamservice.exe；
/// exe 路径不可用时退回主进程名（steam.exe）匹配，避免误杀未知进程。
fn is_group_member(
    name: impl AsRef<std::ffi::OsStr>,
    exe: Option<&Path>,
    steam_dir: &Path,
) -> bool {
    let name = name.as_ref();
    if name.eq_ignore_ascii_case(STEAM_EXCLUDED_PROC) {
        return false;
    }
    match exe {
        Some(path) => {
            // 大小写不敏感 + 目录边界的前缀比较（Windows 路径不区分大小写）。
            let dir = steam_dir.to_string_lossy().to_lowercase();
            let dir = dir.trim_end_matches('\\');
            if is_drive_root(dir) {
                return false; // 盘符根目录（如 c:）前缀过宽，会误伤该盘所有进程
            }
            let exe = path.to_string_lossy().to_lowercase();
            exe == dir || exe.starts_with(&format!("{dir}\\"))
        }
        None => name.eq_ignore_ascii_case(STEAM_PROC),
    }
}

/// 是否为盘符根目录（如 `c:`）：前缀比较会误伤该盘所有进程，判定为不属于进程组。
fn is_drive_root(dir: &str) -> bool {
    let b = dir.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未运行时 kill 直接成功，不报错。
    /// 注意：不能用 temp_dir() 当 steam_dir——CI 上 temp 目录下运行着 runner 的
    /// 辅助进程，会被进程组逻辑误杀（v0.2.3 回归，曾导致 GitHub Actions runner
    /// 失联）。用不存在且唯一的目录。
    #[test]
    fn kill_when_not_running_succeeds() {
        let dir = std::env::temp_dir().join(format!("ost_kill_test_{}", std::process::id()));
        let state = SteamState::new();
        let _ = state.kill(&dir);
    }

    /// 唯一空目录下进程组判定为 false（该目录下不可能有任何进程）。
    #[test]
    fn empty_dir_group_running_is_false() {
        let dir = std::env::temp_dir().join(format!("ost_group_test_{}", std::process::id()));
        let state = SteamState::new();
        assert!(!state.group_running(&dir));
    }

    #[test]
    fn group_member_path_based() {
        let steam = Path::new(r"C:\Program Files (x86)\Steam");
        assert!(is_group_member(
            "steam.exe",
            Some(Path::new(r"C:\Program Files (x86)\Steam\steam.exe")),
            steam,
        ));
        assert!(is_group_member(
            "steamwebhelper.exe",
            Some(Path::new(
                r"C:\Program Files (x86)\Steam\steamwebhelper.exe"
            )),
            steam,
        ));
        // 子目录下的进程也算（bin\cef 等）。
        assert!(is_group_member(
            "crashhandler.exe",
            Some(Path::new(
                r"C:\Program Files (x86)\Steam\bin\cef\crashhandler.exe"
            )),
            steam,
        ));
        // 大小写不敏感。
        assert!(is_group_member(
            "steam.exe",
            Some(Path::new(r"c:\program files (x86)\steam\steam.exe")),
            steam,
        ));
        // Steam 目录外的进程不算（含前缀相似的 Steam1）。
        assert!(!is_group_member(
            "steam.exe",
            Some(Path::new(r"C:\Program Files (x86)\SteamLibrary\steam.exe")),
            steam,
        ));
        assert!(!is_group_member(
            "notepad.exe",
            Some(Path::new(r"C:\Windows\System32\notepad.exe")),
            steam,
        ));
        assert!(!is_group_member(
            "foo.exe",
            Some(Path::new(r"C:\Program Files (x86)\Steam1\foo.exe")),
            steam,
        ));
    }

    #[test]
    fn group_member_excludes_service() {
        let steam = Path::new(r"C:\Program Files (x86)\Steam");
        assert!(!is_group_member(
            "steamservice.exe",
            Some(Path::new(
                r"C:\Program Files (x86)\Steam\bin\steamservice.exe"
            )),
            steam,
        ));
        assert!(!is_group_member(
            "STEAMSERVICE.EXE",
            Some(Path::new(
                r"C:\Program Files (x86)\Steam\bin\steamservice.exe"
            )),
            steam,
        ));
    }

    #[test]
    fn group_member_fallback_to_name() {
        let steam = Path::new(r"C:\Program Files (x86)\Steam");
        // exe 路径不可用时只认主进程名，避免误杀未知进程。
        assert!(is_group_member("steam.exe", None, steam));
        assert!(!is_group_member("steamwebhelper.exe", None, steam));
    }

    #[test]
    fn group_member_rejects_drive_root() {
        // 盘符根目录（C:\）前缀过宽：不应把整盘进程算进组，避免误杀。
        let root = Path::new(r"C:\");
        assert!(!is_group_member(
            "steam.exe",
            Some(Path::new(r"C:\Windows\System32\notepad.exe")),
            root,
        ));
        assert!(!is_group_member(
            "steamwebhelper.exe",
            Some(Path::new(
                r"C:\Program Files (x86)\Steam\steamwebhelper.exe"
            )),
            root,
        ));
    }
}
