mod state;
mod worker;

use std::ffi::c_char;

use std::{
    sync::{
        Arc, Mutex, mpsc,
        mpsc::{Receiver, Sender},
    },
    thread::JoinHandle,
};

use crate::WsppResult;
use crate::callback::{
    Callbacks, OnCloseCallback, OnErrorCallback, OnMessageCallback, OnOpenCallback, OnPongCallback,
};

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
        WsppWsImpl {
            state: WsState::New,
            uri: uri.to_owned(),
            compression,
            event_rx: None,
            cmd_tx: None,
            callbacks: Callbacks::default(),
        }
    }
    pub fn connect(&mut self) -> Result<WsppResult, WsppResult> {
        if matches!(self.state, WsState::Connected | WsState::Connecting) {
            return Err(WsppResult::InvalidState);
        }
        if matches!(self.state, WsState::Unknown) {
            self.cleanup();
        }
        self.state = WsState::Connecting;

        match worker::spawn_ws_worker(
            self.uri.clone(),
            self.compression.clone()
        ) {
            Ok((cmd_tx, event_rx)) => {
                self.cmd_tx = Some(cmd_tx);
                self.event_rx = Some(event_rx);
                self.state = WsState::Connecting;

                Ok(WsppResult::Ok)
            },
            Err(_) => { Err(WsppResult::IoError) }
        }
    }
    pub fn poll(&mut self) -> u64 {
        let mut count = 0;

        if let Some(event_rx) = self.event_rx.take() {
            while let Ok(event) = event_rx.try_recv() {
                self.dispatch(event);
                count += 1;
            }
            self.event_rx = Some(event_rx);
        };

        count
    }
    pub fn close(&mut self, code: u16, reason: &str) -> Result<WsppResult, WsppResult> {
        Ok(WsppResult::Ok)
    }

    pub fn send_message(&mut self, message: &str) -> Result<WsppResult, WsppResult> {
        if let Some(sender) = self.cmd_tx.take() {

            self.cmd_tx = Some(sender);
        }
        Err(WsppResult::InvalidState)
    }

    pub fn get_state(&self) -> WsState {
        self.state
    }

    fn cleanup(&mut self) {
        if let Some(sender) = self.cmd_tx.take() {
            drop(sender);
        }
        if let Some(reciever) = self.event_rx.take() {
            drop(reciever);
        }
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
                    cb(data, data.len() as u64, opcode);
                }
            }
            Event::Pong(data) => {
                if let Some(cb) = self.callbacks.on_pong {
                    cb(data.as_ptr() as *const c_char, data.len() as u64);
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
                if let Some(cb) = self.callbacks.on_error {
                    // store CString if needed
                    cb(msg.as_ptr() as *const c_char);
                }
            }
        }
    }
}
