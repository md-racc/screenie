//! Hyprland: monitors, clients and the active window over its IPC socket.
//!
//! Hyprland reports monitor sizes in device pixels and positions in
//! logical ones; core wants everything logical. Windows are filtered to
//! the ones you can actually see, topmost first.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::{Layout, Monitor, MonitorId, Window, WindowId};

use super::DesktopInfo;
use crate::{Error, Result};

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Workspace {
    pub id: i64,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyprMonitor {
    pub id: i64,
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// Device pixels.
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub transform: i64,
    pub focused: bool,
    pub active_workspace: Workspace,
    #[serde(default)]
    pub special_workspace: Workspace,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyprClient {
    pub address: String,
    pub mapped: bool,
    pub hidden: bool,
    pub at: [f64; 2],
    pub size: [f64; 2],
    pub workspace: Workspace,
    pub floating: bool,
    #[serde(default)]
    pub monitor: i64,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub title: String,
    /// 0 none, 1 maximised, 2 fullscreen.
    pub fullscreen: i64,
    #[serde(rename = "focusHistoryID")]
    pub focus_history_id: i64,
    #[serde(default)]
    pub stable_id: String,
    #[serde(default)]
    pub pid: i64,
    /// Shown on every workspace, above everything else.
    #[serde(default)]
    pub pinned: bool,
}

impl HyprMonitor {
    /// The workspace showing: a special one if it's open, else the active.
    fn shown_workspace(&self) -> i64 {
        if self.special_workspace.name.is_empty() {
            self.active_workspace.id
        } else {
            self.special_workspace.id
        }
    }

    fn logical(&self) -> LogicalRect {
        let (mut w, mut h) = (self.width / self.scale, self.height / self.scale);
        if self.transform % 2 == 1 {
            std::mem::swap(&mut w, &mut h); // rotated 90 or 270
        }
        LogicalRect::new(self.x, self.y, w, h)
    }
}

/// The window id for a client: its address, which is a hex pointer.
pub fn window_id(address: &str) -> WindowId {
    WindowId(u64::from_str_radix(address.trim_start_matches("0x"), 16).unwrap_or(0))
}

/// Focus history counts up from the latest (0); -1 is never focused, and
/// goes last.
fn focus_order(c: &HyprClient) -> i64 {
    if c.focus_history_id < 0 {
        i64::MAX
    } else {
        c.focus_history_id
    }
}

/// Where a client is drawn: pinned windows above floating ones, floating
/// above tiled.
fn stacking_layer(c: &HyprClient) -> u8 {
    if c.floating && c.pinned {
        0
    } else if c.floating || c.fullscreen >= 2 {
        1
    } else {
        2
    }
}

/// The clients you can actually see on the workspaces shown, topmost
/// first: pinned windows, then floating, then tiled, each layer in focus
/// order. A fullscreen window hides everything else on its workspace but
/// pinned floating windows; a maximised one hides the other tiled
/// windows, but floating ones stay above it.
pub fn visible_clients(clients: &[HyprClient], shown: &HashSet<i64>) -> Vec<HyprClient> {
    let on: Vec<&HyprClient> = clients
        .iter()
        .filter(|c| c.mapped && !c.hidden && shown.contains(&c.workspace.id))
        .collect();
    let mut visible: Vec<HyprClient> = on
        .iter()
        .filter(|c| {
            if c.fullscreen != 0 {
                return true;
            }
            let others = || {
                on.iter()
                    .filter(|o| !std::ptr::eq(**o, **c) && o.workspace.id == c.workspace.id)
            };
            let behind_fullscreen =
                !(c.floating && c.pinned) && others().any(|o| o.fullscreen >= 2);
            let behind_maximised = !c.floating && others().any(|o| o.fullscreen == 1);
            !behind_fullscreen && !behind_maximised
        })
        .map(|c| (*c).clone())
        .collect();
    visible.sort_by_key(|c| (stacking_layer(c), focus_order(c)));
    visible
}

/// Core's layout from Hyprland's answers. `active` is `activewindow`'s
/// answer, `None` when nothing is focused.
pub fn layout_from(
    monitors: &[HyprMonitor],
    clients: &[HyprClient],
    active: Option<&HyprClient>,
) -> Result<Layout> {
    let focused = monitors
        .iter()
        .find(|m| m.focused)
        .or(monitors.first())
        .ok_or_else(|| Error::Unavailable("Hyprland reports no monitors".into()))?;
    let shown: HashSet<i64> = monitors.iter().map(HyprMonitor::shown_workspace).collect();
    let windows = visible_clients(clients, &shown)
        .into_iter()
        .map(|c| Window {
            id: window_id(&c.address),
            rect: LogicalRect::new(c.at[0], c.at[1], c.size[0], c.size[1]),
            monitor: u32::try_from(c.monitor).ok().map(MonitorId),
            class: c.class,
            title: c.title,
        })
        .collect();
    let mons = monitors
        .iter()
        .map(|m| Monitor {
            id: MonitorId(m.id as u32),
            name: m.name.clone(),
            rect: m.logical(),
            scale: m.scale,
        })
        .collect();
    let mut layout = Layout::new(mons, windows, MonitorId(focused.id as u32));
    layout.active = active
        .filter(|a| !a.address.is_empty() && shown.contains(&a.workspace.id))
        .map(|a| window_id(&a.address));
    Ok(layout)
}

/// Hyprland's request socket.
fn socket() -> Result<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| Error::Unavailable("not running under Hyprland".into()))?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let path = PathBuf::from(runtime)
        .join("hypr")
        .join(&sig)
        .join(".socket.sock");
    if path.exists() {
        Ok(path)
    } else {
        Ok(PathBuf::from("/tmp/hypr").join(sig).join(".socket.sock"))
    }
}

