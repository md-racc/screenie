//! Select: the base mode. Edit the box, open tools, finish.

use crate::boxes::{self, BoxOutcome, Drag};
use crate::geometry::LogicalRect;
use crate::intent::Intent;
use crate::session::{FinishedAction, Outcome, Session, Transition};

pub use crate::boxes::RATES;

/// What Enter produces in Select (`--result`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnterResult {
    #[default]
    Image,
    Text,
    Colour,
    Code,
    Pin,
    /// Enter opens Measure, where Enter delivers the measurements.
    Measure,
}

impl EnterResult {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "image" => Self::Image,
            "text" => Self::Text,
            "colour" | "color" => Self::Colour,
            "code" => Self::Code,
            "pin" => Self::Pin,
            "measure" => Self::Measure,
            _ => return None,
        })
    }
}

/// The mode the picker opens in (`--mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMode {
    Select,
    Insert,
    Censor,
    Annotate,
    Filter,
    Measure,
}

impl StartMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "select" => Self::Select,
            "insert" => Self::Insert,
            "censor" => Self::Censor,
            "annotate" => Self::Annotate,
            "filter" => Self::Filter,
            "measure" => Self::Measure,
            _ => return None,
        })
    }
}

/// Intents that would change the box (`--lock-box` refuses them).
fn changes_box(intent: Intent) -> bool {
    boxes::starts_change(intent)
        || matches!(
            intent,
            Intent::HeldSteer(..)
                | Intent::HeldTranslate(..)
                | Intent::Adjust(_)
                | Intent::Recall
                | Intent::ToggleCentre
        )
}

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
        // `--result`: what Enter produces.
        let intent = match (intent, self.enter) {
            (Intent::Confirm, EnterResult::Text) => Intent::Ocr,
            (Intent::Confirm, EnterResult::Colour) => Intent::CopyColour,
            (Intent::Confirm, EnterResult::Code) => Intent::DecodeCode,
            (Intent::Confirm, EnterResult::Pin) => Intent::Pin,
            (Intent::Confirm, EnterResult::Measure) => Intent::Measure(crate::measure::Prim::Line),
            (i, _) => i,
        };
        if self.box_locked && changes_box(intent) {
            self.note = Some("the box is locked".into());
            return Transition::Stay;
        }
        if boxes::starts_change(intent) {
            self.remember();
        }
        let mods = Some(self.input_mods());
        match boxes::apply(
            &mut self.sel,
            &self.snap,
            &mut self.select.drag,
            intent,
            mods,
        ) {
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
        // The loupe reopens where it was left (clamped into the box).
        let lens_at = match (intent, self.loupe_at) {
            (Intent::Loupe, Some(px)) => self.pixels.to_logical(px),
            _ => self.sel.rect().centre(),
        };
        if let Some(t) = self.lens_at(intent, lens_at) {
            return t;
        }
        let needs_box = matches!(
            intent,
            Intent::Confirm
                | Intent::Ocr
                | Intent::DecodeCode
                | Intent::Pin
                | Intent::Tool(_)
                | Intent::Insert
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
            Intent::Measure(_) => return self.open_measure(),
            Intent::Insert => {
                let m = crate::insert::InsertMode::new(&self.pixels, self.sel.rect());
                return Transition::Push(crate::session::Mode::Insert(m));
            }
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
            Intent::DecodeCode => {
                let rect = self.sel.rect();
                match self.read_code(rect) {
                    Ok(text) => {
                        return Transition::Finish(Outcome::Done(FinishedAction::Code {
                            rect,
                            text,
                        }));
                    }
                    // Nothing there: say so, and stay (adjust the box, retry).
                    Err(why) => self.note = Some(why),
                }
            }
            Intent::Confirm | Intent::Ocr | Intent::Pin => {
                let rect = self.sel.rect();
                return Transition::Finish(Outcome::Done(match intent {
                    Intent::Ocr => FinishedAction::Ocr { rect },
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

    /// `--mode`: open in that mode, as if its key had been pressed.
    pub fn start_in(&mut self, mode: StartMode) {
        use crate::intent::Tool;
        let intent = match mode {
            StartMode::Select => return,
            StartMode::Insert => Intent::Insert,
            StartMode::Censor => Intent::Tool(Tool::Censor),
            StartMode::Annotate => Intent::Tool(Tool::Annotate),
            StartMode::Filter => Intent::Tool(Tool::Filter),
            StartMode::Measure => Intent::Measure(crate::measure::Prim::Line),
        };
        self.dispatch(intent);
    }

    /// `--result`: what Enter produces in Select.
    pub fn set_result(&mut self, r: EnterResult) {
        self.enter = r;
    }

    /// `--lock-box`: the box can't be changed, only worked inside.
    pub fn lock_box(&mut self) {
        self.box_locked = true;
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
