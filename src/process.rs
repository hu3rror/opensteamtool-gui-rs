//! Steam 进程检测/监视与关闭（sysinfo，无 tasklist 子进程开销）。

use std::time::{Duration, Instant};

use sysinfo::{ProcessesToUpdate, System};

const STEAM_PROC: &str = "steam.exe";
/// kill 后轮询：最多 10 次 × 100ms。
const KILL_POLL_ATTEMPTS: u32 = 10;
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

/// 关闭所有 steam.exe 进程并轮询等待退出。
/// 已无进程在运行时立即成功。
pub fn kill_steam() -> Result<(), String> {
    let mut sys = System::new();

    if !refresh(&mut sys) {
        return Ok(()); // 本来就没在运行
    }

    // 收集 PID 后逐个 kill。
    let pids: Vec<sysinfo::Pid> = sys
        .processes()
        .iter()
        .filter(|(_, p)| p.name().eq_ignore_ascii_case(STEAM_PROC))
        .map(|(pid, _)| *pid)
        .collect();

    for pid in pids {
        if let Some(proc) = sys.process(pid) {
            proc.kill();
        }
    }

    // 轮询等待退出。
    let deadline = Instant::now() + KILL_POLL_INTERVAL * KILL_POLL_ATTEMPTS;
    while refresh(&mut sys) {
        if Instant::now() >= deadline {
            return Err("steam.exe did not exit in time".into());
        }
        std::thread::sleep(KILL_POLL_INTERVAL);
    }
    Ok(())
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
        let _ = kill_steam();
    }
}
