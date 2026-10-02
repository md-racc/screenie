//! Where Select's jumps land: window and monitor edges, the monitors'
//! quarter lines, neighbouring windows, and the selection scope.
//!
//! These resolve the box grammar's jump intents for Select. `AnchorCursor`
//! knows how to move; this knows where to.

use crate::anchor_cursor::AnchorCursor;
use crate::geometry::{Dir, LogicalRect};
use crate::layout::{Layout, Targets, nearest};

/// Snap targets closer than this to where an edge already is are skipped,
/// so a jump never stops on the far side of a window gap.
pub const GAP: f64 = 12.0;

/// The windows and monitors Select can snap to.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapEnv {
    /// Visible windows, topmost first.
    pub windows: Vec<LogicalRect>,
    pub monitors: Vec<LogicalRect>,
    /// Every window and monitor edge.
    pub edges: Targets,
}

impl SnapEnv {
    pub fn new(windows: Vec<LogicalRect>, monitors: Vec<LogicalRect>) -> Self {
        let all: Vec<_> = windows.iter().chain(&monitors).copied().collect();
        Self {
            edges: Targets::edges_of(&all),
            windows,
            monitors,
        }
    }

    pub fn from_layout(layout: &Layout) -> Self {
        Self::new(layout.window_rects(), layout.monitor_rects())
    }

    /// `w b } {`: the cursor to the next window or monitor edge that way, or
    /// the bounds if there's none. `0 $ g G` (`to_limit`): to the bounds.
    pub fn cursor_jump(&self, sel: &mut AnchorCursor, dir: Dir, to_limit: bool) {
        let axis = dir.axis();
        let value = sel.cursor_edge(axis);
        let t = if to_limit {
            None
        } else {
            self.edges.next(axis, value, dir.sign(), GAP)
        };
        let (lo, hi) = sel.limits(axis);
        let t = t.unwrap_or(if dir.sign() < 0.0 { lo } else { hi });
        sel.cursor_to(axis, t);
    }

    /// Translate the box until its leading or trailing edge lands on the
    /// next of `targets`, whichever is nearer.
    fn snap_move(sel: &mut AnchorCursor, dir: Dir, targets: &Targets) {
        let axis = dir.axis();
        let mut best: Option<f64> = None;
        let low_first = match axis {
            crate::geometry::Axis::X => [Dir::Left, Dir::Right],
            crate::geometry::Axis::Y => [Dir::Up, Dir::Down],
        };
        for edge in low_first {
            let e = sel.edge(edge);
            if let Some(t) = targets.next(axis, e, dir.sign(), GAP) {
                if best.is_none_or(|b| (t - e).abs() < b.abs()) {
                    best = Some(t - e);
                }
            }
        }
        if let Some(d) = best {
            let (dx, dy) = dir.vec();
            sel.translate(d * dx.abs(), d * dy.abs());
        }
    }

    /// Ctrl+Shift+hjkl: slide the box, same size, until its leading or
    /// trailing edge lands on the next window or monitor edge, or quarter
    /// line of a monitor. Lines within `GAP` are skipped: a box from a
    /// window is inset by the compositor's gaps, so once one edge is on a
    /// line the other sits just shy of the next, and every other press
    /// would otherwise be a 1-3px hop. For the same reason the leading edge
    /// is then pulled onto a line it lands just shy of.
    pub fn slide(&self, sel: &mut AnchorCursor, dir: Dir) {
        let targets = self.edges.union(&Targets::grid(&self.monitors));
        Self::snap_move(sel, dir, &targets);
        let axis = dir.axis();
        let lead = sel.edge(dir);
        let sign = dir.sign();
        let near = targets
            .on(axis)
            .iter()
            .copied()
            .filter(|t| {
                let d = (t - lead) * sign;
                0.0 < d && d <= GAP
            })
            .reduce(|a, b| {
                if (b - lead).abs() < (a - lead).abs() {
                    b
                } else {
                    a
                }
            });
        if let Some(t) = near {
            let (dx, dy) = dir.vec();
            sel.translate((t - lead) * dx.abs(), (t - lead) * dy.abs());
        }
    }

