//! Insert (`i`): a single pixel position inside the box, like vim's insert
//! cursor. The box stays put; the cursor moves in frame pixels.
//!
//! Tools open from here at the cursor (a censor's top-left on it), and come
//! back here; the loupe hands its position back when it closes.

use crate::geometry::{Dir, LogicalPoint, PixelMap, PixelPoint, PixelRect, rnd};
use crate::input::Rates;
use crate::intent::{DirSet, Intent, Scale};
use crate::lens::{EnhanceMode, LoupeMode};
use crate::session::{FinishedAction, Mode, Outcome, Session, Transition};
use crate::targets::GAP;

/// A tap, frame pixels; `+`/`-` change it within the range.
pub const STEP: i32 = 5;
pub const STEP_MIN: i32 = 1;
pub const STEP_MAX: i32 = 20;
/// Shift: this many steps.
pub const BIG: i32 = 4;
/// Held-key speed: px/s when repeat starts; at the top, crossing the box
/// takes about `CROSS` seconds, within `TOP_MIN`..`TOP_MAX` px/s.
pub const START_PX: f64 = 40.0;
pub const CROSS: f64 = 1.0;
pub const TOP_MIN: f64 = 40.0;
pub const TOP_MAX: f64 = 800.0;

/// A pixel position held inside a pixel box, with the trail `u` walks.
#[derive(Clone, Debug)]
pub struct PointCursor {
    pub at: PixelPoint,
    pub bounds: PixelRect,
    trail: Vec<PixelPoint>,
}

impl PointCursor {
    /// A cursor in `main` (at least one pixel), at its centre.
    pub fn new(map: &PixelMap, main: crate::geometry::LogicalRect) -> Self {
        let b = map.to_pixels(main);
        let bounds = PixelRect::new(b.x, b.y, b.w.max(1), b.h.max(1));
        let at = PixelPoint::new(bounds.x + bounds.w / 2, bounds.y + bounds.h / 2);
        Self {
            at,
            bounds,
            trail: Vec::new(),
        }
    }

    /// Put the cursor at (x, y), clamped into the box; a tap remembers
    /// where it was, a repeat doesn't.
    pub fn put(&mut self, x: i32, y: i32, remember: bool) {
        if remember {
            self.trail.push(self.at);
        }
        let b = self.bounds;
        self.at = PixelPoint::new(x.clamp(b.x, b.x + b.w - 1), y.clamp(b.y, b.y + b.h - 1));
    }

    pub fn shift(&mut self, dx: i32, dy: i32, remember: bool) {
        self.put(self.at.x + dx, self.at.y + dy, remember);
    }

    /// The box's last pixel row or column that way.
    pub fn box_edge(&self, d: Dir) -> i32 {
        let b = self.bounds;
        match d {
            Dir::Left => b.x,
            Dir::Right => b.x + b.w - 1,
            Dir::Up => b.y,
            Dir::Down => b.y + b.h - 1,
        }
    }

    pub fn back(&mut self) {
        if let Some(p) = self.trail.pop() {
            self.at = p;
        }
    }
}

fn vec(d: Dir) -> (i32, i32) {
    let (x, y) = d.vec();
    (x as i32, y as i32)
}

fn vector(dirs: DirSet) -> (i32, i32) {
    let (x, y) = dirs.vector();
    (x as i32, y as i32)
}

#[derive(Clone, Debug)]
pub struct InsertMode {
    pub cursor: PointCursor,
    pub step: i32,
}

impl InsertMode {
    pub fn new(map: &PixelMap, main: crate::geometry::LogicalRect) -> Self {
        Self {
            cursor: PointCursor::new(map, main),
            step: STEP,
        }
    }

    /// Held-key rates, in steps per second: the top speed crosses the box
    /// in about a second along the axis being moved on.
    pub fn rates(&self, held: DirSet, shift: bool) -> Rates {
        let b = self.cursor.bounds;
        let across = |dirs: [Dir; 2], size: i32| {
            if dirs.iter().any(|&d| held.contains(d)) {
                f64::from(size)
            } else {
                0.0
            }
        };
        let extent = across([Dir::Left, Dir::Right], b.w).max(across([Dir::Up, Dir::Down], b.h));
        let top = (extent / CROSS).clamp(TOP_MIN, TOP_MAX);
        let start = START_PX.min(top);
        let step = f64::from(self.step * if shift { BIG } else { 1 });
        Rates {
            start: start / step,
            top: top / step,
        }
    }
}

