//! Censor (`x`): obscuring rectangles inside the main box, in the open
//! layer.
//!
//! The censor box starts as a text-height square at the centre of the main
//! box and is edited with the box grammar. `t` swaps pixelate and bar;
//! `+`/`-` adjust the current kind's amount (each kind keeps its own); `x`
//! locks this censor and starts the next where it was.

use crate::anchor_cursor::AnchorCursor;
use crate::boxes::{self, BoxOutcome, Drag};
use crate::geometry::{LogicalPoint, LogicalRect};
use crate::intent::{Intent, Tool};
use crate::model::{CensorKind, CensorOp, OpData};
use crate::session::{Session, Transition};

/// The starting square, logical px: about a line of UI text.
pub const SIZE: f64 = 20.0;
/// The default pixelate block size, logical px.
pub const PIXELATE_BLOCK: f64 = 10.0;

/// `+`/`-` per kind: step, min, max.
fn range(kind: CensorKind) -> (f64, f64, f64) {
    match kind {
        CensorKind::Pixelate => (2.0, 4.0, 40.0),
        CensorKind::Bar => (0.1, 0.0, 1.0),
    }
}

#[derive(Clone, Debug)]
pub struct CensorMode {
    pub sel: AnchorCursor,
    pub kind: CensorKind,
    /// Each kind's own amount, kept across `t` and `x`.
    pub pixelate: f64,
    pub shade: f64,
    drag: Option<Drag>,
}

fn square_at(c: LogicalPoint, main: LogicalRect) -> AnchorCursor {
    AnchorCursor::new(
        LogicalRect::new(c.x - SIZE / 2.0, c.y - SIZE / 2.0, SIZE, SIZE),
        main,
    )
}

impl CensorMode {
    /// A censor in `main`, centred on `at`.
    pub fn at(main: LogicalRect, at: LogicalPoint) -> Self {
        Self {
            sel: square_at(at, main),
            kind: CensorKind::Pixelate,
            pixelate: PIXELATE_BLOCK,
            shade: 0.0,
            drag: None,
        }
    }

    pub fn amount(&self) -> f64 {
        match self.kind {
            CensorKind::Pixelate => self.pixelate,
            CensorKind::Bar => self.shade,
        }
    }

    fn set_amount(&mut self, v: f64) {
        match self.kind {
            CensorKind::Pixelate => self.pixelate = v,
            CensorKind::Bar => self.shade = v,
        }
    }

    /// The censor being edited, as an operation (none while it's empty).
    pub fn current(&self) -> Option<OpData> {
        (!self.sel.is_empty()).then(|| {
            OpData::Censor(CensorOp {
                rect: self.sel.rect(),
                kind: self.kind,
                amount: self.amount(),
            })
        })
    }

    /// "pixelate 10px", "black bar", "grey bar 30%".
    pub fn describe(&self) -> String {
        let v = self.amount();
        match self.kind {
            CensorKind::Pixelate => format!("pixelate {v}px"),
            CensorKind::Bar => match (v * 10.0).round() as i32 {
                0 => "black bar".into(),
                10 => "white bar".into(),
                _ => format!("grey bar {}%", (v * 100.0).round()),
            },
        }
    }
}

impl Session {
    pub(crate) fn censor_intent(&mut self, c: &mut CensorMode, intent: Intent) -> Transition {
        match boxes::apply(&mut c.sel, &self.snap, &mut c.drag, intent) {
            BoxOutcome::Done => return Transition::Stay,
            BoxOutcome::Click(at) => {
                c.sel.place_cursor(at); // the cursor corner, not the whole box
                return Transition::Stay;
            }
            BoxOutcome::NotMine => {}
        }
        match intent {
            Intent::Adjust(n) => {
                let (step, lo, hi) = range(c.kind);
                let v = (c.amount() + step * f64::from(n)).clamp(lo, hi);
                c.set_amount((v * 100.0).round() / 100.0);
            }
            Intent::CycleKind => {
                c.kind = match c.kind {
                    CensorKind::Pixelate => CensorKind::Bar,
                    CensorKind::Bar => CensorKind::Pixelate,
                };
            }
            Intent::Tool(Tool::Censor) => {
                // Lock this one, and start the next where it was.
                let centre = c.sel.rect().centre();
                self.lock(c.current());
                c.sel = square_at(centre, c.sel.bounds());
            }
            Intent::Tool(other) => {
                // Another tool, same layer.
                self.lock(c.current());
                return Transition::SwitchTool(self.tool_mode(other));
            }
            Intent::Undo => self.undo_layer(),
            Intent::Confirm => {
                self.lock(c.current());
                return self.close_layer(true);
            }
            Intent::Back => return self.back_from_tool(),
            _ => {}
        }
        Transition::Stay
    }
}
