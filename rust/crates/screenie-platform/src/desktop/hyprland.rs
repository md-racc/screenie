//! Hyprland: monitors, clients and the active window over its IPC socket.
//!
//! Hyprland reports monitor sizes in device pixels and positions in
//! logical ones; core wants everything logical. Windows are filtered to
//! the ones you can actually see, topmost first.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

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

/// The address Hyprland knows a window by.
pub fn address(id: WindowId) -> String {
    format!("0x{:x}", id.0)
}

/// The clients you can actually see on the workspaces shown, topmost
/// first. A fullscreen window hides everything else on its workspace; a
/// maximised one hides the other tiled windows, but floating ones stay
/// above it.
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
            let behind_fullscreen = others().any(|o| o.fullscreen >= 2);
            let behind_maximised = !c.floating && others().any(|o| o.fullscreen == 1);
            !behind_fullscreen && !behind_maximised
        })
        .map(|c| (*c).clone())
        .collect();
    visible.sort_by_key(|c| c.focus_history_id);
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

/// One request (`j/monitors`) and its whole answer.
pub fn request(cmd: &str) -> Result<String> {
    let mut s = UnixStream::connect(socket()?)?;
    s.write_all(cmd.as_bytes())?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

fn query<T: for<'de> Deserialize<'de>>(cmd: &str) -> Result<T> {
    let text = request(cmd)?;
    serde_json::from_str(&text).map_err(|e| Error::Protocol(format!("{cmd}: {e}")))
}

/// The live Hyprland session.
pub struct Hyprland;

impl Hyprland {
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
        }
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
        assert_eq!(names(visible_clients(&cs, &ws(&[1]))), ["max", "float"]);
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
        assert_eq!(address(l.active.unwrap()), a.address);
    }

    #[test]
    fn real_client_json_parses() {
        let json = r#"{"address": "0x558311a31620", "mapped": true, "hidden": false, "at": [962, 3],
          "size": [955, 535], "workspace": {"id": 1, "name": "1"}, "floating": false, "monitor": 0,
          "class": "kitty", "title": "~", "pid": 20063, "fullscreen": 0, "focusHistoryID": 2,
          "stableId": "18000008"}"#;
        let c: HyprClient = serde_json::from_str(json).unwrap();
        assert_eq!(window_id(&c.address), WindowId(0x558311a31620));
        assert_eq!(c.stable_id, "18000008");
    }
}
