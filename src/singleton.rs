//! 单实例守卫（Windows）：命名互斥体裁决唯一实例，命名事件唤醒首实例把窗口带回前台。
//!
//! 重复启动的实例在 `acquire` 检测到互斥体已存在时置位唤醒事件并立即退出；
//! 首实例启动一行后台线程 `wait_activate` 阻塞等待该事件，每次置位向 UI 发送
//! `Msg::ActivateRequested`。名字用 `Local\` 会话级作用域：同一登录会话内唯一，
//! 多会话并存互不干扰（便携目录多账号共用时各自独立窗口）。

use std::ptr;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject,
};

const MUTEX_NAME: &str = "Local\\OpenSteamToolManager.Singleton";
const EVENT_NAME: &str = "Local\\OpenSteamToolManager.Activate";

pub struct Singleton {
    _mutex: HANDLE,
    event: HANDLE,
}

// SAFETY: HANDLE 是内核对象引用，跨线程传递不引入内存安全问题（与 single-instance 惯例一致）；
// 守卫在同一时刻仅被单线程访问（等待线程独占），无并发数据竞争。
unsafe impl Send for Singleton {}

/// 尝试成为唯一实例：互斥体已存在 = 已有实例在运行（返回 `None`，调用方应置位唤醒并退出）。
pub fn acquire() -> Option<Singleton> {
    try_acquire_named(MUTEX_NAME, EVENT_NAME)
}

/// 重复启动方置位唤醒事件（仅在 `acquire` 返回 `None` 后调用）。
pub fn signal_activate() {
    signal_named(EVENT_NAME);
}

/// 参数化命名让仓内单测使用独立互斥体/事件名，不与运行中的应用实例冲突。
fn try_acquire_named(mutex_name: &str, event_name: &str) -> Option<Singleton> {
    unsafe {
        let name = wide(mutex_name);
        // SAFETY: null 安全属性；宽字符串以 NUL 结尾；bInitialOwner=false。
        let mutex = CreateMutexW(ptr::null(), 0, name.as_ptr());
        // 句柄为空时是创建失败而非已有实例：按降级处理（无单实例防护但可用性优先，ADR-0017）。
        if !mutex.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            return None;
        }
        let (event, _) = open_or_create_event(event_name);
        Some(Singleton {
            _mutex: mutex,
            event,
        })
    }
}

/// 打开或创建会话级 auto-reset 事件，返回句柄与「是否已存在」（已有实例持有）。
fn open_or_create_event(event_name: &str) -> (HANDLE, bool) {
    let name = wide(event_name);
    unsafe {
        // SAFETY: null 安全属性；宽字符串以 NUL 结尾；auto-reset、初始未置位。
        let event = CreateEventW(ptr::null(), 0, 0, name.as_ptr());
        (
            event,
            !event.is_null() && GetLastError() == ERROR_ALREADY_EXISTS,
        )
    }
}

fn signal_named(event_name: &str) {
    // 启动竞态：首实例建互斥体后、建事件前的窗口期，置位会因事件随后无他句柄而丢失（ADR-0017）；
    // 故循环直到观察到事件已由首实例持有再置位。
    for _ in 0..50 {
        let (event, exists) = open_or_create_event(event_name);
        unsafe {
            // SAFETY: 事件句柄有效（刚创建/打开）；置位或关闭均不涉及其它对象。
            if !exists {
                let _ = CloseHandle(event);
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
            return;
        }
    }
}

impl Singleton {
    /// 阻塞等待一次「重复启动」信号，每次唤醒返回（auto-reset 事件自动复位，供线程循环处理）。
    pub fn wait_activate(&self) {
        if self.event.is_null() {
            // 事件创建失败（系统资源耗尽）：唤醒能力失效；守卫仍需存活以保持多开防护，线程就此挂起。
            std::thread::park();
            return;
        }
        self.wait_timeout(INFINITE);
    }

    /// 限时等待，超时返回 false（仅单测用；生产恒走 `INFINITE`）。
    fn wait_timeout(&self, ms: u32) -> bool {
        unsafe {
            // SAFETY: 事件句柄在线程持守卫期间不会被关闭（守卫随线程移动，drop 才关）。
            WaitForSingleObject(self.event, ms) == WAIT_OBJECT_0
        }
    }
}

impl Drop for Singleton {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.event);
            let _ = CloseHandle(self._mutex);
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_names(tag: &str) -> (String, String) {
        let pid = std::process::id();
        (
            format!("Local\\OstTest.{tag}.{pid}.Mutex"),
            format!("Local\\OstTest.{tag}.{pid}.Event"),
        )
    }

    #[test]
    fn second_acquire_is_denied_until_released() {
        let (m, e) = test_names("acquire");
        let a = try_acquire_named(&m, &e).expect("首实例应成功取得守卫");
        assert!(
            try_acquire_named(&m, &e).is_none(),
            "互斥体已存在时重复启动方应判为已有实例"
        );
        drop(a);
        assert!(
            try_acquire_named(&m, &e).is_some(),
            "首实例释放守卫后应可重新成为唯一实例"
        );
    }

    #[test]
    fn signal_wakes_waiter_then_resets() {
        let (m, e) = test_names("signal");
        let guard = try_acquire_named(&m, &e).expect("首实例应成功取得守卫");
        assert!(!guard.wait_timeout(50), "未置位时限时等待应超时返回 false");
        signal_named(&e);
        assert!(guard.wait_timeout(1000), "置位后等待应被唤醒返回 true");
        assert!(
            !guard.wait_timeout(50),
            "auto-reset 事件唤醒后应自动复位，重复等待不误报"
        );
    }
}
