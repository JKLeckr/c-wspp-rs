use futures::SinkExt;

use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::runtime::Builder;

use url::Url;
use yawc::close::CloseCode;
use yawc::frame::OpCode;
use yawc::{Frame, MaybeTlsStream, Options, WebSocket, WebSocketError};

use crate::logging;

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

pub fn spawn_ws_worker(
    uri: String,
    compression: bool,
) -> Result<(mpsc::Sender<Command>, mpsc::Receiver<Event>), anyhow::Error> {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    let rt = Builder::new_current_thread().enable_all().build()?;
    let url = Url::parse(uri.as_str())?;

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
                        let reason_bytes = reason.unwrap_or_default().into_bytes();
                        let _ = client
                            .send(Frame::close(CloseCode::from(code), reason_bytes))
                            .await;
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
