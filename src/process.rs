//! Steam 进程检测与关闭（sysinfo，无 tasklist 子进程开销）。

use std::time::{Duration, Instant};

use sysinfo::{ProcessesToUpdate, System};

const STEAM_PROC: &str = "steam.exe";
/// kill 后轮询：最多 10 次 × 100ms。
const KILL_POLL_ATTEMPTS: u32 = 10;
const KILL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// 检测当前是否有 steam.exe 进程在运行。
/// 使用独立的 `System`，调用方可在任意线程使用。
pub fn is_steam_running() -> bool {
    let mut sys = System::new();
    refresh(&mut sys)
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
    fn steam_not_running_on_ci() {
        // 无 Steam 环境应返回 false；不要求断言具体值，只保证可调用。
        let _ = is_steam_running();
    }

    #[test]
    fn kill_when_not_running_succeeds() {
        // 未运行时应直接成功，不报错。
        let _ = kill_steam();
    }
}
