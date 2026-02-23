mod state;
mod worker;

use std::ffi::CString;
use std::sync::mpsc::{Receiver, Sender};

use crate::callback::Callbacks;
use crate::logging;
use crate::result::WsppResult;

use worker::{Command, Event};

pub use state::WsState;

pub struct WsppWsImpl {
    state: WsState,
    uri: String,
    compression: bool,
    event_rx: Option<Receiver<Event>>,
    cmd_tx: Option<Sender<Command>>,
    pub callbacks: Callbacks,
}

impl WsppWsImpl {
    pub fn new(uri: &str, compression: bool) -> Self {
        Self {
            state: WsState::New,
            uri: uri.to_owned(),
            compression,
            event_rx: None,
            cmd_tx: None,
            callbacks: Callbacks::default(),
        }
    }

    pub fn connect(&mut self) -> Result<WsppResult, WsppResult> {
        if matches!(
            self.state,
            WsState::Connecting | WsState::Connected | WsState::Closing
        ) {
            return Err(WsppResult::InvalidState);
        }

        self.cleanup();

        match worker::spawn_ws_worker(self.uri.clone(), self.compression) {
            Ok((cmd_tx, event_rx)) => {
                self.cmd_tx = Some(cmd_tx);
                self.event_rx = Some(event_rx);
                self.state = WsState::Connecting;
                logging::emit(3, "wspp connect queued");
                Ok(WsppResult::Ok)
            }
            Err(err) => {
                self.state = WsState::Closed;
                logging::emit(1, &format!("worker spawn failed: {err}"));
                Err(WsppResult::Unknown)
            }
        }
    }

    pub fn poll(&mut self) -> u64 {
        let Some(event_rx) = self.event_rx.take() else {
            return 0;
        };

        let mut count = 0_u64;
        let mut keep_receiver = true;
        while let Ok(event) = event_rx.try_recv() {
            self.dispatch(event);
            count += 1;
            if matches!(self.state, WsState::Closed) {
                keep_receiver = false;
                break;
            }
        }
        if keep_receiver {
            self.event_rx = Some(event_rx);
        }

        count
    }

    pub fn close(&mut self, code: u16, reason: &str) -> Result<WsppResult, WsppResult> {
        if !matches!(
            self.state,
            WsState::Connecting | WsState::Connected | WsState::Closing
        ) {
            return Err(WsppResult::InvalidState);
        }

        let sender = self.cmd_tx.as_ref().ok_or(WsppResult::InvalidState)?;
        sender
            .send(Command::Close {
                code,
                reason: Some(reason.to_owned()),
            })
            .map_err(|_| WsppResult::Unknown)?;

        self.state = WsState::Closing;
        Ok(WsppResult::Ok)
    }

    pub fn send_message(&mut self, message: &str) -> Result<WsppResult, WsppResult> {
        self.send_command(Command::SendText(message.to_owned()))
    }

    pub fn send_binary(&mut self, data: Vec<u8>) -> Result<WsppResult, WsppResult> {
        self.send_command(Command::SendBinary(data))
    }

    pub fn ping(&mut self, data: Vec<u8>) -> Result<WsppResult, WsppResult> {
        self.send_command(Command::Ping(data))
    }

    pub fn shutdown(&mut self) {
        if let Some(sender) = self.cmd_tx.as_ref() {
            let _ = sender.send(Command::Shutdown);
        }
        self.cleanup();
        self.state = WsState::Closed;
    }

    pub fn get_state(&self) -> WsState {
        self.state
    }

    fn send_command(&mut self, cmd: Command) -> Result<WsppResult, WsppResult> {
        if !matches!(self.state, WsState::Connected) {
            return Err(WsppResult::InvalidState);
        }

        let sender = self.cmd_tx.as_ref().ok_or(WsppResult::InvalidState)?;
        sender.send(cmd).map_err(|_| WsppResult::Unknown)?;
        Ok(WsppResult::Ok)
    }

    fn cleanup(&mut self) {
        self.cmd_tx = None;
        self.event_rx = None;
    }

    fn dispatch(&mut self, event: Event) {
        match event {
            Event::Open => {
                self.state = WsState::Connected;
                if let Some(cb) = self.callbacks.on_open {
                    cb();
                }
            }
            Event::Message { data, opcode } => {
                if let Some(cb) = self.callbacks.on_message {
                    cb(data.as_ptr() as *const i8, data.len() as u64, opcode);
                }
            }
            Event::Pong(data) => {
                if let Some(cb) = self.callbacks.on_pong {
                    cb(data.as_ptr() as *const i8, data.len() as u64);
                }
            }
            Event::Close => {
                self.state = WsState::Closed;
                self.cleanup();
                if let Some(cb) = self.callbacks.on_close {
                    cb();
                }
            }
            Event::Error(msg) => {
                self.state = WsState::Closed;
                self.cleanup();

                if let Some(cb) = self.callbacks.on_error {
                    let c_msg =
                        CString::new(msg).unwrap_or_else(|_| CString::new("Unknown").unwrap());
                    cb(c_msg.as_ptr());
                }
            }
        }
    }
}