/// How long Hyprland gets to take a request or answer it.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);

/// One request (`j/monitors`) and its whole answer.
pub fn request(cmd: &str) -> Result<String> {
    request_at(&socket()?, cmd, ANSWER_TIMEOUT)
}

/// A request to the socket at `path`. A Hyprland that stops answering is
/// an error after `timeout`, not a hang.
fn request_at(path: &Path, cmd: &str, timeout: Duration) -> Result<String> {
    let mut s = UnixStream::connect(path)
        .map_err(|e| Error::Unavailable(format!("Hyprland's socket {}: {e}", path.display())))?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    let failed = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
            Error::Unavailable(format!("Hyprland didn't answer {cmd}"))
        }
        _ => Error::Io(e),
    };
    s.write_all(cmd.as_bytes()).map_err(failed)?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(failed)?;
    Ok(out)
}

/// Move process `pid`'s window to `(x, y)`, global logical pixels. Hyprland
/// 0.56 dispatches Lua; older ones take the dispatcher line.
pub fn move_window(pid: u32, x: i32, y: i32) -> Result<()> {
    let lua =
        format!("dispatch hl.dsp.window.move({{ x = {x}, y = {y}, window = \"pid:{pid}\" }})");
    if request(&lua)?.trim() == "ok" {
        return Ok(());
    }
    let old = request(&format!("dispatch movewindowpixel exact {x} {y},pid:{pid}"))?;
    match old.trim() {
        "ok" => Ok(()),
        e => Err(Error::Protocol(format!("moving a window: {e}"))),
    }
}

fn query<T: for<'de> Deserialize<'de>>(cmd: &str) -> Result<T> {
    let text = request(cmd)?;
    serde_json::from_str(&text).map_err(|e| Error::Protocol(format!("{cmd}: {e}")))
}

/// A window by `window:`'s spec (without the prefix): `class:REGEX`,
/// `title:REGEX`, `address:0x…` or `pid:N`. Of several matches, the most
/// recently focused; none matching is `Ok(None)`, a bad spec an error.
pub fn find_window<'a>(
    clients: &'a [HyprClient],
    spec: &str,
) -> std::result::Result<Option<&'a HyprClient>, String> {
    let (key, value) = spec
        .split_once(':')
        .ok_or_else(|| format!("window:{spec}: class:REGEX, title:REGEX, address:0x… or pid:N"))?;
    let pattern = |v: &str| regex::Regex::new(v).map_err(|e| format!("window:{key}: {e}"));
    let matches: Box<dyn Fn(&HyprClient) -> bool> = match key {
        "class" => {
            let re = pattern(value)?;
            Box::new(move |c| re.is_match(&c.class))
        }
        "title" => {
            let re = pattern(value)?;
            Box::new(move |c| re.is_match(&c.title))
        }
        "address" => {
            let a = value.to_lowercase();
            Box::new(move |c| c.address.to_lowercase() == a)
        }
        "pid" => {
            let pid: i64 = value
                .parse()
                .map_err(|_| format!("window:pid:{value}: not a number"))?;
            Box::new(move |c| c.pid == pid)
        }
        _ => return Err(format!("window:{key}: class, title, address or pid")),
    };
    Ok(clients
        .iter()
        .filter(|c| c.mapped && matches(c))
        .min_by_key(|c| focus_order(c)))
}

/// The live Hyprland session.
pub struct Hyprland;

impl Hyprland {
    /// Every client window.
    pub fn clients(&self) -> Result<Vec<HyprClient>> {
        query("j/clients")
    }

