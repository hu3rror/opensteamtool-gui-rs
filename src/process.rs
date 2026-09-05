//! Steam 进程检测/监视与关闭（sysinfo，无 tasklist 子进程开销）。

use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, Instant};

use sysinfo::{ProcessesToUpdate, System};

const STEAM_PROC: &str = "steam.exe";
/// 关闭时轮询等待进程组全部消失的总预算（慢磁盘/高负载也大概率够）。
const KILL_POLL_BUDGET: Duration = Duration::from_secs(5);
/// 进程组判定中排除的服务进程：以服务形式高权限运行，普通权限杀不掉，且不阻塞 steam.exe 启动。
const STEAM_EXCLUDED_PROC: &str = "steamservice.exe";
const KILL_POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Steam 运行状态监视轮询间隔（沿用既有 2s）。
pub const STEAM_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Steam 运行状态边沿事件（自动隐身策略的输入）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SteamEvent {
    /// 检测到 Steam 启动。
    Started,
    /// 检测到 Steam 退出。
    Stopped,
}

/// Steam 运行状态监视器：持有进程表，按间隔轮询并发出边沿事件。
/// 取代「每次调用新建 System」的旧 `is_steam_running`。
pub struct SteamMonitor {
    sys: System,
    running: bool,
    last_check: Instant,
}

impl SteamMonitor {
    /// 创建并立即扫描一次（记录初始状态；启动本身不触发事件）。
    pub fn new() -> Self {
        let mut sys = System::new();
        let running = refresh(&mut sys);
        Self {
            sys,
            running,
            last_check: Instant::now(),
        }
    }

    /// 当前是否在运行。
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// 定时轮询：到间隔才扫描，状态变化返回边沿事件。
    pub fn tick(&mut self) -> Option<SteamEvent> {
        if self.last_check.elapsed() < STEAM_REFRESH_INTERVAL {
            return None;
        }
        self.force_poll()
    }

    /// 立即重扫并返回当前状态（更新内部记录；操作完成后即时判断用）。
    pub fn rescan(&mut self) -> bool {
        let now = refresh(&mut self.sys);
        self.running = now;
        now
    }

    /// 立即扫描一次并做边沿检测（跳过间隔；内部缝，供测试用）。
    fn force_poll(&mut self) -> Option<SteamEvent> {
        self.last_check = Instant::now();
        let now = refresh(&mut self.sys);
        let event = edge(self.running, now);
        self.running = now;
        event
    }
}

/// 边沿检测：状态变化 → 事件；同态 → None。
fn edge(prev: bool, now: bool) -> Option<SteamEvent> {
    match (prev, now) {
        (false, true) => Some(SteamEvent::Started),
        (true, false) => Some(SteamEvent::Stopped),
        _ => None,
    }
}

fn refresh(sys: &mut System) -> bool {
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .values()
        .any(|p| p.name().eq_ignore_ascii_case(STEAM_PROC))
}

/// 关闭所有 Steam 进程组内进程并轮询等待全部退出。
/// 已无进程组在运行时立即成功。
pub fn kill_steam(steam_dir: &Path) -> Result<(), String> {
    // 规范化为绝对路径（用户可能手填相对路径；进程 exe 路径为绝对路径）。
    let steam_dir = std::path::absolute(steam_dir).unwrap_or_else(|_| steam_dir.to_path_buf());
    let mut sys = System::new();

    sys.refresh_processes(ProcessesToUpdate::All, true);
    if !group_running(&sys, &steam_dir) {
        return Ok(()); // 本来就没在运行
    }

    // 终止进程组内全部进程（排除自身），再轮询等整组消失。
    let self_pid = sysinfo::Pid::from_u32(std::process::id());
    let pids: Vec<sysinfo::Pid> = sys
        .processes()
        .iter()
        .filter(|(pid, p)| **pid != self_pid && is_group_member(p.name(), p.exe(), &steam_dir))
        .map(|(pid, _)| *pid)
        .collect();

    for pid in pids {
        if let Some(proc) = sys.process(pid) {
            proc.kill();
        }
    }

    let deadline = Instant::now() + KILL_POLL_BUDGET;
    loop {
        sys.refresh_processes(ProcessesToUpdate::All, true);
        if !group_running(&sys, &steam_dir) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("Steam processes did not exit in time".into());
        }
        std::thread::sleep(KILL_POLL_INTERVAL);
    }
}

