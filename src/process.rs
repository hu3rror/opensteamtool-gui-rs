//! Steam 运行状态监视与自动隐身联动（后台线程 2s 轮询、边沿事件、运行态缓存、rescan 立即轮询）。

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use crate::steam_state::SteamState;

pub const STEAM_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SteamEvent {
    Started,
    Stopped,
}

/// 后台监视线程的上报：边沿事件与 rescan 状态观察（壳按语义映射进编排核心）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MonitorUpdate {
    /// 状态变化边沿（2s 轮询或 rescan 触发，仅在 Started/Stopped 时上报）。
    Edge(SteamEvent),
    /// rescan 请求触发的立即状态观察（恒上报当前运行态）。
    State(bool),
}

/// 后台监视线程句柄：壳持有；`rescan()` 请求立即轮询一次（跳过 2s 间隔）。
pub struct SteamMonitorHandle {
    rescan_tx: mpsc::Sender<()>,
}

impl SteamMonitorHandle {
    /// 请求立即轮询：状态变化先报边沿，恒报一次状态观察。
    pub fn rescan(&self) {
        let _ = self.rescan_tx.send(());
    }
}

pub struct SteamMonitor {
    steam: Arc<SteamState>,
    running: bool,
}

impl SteamMonitor {
    /// 启动后台监视线程：2s 轮询 + 边沿检测，仅状态变化上报边沿事件；rescan 请求触发立即轮询并恒报当前运行态。
    /// 返回句柄与启动初值（同一次 `alive()` 读取，线程与壳注入核心的初值同源，不漏启动边沿）。
    /// 线程随进程退出终止；回调在监视线程内调用，须 `Send`。
    pub fn spawn<F>(steam: Arc<SteamState>, on_update: F) -> (SteamMonitorHandle, bool)
    where
        F: Fn(MonitorUpdate) + Send + 'static,
    {
        let (rescan_tx, rescan_rx) = mpsc::channel();
        let monitor = SteamMonitor::new(&steam);
        let initial_running = monitor.is_running();
        std::thread::spawn(move || {
            let mut monitor = monitor;
            loop {
                // 2s 间隔轮询；rescan 请求（工作流后即时刷新）立即醒来跳过间隔。
                let rescan = match rescan_rx.recv_timeout(STEAM_REFRESH_INTERVAL) {
                    Ok(()) => true,
                    Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                if let Some(event) = monitor.force_poll() {
                    on_update(MonitorUpdate::Edge(event));
                }
                if rescan {
                    on_update(MonitorUpdate::State(monitor.is_running()));
                }
            }
        });
        (SteamMonitorHandle { rescan_tx }, initial_running)
    }

    pub fn new(steam: &Arc<SteamState>) -> Self {
        let running = steam.alive();
        Self {
            steam: steam.clone(),
            running,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// 立即扫描一次并做边沿检测（跳过间隔；内部缝，供测试用）。
    fn force_poll(&mut self) -> Option<SteamEvent> {
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
    use std::time::Instant;

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

    #[test]
    fn spawn_idle_poll_emits_nothing_without_state_change() {
        let steam = Arc::new(SteamState::new());
        let (tx, rx) = mpsc::channel();
        let (_handle, _initial) = SteamMonitor::spawn(steam, move |update| {
            let _ = tx.send(update);
        });
        // 空闲（无状态变化、无 rescan）：2s 轮询不产生任何上报（后台线程不唤醒壳）。
        assert!(
            rx.recv_timeout(Duration::from_millis(500)).is_err(),
            "空闲轮询不得上报消息"
        );
    }

    #[test]
    fn spawn_rescan_reports_state_observation() {
        let steam = Arc::new(SteamState::new());
        let (tx, rx) = mpsc::channel();
        let (handle, initial) = SteamMonitor::spawn(steam, move |update| {
            let _ = tx.send(update);
        });
        handle.rescan();
        // rescan 触发立即轮询：超时前必收到 State 观察（期间若 Steam 状态变化，先收边沿）。
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = None;
        let mut edge_implied = None;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(MonitorUpdate::Edge(SteamEvent::Started)) => {
                    assert!(!initial, "Started 边沿只能在初始未运行时出现");
                    edge_implied = Some(true);
                }
                Ok(MonitorUpdate::Edge(SteamEvent::Stopped)) => {
                    assert!(initial, "Stopped 边沿只能在初始运行时出现");
                    edge_implied = Some(false);
                }
                Ok(MonitorUpdate::State(running)) => {
                    state = Some(running);
                    break;
                }
                Err(_) => break,
            }
        }
        let state = state.expect("rescan 应在超时内产出 State 观察");
        if let Some(implied) = edge_implied {
            assert_eq!(state, implied, "State 观察必须是边沿后的定态");
        }
    }
}