    pub fn active_client(&self) -> Result<Option<HyprClient>> {
        let text = request("j/activewindow")?;
        // With nothing focused, Hyprland answers `{}`.
        Ok(serde_json::from_str(&text).ok())
    }
}

impl DesktopInfo for Hyprland {
    fn layout(&self) -> Result<Layout> {
        let monitors: Vec<HyprMonitor> = query("j/monitors")?;
        let clients: Vec<HyprClient> = query("j/clients")?;
        let active = self.active_client()?;
        layout_from(&monitors, &clients, active.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(name: &str, ws: i64, fullscreen: i64, floating: bool, focus: i64) -> HyprClient {
        HyprClient {
            address: format!("0x{}", focus + 10),
            mapped: true,
            hidden: false,
            at: [0.0, 0.0],
            size: [10.0, 10.0],
            workspace: Workspace {
                id: ws,
                name: ws.to_string(),
            },
            floating,
            monitor: 0,
            class: name.into(),
            title: String::new(),
            fullscreen,
            focus_history_id: focus,
            stable_id: String::new(),
            pid: 100 + focus,
            pinned: false,
        }
    }

    #[test]
    fn a_window_by_class_title_address_or_pid_the_latest_focused_first() {
        let mut a = client("firefox", 1, 0, false, 2);
        a.title = "Docs — Mozilla Firefox".into();
        let b = client("firefox", 1, 0, false, 0); // focused last
        let c = client("kitty", 2, 0, false, 1);
        let all = vec![a.clone(), b.clone(), c.clone()];
        let find = |spec: &str| find_window(&all, spec).map(|w| w.map(|w| w.address.clone()));
        assert_eq!(
            find("class:fire").unwrap(),
            Some(b.address.clone()),
            "most recent"
        );
        assert_eq!(find("class:^kitty$").unwrap(), Some(c.address.clone()));
        assert_eq!(find("title:Docs").unwrap(), Some(a.address.clone()));
        assert_eq!(
            find(&format!("address:{}", c.address)).unwrap(),
            Some(c.address.clone())
        );
        assert_eq!(find("pid:102").unwrap(), Some(a.address.clone()));
        assert_eq!(find("class:slack").unwrap(), None);
        assert!(find("class:(").is_err(), "a bad pattern");
        assert!(find("colour:red").is_err());
        assert!(find("pid:x").is_err());
    }

    fn names(cs: Vec<HyprClient>) -> Vec<String> {
        cs.into_iter().map(|c| c.class).collect()
    }

    fn ws(ids: &[i64]) -> HashSet<i64> {
        ids.iter().copied().collect()
    }

    #[test]
    fn fullscreen_hides_the_rest_of_its_workspace_only() {
        let cs = [
            client("tiled", 1, 0, false, 1),
            client("full", 1, 2, false, 0),
            client("float", 1, 0, true, 2),
            client("other", 2, 0, false, 3),
        ];
        assert_eq!(names(visible_clients(&cs, &ws(&[1, 2]))), ["full", "other"]);
    }

    #[test]
    fn maximised_hides_tiled_but_not_floating() {
        let cs = [
            client("tiled", 1, 0, false, 1),
            client("max", 1, 1, false, 0),
            client("float", 1, 0, true, 2),
        ];
        assert_eq!(names(visible_clients(&cs, &ws(&[1]))), ["float", "max"]);
    }

    #[test]
    fn a_float_stays_above_a_more_recently_focused_tiled_window() {
        let cs = [
            client("browser", 1, 0, false, 0),
            client("float", 1, 0, true, 1),
        ];
        assert_eq!(names(visible_clients(&cs, &ws(&[1]))), ["float", "browser"]);
    }

    #[test]
    fn a_never_focused_window_goes_last_in_its_layer() {
        let cs = [
            client("a", 1, 0, true, -1),
            client("b", 1, 0, true, 0),
            client("tiled", 1, 0, false, 1),
        ];
        assert_eq!(names(visible_clients(&cs, &ws(&[1]))), ["b", "a", "tiled"]);
    }

    #[test]
    fn a_pinned_float_stays_above_fullscreen() {
        let mut pin = client("pin", 1, 0, true, 3);
        pin.pinned = true;
        let cs = [
            client("full", 1, 2, false, 0),
            client("float", 1, 0, true, 1),
            pin,
            client("tiled", 1, 0, false, 2),
        ];
        assert_eq!(names(visible_clients(&cs, &ws(&[1]))), ["pin", "full"]);
    }

    #[test]
    fn plain_workspace_keeps_everything_topmost_first() {
        let cs = [
            client("b", 1, 0, false, 1),
            client("a", 1, 0, false, 0),
            client("off", 9, 0, false, 2),
        ];
        assert_eq!(names(visible_clients(&cs, &ws(&[1]))), ["a", "b"]);
    }

    #[test]
    fn unmapped_and_hidden_are_dropped() {
        let mut gone = client("gone", 1, 0, false, 0);
        gone.mapped = false;
        let mut tabbed = client("tabbed", 1, 0, false, 1);
        tabbed.hidden = true;
        let cs = [gone, tabbed, client("ok", 1, 0, false, 2)];
        assert_eq!(names(visible_clients(&cs, &ws(&[1]))), ["ok"]);
    }

    const MONITORS: &str = r#"[
      {"id": 0, "name": "HDMI-A-1", "x": 0, "y": 0, "width": 1920, "height": 1080, "scale": 1,
       "transform": 0, "focused": false, "activeWorkspace": {"id": 1, "name": "1"},
       "specialWorkspace": {"id": 0, "name": ""}},
      {"id": 1, "name": "DP-3", "x": 1920, "y": 0, "width": 3840, "height": 2160, "scale": 2,
       "transform": 1, "focused": true, "activeWorkspace": {"id": 2, "name": "2"},
       "specialWorkspace": {"id": -98, "name": "special:term"}}
    ]"#;

    #[test]
    fn monitors_are_logical_and_rotated() {
        let ms: Vec<HyprMonitor> = serde_json::from_str(MONITORS).unwrap();
        let l = layout_from(&ms, &[], None).unwrap();
        assert_eq!(
            l.monitors[1].rect,
            LogicalRect::new(1920.0, 0.0, 1080.0, 1920.0)
        );
        assert_eq!(l.monitors[1].scale, 2.0);
        assert_eq!(l.focused, MonitorId(1));
        assert_eq!(l.bounds, LogicalRect::new(0.0, 0.0, 3000.0, 1920.0));
    }

    #[test]
    fn a_special_workspace_replaces_the_active_one() {
        let ms: Vec<HyprMonitor> = serde_json::from_str(MONITORS).unwrap();
        let cs = [
            client("under", 2, 0, false, 0),
            client("term", -98, 0, false, 1),
        ];
        let l = layout_from(&ms, &cs, Some(&cs[0])).unwrap();
        assert_eq!(
            l.windows
                .iter()
                .map(|w| w.class.as_str())
                .collect::<Vec<_>>(),
            ["term"]
        );
        assert_eq!(l.active, None); // the focused window isn't on a shown workspace
    }

    #[test]
    fn the_active_window_starts_the_box() {
        let ms: Vec<HyprMonitor> = serde_json::from_str(MONITORS).unwrap();
        let mut a = client("kitty", 1, 0, false, 0);
        a.at = [3.0, 3.0];
        a.size = [955.0, 1074.0];
        let l = layout_from(&ms, std::slice::from_ref(&a), Some(&a)).unwrap();
        assert_eq!(l.start_rect(), LogicalRect::new(3.0, 3.0, 955.0, 1074.0));
        assert_eq!(l.active, Some(window_id(&a.address)));
    }

    #[test]
    fn a_missing_socket_names_hyprland() {
        let path = Path::new("/nonexistent/hypr/x/.socket.sock");
        let err = request_at(path, "j/monitors", ANSWER_TIMEOUT).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Hyprland") && msg.contains(".socket.sock"),
            "{msg}"
        );
    }

    #[test]
    fn a_silent_hyprland_times_out() {
        let dir = std::env::temp_dir().join(format!("screenie-hypr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".socket.sock");
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let held = std::thread::spawn(move || listener.accept().map(|(s, _)| s));
        let err = request_at(&path, "j/monitors", Duration::from_millis(50)).unwrap_err();
        drop(held.join());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            err.to_string().contains("didn't answer j/monitors"),
            "{err}"
        );
    }

    #[test]
    fn real_client_json_parses() {
        let json = r#"{"address": "0x558311a31620", "mapped": true, "hidden": false, "at": [962, 3],
          "size": [955, 535], "workspace": {"id": 1, "name": "1"}, "floating": false, "monitor": 0,
          "class": "kitty", "title": "~", "pid": 20063, "fullscreen": 0, "focusHistoryID": 2,
          "stableId": "18000008", "pinned": true}"#;
        let c: HyprClient = serde_json::from_str(json).unwrap();
        assert_eq!(window_id(&c.address), WindowId(0x558311a31620));
        assert_eq!(c.stable_id, "18000008");
        assert!(c.pinned);
    }
}
