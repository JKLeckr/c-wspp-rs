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
        self
    }
}

#[cfg(test)]
mod tests {
    use super::WsppResult;

    #[test]
    fn keeps_core_codes() {
        assert_eq!(WsppResult::Ok.to_ffi() as i32, WsppResult::Ok as i32);
        assert_eq!(
            WsppResult::InvalidState.to_ffi() as i32,
            WsppResult::InvalidState as i32
        );
        assert_eq!(
            WsppResult::InvalidArgument.to_ffi() as i32,
            WsppResult::InvalidArgument as i32
        );
    }

    #[test]
    fn keeps_extended_codes() {
        assert_eq!(
            WsppResult::IoError.to_ffi() as i32,
            WsppResult::IoError as i32
        );
        assert_eq!(
            WsppResult::ProtocolError.to_ffi() as i32,
            WsppResult::ProtocolError as i32
        );
    }
}
