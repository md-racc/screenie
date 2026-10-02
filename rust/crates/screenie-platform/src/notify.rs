//! Desktop notifications, through notify-send.

use std::process::{Command, Stdio};

/// Show a notification. Best effort: a missing notify-send is silent.
pub fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "screenie", title, body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
