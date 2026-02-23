use std::ffi::c_char;

pub type OnOpenCallback = extern "C" fn();
pub type OnCloseCallback = extern "C" fn();
pub type OnMessageCallback = extern "C" fn(data: *const c_char, len: u64, op_code: i32);
pub type OnErrorCallback = extern "C" fn(msg: *const c_char);
pub type OnPongCallback = extern "C" fn(data: *const c_char, len: u64);
pub type OnLogCallback = extern "C" fn(level: i32, msg: *const c_char);

#[derive(Default)]
pub struct Callbacks {
    pub on_open: Option<OnOpenCallback>,
    pub on_close: Option<OnCloseCallback>,
    pub on_message: Option<OnMessageCallback>,
    pub on_error: Option<OnErrorCallback>,
    pub on_pong: Option<OnPongCallback>,
}
