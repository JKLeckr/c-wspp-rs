use futures::SinkExt;
use futures::StreamExt;

use std::{
    sync::{
        Arc, Mutex, mpsc,
        mpsc::{Receiver, Sender},
    },
    thread::JoinHandle,
};
use tokio::{net::TcpStream, runtime::Builder, task::yield_now};
use log;
//use tokio::sync::mpsc;

use url::Url;
use yawc::{Frame, MaybeTlsStream, Options, WebSocketError, WebSocket};

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
    compression: bool
) -> Result<(mpsc::Sender<Command>, mpsc::Receiver<Event>), anyhow::Error> {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    let rt = Builder::new_current_thread()
        .enable_all()
        .build()?;

    let url = Url::parse(uri.as_str())?;

    std::thread::spawn(move || {
        rt.block_on(connection_worker(url, compression, event_tx, cmd_rx));
    });

    Ok((cmd_tx, event_rx))
}

async fn connect(
    url: Url,
    compression: bool
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, WebSocketError> {
    let options = if compression {
        Options::default()
            .with_balanced_compression()
    } else {
        Options::default()
            .without_compression()
    };

    let ws = WebSocket::connect(url).with_options(options).await?;

    Ok(ws)
}

async fn connection_worker(
    url: Url,
    compression: bool,
    event_tx: Sender<Event>,
    cmd_rx: Receiver<Command>
) -> Result<(), WebSocketError> {
    log::info!("Connection worker started");
    log::info!("Connecting...");

    let mut client = match connect(url, compression).await {
        Ok(res) => {
            event_tx.send(Event::Open);
            res
        },
        Err(err) => {
            log::error!("Error: {}", err);
            event_tx.send(Event::Error("Connection failure".to_string()));
            yield_now();
            drop(cmd_rx);
            drop(event_tx);
            return Err(err);
        }
    };

    let subscribe_json = r#"{
                "req_id": "1",
                "op": "subscribe",
                "args": [
                    "publicTrade.BTCUSDT"
                ]
            }"#;

    client.send(Frame::text(subscribe_json)).await?;

    Ok(())
}
