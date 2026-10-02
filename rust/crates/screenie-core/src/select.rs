//! Select: the base mode. Edit the box, then finish.

use crate::geometry::{LogicalPoint, LogicalRect};
use crate::input::Rates;
use crate::intent::{Intent, Scale};
use crate::session::{FinishedAction, Outcome, STEP, Session};

/// Box repeat speed, in steps per second: at the start, and at the top.
pub const RATES: Rates = Rates {
    start: 20.0,
    top: 300.0,
};

/// A press that moves further than this (Manhattan, logical px) is a drag.
pub const DRAG_SLOP: f64 = 3.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Drag {
    from: LogicalPoint,
    moved: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SelectState {
    drag: Option<Drag>,
    /// The ring index `.` last gave, and the box it gave: `.` again, while
    /// the box is unchanged, steps to an older one.
    recalled: Option<(usize, LogicalRect)>,
}

impl Session {
    /// Whether the box covers at least one output pixel each way.
    pub(crate) fn capturable(&self) -> bool {
        !self.pixels.to_pixels(self.sel.rect()).is_empty()
    }

    pub(crate) fn select_intent(&mut self, intent: Intent) {
        use Intent::*;
        match intent {
            Steer(d) => {
                self.remember();
                self.sel.step(d, STEP, false);
            }
            Translate(d) => {
                self.remember();
                self.sel.step(d, STEP, true);
            }
            Jump(d) => {
                self.remember();
                self.snap.neighbour(&mut self.sel, d);
            }
            Slide(d) => {
                self.remember();
                self.snap.slide(&mut self.sel, d);
            }
            Scale(d, k) => {
                self.remember();
                match k {
                    self::Scale::Half => self.sel.halve(d),
                    self::Scale::Double => self.sel.double(d),
                }
            }
            // Repeats continue the press that started them: no new entry.
            HeldSteer(dirs, n) => {
                for d in dirs.iter() {
                    self.sel.step(d, f64::from(n) * STEP, false);
                }
            }
            HeldTranslate(dirs, n) => {
                let (dx, dy) = dirs.vector();
                let px = f64::from(n) * STEP;
                self.sel.translate(dx * px, dy * px);
            }
            SwapOpposite => self.sel.swap_corner(true),
            SwapSideways => self.sel.swap_corner(false),
            EdgeJump(d) => {
                self.remember();
                self.snap.cursor_jump(&mut self.sel, d, false);
            }
            FarEdge(d) => {
                self.remember();
                self.snap.cursor_jump(&mut self.sel, d, true);
            }
            ToggleCentre => self.sel.toggle_centre(),
            Undo => self.undo(),
            Adjust(step) => {
                self.remember();
                self.snap.scope(&mut self.sel, step);
            }
            Recall => self.recall(),
            Confirm | Ocr | DecodeCode | Pin => {
                if !self.capturable() {
                    self.note = Some("empty box".into());
                    return;
                }
                let rect = self.sel.rect();
                self.finish(Outcome::Done(match intent {
                    Ocr => FinishedAction::Ocr { rect },
                    DecodeCode => FinishedAction::Code { rect },
                    Pin => FinishedAction::Pin { rect },
                    _ => FinishedAction::Image { rect },
                }));
            }
            CopyColour => {
                let pixel = self.cursor_pixel();
                self.finish(Outcome::Done(FinishedAction::Colour { pixel }));
            }
            Back => self.finish(Outcome::Cancelled),
            Press(at) => {
                self.remember();
                self.select.drag = Some(Drag {
                    from: at,
                    moved: false,
                });
            }
            Motion(at) => {
                let Some(drag) = &mut self.select.drag else {
                    return;
                };
                let (dx, dy) = (at.x - drag.from.x, at.y - drag.from.y);
                if drag.moved || dx.abs() + dy.abs() > DRAG_SLOP {
                    drag.moved = true;
                    // Unrounded: the crop snaps to pixels, nothing coarser.
                    let from = drag.from;
                    self.sel.drag(from, at);
                }
            }
            Release(at) => {
                if let Some(Drag { moved: false, .. }) = self.select.drag.take() {
                    // A click snaps to the window under the pointer, or
                    // the nearest monitor.
                    if let Some(r) = self.snap.click_target(at) {
                        self.sel.set_rect(r);
                    }
                }
            }
            Quit | ToggleHelp => {}
        }
    }

    /// `.`: the last accepted box; `.` again, while the box is still the
    /// one it gave, steps back to older ones, and round.
    fn recall(&mut self) {
        let n = self.recall.len();
        if n == 0 {
            self.note = Some("no last selection".into());
            return;
        }
        let i = match self.select.recalled {
            Some((i, r)) if self.sel.rect().nearly(&r) => (i + 1) % n,
            _ => 0,
        };
        self.remember();
        self.sel.leave_centre();
        self.sel.set_rect(self.recall[i]);
        self.select.recalled = Some((i, self.sel.rect()));
        self.note = Some(if n > 1 {
            format!("last {}/{n}", i + 1)
        } else {
            "last".into()
        });
    }
}
