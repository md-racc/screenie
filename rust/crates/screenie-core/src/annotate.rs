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
use crate::model::{Heads, LineOp, OpData, Rgb, ShapeKind, ShapeOp, StrokeOp};

/// What a Shape box draws: an outline (rect, circle), or the anchor to
/// cursor as a line or arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeTool {
    Rect,
    Circle,
    Line,
    Arrow,
}

impl ShapeTool {
    pub fn name(self) -> &'static str {
        match self {
            ShapeTool::Rect => "rect",
            ShapeTool::Circle => "circle",
            ShapeTool::Line => "line",
            ShapeTool::Arrow => "arrow",
        }
    }

    /// `t`: rect, circle, line, arrow, and round.
    fn next(self) -> ShapeTool {
        match self {
            ShapeTool::Rect => ShapeTool::Circle,
            ShapeTool::Circle => ShapeTool::Line,
            ShapeTool::Line => ShapeTool::Arrow,
            ShapeTool::Arrow => ShapeTool::Rect,
        }
    }
}
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
/// The highlighter: this many times the ink width, at this opacity.
pub const HIGHLIGHT_SCALE: f64 = 5.0;
pub const HIGHLIGHT_ALPHA: f64 = 0.4;

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
    /// The pen highlights instead of inking.
    pub highlighter: bool,
    /// The next callout's number, when set with `N`.
    next_callout: Option<u32>,
    /// The pen point was moved by the keyboard (so it's drawn), and the
    /// line in progress is the keyboard's.
    pub keyboard: bool,
}

impl AnnotateMode {
    pub fn pointer(&self) -> LogicalPoint {
        self.pointer
    }

    /// Whether a line is being drawn.
    pub fn drawing(&self) -> bool {
        self.trace.is_some()
    }

