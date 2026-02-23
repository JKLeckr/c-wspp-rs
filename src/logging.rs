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
