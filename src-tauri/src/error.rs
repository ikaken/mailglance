use serde::{Serialize, Serializer};

/// Error passed to the frontend as a plain message.
/// `status` keeps the HTTP status of Gmail API failures so callers can react to 401/404.
#[derive(Debug)]
pub struct Error {
    pub message: String,
    pub status: Option<u16>,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), status: None }
    }

    pub fn http(status: u16, message: impl Into<String>) -> Self {
        Self { message: message.into(), status: Some(status) }
    }
}

impl<E: std::error::Error> From<E> for Error {
    fn from(e: E) -> Self {
        Self::new(e.to_string())
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.message)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