impl Session {
    /// The top-left corner of a pixel, logical.
    fn pixel_corner(&self, p: PixelPoint) -> LogicalPoint {
        let c = self.pixels.to_logical(p);
        let half = 0.5 / self.pixels.scale();
        LogicalPoint::new(c.x - half, c.y - half)
    }

    /// `w b } {`: the next window or monitor edge that way, else the box's.
    fn insert_jump(&self, m: &mut InsertMode, d: Dir) {
        let c = &mut m.cursor;
        let at = self.pixels.to_logical(c.at);
        let axis = d.axis();
        let edge = c.box_edge(d);
        let target = match self.snap.edges.next(axis, at.get(axis), d.sign(), GAP) {
            None => edge,
            Some(t) => {
                let (px, py) = self.pixels.to_device(LogicalPoint::new(t, t));
                let v = match axis {
                    crate::geometry::Axis::X => px,
                    crate::geometry::Axis::Y => py,
                };
                // The pixel just inside the edge it's heading for.
                let p = v.floor() as i32 - i32::from(d.sign() > 0.0);
                if f64::from(p - edge) * d.sign() > 0.0 {
                    edge
                } else {
                    p
                }
            }
        };
        match axis {
            crate::geometry::Axis::X => c.put(target, c.at.y, true),
            crate::geometry::Axis::Y => c.put(c.at.x, target, true),
        }
    }

    pub(crate) fn insert_intent(&mut self, m: &mut InsertMode, intent: Intent) -> Transition {
        let shift = self.input_mods().shift;
        let step = m.step;
        let c = &mut m.cursor;
        match intent {
            Intent::Steer(d) => {
                let (dx, dy) = vec(d);
                c.shift(dx * step, dy * step, true);
            }
            Intent::Translate(d) => {
                let (dx, dy) = vec(d);
                c.shift(dx * step * BIG, dy * step * BIG, true);
            }
            Intent::Scale(d, Scale::Half) => {
                // Halfway to the box edge that way.
                let e = c.box_edge(d);
                let half = |v: i32| rnd(f64::from(v + e) / 2.0);
                match d.axis() {
                    crate::geometry::Axis::X => c.put(half(c.at.x), c.at.y, true),
                    crate::geometry::Axis::Y => c.put(c.at.x, half(c.at.y), true),
                }
            }
            Intent::HeldSteer(dirs, n) | Intent::HeldTranslate(dirs, n) => {
                let k = step * n as i32 * if shift { BIG } else { 1 };
                let (dx, dy) = vector(dirs);
                c.shift(dx * k, dy * k, false);
            }
            Intent::EdgeJump(d) => self.insert_jump(m, d),
            Intent::FarEdge(d) => {
                let e = c.box_edge(d);
                match d.axis() {
                    crate::geometry::Axis::X => c.put(e, c.at.y, true),
                    crate::geometry::Axis::Y => c.put(c.at.x, e, true),
                }
            }
            Intent::Middle => {
                let b = c.bounds;
                c.put(b.x + b.w / 2, b.y + b.h / 2, true);
            }
            Intent::Adjust(n) => m.step = (m.step + n).clamp(STEP_MIN, STEP_MAX),
            Intent::Undo => c.back(),
            Intent::Press(at) => {
                let p = self.pixels.pixel_under(at);
                m.cursor.put(p.x, p.y, true);
            }
            Intent::Loupe => {
                let main = self.sel.rect();
                return Transition::Push(Mode::Loupe(LoupeMode::new(
                    &self.pixels,
                    main,
                    Some(c.at),
                )));
            }
            Intent::Enhance => {
                let at = self.pixels.to_logical(c.at);
                return Transition::Push(Mode::Enhance(EnhanceMode::new(self.sel.rect(), at)));
            }
            Intent::Tool(t) => {
                // A censor's top-left on the cursor's pixel; others at the box.
                let corner = self.pixel_corner(m.cursor.at);
                return Transition::OpenTool(
                    self.tool_mode_at(t, crate::session::Place::Corner(corner)),
                );
            }
            Intent::CopyColour => {
                let pixel = c.at;
                return Transition::Finish(Outcome::Done(FinishedAction::Colour { pixel }));
            }
            Intent::Confirm | Intent::Back => return Transition::Pop,
            _ => {}
        }
        Transition::Stay
    }
}
