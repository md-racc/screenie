//! Annotate (`a`) and its Shape child: visible markup in the open layer.
//!
//! The pen is the default: a drag draws a freehand line, locked in when
//! the button comes up. `1`-`5` pick an ink, `p` picks up the colour under
//! the pointer, `+`/`-` set the width (held, they repeat). `r` and `c` open
//! a rectangle or circle as a box of its own (Shape), edited with the box
//! grammar; `a` there locks it and returns to the pen, Esc drops it.
//!
//! The ink (colour and width) belongs to the session, so the pen and
//! shapes share it, and it lasts across layers.

use crate::anchor_cursor::AnchorCursor;
use crate::boxes::{self, BoxOutcome, Drag};
use crate::geometry::{LogicalPoint, LogicalRect};
use crate::intent::{Intent, Tool};
use crate::model::{OpData, Rgb, ShapeKind, ShapeOp, StrokeOp};
use crate::session::{Mode, Session, Transition};

/// The ink palette, by key: one declaration for keys, palette and colour.
pub const INKS: [(char, &str, Rgb); 5] = [
    ('1', "white", [1.0, 1.0, 1.0]),
    ('2', "black", [0.0, 0.0, 0.0]),
    ('3', "red", [0.9, 0.12, 0.12]),
    ('4', "green", [0.1, 0.75, 0.25]),
    ('5', "blue", [0.15, 0.45, 1.0]),
];
/// Red.
pub const DEFAULT_INK: usize = 2;
pub const WIDTH: f64 = 3.0;
pub const WIDTH_MIN: f64 = 1.0;
pub const WIDTH_MAX: f64 = 40.0;

/// The colour annotations are drawn in: a palette entry, or a colour
/// picked up from the picture (named by its hex).
#[derive(Clone, Debug, PartialEq)]
pub struct Ink {
    pub name: String,
    pub rgb: Rgb,
}

impl Ink {
    pub fn palette(i: usize) -> Ink {
        let (_, name, rgb) = INKS[i];
        Ink {
            name: name.into(),
            rgb,
        }
    }