    /// Ctrl+hjkl: select the nearest window that way. In order of
    /// preference: windows beside the box; then windows the box sits inside
    /// (from a floating window back to the maximised one under it); then
    /// monitors.
    pub fn neighbour(&self, sel: &mut AnchorCursor, dir: Dir) {
        let c = sel.rect().centre();
        let (dx, dy) = dir.vec();
        let beside: Vec<_> = self
            .windows
            .iter()
            .filter(|r| !r.contains(c))
            .copied()
            .collect();
        let around: Vec<_> = self
            .windows
            .iter()
            .filter(|r| r.contains(c))
            .copied()
            .collect();
        for rects in [&beside, &around, &self.monitors] {
            let mut best: Option<(f64, LogicalRect)> = None;
            for r in rects {
                let rc = r.centre();
                let along = (rc.x - c.x) * dx + (rc.y - c.y) * dy;
                if along < 1.0 || r.nearly(&sel.rect()) {
                    continue; // not that way, or already on it
                }
                let perp = ((rc.x - c.x) * dy).abs() + ((rc.y - c.y) * dx).abs();
                let score = along + 2.0 * perp;
                if best.is_none_or(|(s, _)| score < s) {
                    best = Some((score, *r));
                }
            }
            if let Some((_, r)) = best {
                sel.set_rect(r);
                return;
            }
        }
    }

    /// `+` / `-`: the scope, from the nearest window to its monitor to
    /// everything (the bounds); it stops at either end. A box of its own
    /// widens to its window on `+`, and `-` leaves it alone.
    pub fn scope(&self, sel: &mut AnchorCursor, step: i32) {
        let c = sel.rect().centre();
        let mut steps: Vec<LogicalRect> = Vec::new();
        steps.extend(nearest(&self.windows, c));
        steps.extend(nearest(&self.monitors, c));
        steps.push(sel.bounds());
        // A fullscreen window is its monitor; one monitor is everything.
        steps.dedup_by(|b, a| b.nearly(a));
        match steps.iter().position(|s| sel.rect().nearly(s)) {
            None if step > 0 => sel.set_rect(steps[0]),
            None => {}
            Some(i) => {
                let j = (i as i64 + i64::from(step)).clamp(0, steps.len() as i64 - 1);
                sel.set_rect(steps[j as usize]);
            }
        }
    }

