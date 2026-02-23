#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod callback;
mod client;
mod logging;
mod result;

use std::ffi::{CStr, c_char, c_void};
use std::thread;
use std::time::Duration;

use callback::{
    OnCloseCallback, OnErrorCallback, OnLogCallback, OnMessageCallback, OnOpenCallback,
    OnPongCallback,
};
use client::{WsState, WsppWsImpl};
use result::WsppResult;

static WSPP_ABI_VERSION: u64 = 1;

pub struct WsppWs {
    _private: [u8; 0],
}

#[inline]
unsafe fn ws_mut<'a>(ws: *mut WsppWs) -> Option<&'a mut WsppWsImpl> {
    unsafe { ws.cast::<WsppWsImpl>().as_mut() }
}

fn ffi_result(res: Result<WsppResult, WsppResult>) -> WsppResult {
    match res {
        Ok(v) => v.to_ffi(),
        Err(v) => v.to_ffi(),
    }
}

unsafe fn cstr(ptr: *const c_char) -> Result<&'static str, WsppResult> {
    if ptr.is_null() {
        return Err(WsppResult::InvalidArgument);
    }

    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| WsppResult::InvalidArgument)
}

unsafe fn data_slice<'a>(data: *const c_void, len: u64) -> Result<&'a [u8], WsppResult> {
    let len_usize = usize::try_from(len).map_err(|_| WsppResult::InvalidArgument)?;
    if len_usize > 0 && data.is_null() {
        return Err(WsppResult::InvalidArgument);
    }

    Ok(unsafe { std::slice::from_raw_parts(data as *const u8, len_usize) })
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_abi_version() -> u64 {
    WSPP_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_log_handler(callback: Option<OnLogCallback>) {
    logging::set_log_handler(callback);
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_loglevel(level: i32) {
    logging::set_log_level(level);
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_new(uri: *const c_char) -> *mut WsppWs {
    wspp_new_ext(uri, true)
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_new_ext(uri: *const c_char, compression: bool) -> *mut WsppWs {
    if uri.is_null() {
        return std::ptr::null_mut();
    }

    let uri_str = match unsafe { cstr(uri) } {
        Ok(uri) => uri,
        Err(_) => return std::ptr::null_mut(),
    };

    Box::into_raw(Box::new(WsppWsImpl::new(uri_str, compression))) as *mut WsppWs
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_delete(ws: *mut WsppWs) {
    if ws.is_null() {
        return;
    }

    if let Some(inner) = unsafe { ws_mut(ws) } {
        inner.shutdown();
    }

    unsafe {
        drop(Box::from_raw(ws.cast::<WsppWsImpl>()));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_poll(ws: *mut WsppWs) -> u64 {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return 0;
    };

    ws.poll()
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_run(ws: *mut WsppWs) -> u64 {
    let mut handled = 0_u64;

    loop {
        handled = handled.saturating_add(wspp_poll(ws));
        if wspp_stopped(ws) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    handled
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_stopped(ws: *mut WsppWs) -> bool {
    let Some(wsp) = (unsafe { ws_mut(ws) }) else {
        return true;
    };

    matches!(wsp.get_state(), WsState::Closed)
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_connect(ws: *mut WsppWs) -> WsppResult {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    ffi_result(ws.connect())
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_close(ws: *mut WsppWs, code: u16, reason: *const c_char) -> WsppResult {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    let reason_str = if reason.is_null() {
        ""
    } else {
        match unsafe { cstr(reason) } {
            Ok(r) => r,
            Err(e) => return e.to_ffi(),
        }
    };

    ffi_result(ws.close(code, reason_str))
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_send_text(ws: *mut WsppWs, message: *const c_char) -> WsppResult {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    let message_str = match unsafe { cstr(message) } {
        Ok(s) => s,
        Err(e) => return e.to_ffi(),
    };

    ffi_result(ws.send_message(message_str))
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_send_binary(ws: *mut WsppWs, data: *const c_void, len: u64) -> WsppResult {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    let bytes = match unsafe { data_slice(data, len) } {
        Ok(s) => s,
        Err(e) => return e.to_ffi(),
    };

    ffi_result(ws.send_binary(bytes.to_vec()))
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_ping(ws: *mut WsppWs, data: *const c_void, len: u64) -> WsppResult {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    let bytes = match unsafe { data_slice(data, len) } {
        Ok(s) => s,
        Err(e) => return e.to_ffi(),
    };

    ffi_result(ws.ping(bytes.to_vec()))
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_open_handler(ws: *mut WsppWs, f: Option<OnOpenCallback>) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_open = f;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_close_handler(ws: *mut WsppWs, f: Option<OnCloseCallback>) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_close = f;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_message_handler(ws: *mut WsppWs, f: Option<OnMessageCallback>) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_message = f;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_error_handler(ws: *mut WsppWs, f: Option<OnErrorCallback>) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_error = f;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_pong_handler(ws: *mut WsppWs, f: Option<OnPongCallback>) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_pong = f;
    }
}
