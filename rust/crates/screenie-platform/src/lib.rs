//! Desktop integration: capture, desktop layout, overlays, clipboard.
//!
//! Each service is narrow and separately optional. Nothing here interprets
//! picker input: it only supplies the environment core asks about.

pub mod clipboard;
pub mod codes;
pub mod desktop;
mod error;
pub mod fonts;
pub mod notify;
pub mod ocr;
pub mod pins;
pub mod wayland;

pub use error::{Error, Result};
