//! Steam 进程监视与自动隐身联动。
//!
//! 进程表本体与三查询（`alive` / `group_running` / `kill`）在
//! [`crate::steam_state::SteamState`]（共享 `Mutex<System>`，深模块）；
//! 本模块是它之上的 UI 侧适配：2s 节流轮询、边沿事件、运行态缓存。

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::steam_state::SteamState;

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

/// Steam 运行状态监视器：持有共享 [`SteamState`]，按间隔轮询并发出边沿事件。
/// 取代「每次调用新建 System」的旧 `is_steam_running`。
pub struct SteamMonitor {
    steam: Arc<SteamState>,
    running: bool,
    last_check: Instant,
}

impl SteamMonitor {
    /// 创建并立即扫描一次（记录初始状态；启动本身不触发事件）。
    pub fn new(steam: &Arc<SteamState>) -> Self {
        let running = steam.alive();
        Self {
            steam: steam.clone(),
            running,
            last_check: Instant::now(),
        }
    }

    /// 当前是否在运行（读最近一次扫描的缓存，不触发刷新）。
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

/// 边沿检测：状态变化 → 事件；同态 → None。
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
        // 冒烟：监视器管线能跑通且不自相矛盾。不断言「无事件」——本机可能恰好有
        // Steam 启动/退出（真实事件）；改断言事件与状态转移一致：CI 无 Steam 时恒为
        // None，有 Steam 时 Started/Stopped 必对应 is_running() 的相应变化，两环境都
        // 稳定（原断言在 Steam 于两次扫描间启动的开发机上误红，属环境依赖而非缺陷）。
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
