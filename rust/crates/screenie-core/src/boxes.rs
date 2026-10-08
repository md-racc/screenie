//! The box grammar, shared by every mode with a box (Select, Censor, and
//! later Filter and Shape): the modifier grammar on hjkl, the box keys,
//! drag to draw. What a click means, and what the box is for, belong to
//! the mode.

use crate::anchor_cursor::AnchorCursor;
use crate::geometry::{Axis, End, LogicalPoint};
use crate::input::Rates;
use crate::intent::{Intent, Scale};
use crate::keys::Mods;
use crate::targets::SnapEnv;

/// A box step (a tap), in logical pixels.
pub const STEP: f64 = 5.0;

/// Box repeat speed, in steps per second: at the start, and at the top.
pub const RATES: Rates = Rates {
    start: 20.0,
    top: 300.0,
};

/// A press that moves further than this (Manhattan, logical px) is a drag.
pub const DRAG_SLOP: f64 = 3.0;

/// How near the box's edge a press takes hold of it, logical px.
pub const GRAB: f64 = 6.0;

/// What a press takes hold of (docs/mouse.md).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Handle {
    /// Nothing: a drag draws a new box.
    #[default]
    None,
    /// The whole box: Alt+drag inside it moves it.
    Box,
    /// Its edges, per axis `[x, y]`: an edge, or (both) a corner.
    Edges([Option<End>; 2]),
}

/// What a press at `at` takes hold of: with Alt, inside the box, the box;
/// within `GRAB` of an edge (inside or out), that edge, and of a corner,
/// both.
pub fn handle_at(sel: &AnchorCursor, at: LogicalPoint, mods: Mods) -> Handle {
    let r = sel.rect();
    if mods.alt {
        return if r.contains(at) {
            Handle::Box
        } else {
            Handle::None
        };
    }
    // The nearer end of an axis within reach, if `along` is beside the box.
    let near = |p: f64, lo: f64, hi: f64, along: f64, a_lo: f64, a_hi: f64| {
        if along < a_lo - GRAB || along > a_hi + GRAB {
            return None;
        }
        let (dl, dh) = ((p - lo).abs(), (p - hi).abs());
        match (dl <= GRAB, dh <= GRAB) {
            (true, true) if dh < dl => Some(End::High),
            (true, _) => Some(End::Low),
            (_, true) => Some(End::High),
            _ => None,
        }
    };
    let x = near(at.x, r.x, r.right(), at.y, r.y, r.bottom());
    let y = near(at.y, r.y, r.bottom(), at.x, r.x, r.right());
    if x.is_none() && y.is_none() {
        Handle::None
    } else {
        Handle::Edges([x, y])
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drag {
    from: LogicalPoint,
    moved: bool,
    handle: Handle,
    /// A held edge's distance from the pointer, `[x, y]`, kept as it moves.
    offset: [f64; 2],
    /// Where the pointer was last (moving the box).
    last: LogicalPoint,
}

/// What the box made of an intent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxOutcome {
    /// Not a box intent: the mode's to handle.
    NotMine,
    /// Handled.
    Done,
    /// A click (a press and release without a drag), for the mode.
    Click(LogicalPoint),
}

/// Whether an intent starts a new change to the box (Select records the
/// box before it in project history). Repeats and drag motion continue
/// the change their press started.
pub fn starts_change(intent: Intent) -> bool {
    use Intent::*;
    matches!(
        intent,
        Steer(_)
            | Translate(_)
            | Jump(_)
            | Slide(_)
            | Scale(..)
            | EdgeJump(_)
            | FarEdge(_)
            | PictureEdge(_)
            | Press(_)
    )
}

impl Drag {
    /// What the press took hold of.
    pub fn handle(&self) -> Handle {
        self.handle
    }
}

/// Apply a box intent to `sel`, with jumps resolved against `snap`.
/// With `handles` (the modifiers held), a press can take hold of the box
/// or its edges; without, a drag only ever draws a new box.
pub fn apply(
    sel: &mut AnchorCursor,
    snap: &SnapEnv,
    drag: &mut Option<Drag>,
    intent: Intent,
    handles: Option<Mods>,
) -> BoxOutcome {
    use Intent::*;
    match intent {
        Steer(d) => sel.step(d, STEP, false),
        Translate(d) => sel.step(d, STEP, true),
        Jump(d) => snap.neighbour(sel, d),
        Slide(d) => snap.slide(sel, d),
        Scale(d, self::Scale::Half) => sel.halve(d),
        Scale(d, self::Scale::Double) => sel.double(d),
        HeldSteer(dirs, n) => {
            for d in dirs.iter() {
                sel.step(d, f64::from(n) * STEP, false);
            }
        }
        HeldTranslate(dirs, n) => {
            let (dx, dy) = dirs.vector();
            let px = f64::from(n) * STEP;
            sel.translate(dx * px, dy * px);
        }
        SwapOpposite => sel.swap_corner(true),
        SwapSideways => sel.swap_corner(false),
        EdgeJump(d) => snap.cursor_jump(sel, d, false),
        FarEdge(d) => snap.cursor_jump(sel, d, true),
        ToggleCentre => sel.toggle_centre(),
        Press(at) => {
            let handle = handles.map_or(Handle::None, |m| handle_at(sel, at, m));
            let r = sel.rect();
            let mut offset = [0.0; 2];
            if let Handle::Edges([x, y]) = handle {
                if let Some(e) = x {
                    offset[0] = if e == End::Low { r.x } else { r.right() } - at.x;
                }
                if let Some(e) = y {
                    offset[1] = if e == End::Low { r.y } else { r.bottom() } - at.y;
                }
            }
            *drag = Some(Drag {
                from: at,
                moved: false,
                handle,
                offset,
                last: at,
            });
        }
        Motion(at) => {
            if let Some(d) = drag {
                let (dx, dy) = (at.x - d.from.x, at.y - d.from.y);
                if d.moved || dx.abs() + dy.abs() > DRAG_SLOP {
                    let first = !d.moved;
                    d.moved = true;
                    // Unrounded: the crop snaps to pixels, nothing coarser.
                    match d.handle {
                        Handle::None => sel.drag(d.from, at),
                        Handle::Box => sel.translate(at.x - d.last.x, at.y - d.last.y),
                        Handle::Edges(ends) => {
                            let held = [(Axis::X, ends[0], at.x), (Axis::Y, ends[1], at.y)];
                            for (i, (axis, end, p)) in held.into_iter().enumerate() {
                                let Some(end) = end else { continue };
                                if first {
                                    // Not on the press: a click is still the mode's.
                                    sel.take_cursor(axis, end);
                                }
                                sel.cursor_to(axis, p + d.offset[i]);
                            }
                        }
                    }
                    d.last = at;
                }
            }
        }
        Release(at) => {
            if let Some(Drag { moved: false, .. }) = drag.take() {
                return BoxOutcome::Click(at);
            }
        }
        _ => return BoxOutcome::NotMine,
    }
    BoxOutcome::Done
}
