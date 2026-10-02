//! screenie's core: the picker's product logic, with no display server,
//! renderer, clipboard or OCR engine.
//!
//! Input flows raw event -> `input` (held keys, repeat, keymap) -> `Intent`
//! -> the top mode -> a transition -> `Session`. Everything here can be
//! driven headlessly, by events or by a `--keys` script.

pub mod anchor_cursor;
pub mod annotate;
pub mod boxes;
pub mod censor;
pub mod filter;
pub mod geometry;
pub mod history;
pub mod input;
pub mod insert;
pub mod intent;
pub mod keymap;
pub mod keys;
pub mod layers;
pub mod layout;
pub mod legend;
pub mod lens;
pub mod lists;
pub mod model;
pub mod ocr;
pub mod recall;
pub mod script;
pub mod select;
pub mod session;
pub mod tags;
pub mod targets;
pub mod text;
