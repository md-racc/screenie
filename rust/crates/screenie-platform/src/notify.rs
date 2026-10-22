//! Desktop notifications, through notify-send.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
/// for that goes without one. The wait is kept to time: the notifier and
/// gdbus run beside it, never holding it up.
pub fn notify_during(title: &str, body: &str, wait: Duration) {
    notify_during_with(&["notify-send"], title, body, wait);
}

/// How long to wait for notify-send to say the notification's id.
const ID_WAIT: Duration = Duration::from_millis(200);

/// `notify_during`, with the notifier's command (a program and its first
/// arguments).
fn notify_during_with(notifier: &[&str], title: &str, body: &str, wait: Duration) {
    let start = Instant::now();
    let Some(shown) = shown_for(wait) else {
        sleep_until(start + wait);
        return;
    };
    let id = notify_closable(notifier, title, body, shown);
    sleep_until(start + shown);
    if let Some(id) = id {
        close(id);
    }
    sleep_until(start + wait);
}

fn sleep_until(deadline: Instant) {
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
}

/// How long the notification for a `wait` stays up, if it gets one.
fn shown_for(wait: Duration) -> Option<Duration> {
    wait.checked_sub(CLEAR).filter(|&shown| shown >= CLEAR)
}

/// A notification that expires after `expire` and stays out of the
/// daemon's history, and its id (to close it, for daemons that ignore the
/// expiry) if the notifier says it within `ID_WAIT`. A notify-send too old
/// for `-p` gets the plain form instead, which just expires.
fn notify_closable(notifier: &[&str], title: &str, body: &str, expire: Duration) -> Option<u32> {
    let (prog, first) = notifier.split_first()?;
    let ms = expire.as_millis().to_string();
    let command = |print_id: bool| {
        let mut c = Command::new(prog);
        c.args(first).args(["-a", "screenie"]);
        if print_id {
            c.arg("-p");
        }
        c.args(["-h", "boolean:transient:true", "-t", &ms, title, body])
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        c
    };
    let child = command(true).stdout(Stdio::piped()).spawn().ok()?;
    let mut plain = command(false);
    plain.stdout(Stdio::null());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok(out) = child.wait_with_output() else {
            let _ = tx.send(None);
            return;
        };
        let id = String::from_utf8_lossy(&out.stdout).trim().parse().ok();
        let _ = tx.send(id);
        if !out.status.success() && out.stdout.is_empty() {
            let _ = plain.status();
        }
    });
    rx.recv_timeout(ID_WAIT).ok().flatten()
}

/// Close notification `id`. gdbus comes with glib, as notify-send does.
/// It runs beside the wait, not holding it up.
fn close(id: u32) {
    let child = Command::new("gdbus")
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
        .spawn();
    if let Ok(mut child) = child {
        std::thread::spawn(move || child.wait());
    }
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

    #[cfg(unix)]
    fn timed(notifier: &[&str]) -> Duration {
        let start = Instant::now();
        notify_during_with(notifier, "t", "b", Duration::from_secs(1));
        start.elapsed()
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_notifier_does_not_stretch_the_wait() {
        let took = timed(&["sh", "-c", "exec sleep 5", "notify-send"]);
        assert!(took >= Duration::from_secs(1), "{took:?}");
        assert!(took < Duration::from_millis(1300), "{took:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_notifier_without_print_id_still_waits_to_time() {
        // Fails with nothing said, as a notify-send without -p does.
        let took = timed(&["sh", "-c", "exit 1", "notify-send"]);
        assert!(took >= Duration::from_secs(1), "{took:?}");
        assert!(took < Duration::from_millis(1300), "{took:?}");
    }
}
