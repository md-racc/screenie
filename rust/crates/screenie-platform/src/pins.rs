//! Pins: a floating copy of an image, on every workspace, in screenie's
//! own window (app id `screenie-pin`, which a Hyprland window rule floats
//! and pins). Each pin is a small detached screenie process
//! (`__serve-pin`), with a record in the runtime directory, so `pin list`
//! can find them and `pin close` end them: no daemon.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use screenie_core::geometry::LogicalRect;

pub const APP_ID: &str = "screenie-pin";

/// The internal command a pin runs as.
pub const SERVE_ARG: &str = "__serve-pin";

/// A pin, as its record says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub id: u32,
    pub pid: u32,
    /// The image it shows (its own copy, removed with it).
    pub path: PathBuf,
    /// Where it was put, logical.
    pub at: Option<[f64; 2]>,
    pub size: [u32; 2],
}

fn record(dir: &Path, id: u32) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// Whether `pid` is a pin server still running.
fn alive(pid: u32) -> bool {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .is_ok_and(|c| c.split(|&b| b == 0).any(|a| a == SERVE_ARG.as_bytes()))
}

/// The open pins, by id; records of pins that have gone are removed.
pub fn list(dir: &Path) -> Vec<Pin> {
    let mut pins: Vec<Pin> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let pin: Pin = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
            if alive(pin.pid) {
                Some(pin)
            } else {
                let _ = std::fs::remove_file(e.path());
                None
            }
        })
        .collect();
    pins.sort_by_key(|p| p.id);
    pins
}

/// The smallest id no open pin has.
fn free_id(dir: &Path) -> u32 {
    let taken: Vec<u32> = list(dir).iter().map(|p| p.id).collect();
    (1..).find(|i| !taken.contains(i)).expect("an id")
}

/// Float the PNG at `path` (screenie's own copy, which the pin removes when
/// it closes), `size` device pixels at `scale` a logical pixel, at `at`
/// (logical) if given. Its id.
pub fn pin(
    exe: &Path,
    dir: &Path,
    path: &Path,
    size: [u32; 2],
    scale: f64,
    at: Option<LogicalRect>,
) -> Result<u32, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let id = free_id(dir);
    let at = at.map(|r| [r.x.round(), r.y.round()]);
    let mut cmd = Command::new(exe);
    cmd.arg(SERVE_ARG)
        .arg(id.to_string())
        .arg(path)
        .arg(scale.to_string());
    if let Some([x, y]) = at {
        cmd.arg(format!("{x},{y}"));
    }
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Outlive the parent, apart from its terminal and its directory.
        .process_group(0)
        .current_dir("/")
        .spawn()
        .map_err(|e| format!("starting the pin: {e}"))?;
    let pin = Pin {
        id,
        pid: child.id(),
        path: path.to_path_buf(),
        at,
        size,
    };
    write(dir, &pin)?;
    Ok(id)
}

fn write(dir: &Path, pin: &Pin) -> Result<(), String> {
    let json = serde_json::to_vec(pin).map_err(|e| e.to_string())?;
    let file = record(dir, pin.id);
    std::fs::write(&file, json).map_err(|e| format!("{}: {e}", file.display()))
}

/// End the pins with these ids (all of them for `None`): the ids closed.
pub fn close(dir: &Path, ids: Option<&[u32]>) -> Vec<u32> {
    use rustix::process::{Pid, Signal, kill_process};
    let mut closed = Vec::new();
    for pin in list(dir) {
        if ids.is_some_and(|ids| !ids.contains(&pin.id)) {
            continue;
        }
        let Some(pid) = Pid::from_raw(pin.pid as i32) else {
            continue;
        };
        if kill_process(pid, Signal::TERM).is_ok() {
            closed.push(pin.id);
        }
    }
    closed
}

/// A pin server, closing: its record and its image go.
pub fn forget(dir: &Path, id: u32, image: &Path) {
    let _ = std::fs::remove_file(record(dir, id));
    let _ = std::fs::remove_file(image);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_of_pins_that_have_gone_are_dropped() {
        let dir = std::env::temp_dir().join(format!("screenie-pins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A record whose process isn't a pin server (this test's isn't).
        let gone = Pin {
            id: 1,
            pid: std::process::id(),
            path: dir.join("a.png"),
            at: None,
            size: [10, 10],
        };
        write(&dir, &gone).unwrap();
        assert!(list(&dir).is_empty(), "not a pin server: dropped");
        assert!(!record(&dir, 1).exists());
        assert_eq!(free_id(&dir), 1);
        assert!(close(&dir, None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_record_has_the_position_the_pin_was_moved_to() {
        let dir = std::env::temp_dir().join(format!("screenie-pin-at-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let at = LogicalRect::new(909.5, 3.2, 10.0, 10.0);
        let id = pin(
            Path::new("/bin/true"),
            &dir,
            &dir.join("a.png"),
            [10, 10],
            1.0,
            Some(at),
        )
        .unwrap();
        let rec: Pin = serde_json::from_slice(&std::fs::read(record(&dir, id)).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(rec.at, Some([910.0, 3.0]));
    }
}
