#[derive(Debug, Clone, Copy)]
pub enum WsState {
    New = 0,
    Connecting = 1,
    Connected = 2,
    Closing = 3,
    Closed = 4,
    Unknown = -1,
}
