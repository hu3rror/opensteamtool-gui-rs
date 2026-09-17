//! 「忙碌门禁」模块：交互类后台操作互斥的单一入口。
//!
//! 深模块——小接口（`start` / `replace` / `clear` / `current` / `is_busy`），
//! 大实现（防重入、阶段更新、忙碌态类型级不变量）。纯 std，无 IO、无线程、
//! 无 egui、无 i18n 依赖。compat 探针/刷新/预热是只读后台操作，不进本门禁
//! （互斥由 compat_flow 在途去重承担），见 CONTEXT.md「忙碌门禁」。

/// 后台操作期间显示的忙碌/阶段文案类型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BusyKind {
    Deploying,
    Uninstalling,
    Launching,
    Checking,
    Downloading,
    ClosingSteam,
}

/// 交互类后台操作的互斥门禁：持有唯一的 `Option<BusyKind>`——类型即不变量，
/// 不存在「忙碌但无种类」的失效态。同时刻仅一个操作可 `start` 进入；阶段
/// 更新走 `replace`，完成走 `clear`。
pub struct BusyGate {
    current: Option<BusyKind>,
}

impl BusyGate {
    /// 空闲门禁。
    pub fn new() -> Self {
        Self { current: None }
    }

    /// 尝试进入忙碌态：空闲时放行并记录种类（返回 true）；
    /// 忙碌时拒绝（返回 false，静默——按钮本就禁用，此路径理论不可达）。
    pub fn start(&mut self, kind: BusyKind) -> bool {
        if self.current.is_some() {
            return false;
        }
        self.current = Some(kind);
        true
    }

    /// 当前忙碌种类（空闲为 None）。
    pub fn current(&self) -> Option<BusyKind> {
        self.current
    }

    /// 是否有交互类后台操作在途。
    pub fn is_busy(&self) -> bool {
        self.current.is_some()
    }

    /// 归位空闲（后台完成消息的统一出口；空闲时幂等）。
    pub fn clear(&mut self) {
        self.current = None;
    }

    /// 阶段更新（后台 `Msg::Phase` 消息）：忙碌中替换种类。空闲时调用是逻辑
    /// 错误（阶段消息只在后台执行期间到达），debug 构建断言拦截。
    pub fn replace(&mut self, kind: BusyKind) {
        debug_assert!(self.is_busy(), "replace on idle gate");
        self.current = Some(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空闲时 start 放行：返回 true，current 记录该种类、is_busy 置位。
    #[test]
    fn start_while_idle_admits_and_records_kind() {
        let mut gate = BusyGate::new();
        assert!(gate.start(BusyKind::Deploying));
        assert!(gate.is_busy());
        assert_eq!(gate.current(), Some(BusyKind::Deploying));
    }

    /// 忙碌时 start 拒绝：返回 false，current 保持原值（防重入）。
    #[test]
    fn start_while_busy_rejects_and_keeps_current() {
        let mut gate = BusyGate::new();
        assert!(gate.start(BusyKind::ClosingSteam));
        assert!(!gate.start(BusyKind::Checking));
        assert_eq!(gate.current(), Some(BusyKind::ClosingSteam));
        assert!(gate.is_busy());
    }

    /// clear 归位：is_busy 变 false、current 变 None（后台完成消息的统一出口）。
    #[test]
    fn clear_returns_to_idle() {
        let mut gate = BusyGate::new();
        assert!(gate.start(BusyKind::Launching));
        gate.clear();
        assert!(!gate.is_busy());
        assert_eq!(gate.current(), None);
    }

    /// 空闲时 clear 幂等：无副作用。
    #[test]
    fn clear_is_idempotent_when_idle() {
        let mut gate = BusyGate::new();
        gate.clear();
        assert!(!gate.is_busy());
        assert_eq!(gate.current(), None);
    }

    /// replace 阶段更新：忙碌中替换 current（Phase 消息语义，点击即见阶段文案）。
    #[test]
    fn replace_switches_phase_while_busy() {
        let mut gate = BusyGate::new();
        assert!(gate.start(BusyKind::ClosingSteam));
        gate.replace(BusyKind::Deploying);
        assert_eq!(gate.current(), Some(BusyKind::Deploying));
        assert!(gate.is_busy());
    }

    /// 空闲时 replace 是逻辑错误（Phase 消息只在后台执行期间到达）：debug 构建 panic。
    #[test]
    #[should_panic]
    fn replace_on_idle_gate_panics_in_debug() {
        let mut gate = BusyGate::new();
        gate.replace(BusyKind::Launching);
    }

    /// 全变体完整性：每个 BusyKind 均可经 start 进入、如实 current 返回、clear 归位。
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