    /// A click: the window under the pointer (topmost), else the nearest
    /// monitor.
    pub fn click_target(&self, p: crate::geometry::LogicalPoint) -> Option<LogicalRect> {
        self.windows
            .iter()
            .find(|r| r.contains(p))
            .copied()
            .or_else(|| nearest(&self.monitors, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::LogicalPoint;

    const LAYOUT: LogicalRect = LogicalRect::new(0.0, 0.0, 3840.0, 1080.0);
    const MONITORS: [LogicalRect; 2] = [
        LogicalRect::new(0.0, 0.0, 1920.0, 1080.0),
        LogicalRect::new(1920.0, 0.0, 1920.0, 1080.0),
    ];
    const WINDOWS: [LogicalRect; 3] = [
        LogicalRect::new(1923.0, 3.0, 955.0, 1074.0),
        LogicalRect::new(2883.0, 3.0, 954.0, 1074.0),
        LogicalRect::new(3.0, 3.0, 955.0, 1074.0),
    ];

    fn r(x: f64, y: f64, w: f64, h: f64) -> LogicalRect {
        LogicalRect::new(x, y, w, h)
    }

    fn env() -> SnapEnv {
        SnapEnv::new(WINDOWS.to_vec(), MONITORS.to_vec())
    }

    fn sel(rect: LogicalRect) -> AnchorCursor {
        AnchorCursor::new(rect, LAYOUT)
    }

    #[test]
    fn a_jump_skips_the_window_gap() {
        let (e, mut s) = (env(), sel(r(2000.0, 100.0, 200.0, 100.0)));
        e.cursor_jump(&mut s, Dir::Right, false);
        assert_eq!(s.edge(Dir::Right), 2878.0);
        e.cursor_jump(&mut s, Dir::Right, false); // 2883 is across a 5px gap
        assert_eq!(s.edge(Dir::Right), 3837.0);
    }

    #[test]
    fn a_far_jump_goes_to_the_bounds() {
        let (e, mut s) = (
            env(),
            AnchorCursor::new(
                r(2050.0, 125.0, 100.0, 50.0),
                r(2000.0, 100.0, 200.0, 100.0),
            ),
        );
        e.cursor_jump(&mut s, Dir::Right, true);
        assert_eq!(s.edge(Dir::Right), 2200.0);
    }

    #[test]
    fn neighbour_skips_the_window_the_box_is_in() {
        let (e, mut s) = (env(), sel(r(2000.0, 100.0, 200.0, 100.0)));
        e.neighbour(&mut s, Dir::Right);
        assert_eq!(s.rect(), WINDOWS[1]);
    }

    #[test]
    fn neighbour_prefers_windows_over_monitors() {
        let (e, mut s) = (env(), sel(WINDOWS[0]));
        e.neighbour(&mut s, Dir::Left);
        assert_eq!(s.rect(), WINDOWS[2]);
    }

    #[test]
    fn scope_steps_window_monitor_all_and_back() {
        let (e, mut s) = (env(), sel(r(2000.0, 100.0, 200.0, 100.0)));
        let mut seen = vec![];
        for step in [1, 1, 1, -1, -1, -1] {
            e.scope(&mut s, step);
            seen.push(s.rect());
        }
        assert_eq!(
            seen,
            [
                WINDOWS[0],
                MONITORS[1],
                LAYOUT,
                MONITORS[1],
                WINDOWS[0],
                WINDOWS[0]
            ]
        );
    }

    #[test]
    fn minus_leaves_a_box_of_its_own_alone() {
        let (e, mut s) = (env(), sel(r(200.0, 300.0, 200.0, 200.0)));
        e.scope(&mut s, -1);
        assert_eq!(s.rect(), r(200.0, 300.0, 200.0, 200.0));
    }

    #[test]
    fn slides_to_the_next_quarter_line() {
        let e = SnapEnv::new(vec![], MONITORS.to_vec());
        let mut s = sel(r(100.0, 100.0, 300.0, 100.0));
        e.slide(&mut s, Dir::Right); // right edge 400 -> 480 (the first quarter line)
        assert_eq!(s.rect(), r(180.0, 100.0, 300.0, 100.0));
        e.slide(&mut s, Dir::Right); // left edge 180 -> 480 beats right edge 480 -> 960
        assert_eq!(s.rect(), r(480.0, 100.0, 300.0, 100.0));
        e.slide(&mut s, Dir::Left);
        assert_eq!(s.rect(), r(180.0, 100.0, 300.0, 100.0)); // right edge back on 480
        e.slide(&mut s, Dir::Down); // bottom 200 -> 270 (a quarter of 1080)
        assert_eq!(s.rect(), r(180.0, 170.0, 300.0, 100.0));
    }

    #[test]
    fn a_box_from_a_window_never_hops_a_few_pixels() {
        let e = env();
        let mut s = sel(WINDOWS[2]); // a tiled window, inset 3px by the gaps
        s.halve(Dir::Down);
        s.halve(Dir::Down); // its bottom quarter: 268.5 tall, not 270
        let mut tops = vec![];
        for _ in 0..3 {
            let before = s.rect().y;
            e.slide(&mut s, Dir::Up);
            assert!(before - s.rect().y > GAP, "always a real step");
            tops.push(s.rect().y);
        }
        assert_eq!(tops, [540.0, 270.0, 0.0]); // the top edge exactly on each line
        e.slide(&mut s, Dir::Up);
        assert_eq!(s.rect().y, 0.0); // at the top: nothing left to do
    }

    const FS: LogicalRect = LogicalRect::new(0.0, 0.0, 1920.0, 1080.0);

    #[test]
    fn with_a_fullscreen_window_ctrl_l_then_ctrl_h_is_a_round_trip() {
        let e = SnapEnv::new(vec![FS, WINDOWS[0], WINDOWS[1]], MONITORS.to_vec());
        let mut s = sel(FS);
        e.neighbour(&mut s, Dir::Right);
        assert_eq!(s.rect(), WINDOWS[0]);
        e.neighbour(&mut s, Dir::Left);
        assert_eq!(s.rect(), FS);
    }

    #[test]
    fn from_a_floating_window_back_to_the_one_under_it() {
        let floating = r(200.0, 300.0, 400.0, 300.0);
        let e = SnapEnv::new(vec![floating, FS], MONITORS.to_vec());
        let mut s = sel(floating);
        e.neighbour(&mut s, Dir::Right); // nothing beside it that way
        assert_eq!(s.rect(), FS);
    }

    #[test]
    fn a_click_on_empty_space_is_the_nearest_monitor() {
        let e = env();
        assert_eq!(
            e.click_target(LogicalPoint::new(1000.0, 500.0)),
            Some(MONITORS[0])
        );
        assert_eq!(
            e.click_target(LogicalPoint::new(2000.0, 500.0)),
            Some(WINDOWS[0])
        );
    }
}
