//! Coordinates: logical desktop space, image pixels, and the one mapping
//! between them.
//!
//! Picker interaction happens in logical coordinates (`LogicalPoint`,
//! `LogicalRect`), which may be fractional. Image operations happen in
//! pixels (`PixelPoint`, `PixelRect`). The only conversion is `PixelMap`,
//! which applies `rnd(v) = floor(v + 0.5)` independently at every edge, so
//! a crop, its edge pixels and the cursor pixel always agree.

use serde::{Deserialize, Serialize};

/// A point in logical desktop coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

impl LogicalPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn get(self, axis: Axis) -> f64 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
        }
    }
}

/// A rectangle in logical desktop coordinates: origin and size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LogicalRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl LogicalRect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// From its low and high edge on each axis.
    pub fn from_edges(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self::new(left, top, right - left, bottom - top)
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn centre(&self) -> LogicalPoint {
        LogicalPoint::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// The low (left/top) and high (right/bottom) edge on an axis.
    pub fn span(&self, axis: Axis) -> (f64, f64) {
        match axis {
            Axis::X => (self.x, self.right()),
            Axis::Y => (self.y, self.bottom()),
        }
    }

    pub fn size(&self, axis: Axis) -> f64 {
        match axis {
            Axis::X => self.w,
            Axis::Y => self.h,
        }
    }

    /// Half-open containment: the right and bottom edges are outside.
    pub fn contains(&self, p: LogicalPoint) -> bool {
        self.x <= p.x && p.x < self.right() && self.y <= p.y && p.y < self.bottom()
    }

    /// The squared distance from a point to the rectangle (0 inside).
    pub fn dist2(&self, p: LogicalPoint) -> f64 {
        let dx = (self.x - p.x).max(0.0).max(p.x - self.right());
        let dy = (self.y - p.y).max(0.0).max(p.y - self.bottom());
        dx * dx + dy * dy
    }

    /// The same rectangle, give or take under half a logical pixel on each
    /// coordinate: how a box is matched against a window or monitor.
    pub fn nearly(&self, other: &LogicalRect) -> bool {
        (self.x - other.x).abs() < 0.5
            && (self.y - other.y).abs() < 0.5
            && (self.w - other.w).abs() < 0.5
            && (self.h - other.h).abs() < 0.5
    }

    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    pub fn intersects(&self, other: &LogicalRect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    pub const BOTH: [Axis; 2] = [Axis::X, Axis::Y];
}

/// One of the four directions `h j k l` mean everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Left, Dir::Right, Dir::Up, Dir::Down];

    pub fn axis(self) -> Axis {
        match self {
            Dir::Left | Dir::Right => Axis::X,
            Dir::Up | Dir::Down => Axis::Y,
        }
    }

    /// -1 toward the low edge (left, up), +1 toward the high edge.
    pub fn sign(self) -> f64 {
        match self {
            Dir::Left | Dir::Up => -1.0,
            Dir::Right | Dir::Down => 1.0,
        }
    }

    /// The unit vector.
    pub fn vec(self) -> (f64, f64) {
        match self.axis() {
            Axis::X => (self.sign(), 0.0),
            Axis::Y => (0.0, self.sign()),
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
        }
    }

    /// The edge of a box this direction points at.
    pub fn end(self) -> End {
        if self.sign() < 0.0 {
            End::Low
        } else {
            End::High
        }
    }
}

/// Which edge on an axis: low (left, top) or high (right, bottom).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum End {
    Low,
    High,
}

impl End {
    pub fn flip(self) -> End {
        match self {
            End::Low => End::High,
            End::High => End::Low,
        }
    }
}

/// A pixel position in the captured image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PixelPoint {
    pub x: i32,
    pub y: i32,
}

impl PixelPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A rectangle of image pixels. `w` and `h` are never negative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PixelRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn is_empty(&self) -> bool {
        self.w < 1 || self.h < 1
    }
}

/// The settled logical-to-pixel rounding rule: round half up.
pub fn rnd(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

/// The one authoritative mapping between logical coordinates and the
/// pixels of a capture.
///
/// The capture covers `origin` .. `origin + size / scale` logically, at
/// `scale` pixels per logical pixel (one capture, at the highest monitor
/// scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelMap {
    origin: LogicalPoint,
    scale: f64,
    width: i32,
    height: i32,
}

