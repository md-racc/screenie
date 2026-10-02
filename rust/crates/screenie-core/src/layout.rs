//! The desktop as the picker sees it: monitors, visible windows, the active
//! window and the bounds of everything, in logical coordinates.
//!
//! The platform crate produces this. Nothing here knows which compositor
//! it came from.

use serde::{Deserialize, Serialize};

use crate::geometry::{Axis, LogicalPoint, LogicalRect};

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
    /// The windows you can see, topmost (most recently focused) first.
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

/// Snap target coordinates per axis, sorted and without repeats.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

fn sorted_unique(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

impl Targets {
    /// Every left/right and top/bottom edge of some rectangles.
    pub fn edges_of(rects: &[LogicalRect]) -> Self {
        Self {
            x: sorted_unique(rects.iter().flat_map(|r| [r.x, r.right()]).collect()),
            y: sorted_unique(rects.iter().flat_map(|r| [r.y, r.bottom()]).collect()),
        }
    }

    /// Each monitor's edges and its quarter, half and three-quarter lines.
    pub fn grid(monitors: &[LogicalRect]) -> Self {
        let lines = |o: f64, s: f64| (0..5).map(move |i| o + s * f64::from(i) / 4.0);
        Self {
            x: sorted_unique(monitors.iter().flat_map(|m| lines(m.x, m.w)).collect()),
            y: sorted_unique(monitors.iter().flat_map(|m| lines(m.y, m.h)).collect()),
        }
    }

    pub fn union(&self, other: &Targets) -> Self {
        Self {
            x: sorted_unique(self.x.iter().chain(&other.x).copied().collect()),
            y: sorted_unique(self.y.iter().chain(&other.y).copied().collect()),
        }
    }

    pub fn on(&self, axis: Axis) -> &[f64] {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
        }
    }

    /// The nearest target more than `gap` past `value` in direction `sign`
    /// (+1 or -1). Targets within the gap are the far side of the window
    /// gap an edge is sitting in, so they're skipped.
    pub fn next(&self, axis: Axis, value: f64, sign: f64, gap: f64) -> Option<f64> {
        let past = self
            .on(axis)
            .iter()
            .copied()
            .filter(|t| (t - value) * sign > gap);
        if sign > 0.0 {
            past.reduce(f64::min)
        } else {
            past.reduce(f64::max)
        }
    }
}

/// The first rectangle containing the point (they're topmost first), else
/// the one nearest it. `None` only for an empty list.
pub fn nearest(rects: &[LogicalRect], p: LogicalPoint) -> Option<LogicalRect> {
    if let Some(hit) = rects.iter().find(|r| r.contains(p)) {
        return Some(*hit);
    }
    let mut best: Option<(f64, LogicalRect)> = None;
    for r in rects {
        let d = r.dist2(p);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, *r));
        }
    }
    best.map(|(_, r)| r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_are_sorted_and_unique() {
        let t = Targets::edges_of(&[
            LogicalRect::new(10.0, 0.0, 10.0, 5.0),
            LogicalRect::new(0.0, 0.0, 10.0, 5.0),
        ]);
        assert_eq!(t.x, vec![0.0, 10.0, 20.0]);
        assert_eq!(t.y, vec![0.0, 5.0]);
    }

    #[test]
    fn the_grid_covers_every_monitor() {
        let g = Targets::grid(&[
            LogicalRect::new(0.0, 0.0, 1920.0, 1080.0),
            LogicalRect::new(1920.0, 0.0, 1920.0, 1080.0),
        ]);
        assert_eq!(
            g.x,
            vec![
                0.0, 480.0, 960.0, 1440.0, 1920.0, 2400.0, 2880.0, 3360.0, 3840.0
            ]
        );
        assert_eq!(g.y, vec![0.0, 270.0, 540.0, 810.0, 1080.0]);
    }

    #[test]
    fn next_skips_targets_inside_the_gap() {
        let t = Targets {
            x: vec![0.0, 958.0, 1920.0, 1923.0],
            y: vec![],
        };
        assert_eq!(t.next(Axis::X, 1918.0, 1.0, 12.0), None);
        assert_eq!(t.next(Axis::X, 1918.0, -1.0, 12.0), Some(958.0));
        assert_eq!(t.next(Axis::X, 900.0, 1.0, 12.0), Some(958.0));
    }

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
