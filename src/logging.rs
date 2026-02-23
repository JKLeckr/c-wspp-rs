use std::ffi::CString;
use std::sync::RwLock;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::callback::OnLogCallback;

const LOG_OFF: i32 = 0;
const LOG_TRACE: i32 = 5;

static LOG_LEVEL: AtomicI32 = AtomicI32::new(1);
static LOG_HANDLER: RwLock<Option<OnLogCallback>> = RwLock::new(None);

pub fn set_log_handler(handler: Option<OnLogCallback>) {
    if let Ok(mut slot) = LOG_HANDLER.write() {
        *slot = handler;
    }
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

    let handler = match LOG_HANDLER.read() {
        Ok(slot) => *slot,
        Err(_) => None,
    };
    let Some(handler) = handler else {
        return;
    };

    let c_msg = match CString::new(msg) {
        Ok(s) => s,
        Err(_) => return,
    };

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

    #[test]
    fn handler_updates_are_thread_safe() {
        let _guard = TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock poisoned");

        reset();
        set_log_level(5);

        let toggler = std::thread::spawn(|| {
            for _ in 0..500 {
                set_log_handler(Some(test_logger));
                set_log_handler(None);
            }
        });
        let emitter = std::thread::spawn(|| {
            for _ in 0..1000 {
                emit(1, "race");
            }
        });

        toggler.join().expect("toggler panicked");
        emitter.join().expect("emitter panicked");

        set_log_handler(None);
    }
}
