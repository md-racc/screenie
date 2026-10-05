//! The two-point box: an anchor corner and a cursor corner, like vim's
//! visual-block mode.
//!
//! The box is stored as its rectangle plus which corner the cursor is on
//! (the anchor is the opposite one). It knows only universally true
//! geometry: moving the cursor (crossing the anchor flips it), corner
//! swaps, translation, scaling and centre mode, all kept inside `bounds`.
//! What the box is for, and where a jump should land, belong to the mode.

use crate::geometry::{Axis, Dir, End, LogicalPoint, LogicalRect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorCursor {
    rect: LogicalRect,
    bounds: LogicalRect,
    /// The cursor's edge on each axis, `[x, y]`.
    cursor: [End; 2],
    /// Centre mode: steering resizes symmetrically around the centre.
    centre: bool,
}

fn ax(axis: Axis) -> usize {
    match axis {
        Axis::X => 0,
        Axis::Y => 1,
    }
}

/// A box's edges, `[axis][end]`.
type Edges = [[f64; 2]; 2];

fn end_ix(end: End) -> usize {
    match end {
        End::Low => 0,
        End::High => 1,
    }
}

impl AnchorCursor {
    /// A box with its cursor on the bottom-right corner, held in bounds.
    pub fn new(rect: LogicalRect, bounds: LogicalRect) -> Self {
        let mut b = Self {
            rect,
            bounds,
            cursor: [End::High, End::High],
            centre: false,
        };
        b.clamp();
        b
    }

    pub fn rect(&self) -> LogicalRect {
        self.rect
    }

    pub fn bounds(&self) -> LogicalRect {
        self.bounds
    }

    pub fn is_centred(&self) -> bool {
        self.centre
    }

    /// The cursor's edge on an axis.
    pub fn cursor_end(&self, axis: Axis) -> End {
        self.cursor[ax(axis)]
    }

    /// The cursor corner, or the centre in centre mode.
    pub fn cursor_point(&self) -> LogicalPoint {
        if self.centre {
            return self.rect.centre();
        }
        let e = self.edges();
        LogicalPoint::new(e[0][end_ix(self.cursor[0])], e[1][end_ix(self.cursor[1])])
    }

    /// The anchor corner (opposite the cursor).
    pub fn anchor_point(&self) -> LogicalPoint {
        let e = self.edges();
        LogicalPoint::new(
            e[0][end_ix(self.cursor[0].flip())],
            e[1][end_ix(self.cursor[1].flip())],
        )
    }

    /// The cursor's edge coordinate on an axis.
    pub fn cursor_edge(&self, axis: Axis) -> f64 {
        self.edges()[ax(axis)][end_ix(self.cursor[ax(axis)])]
    }

    pub fn edge(&self, dir: Dir) -> f64 {
        self.edges()[ax(dir.axis())][end_ix(dir.end())]
    }

    fn edges(&self) -> Edges {
        let r = self.rect;
        [[r.x, r.right()], [r.y, r.bottom()]]
    }

    fn set_edges(&mut self, e: Edges) {
        self.rect = LogicalRect::from_edges(e[0][0], e[1][0], e[0][1], e[1][1]);
        self.clamp();
    }

    /// The bounds' low and high edge on an axis.
    pub fn limits(&self, axis: Axis) -> (f64, f64) {
        self.bounds.span(axis)
    }

    /// Hold the box inside the bounds: too big shrinks it, outside moves it.
    fn clamp(&mut self) {
        let b = self.bounds;
        let w = self.rect.w.min(b.w).max(0.0);
        let h = self.rect.h.min(b.h).max(0.0);
        let x = self.rect.x.min(b.x + b.w - w).max(b.x);
        let y = self.rect.y.min(b.y + b.h - h).max(b.y);
        self.rect = LogicalRect::new(x, y, w, h);
    }

    /// Replace the rectangle (clamped). The cursor stays on the same corner.
    pub fn set_rect(&mut self, r: LogicalRect) {
        self.rect = r;
        self.clamp();
    }

    /// Less than half a logical pixel on either axis.
    pub fn is_empty(&self) -> bool {
        self.rect.w < 0.5 || self.rect.h < 0.5
    }

    // --- the cursor ---------------------------------------------------------

    /// Put the cursor's edge on an axis at `value`. Crossing the anchor
    /// swaps which side the cursor is on. In centre mode the centre stays
    /// put and the box grows or shrinks symmetrically, bottoming out at zero.
    pub fn cursor_to(&mut self, axis: Axis, value: f64) {
        let mut e = self.edges();
        let a = ax(axis);
        let (lo_lim, hi_lim) = self.limits(axis);
        let cur = self.cursor[a];
        let [lo, hi] = e[a];
        if self.centre {
            let c = (lo + hi) / 2.0;
            let delta = value - e[a][end_ix(cur)];
            let grow = if cur == End::High { delta } else { -delta };
            let half = ((hi - lo) / 2.0 + grow)
                .min(c - lo_lim)
                .min(hi_lim - c)
                .max(0.0);
            e[a] = [c - half, c + half];
            self.set_edges(e);
            return;
        }
        let anchor = e[a][end_ix(cur.flip())];
        let current = e[a][end_ix(cur)];
        let value = value.min(hi_lim).max(lo_lim);
        if value != current {
            self.cursor[a] = if value > anchor || (value == anchor && value > current) {
                End::High
            } else {
                End::Low
            };
        }
        e[a] = [value.min(anchor), value.max(anchor)];
        self.set_edges(e);
    }

    /// Make `end` the cursor's edge on `axis`, moving nothing (an edge
    /// taken hold of with the mouse).
    pub fn take_cursor(&mut self, axis: Axis, end: End) {
        self.cursor[ax(axis)] = end;
    }

    /// Move the cursor `px` toward `dir`.
    pub fn cursor_by(&mut self, dir: Dir, px: f64) {
        let axis = dir.axis();
        self.cursor_to(axis, self.cursor_edge(axis) + dir.sign() * px);
    }

    /// `o` (both) moves the cursor to the opposite corner; `O` to the
    /// corner sideways. Centre mode has no corners.
    pub fn swap_corner(&mut self, both: bool) {
        if self.centre {
            return;
        }
        self.cursor[0] = self.cursor[0].flip();
        if both {
            self.cursor[1] = self.cursor[1].flip();
        }
    }

    /// Centre mode on or off. The cursor goes to the top-left either way,
    /// so in centre mode `h`/`k` grow and `l`/`j` shrink.
    pub fn toggle_centre(&mut self) {
        self.centre = !self.centre;
        self.cursor = [End::Low, End::Low];
    }

    /// Leave centre mode if it's on.
    pub fn leave_centre(&mut self) {
        if self.centre {
            self.toggle_centre();
        }
    }

    // --- the whole box ----------------------------------------------------------

    /// Translate the box (both corners), stopping at the bounds.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.set_rect(self.rect.translated(dx, dy));
    }

    /// One keyboard step: translate the box, or move the cursor.
    pub fn step(&mut self, dir: Dir, px: f64, translate: bool) {
        if translate {
            let (dx, dy) = dir.vec();
            self.translate(dx * px, dy * px);
        } else {
            self.cursor_by(dir, px);
        }
    }

    /// Keep the half toward `dir`.
    pub fn halve(&mut self, dir: Dir) {
        let mut e = self.edges();
        let a = ax(dir.axis());
        let mid = (e[a][0] + e[a][1]) / 2.0;
        e[a][end_ix(dir.end().flip())] = mid;
        self.set_edges(e);
    }

    /// Grow toward `dir` by the box's own size, the inverse of `halve`;
    /// it stops at the bounds.
    pub fn double(&mut self, dir: Dir) {
        let mut e = self.edges();
        let axis = dir.axis();
        let a = ax(axis);
        let size = e[a][1] - e[a][0];
        let (lo, hi) = self.limits(axis);
        let i = end_ix(dir.end());
        e[a][i] = (e[a][i] + size * dir.sign()).min(hi).max(lo);
        self.set_edges(e);
    }

    /// A drag from `from` (the anchor) to `to` (the cursor). Both are held
    /// inside the bounds, so dragging past an edge stops the box there.
    pub fn drag(&mut self, from: LogicalPoint, to: LogicalPoint) {
        let (lx, hx) = self.limits(Axis::X);
        let (ly, hy) = self.limits(Axis::Y);
        let (x0, gx) = (from.x.min(hx).max(lx), to.x.min(hx).max(lx));
        let (y0, gy) = (from.y.min(hy).max(ly), to.y.min(hy).max(ly));
        self.set_rect(LogicalRect::new(
            x0.min(gx),
            y0.min(gy),
            (gx - x0).abs(),
            (gy - y0).abs(),
        ));
        self.cursor = [
            if gx >= x0 { End::High } else { End::Low },
            if gy >= y0 { End::High } else { End::Low },
        ];
        self.centre = false;
    }

    /// A click that places the cursor corner there; the anchor stays.
    pub fn place_cursor(&mut self, p: LogicalPoint) {
        self.leave_centre();
        self.cursor_to(Axis::X, p.x);
        self.cursor_to(Axis::Y, p.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: LogicalRect = LogicalRect::new(0.0, 0.0, 3840.0, 1080.0);

    fn rect(x: f64, y: f64, w: f64, h: f64) -> LogicalRect {
        LogicalRect::new(x, y, w, h)
    }

    fn sel() -> AnchorCursor {
        AnchorCursor::new(rect(2000.0, 100.0, 200.0, 100.0), LAYOUT)
    }

    #[test]
    fn a_tap_moves_the_cursor_corner() {
        let mut s = sel();
        s.cursor_by(Dir::Right, 5.0);
        assert_eq!(s.rect(), rect(2000.0, 100.0, 205.0, 100.0));
    }

    #[test]
    fn crossing_the_anchor_flips_the_cursor() {
        let mut s = sel();
        for _ in 0..50 {
            s.cursor_by(Dir::Left, 5.0);
        }
        assert_eq!(s.rect(), rect(1950.0, 100.0, 50.0, 100.0));
        assert_eq!(
            (s.cursor_end(Axis::X), s.cursor_end(Axis::Y)),
            (End::Low, End::High)
        );
    }

    #[test]
    fn o_swaps_to_the_opposite_corner_and_capital_o_sideways() {
        let mut s = sel();
        s.swap_corner(true);
        assert_eq!(
            (s.cursor_end(Axis::X), s.cursor_end(Axis::Y)),
            (End::Low, End::Low)
        );
        s.swap_corner(false);
        assert_eq!(
            (s.cursor_end(Axis::X), s.cursor_end(Axis::Y)),
            (End::High, End::Low)
        );
        assert_eq!(s.cursor_point(), LogicalPoint::new(2200.0, 100.0));
        assert_eq!(s.anchor_point(), LogicalPoint::new(2000.0, 200.0));
    }

    #[test]
    fn centre_mode_h_grows_j_shrinks_symmetrically() {
        let mut s = sel();
        s.toggle_centre();
        s.cursor_by(Dir::Left, 5.0);
        assert_eq!(s.rect(), rect(1995.0, 100.0, 210.0, 100.0));
        s.cursor_by(Dir::Down, 5.0);
        assert_eq!(s.rect(), rect(1995.0, 105.0, 210.0, 90.0));
    }

    #[test]
    fn centre_mode_stops_at_zero() {
        let mut s = sel();
        s.toggle_centre();
        for _ in 0..100 {
            s.cursor_by(Dir::Down, 5.0);
        }
        assert_eq!(s.rect().h, 0.0);
        assert_eq!(s.rect().centre().y, 150.0);
    }

    #[test]
    fn centre_mode_has_no_corners_to_swap() {
        let mut s = sel();
        s.toggle_centre();
        s.swap_corner(true);
        assert_eq!(s.cursor_end(Axis::X), End::Low);
        assert_eq!(s.cursor_point(), s.rect().centre());
    }

    #[test]
    fn double_is_the_inverse_of_halve() {
        let mut s = sel();
        s.halve(Dir::Left);
        assert_eq!(s.rect(), rect(2000.0, 100.0, 100.0, 100.0));
        s.double(Dir::Right);
        assert_eq!(s.rect(), rect(2000.0, 100.0, 200.0, 100.0));
        s.double(Dir::Up); // up by its height, stopping at the top
        assert_eq!(s.rect(), rect(2000.0, 0.0, 200.0, 200.0));
        s.double(Dir::Left);
        assert_eq!(s.rect(), rect(1800.0, 0.0, 400.0, 200.0));
    }

    #[test]
    fn a_box_inside_a_box_stays_inside() {
        let mut s = AnchorCursor::new(
            rect(2050.0, 125.0, 100.0, 50.0),
            rect(2000.0, 100.0, 200.0, 100.0),
        );
        s.translate(1000.0, 1000.0);
        assert_eq!(s.rect(), rect(2100.0, 150.0, 100.0, 50.0));
        s.cursor_to(Axis::X, 1e9);
        assert_eq!(s.rect().right(), 2200.0);
    }

    #[test]
    fn a_new_box_too_big_for_its_bounds_shrinks() {
        let s = AnchorCursor::new(rect(-50.0, 10.0, 5000.0, 50.0), LAYOUT);
        assert_eq!(s.rect(), rect(0.0, 10.0, 3840.0, 50.0));
    }

    #[test]
    fn a_drag_past_the_bounds_stops_at_the_edge() {
        let mut s = AnchorCursor::new(
            rect(110.0, 180.0, 20.0, 20.0),
            rect(100.0, 170.0, 560.0, 90.0),
        );
        s.drag(
            LogicalPoint::new(600.0, 200.0),
            LogicalPoint::new(700.0, 230.0),
        );
        assert_eq!(s.rect(), rect(600.0, 200.0, 60.0, 30.0));
    }

    #[test]
    fn a_drag_up_and_left_puts_the_cursor_top_left() {
        let mut s = sel();
        s.drag(
            LogicalPoint::new(300.0, 300.0),
            LogicalPoint::new(200.0, 250.0),
        );
        assert_eq!(s.rect(), rect(200.0, 250.0, 100.0, 50.0));
        assert_eq!(s.cursor_point(), LogicalPoint::new(200.0, 250.0));
    }

    #[test]
    fn a_click_places_the_cursor_corner_and_the_anchor_stays() {
        let mut s = AnchorCursor::new(
            rect(100.0, 170.0, 20.0, 20.0),
            rect(100.0, 170.0, 560.0, 90.0),
        );
        s.place_cursor(LogicalPoint::new(400.0, 240.0));
        assert_eq!(s.rect(), rect(100.0, 170.0, 300.0, 70.0));
    }
}
