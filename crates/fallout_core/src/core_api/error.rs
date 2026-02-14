use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreErrorCode {
    Io,
    Parse,
    GameDetectionAmbiguous,
    UnsupportedOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError {
    pub code: CoreErrorCode,
    pub message: String,
}

impl CoreError {
    pub fn new(code: CoreErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for CoreError {}
