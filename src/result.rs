#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WsppResult {
    Ok = 0,
    InvalidState = 1,
    InvalidArgument = 2,
    IoError = 9,
    ProtocolError = 10,
    Unknown = -1,
}

impl WsppResult {
    pub fn to_ffi(self) -> Self {
        match self {
            Self::Ok | Self::InvalidState | Self::InvalidArgument => self,
            _ => Self::Unknown,
        }
    }
}