    pub fn new(main: LogicalRect) -> Self {
        Self {
            main,
            pointer: main.centre(),
            trace: None,
            highlighter: false,
            next_callout: None,
            keyboard: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShapeMode {
    pub sel: AnchorCursor,
    pub kind: ShapeTool,
    /// An arrow's heads: at the cursor, or (`V`) both ends.
    pub both_heads: bool,
    drag: Option<Drag>,
}

impl ShapeMode {
    /// A shape in the middle quarter of `main`.
    pub fn new(main: LogicalRect, kind: ShapeTool) -> Self {
        let r = LogicalRect::new(
            main.x + main.w / 4.0,
            main.y + main.h / 4.0,
            main.w / 2.0,
            main.h / 2.0,
        );
        Self {
            sel: AnchorCursor::new(r, main),
            kind,
            both_heads: false,
            drag: None,
        }
    }
}

impl Session {
    /// The pen's op for a trace: ink, or the highlighter's.
    fn pen_op(&self, a: &AnnotateMode, points: &[LogicalPoint]) -> Option<OpData> {
        if !a.highlighter {
            return self.stroke(points);
        }
        (!points.is_empty()).then(|| {
            OpData::Highlight(crate::model::HighlightOp {
                points: points.to_vec(),
                rgb: self.ink.rgb,
                width: self.ink_width * HIGHLIGHT_SCALE,
                alpha: HIGHLIGHT_ALPHA,
            })
        })
    }

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
        let outline = |kind| {
            (!s.sel.is_empty()).then(|| {
                OpData::Shape(ShapeOp {
                    kind,
                    rect: s.sel.rect(),
                    rgb: self.ink.rgb,
                    width: self.ink_width,
                })
            })
        };
        let line = |heads| {
            let (a, b) = (s.sel.anchor_point(), s.sel.cursor_point());
            (a != b).then_some(OpData::Line(LineOp {
                a,
                b,
                rgb: self.ink.rgb,
                width: self.ink_width,
                heads,
            }))
        };
        match s.kind {
            ShapeTool::Rect => outline(ShapeKind::Rect),
            ShapeTool::Circle => outline(ShapeKind::Circle),
            ShapeTool::Line => line(Heads::None),
            ShapeTool::Arrow => line(if s.both_heads {
                Heads::Both
            } else {
                Heads::End
            }),
        }
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
        let step = |a: &mut AnnotateMode, dx: f64, dy: f64| {
            let main = a.main;
            a.pointer = LogicalPoint::new(
                (a.pointer.x + dx).clamp(main.x, main.right()),
                (a.pointer.y + dy).clamp(main.y, main.bottom()),
            );
            a.keyboard = true;
            if let Some(t) = &mut a.trace {
                t.push(a.pointer);
            }
        };
        let s5 = crate::boxes::STEP;
        match intent {
            Intent::Steer(d) | Intent::Translate(d) => {
                let k = if matches!(intent, Intent::Translate(_)) {
                    4.0
                } else {
                    1.0
                };
                let (dx, dy) = d.vec();
                step(a, dx * s5 * k, dy * s5 * k);
            }
            Intent::HeldSteer(dirs, n) | Intent::HeldTranslate(dirs, n) => {
                let k = if matches!(intent, Intent::HeldTranslate(..)) {
                    4.0
                } else {
                    1.0
                };
                let (dx, dy) = dirs.vector();
                step(a, dx * s5 * k * f64::from(n), dy * s5 * k * f64::from(n));
            }
            Intent::PenToggle => {
                if a.trace.is_some() {
                    self.lock_trace(a);
                } else {
                    a.trace = Some(vec![a.pointer]);
                    a.keyboard = true;
                }
            }
            Intent::Press(p) => {
                a.keyboard = false;
                a.trace = Some(vec![p]);
            }
            Intent::Motion(p) => {
                a.pointer = p;
                a.keyboard = false;
                if let Some(t) = &mut a.trace {
                    t.push(p);
                }
            }
            Intent::Release(p) => {
                if let Some(mut t) = a.trace.take() {
                    if t.last() != Some(&p) {
                        t.push(p); // right up to where it was let go
                    }
                    self.lock(self.pen_op(a, &t));
                }
            }
            Intent::Eyedropper => self.eyedrop(a.pointer),
            Intent::Insert => {
                self.lock_trace(a);
                return Transition::Push(Mode::Text(crate::text::TextMode::annotation(a.pointer)));
            }
            Intent::CalloutNumber => {
                return Transition::Push(Mode::Text(crate::text::TextMode::callout_number()));
            }
            Intent::Callout => {
                let n = a
                    .next_callout
                    .take()
                    .unwrap_or_else(|| self.last_callout() + 1);
                let size = 16.0 + 2.0 * self.ink_width;
                self.lock(Some(OpData::Callout(crate::model::CalloutOp {
                    centre: a.pointer,
                    n,
                    rgb: self.ink.rgb,
                    size,
                })));
            }
            Intent::ToggleHighlighter => {
                self.lock_trace(a);
                a.highlighter = !a.highlighter;
            }
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

    /// The open layer's highest callout number (0 with none).
    fn last_callout(&self) -> u32 {
        self.open_layer().map_or(0, |l| {
            l.ops
                .iter()
                .filter_map(|o| match &o.data {
                    OpData::Callout(c) => Some(c.n),
                    _ => None,
                })
                .max()
                .unwrap_or(0)
        })
    }

    /// Text came back from the text input.
    pub(crate) fn annotate_text(
        &mut self,
        a: &mut AnnotateMode,
        text: String,
        entry: &crate::text::TextMode,
    ) {
        match entry.purpose {
            crate::text::TextPurpose::Annotation => {
                if !text.is_empty() {
                    let op = self.text_op(entry, &text);
                    self.lock(Some(OpData::Text(op)));
                }
            }
            crate::text::TextPurpose::CalloutNumber => match text.trim().parse::<u32>() {
                Ok(n) => a.next_callout = Some(n),
                Err(_) => self.note = Some(format!("not a number: {text:?}")),
            },
        }
    }

    fn lock_trace(&mut self, a: &mut AnnotateMode) {
        if let Some(t) = a.trace.take() {
            self.lock(self.pen_op(a, &t));
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
        match intent {
            Intent::CycleKind => s.kind = s.kind.next(),
            Intent::ToggleHeads if s.kind == ShapeTool::Arrow => s.both_heads = !s.both_heads,
            _ => {}
        }
        Transition::Stay
    }

    /// The pen's line in progress, or the shape being edited.
    pub(crate) fn annotate_editing(&self, mode: &Mode) -> Option<OpData> {
        match mode {
            Mode::Annotate(a) => a.trace.as_deref().and_then(|t| self.pen_op(a, t)),
            Mode::Shape(s) => self.shape(s),
            _ => None,
        }
    }
}
