use futures::SinkExt;

use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use tokio::net::TcpStream;
use tokio::runtime::Builder;

use url::Url;
use yawc::close::CloseCode;
use yawc::frame::OpCode;
use yawc::{Frame, MaybeTlsStream, Options, WebSocket, WebSocketError};

use crate::logging;
use crate::result::WsppResult;

const CLOSE_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum Event {
    Open,
    Close,
    Message { data: Vec<u8>, opcode: i32 },
    Pong(Vec<u8>),
    Error(String),
}

#[derive(Debug)]
pub enum Command {
    SendText(String),
    SendBinary(Vec<u8>),
    Ping(Vec<u8>),
    Close { code: u16, reason: Option<String> },
    Shutdown,
}

#[derive(Debug)]
pub enum WorkerStartError {
    InvalidUrl(url::ParseError),
    RuntimeInit(std::io::Error),
}

impl WorkerStartError {
    pub fn to_wspp_result(&self) -> WsppResult {
        match self {
            Self::InvalidUrl(_) => WsppResult::InvalidArgument,
            Self::RuntimeInit(_) => WsppResult::IoError,
        }
    }
}

impl std::fmt::Display for WorkerStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl(err) => write!(f, "invalid url: {err}"),
            Self::RuntimeInit(err) => write!(f, "runtime init failed: {err}"),
        }
    }
}

impl std::error::Error for WorkerStartError {}

pub fn spawn_ws_worker(
    uri: String,
    compression: bool,
) -> Result<(mpsc::Sender<Command>, mpsc::Receiver<Event>), WorkerStartError> {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    let rt = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(WorkerStartError::RuntimeInit)?;
    let url = Url::parse(uri.as_str()).map_err(WorkerStartError::InvalidUrl)?;

    std::thread::spawn(move || {
        rt.block_on(connection_worker(url, compression, event_tx, cmd_rx));
    });

    Ok((cmd_tx, event_rx))
}

async fn connect(
    url: Url,
    compression: bool,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, WebSocketError> {
    let options = if compression {
        Options::default().with_balanced_compression()
    } else {
        Options::default().without_compression()
    };

    WebSocket::connect(url).with_options(options).await
}

async fn connection_worker(
    url: Url,
    compression: bool,
    event_tx: Sender<Event>,
    cmd_rx: Receiver<Command>,
) {
    logging::emit(3, "connection worker started");

    let mut client = match connect(url, compression).await {
        Ok(client) => {
            let _ = event_tx.send(Event::Open);
            client
        }
        Err(err) => {
            let _ = event_tx.send(Event::Error(err.to_string()));
            return;
        }
    };

    let mut closing_requested = false;
    let mut close_started_at: Option<Instant> = None;

    loop {
        let mut should_stop = false;
        let mut disconnected = false;

        loop {
            match cmd_rx.try_recv() {
                Ok(cmd) => match cmd {
                    Command::SendText(message) => {
                        if let Err(err) = client.send(Frame::text(message.into_bytes())).await {
                            let _ = event_tx.send(Event::Error(err.to_string()));
                            should_stop = true;
                            break;
                        }
                    }
                    Command::SendBinary(data) => {
                        if let Err(err) = client.send(Frame::binary(data)).await {
                            let _ = event_tx.send(Event::Error(err.to_string()));
                            should_stop = true;
                            break;
                        }
                    }
                    Command::Ping(data) => {
                        if let Err(err) = client.send(Frame::ping(data)).await {
                            let _ = event_tx.send(Event::Error(err.to_string()));
                            should_stop = true;
                            break;
                        }
                    }
                    Command::Close { code, reason } => {
                        closing_requested = true;
                        if close_started_at.is_none() {
                            close_started_at = Some(Instant::now());
                        }
                        let reason_bytes = reason.unwrap_or_default().into_bytes();
                        if let Err(err) = client
                            .send(Frame::close(CloseCode::from(code), reason_bytes))
                            .await
                        {
                            if !err.is_closed() {
                                let _ = event_tx.send(Event::Error(err.to_string()));
                            }
                            let _ = event_tx.send(Event::Close);
                            return;
                        }
                    }
                    Command::Shutdown => {
                        let _ = client
                            .send(Frame::close(CloseCode::Away, b"Going away".as_slice()))
                            .await;
                        let _ = event_tx.send(Event::Close);
                        return;
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if should_stop || disconnected {
            let _ = event_tx.send(Event::Close);
            return;
        }

        if close_timed_out(close_started_at, Instant::now(), CLOSE_WAIT_TIMEOUT) {
            logging::emit(2, "close handshake timed out; forcing closed state");
            let _ = event_tx.send(Event::Close);
            return;
        }

        match tokio::time::timeout(Duration::from_millis(10), client.next_frame()).await {
            Ok(Ok(frame)) => match frame.opcode() {
                OpCode::Text => {
                    let _ = event_tx.send(Event::Message {
                        data: frame.payload().to_vec(),
                        opcode: 1,
                    });
                }
                OpCode::Binary => {
                    let _ = event_tx.send(Event::Message {
                        data: frame.payload().to_vec(),
                        opcode: 2,
                    });
                }
                OpCode::Ping => {
                    let _ = event_tx.send(Event::Message {
                        data: frame.payload().to_vec(),
                        opcode: 9,
                    });
                }
                OpCode::Pong => {
                    let _ = event_tx.send(Event::Pong(frame.payload().to_vec()));
                }
                OpCode::Close => {
                    let _ = event_tx.send(Event::Close);
                    return;
                }
                OpCode::Continuation => {}
            },
            Ok(Err(err)) => {
                if !closing_requested {
                    let _ = event_tx.send(Event::Error(err.to_string()));
                }
                let _ = event_tx.send(Event::Close);
                return;
            }
            Err(_) => {}
        }
    }
}

fn close_timed_out(started_at: Option<Instant>, now: Instant, timeout: Duration) -> bool {
    match started_at {
        Some(started) => now.duration_since(started) >= timeout,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::close_timed_out;
    use super::WorkerStartError;
    use crate::result::WsppResult;

    #[test]
    fn start_error_maps_invalid_url() {
        let err = WorkerStartError::InvalidUrl(url::ParseError::RelativeUrlWithoutBase);
        assert_eq!(err.to_wspp_result(), WsppResult::InvalidArgument);
    }

    #[test]
    fn start_error_maps_runtime_init() {
        let err = WorkerStartError::RuntimeInit(std::io::Error::other("runtime"));
        assert_eq!(err.to_wspp_result(), WsppResult::IoError);
    }

    #[test]
    fn close_timeout_only_after_threshold() {
        let start = Instant::now();
        assert!(!close_timed_out(
            Some(start),
            start + Duration::from_secs(4),
            Duration::from_secs(5)
        ));
        assert!(close_timed_out(
            Some(start),
            start + Duration::from_secs(5),
            Duration::from_secs(5)
        ));
    }

    #[test]
    fn close_timeout_is_false_without_close_request() {
        let now = Instant::now();
        assert!(!close_timed_out(None, now, Duration::from_secs(5)));
    }
}