impl PixelMap {
    /// A capture of `width` x `height` pixels covering `layout`.
    pub fn new(layout: LogicalRect, width: i32, height: i32) -> Self {
        Self {
            origin: LogicalPoint::new(layout.x, layout.y),
            scale: f64::from(width) / layout.w,
            width,
            height,
        }
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    fn px(&self, v: f64, axis: Axis) -> f64 {
        let o = match axis {
            Axis::X => self.origin.x,
            Axis::Y => self.origin.y,
        };
        (v - o) * self.scale
    }

    /// A logical rectangle as image pixels, rounding each edge on its own.
    pub fn to_pixels(&self, r: LogicalRect) -> PixelRect {
        let x0 = rnd(self.px(r.x, Axis::X));
        let y0 = rnd(self.px(r.y, Axis::Y));
        let x1 = rnd(self.px(r.right(), Axis::X));
        let y1 = rnd(self.px(r.bottom(), Axis::Y));
        PixelRect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    /// The centre of a pixel, in logical coordinates.
    pub fn to_logical(&self, p: PixelPoint) -> LogicalPoint {
        LogicalPoint::new(
            self.origin.x + (f64::from(p.x) + 0.5) / self.scale,
            self.origin.y + (f64::from(p.y) + 0.5) / self.scale,
        )
    }

    /// A pixel position held inside the image.
    pub fn clamp(&self, x: i32, y: i32) -> PixelPoint {
        PixelPoint::new(
            x.clamp(0, (self.width - 1).max(0)),
            y.clamp(0, (self.height - 1).max(0)),
        )
    }

    /// The pixel under a logical point (a click, a centred cursor).
    pub fn pixel_under(&self, p: LogicalPoint) -> PixelPoint {
        self.clamp(
            self.px(p.x, Axis::X).floor() as i32,
            self.px(p.y, Axis::Y).floor() as i32,
        )
    }

    /// The pixel at a box corner that lies inside the box: a left or top
    /// edge's own pixel, or the one just before a right or bottom edge. It
    /// uses the same rounding as `to_pixels`, so it is always in the crop.
    pub fn edge_pixel(&self, p: LogicalPoint, right: bool, bottom: bool) -> PixelPoint {
        self.clamp(
            rnd(self.px(p.x, Axis::X)) - i32::from(right),
            rnd(self.px(p.y, Axis::Y)) - i32::from(bottom),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rnd_rounds_half_up() {
        assert_eq!(rnd(0.5), 1);
        assert_eq!(rnd(1.5), 2);
        assert_eq!(rnd(2.5), 3);
        assert_eq!(rnd(-0.5), 0);
        assert_eq!(rnd(2.49), 2);
    }

    #[test]
    fn rect_containment_is_half_open() {
        let r = LogicalRect::new(10.0, 10.0, 10.0, 10.0);
        assert!(r.contains(LogicalPoint::new(10.0, 10.0)));
        assert!(!r.contains(LogicalPoint::new(20.0, 15.0)));
        assert_eq!(r.dist2(LogicalPoint::new(23.0, 24.0)), 9.0 + 16.0);
    }

    #[test]
    fn at_scale_1_pixels_are_logical() {
        let m = PixelMap::new(LogicalRect::new(0.0, 0.0, 3840.0, 1080.0), 3840, 1080);
        let r = LogicalRect::new(100.0, 170.0, 560.0, 90.0);
        assert_eq!(m.to_pixels(r), PixelRect::new(100, 170, 560, 90));
        assert_eq!(
            m.edge_pixel(LogicalPoint::new(660.0, 260.0), true, true),
            PixelPoint::new(659, 259)
        );
        assert_eq!(
            m.to_logical(PixelPoint::new(5, 6)),
            LogicalPoint::new(5.5, 6.5)
        );
    }

    /// The prototype's audit b8: at 1.5x the corner pixels are the crop's
    /// first and last columns, whichever way the edges round.
    #[test]
    fn the_cursor_pixel_is_inside_the_crop_at_1_5x() {
        let m = PixelMap::new(LogicalRect::new(0.0, 0.0, 1000.0, 600.0), 1500, 900);
        for x in [101.0, 103.0] {
            let rect = LogicalRect::new(x, 10.0, 100.0, 50.0);
            let px = m.to_pixels(rect);
            let left = m.edge_pixel(LogicalPoint::new(rect.x, rect.y), false, false);
            let right = m.edge_pixel(LogicalPoint::new(rect.right(), rect.y), true, false);
            assert_eq!((left.x, right.x), (px.x, px.x + px.w - 1), "{rect:?}");
        }
    }

    #[test]
    fn a_sub_pixel_box_is_empty() {
        let m = PixelMap::new(LogicalRect::new(0.0, 0.0, 3840.0, 1080.0), 3840, 1080);
        assert!(
            m.to_pixels(LogicalRect::new(100.5, 170.0, 0.6, 90.0))
                .is_empty()
        );
    }

    #[test]
    fn pixel_under_clamps_to_the_image() {
        let m = PixelMap::new(LogicalRect::new(0.0, 0.0, 100.0, 100.0), 200, 200);
        assert_eq!(
            m.pixel_under(LogicalPoint::new(10.7, 0.2)),
            PixelPoint::new(21, 0)
        );
        assert_eq!(
            m.pixel_under(LogicalPoint::new(500.0, -3.0)),
            PixelPoint::new(199, 0)
        );
    }

    #[test]
    fn an_offset_layout_maps_from_its_origin() {
        let m = PixelMap::new(LogicalRect::new(-1920.0, 0.0, 3840.0, 1080.0), 3840, 1080);
        assert_eq!(
            m.to_pixels(LogicalRect::new(0.0, 0.0, 10.0, 10.0)),
            PixelRect::new(1920, 0, 10, 10)
        );
    }
}
