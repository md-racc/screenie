//! Pixels: the frozen frame, crops, rendering and encoding.
//!
//! Depends on `screenie-core` for geometry; holds no input semantics.

pub mod compose;
pub mod decode;
pub mod encode;
pub mod frame;
pub mod image;
pub mod picture;
pub mod render;
pub mod ui;
pub mod view;

pub use frame::Frame;
pub use image::{Image, Rgb};
pub use picture::Picture;
