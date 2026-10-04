//! 交互类后台操作的互斥门禁：同时刻仅一个操作可进入。

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BusyKind {
    Deploying,
    Uninstalling,
    Launching,
    Checking,
    Downloading,
    ClosingSteam,
}

impl BusyKind {
    /// 更新流程专属忙碌（检查 / 下载）：进度只落在「补丁更新检查」按钮位原位
    /// （§26.2 / §26.5 / §34），不驱动 Hero 阶段化、不进状态栏。交互类忙碌才走 Hero 原位。
    pub fn is_update_flow(self) -> bool {
        matches!(self, BusyKind::Checking | BusyKind::Downloading)
    }
}

/// 交互类后台操作的互斥门禁：持有唯一的 `Option<BusyKind>`——类型即不变量，不存在「忙碌但无种类」的失效态。
pub struct BusyGate {
    current: Option<BusyKind>,
}

impl BusyGate {
    pub fn new() -> Self {
        Self { current: None }
    }

    pub fn start(&mut self, kind: BusyKind) -> bool {
        if self.current.is_some() {
            return false;
        }
        self.current = Some(kind);
        true
    }

    pub fn current(&self) -> Option<BusyKind> {
        self.current
    }

    pub fn is_busy(&self) -> bool {
        self.current.is_some()
    }

    /// 归位空闲（后台完成消息的统一出口；空闲时幂等）。
    pub fn clear(&mut self) {
        self.current = None;
    }

    /// 阶段更新：忙碌中替换种类；空闲时调用是逻辑错误（debug 构建断言拦截）。
    pub fn replace(&mut self, kind: BusyKind) {
        debug_assert!(self.is_busy(), "replace on idle gate");
        self.current = Some(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_while_idle_admits_and_records_kind() {
        let mut gate = BusyGate::new();
        assert!(gate.start(BusyKind::Deploying));
        assert!(gate.is_busy());
        assert_eq!(gate.current(), Some(BusyKind::Deploying));
    }

    #[test]
    fn start_while_busy_rejects_and_keeps_current() {
        let mut gate = BusyGate::new();
        assert!(gate.start(BusyKind::ClosingSteam));
        assert!(!gate.start(BusyKind::Checking));
        assert_eq!(gate.current(), Some(BusyKind::ClosingSteam));
        assert!(gate.is_busy());
    }

    #[test]
    fn clear_returns_to_idle() {
        let mut gate = BusyGate::new();
        assert!(gate.start(BusyKind::Launching));
        gate.clear();
        assert!(!gate.is_busy());
        assert_eq!(gate.current(), None);
    }

    #[test]
    fn clear_is_idempotent_when_idle() {
        let mut gate = BusyGate::new();
        gate.clear();
        assert!(!gate.is_busy());
        assert_eq!(gate.current(), None);
    }

    #[test]
    fn replace_switches_phase_while_busy() {
        let mut gate = BusyGate::new();
        assert!(gate.start(BusyKind::ClosingSteam));
        gate.replace(BusyKind::Deploying);
        assert_eq!(gate.current(), Some(BusyKind::Deploying));
        assert!(gate.is_busy());
    }

    #[test]
    #[should_panic]
    fn replace_on_idle_gate_panics_in_debug() {
        let mut gate = BusyGate::new();
        gate.replace(BusyKind::Launching);
    }

    #[test]
    fn every_busy_kind_enters_reports_and_clears() {
        for kind in [
            BusyKind::Deploying,
            BusyKind::Uninstalling,
            BusyKind::Launching,
            BusyKind::Checking,
            BusyKind::Downloading,
            BusyKind::ClosingSteam,
        ] {
            let mut gate = BusyGate::new();
            assert!(gate.start(kind), "{kind:?} 应可进入");
            assert_eq!(gate.current(), Some(kind));
            gate.clear();
            assert!(!gate.is_busy());
        }
    }
}
