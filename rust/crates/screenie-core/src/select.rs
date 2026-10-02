//! Select: the base mode. Edit the box, open tools, finish.

use crate::boxes::{self, BoxOutcome, Drag};
use crate::geometry::LogicalRect;
use crate::intent::Intent;
use crate::session::{FinishedAction, Outcome, Session, Transition};

pub use crate::boxes::RATES;

#[derive(Clone, Debug, Default)]
pub(crate) struct SelectState {
    pub(crate) drag: Option<Drag>,
    /// The ring index `.` last gave, and the box it gave: `.` again, while
    /// the box is unchanged, steps to an older one.
    recalled: Option<(usize, LogicalRect)>,
}

impl Session {
    /// Whether the box covers at least one output pixel each way.
    pub(crate) fn capturable(&self) -> bool {
        !self.pixels.to_pixels(self.sel.rect()).is_empty()
    }

    pub(crate) fn select_intent(&mut self, intent: Intent) -> Transition {
        if boxes::starts_change(intent) {
            self.remember();
        }
        match boxes::apply(&mut self.sel, &self.snap, &mut self.select.drag, intent) {
            BoxOutcome::Done => return Transition::Stay,
            BoxOutcome::Click(at) => {
                // A click snaps to the window under the pointer, or the
                // nearest monitor.
                if let Some(r) = self.snap.click_target(at) {
                    self.sel.set_rect(r);
                }
                return Transition::Stay;
            }
            BoxOutcome::NotMine => {}
        }
        let needs_box = matches!(
            intent,
            Intent::Confirm | Intent::Ocr | Intent::DecodeCode | Intent::Pin | Intent::Tool(_)
        );
        if needs_box && !self.capturable() {
            self.note = Some("empty box".into());
            return Transition::Stay;
        }
        match intent {
            Intent::Undo => self.undo(),
            Intent::Adjust(step) => {
                self.remember();
                self.snap.scope(&mut self.sel, step);
            }
            Intent::Recall => self.recall(),
            Intent::Choose(n) => {
                // The layer view, at that layer.
                if self.project.layers.is_empty() {
                    self.note = Some("no layers yet".into());
                } else {
                    let m = self.layers_at(usize::from(n).saturating_sub(1));
                    return Transition::Push(crate::session::Mode::Layers(m));
                }
            }
            Intent::Tool(tool) => return Transition::OpenTool(self.tool_mode(tool)),
            Intent::Confirm | Intent::Ocr | Intent::DecodeCode | Intent::Pin => {
                let rect = self.sel.rect();
                return Transition::Finish(Outcome::Done(match intent {
                    Intent::Ocr => FinishedAction::Ocr { rect },
                    Intent::DecodeCode => FinishedAction::Code { rect },
                    Intent::Pin => FinishedAction::Pin { rect },
                    _ => FinishedAction::Image { rect },
                }));
            }
            Intent::CopyColour => {
                let pixel = self.cursor_pixel().expect("Select has a cursor");
                return Transition::Finish(Outcome::Done(FinishedAction::Colour { pixel }));
            }
            Intent::Back => return Transition::Finish(Outcome::Cancelled),
            _ => {}
        }
        Transition::Stay
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
