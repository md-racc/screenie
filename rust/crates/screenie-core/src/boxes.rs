//! The box grammar, shared by every mode with a box (Select, Censor, and
//! later Filter and Shape): the modifier grammar on hjkl, the box keys,
//! drag to draw. What a click means, and what the box is for, belong to
//! the mode.

use crate::anchor_cursor::AnchorCursor;
use crate::geometry::LogicalPoint;
use crate::input::Rates;
use crate::intent::{Intent, Scale};
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drag {
    from: LogicalPoint,
    moved: bool,
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
            | Press(_)
    )
}

/// Apply a box intent to `sel`, with jumps resolved against `snap`.
pub fn apply(
    sel: &mut AnchorCursor,
    snap: &SnapEnv,
    drag: &mut Option<Drag>,
    intent: Intent,
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
            *drag = Some(Drag {
                from: at,
                moved: false,
            });
        }
        Motion(at) => {
            if let Some(d) = drag {
                let (dx, dy) = (at.x - d.from.x, at.y - d.from.y);
                if d.moved || dx.abs() + dy.abs() > DRAG_SLOP {
                    d.moved = true;
                    // Unrounded: the crop snaps to pixels, nothing coarser.
                    sel.drag(d.from, at);
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
