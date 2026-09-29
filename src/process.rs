//! Steam 运行状态监视与自动隐身联动（2s 轮询、边沿事件、运行态缓存）。

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::steam_state::SteamState;

pub const STEAM_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SteamEvent {
    Started,
    Stopped,
}

pub struct SteamMonitor {
    steam: Arc<SteamState>,
    running: bool,
    last_check: Instant,
}

impl SteamMonitor {
    pub fn new(steam: &Arc<SteamState>) -> Self {
        let running = steam.alive();
        Self {
            steam: steam.clone(),
            running,
            last_check: Instant::now(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn tick(&mut self) -> Option<SteamEvent> {
        if self.last_check.elapsed() < STEAM_REFRESH_INTERVAL {
            return None;
        }
        self.force_poll()
    }

    pub fn rescan(&mut self) -> bool {
        let now = self.steam.alive();
        self.running = now;
        now
    }

    /// 立即扫描一次并做边沿检测（跳过间隔；内部缝，供测试用）。
    fn force_poll(&mut self) -> Option<SteamEvent> {
        self.last_check = Instant::now();
        let now = self.steam.alive();
        let event = edge(self.running, now);
        self.running = now;
        event
    }
}

fn edge(prev: bool, now: bool) -> Option<SteamEvent> {
    match (prev, now) {
        (false, true) => Some(SteamEvent::Started),
        (true, false) => Some(SteamEvent::Stopped),
        _ => None,
    }
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
        let steam = Arc::new(SteamState::new());
        let mut m = SteamMonitor::new(&steam);
        let initial = m.is_running();
        let event = m.force_poll();
        match event {
            None => assert_eq!(m.is_running(), initial, "无事件时状态应不变"),
            Some(SteamEvent::Started) => {
                assert!(!initial, "Started 事件只能在初始未运行时出现");
                assert!(m.is_running(), "Started 事件应对应 Steam 运行中");
            }
            Some(SteamEvent::Stopped) => {
                assert!(initial, "Stopped 事件只能在初始运行时出现");
                assert!(!m.is_running(), "Stopped 事件应对应 Steam 已退出");
            }
        }
    }
}
