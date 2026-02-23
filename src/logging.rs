use std::ffi::CString;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use crate::callback::OnLogCallback;

const LOG_OFF: i32 = 0;
const LOG_TRACE: i32 = 5;

static LOG_LEVEL: AtomicI32 = AtomicI32::new(1);
static LOG_HANDLER: AtomicUsize = AtomicUsize::new(0);

pub fn set_log_handler(handler: Option<OnLogCallback>) {
    let value = handler.map(|f| f as usize).unwrap_or(0);
    LOG_HANDLER.store(value, Ordering::Relaxed);
}

pub fn set_log_level(level: i32) {
    let clamped = level.clamp(LOG_OFF, LOG_TRACE);
    LOG_LEVEL.store(clamped, Ordering::Relaxed);
}

pub fn emit(level: i32, msg: &str) {
    if level <= LOG_OFF {
        return;
    }
    if level > LOG_LEVEL.load(Ordering::Relaxed) {
        return;
    }

    let handler_raw = LOG_HANDLER.load(Ordering::Relaxed);
    if handler_raw == 0 {
        return;
    }

    let c_msg = match CString::new(msg) {
        Ok(s) => s,
        Err(_) => return,
    };

    let handler: OnLogCallback = unsafe { std::mem::transmute(handler_raw) };
    handler(level, c_msg.as_ptr());
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;
    use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    use super::{emit, set_log_handler, set_log_level};

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static LAST_LEVEL: AtomicI32 = AtomicI32::new(-1);

    extern "C" fn test_logger(level: i32, msg: *const i8) {
        let _ = unsafe { CStr::from_ptr(msg) };
        LAST_LEVEL.store(level, Ordering::Relaxed);
        CALLS.fetch_add(1, Ordering::Relaxed);
    }

    fn reset() {
        CALLS.store(0, Ordering::Relaxed);
        LAST_LEVEL.store(-1, Ordering::Relaxed);
        set_log_handler(Some(test_logger));
    }

    #[test]
    fn respects_level_filter() {
        let _guard = TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock poisoned");

        reset();
        set_log_level(1);
        emit(3, "hidden");
        emit(1, "shown");

        assert_eq!(CALLS.load(Ordering::Relaxed), 1);
        assert_eq!(LAST_LEVEL.load(Ordering::Relaxed), 1);
        set_log_handler(None);
    }

    #[test]
    fn clamps_loglevel_values() {
        let _guard = TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock poisoned");

        reset();
        set_log_level(99);
        emit(5, "shown at trace");
        assert_eq!(CALLS.load(Ordering::Relaxed), 1);

        set_log_level(-10);
        emit(1, "hidden at off");
        assert_eq!(CALLS.load(Ordering::Relaxed), 1);
        set_log_handler(None);
    }
}
