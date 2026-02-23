mod callback;
mod client;
mod result;

use std::ffi::{c_char, c_void};
use std::usize;
use env_logger;

use callback::{
    OnCloseCallback,
    OnErrorCallback,
    OnMessageCallback,
    OnOpenCallback,
    OnPongCallback
};
use result::WsppResult;
use client::WsppWsImpl;

use crate::client::WsState;

// This is only changed when the ABI changes.
static WSPP_ABI_VERSION: u64 = 1;

pub struct WsppWs {
    _private: [u8; 0],
}

#[inline]
unsafe fn ws_mut<'a>(ws: *mut WsppWs) -> Option<&'a mut client::WsppWsImpl> {
    unsafe {
        ws.cast::<client::WsppWsImpl>().as_mut()
    }
}

unsafe fn cstr<'a>(ptr: *const c_char) -> Result<&'a str, WsppResult> {
    if ptr.is_null() {
        return Err(WsppResult::InvalidState);
    }
    unsafe {
        std::ffi::CStr::from_ptr(ptr)
            .to_str()
            .map_err(|_| WsppResult::InvalidState)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_abi_version() -> u64 {
    WSPP_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_log_init() -> bool {
    match env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).try_init() {
        Ok(()) => true,
        Err(_) => false
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_new(uri: *const c_char) -> *mut WsppWs {
    wspp_new_ext(uri, true)
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_new_ext(uri: *const c_char, compression: bool) -> *mut WsppWs {
    let uri_str = (unsafe { std::ffi::CStr::from_ptr(uri) }).to_str().unwrap_or_default();
    Box::into_raw(Box::new(WsppWsImpl::new(uri_str, compression))) as *mut WsppWs
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_delete(ws: *mut WsppWs) {
    if let Some(wsp) = unsafe { ws_mut(ws) } {
        wsp.close(1001, "Getting Removed");
        unsafe {
            drop(Box::from_raw(ws.cast::<WsppWsImpl>()));
        }
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_poll(ws: *mut WsppWs) -> u64 {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return 0;
    };
    ws.poll()
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_run(_ws: *mut WsppWs) -> u64 {
    0 // Stub, not needed in this implementation
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_stopped(ws: *mut WsppWs) -> bool {
    if let Some(wsp) = unsafe { ws_mut(ws) } {
        return matches!(wsp.get_state(), WsState::Closed);
    };
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_connect(ws: *mut WsppWs) -> WsppResult {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };
    ws.connect().unwrap_or_else(|err| err)
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_close(ws: *mut WsppWs, code: u16, reason: *const c_char) -> WsppResult {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    let reason_str = (unsafe { std::ffi::CStr::from_ptr(reason) }).to_str().unwrap_or_else(|_| "Unknown");
    ws.close(code, reason_str).unwrap_or_else(|err| err)
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_send_text(ws: *mut WsppWs, message: *const c_char) -> WsppResult {
    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    if let Ok(message_str) = (unsafe { std::ffi::CStr::from_ptr(message) }).to_str() {
        return ws.send_message(message_str).unwrap_or_else(|err| err);
    };

    WsppResult::InvalidArgument
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_send_binary(ws: *mut WsppWs, data: *const c_void, len: u64) -> WsppResult {
    if len > usize::MAX as u64 {
        return WsppResult::InvalidArgument;
    }

    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    WsppResult::Ok
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_ping(ws: *mut WsppWs, data: *const c_void, len: u64) -> WsppResult {
    if len > usize::MAX as u64 {
        return WsppResult::InvalidArgument;
    }

    let Some(ws) = (unsafe { ws_mut(ws) }) else {
        return WsppResult::InvalidState;
    };

    WsppResult::Ok
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_open_handler(ws: *mut WsppWs, f: OnOpenCallback) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_open = Some(f);
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_close_handler(ws: *mut WsppWs, f: OnCloseCallback) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_close = Some(f);
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_message_handler(ws: *mut WsppWs, f: OnMessageCallback) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_message = Some(f);
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_error_handler(ws: *mut WsppWs, f: OnErrorCallback) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_error = Some(f);
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn wspp_set_pong_handler(ws: *mut WsppWs, f: OnPongCallback) {
    if let Some(ws) = unsafe { ws_mut(ws) } {
        ws.callbacks.on_pong = Some(f);
    };
}
