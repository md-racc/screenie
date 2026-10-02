//! Pixels: the frozen frame, crops, rendering and encoding.
//!
//! Depends on `screenie-core` for geometry; holds no input semantics.

pub mod compose;
pub mod encode;
pub mod frame;
pub mod image;
pub mod ui;

pub use frame::Frame;
pub use image::{Image, Rgb};
