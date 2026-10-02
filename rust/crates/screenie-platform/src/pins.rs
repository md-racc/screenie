//! Pins: a floating copy of an image, on every workspace.
//!
//! For now an external viewer (swayimg, with app id `screenie-pin`, which
//! a Hyprland window rule pins), as in the prototype; the picker doesn't
//! know how pins are shown.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use screenie_core::geometry::LogicalRect;

pub const APP_ID: &str = "screenie-pin";

/// Float the PNG at `path` over the spot `rect` it came from.
pub fn pin(path: &Path, rect: LogicalRect) -> Result<(), String> {
    let (x, y) = (rect.x.round() as i64, rect.y.round() as i64);
    let (w, h) = (rect.w.round() as i64, rect.h.round() as i64);
    Command::new("swayimg")
        .args(["-a", APP_ID])
        .arg(format!("-P{x},{y}"))
        .arg(format!("-S{w},{h}"))
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|_| "swayimg is not installed".to_string())
}