/// 进程组内是否有进程在运行（src/process.rs「关闭 Steam」口径）。
/// 供写 `localconfig.vdf` 等需要「Steam 彻底未运行」的门闩使用（每次调用全量扫描，
/// 比 `SteamMonitor::is_running`（仅看 steam.exe、2s 缓存）彻底：steam.exe 退出后
/// 残留的 steamwebhelper 等孤儿进程也会被计入）。
pub fn steam_group_running(steam_dir: &Path) -> bool {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    group_running(&sys, steam_dir)
}

/// 进程组内是否有进程在运行（关闭流程的「Steam 在运行」口径）。
fn group_running(sys: &System, steam_dir: &Path) -> bool {
    sys.processes()
        .values()
        .any(|p| is_group_member(p.name(), p.exe(), steam_dir))
}

/// 是否属于 Steam 进程组：exe 路径位于 Steam 目录下的进程，排除 steamservice.exe；
/// exe 路径不可用时退回主进程名（steam.exe）匹配，避免误杀未知进程。
fn is_group_member(name: impl AsRef<OsStr>, exe: Option<&Path>, steam_dir: &Path) -> bool {
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

/// 当前是否有 steam.exe 在运行（启动验证口径，同 SteamMonitor）。
pub fn steam_alive() -> bool {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .values()
        .any(|p| p.name().eq_ignore_ascii_case(STEAM_PROC))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_detects_transitions() {
        assert_eq!(edge(false, false), None);
        assert_eq!(edge(true, true), None);
        assert_eq!(edge(false, true), Some(SteamEvent::Started));
        assert_eq!(edge(true, false), Some(SteamEvent::Stopped));
    }

    #[test]
    fn monitor_smoke_on_ci() {
        // 无 Steam 环境（同旧 CI 测试假设）：初始扫描后连续 force_poll 无事件，
        // 状态与初始一致（Steam 在两次扫描间启动/退出的概率可忽略）。
        let mut m = SteamMonitor::new();
        let initial = m.is_running();
        assert_eq!(m.force_poll(), None);
        assert_eq!(m.is_running(), initial);
    }

    #[test]
    fn kill_when_not_running_succeeds() {
        // 未运行时应直接成功，不报错。
        // 注意：不能用 temp_dir() 当 steam_dir——CI 上 temp 目录下
        // 运行着 runner 的辅助进程，会被进程组逻辑误杀（v0.2.3 回归，
        // 曾导致 GitHub Actions runner 失联）。用不存在且唯一的目录。
        let dir = std::env::temp_dir().join(format!("ost_kill_test_{}", std::process::id()));
        let _ = kill_steam(&dir);
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
            Some(Path::new(r"C:\Program Files (x86)\Steam\steamwebhelper.exe")),
            steam,
        ));
        // 子目录下的进程也算（bin\cef 等）。
        assert!(is_group_member(
            "crashhandler.exe",
            Some(Path::new(r"C:\Program Files (x86)\Steam\bin\cef\crashhandler.exe")),
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
            Some(Path::new(r"C:\Program Files (x86)\Steam\bin\steamservice.exe")),
            steam,
        ));
        assert!(!is_group_member(
            "STEAMSERVICE.EXE",
            Some(Path::new(r"C:\Program Files (x86)\Steam\bin\steamservice.exe")),
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
            Some(Path::new(r"C:\Program Files (x86)\Steam\steamwebhelper.exe")),
            root,
        ));
    }
}
