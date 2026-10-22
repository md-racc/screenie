//! The desktop as the picker sees it: monitors, visible windows, the active
//! window and the bounds of everything, in logical coordinates.
//!
//! The platform crate produces this. Nothing here knows which compositor
//! it came from.

use serde::{Deserialize, Serialize};

use crate::geometry::{LogicalPoint, LogicalRect};

/// Stable for one run: the platform's own identity for a monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorId(pub u32);

/// Stable for one run: the platform's own identity for a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Monitor {
    pub id: MonitorId,
    /// The connector name (`DP-1`), which recall anchors boxes to.
    pub name: String,
    pub rect: LogicalRect,
    /// Device pixels per logical pixel.
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub id: WindowId,
    pub rect: LogicalRect,
    pub monitor: Option<MonitorId>,
    pub class: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub monitors: Vec<Monitor>,
    /// The windows you can see, topmost first.
    pub windows: Vec<Window>,
    pub active: Option<WindowId>,
    pub focused: MonitorId,
    /// The bounding box of every monitor.
    pub bounds: LogicalRect,
}

impl Layout {
    /// A layout from its monitors, with `bounds` computed from them.
    pub fn new(monitors: Vec<Monitor>, windows: Vec<Window>, focused: MonitorId) -> Self {
        let bounds = bounding(monitors.iter().map(|m| m.rect));
        Self {
            monitors,
            windows,
            active: None,
            focused,
            bounds,
        }
    }

    pub fn monitor(&self, id: MonitorId) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.id == id)
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    pub fn monitor_rects(&self) -> Vec<LogicalRect> {
        self.monitors.iter().map(|m| m.rect).collect()
    }

    pub fn window_rects(&self) -> Vec<LogicalRect> {
        self.windows.iter().map(|w| w.rect).collect()
    }

    /// Where the box starts: the active window, else the middle quarter of
    /// the focused monitor.
    pub fn start_rect(&self) -> LogicalRect {
        if let Some(w) = self.active.and_then(|id| self.window(id)) {
            return w.rect;
        }
        let f = self
            .monitor(self.focused)
            .or(self.monitors.first())
            .map_or(self.bounds, |m| m.rect);
        LogicalRect::new(f.x + f.w / 4.0, f.y + f.h / 4.0, f.w / 2.0, f.h / 2.0)
    }

    /// The monitor holding a point, if any.
    pub fn monitor_at(&self, p: LogicalPoint) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.rect.contains(p))
    }
}

/// The bounding box of some rectangles.
pub fn bounding(rects: impl IntoIterator<Item = LogicalRect>) -> LogicalRect {
    let mut it = rects.into_iter();
    let Some(first) = it.next() else {
        return LogicalRect::default();
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.right(), first.bottom());
    for r in it {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.right());
        y1 = y1.max(r.bottom());
    }
    LogicalRect::from_edges(x0, y0, x1, y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_is_the_active_window_else_the_middle_of_the_focused_monitor() {
        let mon = |id, x| Monitor {
            id: MonitorId(id),
            name: format!("DP-{id}"),
            rect: LogicalRect::new(x, 0.0, 1920.0, 1080.0),
            scale: 1.0,
        };
        let win = Window {
            id: WindowId(7),
            rect: LogicalRect::new(3.0, 3.0, 955.0, 1074.0),
            monitor: Some(MonitorId(1)),
            class: "foot".into(),
            title: "~".into(),
        };
        let mut l = Layout::new(vec![mon(1, 0.0), mon(2, 1920.0)], vec![win], MonitorId(2));
        assert_eq!(l.bounds, LogicalRect::new(0.0, 0.0, 3840.0, 1080.0));
        assert_eq!(
            l.start_rect(),
            LogicalRect::new(2400.0, 270.0, 960.0, 540.0)
        );
        l.active = Some(WindowId(7));
        assert_eq!(l.start_rect(), LogicalRect::new(3.0, 3.0, 955.0, 1074.0));
    }
}
