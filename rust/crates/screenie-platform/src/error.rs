//! Platform failures, by class (docs/architecture.md, Error handling).

use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    /// The capability isn't there (no Hyprland, no screencopy).
    Unavailable(String),
    /// The environment answered, but not in a way we understand.
    Protocol(String),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unavailable(s) => write!(f, "unavailable: {s}"),
            Error::Protocol(s) => write!(f, "protocol error: {s}"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
