//! Text input: a temporary child that suspends command letters. Typed
//! keys are literal text until Enter (accept) or Esc (cancel); the child
//! returns the text, and its parent says what it means (architecture §20).
//!
//! For a text annotation it also previews the text where it will go, and
//! Ctrl keys place and style it: Ctrl+hjkl nudge it 1px (Ctrl+Shift 10px),
//! Ctrl+`+`/`-` size, Ctrl+1-5 ink, Ctrl+b the chip behind it.

use crate::geometry::LogicalPoint;
use crate::intent::Intent;
use crate::model::{OpData, TextOp};
use crate::session::{ChildResult, Session, Transition};

/// Text size, logical px: the start, and the range in steps of 2.
pub const SIZE: f64 = 18.0;
pub const SIZE_MIN: f64 = 8.0;
pub const SIZE_MAX: f64 = 96.0;

/// What the text is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextPurpose {
    /// A text annotation (Annotate's `i`).
    Annotation,
    /// The next callout's number (Annotate's `N`).
    CalloutNumber,
    /// A measurement's real length (Measure's calibration).
    Calibration,
}

#[derive(Clone, Debug)]
pub struct TextMode {
    pub text: String,
    pub purpose: TextPurpose,
    /// An annotation's top-left.
    pub at: LogicalPoint,
    pub size: f64,
    pub chip: bool,
}

impl TextMode {
    pub fn annotation(at: LogicalPoint) -> Self {
        Self {
            text: String::new(),
            purpose: TextPurpose::Annotation,
            at,
            size: SIZE,
            chip: true,
        }
    }

    /// Typed at a measurement's chip.
    pub fn calibration(at: LogicalPoint) -> Self {
        Self {
            purpose: TextPurpose::Calibration,
            ..Self::annotation(at)
        }
    }

    pub fn callout_number() -> Self {
        Self {
            purpose: TextPurpose::CalloutNumber,
            ..Self::annotation(LogicalPoint::default())
        }
    }
}

impl Session {
    pub(crate) fn text_op(&self, t: &TextMode, text: &str) -> TextOp {
        TextOp {
            origin: t.at,
            text: text.to_string(),
            rgb: self.ink.rgb,
            size: t.size,
            chip: t.chip,
        }
    }

    /// What a text annotation will look like, a caret at its end.
    pub(crate) fn text_preview(&self, t: &TextMode) -> Option<OpData> {
        (t.purpose == TextPurpose::Annotation)
            .then(|| OpData::Text(self.text_op(t, &format!("{}|", t.text))))
    }

    pub(crate) fn text_intent(&mut self, t: &mut TextMode, intent: Intent) -> Transition {
        let styled = t.purpose == TextPurpose::Annotation;
        match intent {
            Intent::Type(c) => t.text.push(c),
            Intent::Newline if styled => t.text.push('\n'),
            Intent::Backspace => {
                t.text.pop();
            }
            Intent::Confirm => {
                return Transition::PopWith(ChildResult::Text(t.text.clone(), t.clone()));
            }
            Intent::Back => return Transition::Pop,
            Intent::Jump(d) | Intent::Slide(d) if styled => {
                let k = if matches!(intent, Intent::Slide(_)) {
                    10.0
                } else {
                    1.0
                };
                let (dx, dy) = d.vec();
                t.at = LogicalPoint::new(t.at.x + dx * k, t.at.y + dy * k);
            }
            Intent::Adjust(n) if styled => {
                t.size = (t.size + 2.0 * f64::from(n)).clamp(SIZE_MIN, SIZE_MAX);
            }
            Intent::Choose(n) if styled && (1..=5).contains(&n) => {
                self.ink = crate::annotate::Ink::palette(usize::from(n) - 1);
            }
            Intent::ToggleChip if styled => t.chip = !t.chip,
            _ => {}
        }
        Transition::Stay
    }

    pub(crate) fn text_status(&self, t: &TextMode) -> String {
        match t.purpose {
            TextPurpose::Annotation => {
                let chip = if t.chip { "  chip" } else { "" };
                format!("{} {}px{chip}", self.ink.name, t.size)
            }
            TextPurpose::CalloutNumber => format!("next callout number: {}|", t.text),
            TextPurpose::Calibration => {
                format!("= {}|  (Enter sets the scale, Esc stays in pixels)", t.text)
            }
        }
    }
}