    pub fn picked(c: [u8; 3]) -> Ink {
        Ink {
            name: format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]),
            rgb: c.map(|v| f64::from(v) / 255.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AnnotateMode {
    main: LogicalRect,
    /// The last pointer position, for `p`.
    pointer: LogicalPoint,
    /// The line being drawn.
    trace: Option<Vec<LogicalPoint>>,
}

impl AnnotateMode {
    pub fn new(main: LogicalRect) -> Self {
        Self {
            main,
            pointer: main.centre(),
            trace: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShapeMode {
    pub sel: AnchorCursor,
    pub kind: ShapeKind,
    drag: Option<Drag>,
}

impl ShapeMode {
    /// A shape in the middle quarter of `main`.
    pub fn new(main: LogicalRect, kind: ShapeKind) -> Self {
        let r = LogicalRect::new(
            main.x + main.w / 4.0,
            main.y + main.h / 4.0,
            main.w / 2.0,
            main.h / 2.0,
        );
        Self {
            sel: AnchorCursor::new(r, main),
            kind,
            drag: None,
        }
    }
}

impl Session {
    pub(crate) fn stroke(&self, points: &[LogicalPoint]) -> Option<OpData> {
        (!points.is_empty()).then(|| {
            OpData::Stroke(StrokeOp {
                points: points.to_vec(),
                rgb: self.ink.rgb,
                width: self.ink_width,
            })
        })
    }

    pub(crate) fn shape(&self, s: &ShapeMode) -> Option<OpData> {
        (!s.sel.is_empty()).then(|| {
            OpData::Shape(ShapeOp {
                kind: s.kind,
                rect: s.sel.rect(),
                rgb: self.ink.rgb,
                width: self.ink_width,
            })
        })
    }

    /// Colour and width keys, shared by the pen and shapes.
    fn ink_intent(&mut self, intent: Intent) -> bool {
        match intent {
            Intent::Choose(n) if (1..=INKS.len() as u8).contains(&n) => {
                self.ink = Ink::palette(usize::from(n) - 1);
            }
            Intent::Adjust(n) => {
                self.ink_width = (self.ink_width + f64::from(n)).clamp(WIDTH_MIN, WIDTH_MAX);
            }
            _ => return false,
        }
        true
    }

    /// `p`: the colour under a point becomes the ink.
    fn eyedrop(&mut self, at: LogicalPoint) {
        let px = self.pixels.pixel_under(at);
        if let Some(c) = self.sample(px) {
            self.ink = Ink::picked(c);
        }
    }

    /// The build tool a tool key opens, in this layer.
    pub(crate) fn tool_mode(&self, tool: Tool) -> Mode {
        self.tool_mode_at(tool, crate::session::Place::Default)
    }

    /// The build tool a tool key opens, placed (a censor goes where it's
    /// asked; the pen and filters take the whole box).
    pub(crate) fn tool_mode_at(&self, tool: Tool, place: crate::session::Place) -> Mode {
        use crate::session::Place;
        let main = self.sel.rect();
        match tool {
            Tool::Censor => Mode::Censor(match place {
                Place::Default => crate::censor::CensorMode::at(main, main.centre()),
                Place::Centre(at) => crate::censor::CensorMode::at(main, at),
                Place::Corner(at) => crate::censor::CensorMode::from_corner(main, at),
            }),
            Tool::Annotate => Mode::Annotate(AnnotateMode::new(main)),
            Tool::Filter => Mode::Filter(crate::filter::FilterMode::new(main)),
        }
    }

    pub(crate) fn annotate_intent(&mut self, a: &mut AnnotateMode, intent: Intent) -> Transition {
        if self.ink_intent(intent) {
            return Transition::Stay;
        }
        match intent {
            Intent::Press(p) => a.trace = Some(vec![p]),
            Intent::Motion(p) => {
                a.pointer = p;
                if let Some(t) = &mut a.trace {
                    t.push(p);
                }
            }
            Intent::Release(p) => {
                if let Some(mut t) = a.trace.take() {
                    if t.last() != Some(&p) {
                        t.push(p); // right up to where it was let go
                    }
                    self.lock(self.stroke(&t));
                }
            }
            Intent::Eyedropper => self.eyedrop(a.pointer),
            Intent::Loupe | Intent::Enhance => {
                return self.lens_at(intent, a.pointer).expect("a lens");
            }
            Intent::Shape(kind) => {
                return Transition::Push(Mode::Shape(ShapeMode::new(a.main, kind)));
            }
            // A line still being drawn is kept, whatever ends it.
            Intent::Tool(Tool::Annotate) => self.lock_trace(a),
            Intent::Tool(other) => {
                self.lock_trace(a);
                return Transition::SwitchTool(self.tool_mode(other));
            }
            Intent::Undo => self.undo_layer(),
            Intent::Confirm => {
                self.lock_trace(a);
                return self.close_layer(true);
            }
            Intent::Back => return self.back_from_tool(),
            _ => {}
        }
        Transition::Stay
    }

    fn lock_trace(&mut self, a: &mut AnnotateMode) {
        if let Some(t) = a.trace.take() {
            self.lock(self.stroke(&t));
        }
    }

    pub(crate) fn shape_intent(&mut self, s: &mut ShapeMode, intent: Intent) -> Transition {
        if intent == Intent::Eyedropper {
            self.eyedrop(s.sel.cursor_point()); // at the shape's cursor
            return Transition::Stay;
        }
        if self.ink_intent(intent) {
            return Transition::Stay;
        }
        match intent {
            Intent::Tool(Tool::Annotate) => {
                // Lock it, back to the pen for the next.
                self.lock(self.shape(s));
                return Transition::Pop;
            }
            Intent::Back => return Transition::Pop, // drop just this shape
            Intent::Tool(other) => {
                self.lock(self.shape(s));
                return Transition::SwitchTool(self.tool_mode(other));
            }
            Intent::Undo => {
                self.undo_layer();
                return Transition::Stay;
            }
            Intent::Confirm => {
                self.lock(self.shape(s));
                return self.close_layer(true);
            }
            _ => {}
        }
        if let Some(t) = self.lens_at(intent, s.sel.rect().centre()) {
            return t;
        }
        match boxes::apply(&mut s.sel, &self.snap, &mut s.drag, intent) {
            BoxOutcome::Done => return Transition::Stay,
            BoxOutcome::Click(at) => {
                s.sel.place_cursor(at);
                return Transition::Stay;
            }
            BoxOutcome::NotMine => {}
        }
        if intent == Intent::CycleKind {
            s.kind = match s.kind {
                ShapeKind::Rect => ShapeKind::Circle,
                ShapeKind::Circle => ShapeKind::Rect,
            };
        }
        Transition::Stay
    }

    /// The pen's line in progress, or the shape being edited.
    pub(crate) fn annotate_editing(&self, mode: &Mode) -> Option<OpData> {
        match mode {
            Mode::Annotate(a) => a.trace.as_deref().and_then(|t| self.stroke(t)),
            Mode::Shape(s) => self.shape(s),
            _ => None,
        }
    }
}
