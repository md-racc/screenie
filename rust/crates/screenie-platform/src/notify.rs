//! Desktop notifications, through notify-send.

use std::process::{Command, Stdio};
use std::time::Duration;

/// Show a notification. Best effort: a missing notify-send is silent.
pub fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "screenie", title, body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// How long a notification is closed before what it announces, so the
/// daemon's fade-out is over too.
const CLEAR: Duration = Duration::from_millis(500);

/// Wait out `wait` with a notification saying so, closed `CLEAR` before
/// the end so a capture right after doesn't include it. A wait too short
/// for that goes without one.
pub fn notify_during(title: &str, body: &str, wait: Duration) {
    let Some(shown) = shown_for(wait) else {
        std::thread::sleep(wait);
        return;
    };
    let id = notify_closable(title, body, shown);
    std::thread::sleep(shown);
    if let Some(id) = id {
        close(id);
    }
    std::thread::sleep(wait - shown);
}

/// How long the notification for a `wait` stays up, if it gets one.
fn shown_for(wait: Duration) -> Option<Duration> {
    wait.checked_sub(CLEAR).filter(|&shown| shown >= CLEAR)
}

/// A notification that expires after `expire` and stays out of the
/// daemon's history, and its id (to close it, for daemons that ignore the
/// expiry).
fn notify_closable(title: &str, body: &str, expire: Duration) -> Option<u32> {
    let out = Command::new("notify-send")
        .args(["-a", "screenie", "-p", "-h", "boolean:transient:true", "-t"])
        .arg(expire.as_millis().to_string())
        .args([title, body])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Close notification `id`. gdbus comes with glib, as notify-send does.
fn close(id: u32) {
    let _ = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.Notifications",
            "--object-path",
            "/org/freedesktop/Notifications",
            "--method",
            "org.freedesktop.Notifications.CloseNotification",
        ])
        .arg(id.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notification_closes_half_a_second_before_the_wait_ends() {
        let s = |secs: f64| shown_for(Duration::from_secs_f64(secs));
        assert_eq!(s(0.5), None);
        assert_eq!(s(0.9), None);
        assert_eq!(s(1.0), Some(Duration::from_millis(500)));
        assert_eq!(s(3.0), Some(Duration::from_millis(2500)));
        assert_eq!(s(3600.0), Some(Duration::from_millis(3_599_500)));
    }
}
