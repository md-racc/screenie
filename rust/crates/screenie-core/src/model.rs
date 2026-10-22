//! The durable project: what exists now, as plain data.
//!
//! A project is the selection and an ordered stack of layers, each an
//! ordered list of operations clipped to the box it was opened in. Layers
//! and operations have stable IDs: reordering changes position, never
//! identity. The source capture and layout travel with it in the project
//! file ([`ProjectFile`]); undo history and other runtime state never do.

use serde::{Deserialize, Serialize};

use crate::geometry::{LogicalPoint, LogicalRect};

/// The project format's version. Bump it on any breaking change, with a
/// migration.
pub const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayerId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CensorKind {
    Pixelate,
    Bar,
}

/// An obscuring rectangle. `amount` is the pixelate block size (logical
/// px) or the bar's shade (0 black to 1 white).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CensorOp {
    pub rect: LogicalRect,
    pub kind: CensorKind,
    pub amount: f64,
}

/// An opaque colour, 0..1 a channel.
pub type Rgb = [f64; 3];

/// A freehand line through `points`, with round caps and joins. One point
/// is a dot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokeOp {
    pub points: Vec<LogicalPoint>,
    pub rgb: Rgb,
    /// Line width, logical px.
    pub width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    Rect,
    /// An ellipse filling the rect: a square makes a circle.
    Circle,
}

impl ShapeKind {
    pub fn name(self) -> &'static str {
        match self {
            ShapeKind::Rect => "rect",
            ShapeKind::Circle => "circle",
        }
    }
}

/// An outlined rectangle or ellipse.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeOp {
    pub kind: ShapeKind,
    pub rect: LogicalRect,
    pub rgb: Rgb,
    pub width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterKind {
    /// amount 0.25-1: how far toward grey.
    Grey,
    /// No amount.
    Invert,
    /// amount -5..5: 10% toward white (or black) a step.
    Brightness,
    /// amount -4..4: an S-curve per step (or toward mid-grey, 20% a step).
    Contrast,
    /// amount: radius, logical px.
    Blur,
}

/// An image filter over a rect. A muted filter stays in the stack but
/// isn't applied.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilterOp {
    pub rect: LogicalRect,
    pub kind: FilterKind,
    pub amount: f64,
    #[serde(default)]
    pub muted: bool,
}

/// A highlighter trace: the pen's path, drawn translucent and multiplied,
/// so it marks without hiding what's under it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HighlightOp {
    pub points: Vec<LogicalPoint>,
    pub rgb: Rgb,
    pub width: f64,
    /// 0..1.
    pub alpha: f64,
}

/// Text, its top-left at `origin`; lines split on `\n`. With `chip`, a
/// black or white backing (whichever contrasts with the ink) behind it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextOp {
    pub origin: LogicalPoint,
    pub text: String,
    pub rgb: Rgb,
    /// Font size, logical px.
    pub size: f64,
    pub chip: bool,
}

impl TextOp {
    /// Line height, logical px.
    pub fn line_height(&self) -> f64 {
        self.size * 1.25
    }

    /// The chip's padding, logical px.
    pub fn pad(&self) -> f64 {
        self.size * 0.3
    }
}

/// A numbered marker: a disc of the ink, the number in black or white.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalloutOp {
    pub centre: LogicalPoint,
    pub n: u32,
    pub rgb: Rgb,
    /// Diameter, logical px.
    pub size: f64,
}

/// Which ends of a line carry an arrowhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Heads {
    None,
    /// At `b`.
    End,
    Both,
}

/// A straight line from `a` to `b`, maybe an arrow.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineOp {
    pub a: LogicalPoint,
    pub b: LogicalPoint,
    pub rgb: Rgb,
    pub width: f64,
    pub heads: Heads,
}

impl LineOp {
    /// "line" or "arrow".
    pub fn heads_name(&self) -> &'static str {
        if self.heads == Heads::None {
            "line"
        } else {
            "arrow"
        }
    }

    /// An arrowhead's length, logical px: it scales with the width.
    pub fn head_len(&self) -> f64 {
        (self.width * 4.0).max(10.0)
    }
}

fn grown(r: LogicalRect, m: f64) -> LogicalRect {
    LogicalRect::new(r.x - m, r.y - m, r.w + 2.0 * m, r.h + 2.0 * m)
}

/// The box of a line through `points`, `width` wide.
fn points_bbox(points: &[LogicalPoint], width: f64) -> LogicalRect {
    if points.is_empty() {
        return LogicalRect::default();
    }
    let xs = points.iter().map(|p| p.x);
    let ys = points.iter().map(|p| p.y);
    let (x0, x1) = (
        xs.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
    );
    let (y0, y1) = (
        ys.clone().fold(f64::INFINITY, f64::min),
        ys.fold(f64::NEG_INFINITY, f64::max),
    );
    grown(LogicalRect::from_edges(x0, y0, x1, y1), width)
}

/// What an operation does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum OpData {
    Censor(CensorOp),
    Stroke(StrokeOp),
    Shape(ShapeOp),
    Line(LineOp),
    Highlight(HighlightOp),
    Text(TextOp),
    Callout(CalloutOp),
    Filter(FilterOp),
}

impl OpData {
    /// A conservative box of what the operation can change.
    pub fn bbox(&self) -> LogicalRect {
        match self {
            OpData::Censor(c) => c.rect,
            OpData::Stroke(s) => points_bbox(&s.points, s.width),
            OpData::Shape(s) => grown(s.rect, s.width),
            OpData::Text(t) => {
                // Monospace, generously: no font is needed to bound it.
                let lines = t.text.split('\n');
                let cols = lines.clone().map(|l| l.chars().count()).max().unwrap_or(0);
                let rows = lines.count().max(1);
                let w = cols as f64 * t.size * 0.7;
                let h = rows as f64 * t.line_height();
                grown(
                    LogicalRect::new(t.origin.x, t.origin.y, w, h),
                    t.pad() + 2.0,
                )
            }
            OpData::Callout(c) => {
                let r = c.size / 2.0;
                LogicalRect::new(
                    c.centre.x - r - 1.0,
                    c.centre.y - r - 1.0,
                    c.size + 2.0,
                    c.size + 2.0,
                )
            }
            OpData::Highlight(h) => points_bbox(&h.points, h.width),
            OpData::Line(l) => grown(
                LogicalRect::from_edges(
                    l.a.x.min(l.b.x),
                    l.a.y.min(l.b.y),
                    l.a.x.max(l.b.x),
                    l.a.y.max(l.b.y),
                ),
                l.width.max(l.head_len()),
            ),
            OpData::Filter(f) => f.rect,
        }
    }

    /// The layer list's word for it.
    pub fn noun(&self) -> &'static str {
        match self {
            OpData::Censor(_) => "censor",
            OpData::Stroke(_) => "pen",
            OpData::Shape(_) => "shape",
            OpData::Line(l) => l.heads_name(),
            OpData::Highlight(_) => "highlight",
            OpData::Text(_) => "text",
            OpData::Callout(_) => "callout",
            OpData::Filter(_) => "filter",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub id: OpId,
    #[serde(flatten)]
    pub data: OpData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    /// Pixels outside this are never touched.
    pub clip: LogicalRect,
    #[serde(default)]
    pub hidden: bool,
    pub ops: Vec<Op>,
}

impl Layer {
    pub fn new(id: LayerId, clip: LogicalRect) -> Self {
        Self {
            id,
            clip,
            hidden: false,
            ops: Vec::new(),
        }
    }

    /// What's in it, for the layer list: "censor ×2, filter".
    pub fn describe(&self) -> String {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for op in &self.ops {
            let n = op.data.noun();
            match counts.iter_mut().find(|(k, _)| *k == n) {
                Some((_, c)) => *c += 1,
                None => counts.push((n, 1)),
            }
        }
        if counts.is_empty() {
            return "empty".into();
        }
        counts
            .iter()
            .map(|&(n, c)| {
                if c == 1 {
                    n.to_string()
                } else {
                    format!("{n} ×{c}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The area its ops can touch, within the clip (`None` if nothing).
    pub fn bbox(&self) -> Option<LogicalRect> {
        if self.ops.is_empty() {
            return None;
        }
        let u = crate::layout::bounding(self.ops.iter().map(|o| o.data.bbox()));
        let c = self.clip;
        let (x0, y0) = (u.x.max(c.x), u.y.max(c.y));
        let (x1, y1) = (u.right().min(c.right()), u.bottom().min(c.bottom()));
        Some(LogicalRect::new(
            x0,
            y0,
            (x1 - x0).max(0.0),
            (y1 - y0).max(0.0),
        ))
    }

    pub fn op_index(&self, id: OpId) -> Option<usize> {
        self.ops.iter().position(|o| o.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub selection: LogicalRect,
    /// Bottom first: later layers apply over earlier ones.
    pub layers: Vec<Layer>,
    /// The next stable ID to hand out (shared by layers and ops).
    next_id: u64,
    /// Measure's calibration, if one was set: `edit last` keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure_scale: Option<MeasureScale>,
}

/// Measure's scale as saved: `px` capture pixels are `real` of `unit`
/// (a standard unit's name, or any other name), good to `error`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeasureScale {
    pub px: f64,
    pub real: f64,
    pub unit: String,
    pub error: f64,
}

impl Project {
    pub fn new(selection: LogicalRect) -> Self {
        Self {
            version: VERSION,
            selection,
            layers: Vec::new(),
            next_id: 1,
            measure_scale: None,
        }
    }

    fn fresh(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn new_layer_id(&mut self) -> LayerId {
        LayerId(self.fresh())
    }

    pub fn new_op_id(&mut self) -> OpId {
        OpId(self.fresh())
    }

    pub fn layer_index(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("plain data serializes")
    }

    pub fn from_json(s: &str) -> Result<Project, String> {
        let p: Project = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if p.version > VERSION {
            return Err(format!(
                "project version {} is newer than this screenie",
                p.version
            ));
        }
        Ok(p)
    }
}

/// What a project file holds beside the capture: the project, and the
/// layout the capture was taken on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub format: String,
    pub project: Project,
    pub layout: crate::layout::Layout,
}

/// The project file's `format` tag.
pub const FORMAT: &str = "screenie-project";

impl ProjectFile {
    pub fn new(project: Project, layout: crate::layout::Layout) -> Self {
        Self {
            format: FORMAT.into(),
            project,
            layout,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("plain data serializes")
    }

    pub fn from_json(s: &str) -> Result<ProjectFile, String> {
        let f: ProjectFile = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if f.format != FORMAT {
            return Err(format!("not a screenie project ({:?})", f.format));
        }
        if f.project.version > VERSION {
            return Err(format!(
                "project version {} is newer than this screenie",
                f.project.version
            ));
        }
        Ok(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn censor(x: f64, kind: CensorKind) -> OpData {
        OpData::Censor(CensorOp {
            rect: LogicalRect::new(x, 10.0, 20.0, 20.0),
            kind,
            amount: 10.0,
        })
    }

    fn sample() -> Project {
        let mut p = Project::new(LogicalRect::new(100.0, 170.0, 560.0, 90.0));
        let id = p.new_layer_id();
        let mut l = Layer::new(id, p.selection);
        for (x, k) in [(0.0, CensorKind::Pixelate), (5.0, CensorKind::Bar)] {
            let op = Op {
                id: p.new_op_id(),
                data: censor(x, k),
            };
            l.ops.push(op);
        }
        p.layers.push(l);
        p
    }

    #[test]
    fn ids_are_unique_and_survive_a_round_trip() {
        let mut p = sample();
        let ids: Vec<u64> = std::iter::once(p.layers[0].id.0)
            .chain(p.layers[0].ops.iter().map(|o| o.id.0))
            .collect();
        assert_eq!(ids, [1, 2, 3]);
        let mut back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.new_op_id(), p.new_op_id()); // the counter travels too
    }

    #[test]
    fn the_format_is_readable() {
        let json = sample().to_json();
        assert!(json.contains(r#""version":1"#), "{json}");
        assert!(json.contains(r#""op":"censor","rect":{"x":0.0,"y":10.0,"w":20.0,"h":20.0},"kind":"pixelate","amount":10.0"#), "{json}");
    }

    #[test]
    fn a_project_file_round_trips_with_its_layout() {
        use crate::layout::{Layout, Monitor, MonitorId};
        let layout = Layout::new(
            vec![Monitor {
                id: MonitorId(0),
                name: "DP-1".into(),
                rect: LogicalRect::new(0.0, 0.0, 1920.0, 1080.0),
                scale: 1.0,
            }],
            vec![],
            MonitorId(0),
        );
        let f = ProjectFile::new(sample(), layout);
        assert_eq!(ProjectFile::from_json(&f.to_json()).unwrap(), f);
        assert!(ProjectFile::from_json(&f.to_json().replace(FORMAT, "other")).is_err());
    }

    #[test]
    fn a_newer_project_is_refused() {
        let json = sample()
            .to_json()
            .replace(r#""version":1"#, r#""version":99"#);
        assert!(Project::from_json(&json).unwrap_err().contains("newer"));
    }

    #[test]
    fn strokes_and_shapes_round_trip_and_bound_their_ink() {
        let stroke = OpData::Stroke(StrokeOp {
            points: vec![LogicalPoint::new(10.0, 10.0), LogicalPoint::new(30.0, 20.0)],
            rgb: [0.9, 0.12, 0.12],
            width: 3.0,
        });
        assert_eq!(stroke.bbox(), LogicalRect::new(7.0, 7.0, 26.0, 16.0));
        let OpData::Stroke(st) = &stroke else {
            unreachable!()
        };
        let highlight = OpData::Highlight(HighlightOp {
            points: st.points.clone(),
            rgb: st.rgb,
            width: st.width,
            alpha: 0.4,
        });
        assert_eq!(highlight.bbox(), stroke.bbox());
        let shape = OpData::Shape(ShapeOp {
            kind: ShapeKind::Circle,
            rect: LogicalRect::new(0.0, 0.0, 10.0, 10.0),
            rgb: [0.0, 0.0, 1.0],
            width: 2.0,
        });
        for op in [stroke, shape] {
            let json = serde_json::to_string(&op).unwrap();
            assert_eq!(serde_json::from_str::<OpData>(&json).unwrap(), op, "{json}");
        }
    }

    /// Saved projects exist in this format: if this fails, bump VERSION
    /// and add a migration rather than editing the literal.
    const V1: &str = r#"{
        "format": "screenie-project",
        "project": {
            "version": 1,
            "selection": {"x": 100.0, "y": 170.0, "w": 560.0, "h": 90.0},
            "layers": [
                {"id": 1, "clip": {"x": 100.0, "y": 170.0, "w": 560.0, "h": 90.0}, "hidden": false, "ops": [
                    {"id": 2, "op": "censor", "rect": {"x": 110.0, "y": 180.0, "w": 20.0, "h": 20.0}, "kind": "pixelate", "amount": 10.0},
                    {"id": 3, "op": "censor", "rect": {"x": 140.0, "y": 180.0, "w": 20.0, "h": 20.0}, "kind": "bar", "amount": 0.5},
                    {"id": 4, "op": "stroke", "points": [{"x": 120.0, "y": 200.0}, {"x": 130.0, "y": 210.0}], "rgb": [0.9, 0.12, 0.12], "width": 3.0},
                    {"id": 5, "op": "shape", "kind": "circle", "rect": {"x": 200.0, "y": 180.0, "w": 40.0, "h": 30.0}, "rgb": [1.0, 1.0, 1.0], "width": 2.0},
                    {"id": 6, "op": "line", "a": {"x": 250.0, "y": 190.0}, "b": {"x": 300.0, "y": 220.0}, "rgb": [0.0, 0.0, 0.0], "width": 3.0, "heads": "end"},
                    {"id": 7, "op": "line", "a": {"x": 250.0, "y": 230.0}, "b": {"x": 300.0, "y": 240.0}, "rgb": [0.0, 0.0, 0.0], "width": 3.0, "heads": "both"},
                    {"id": 8, "op": "line", "a": {"x": 310.0, "y": 230.0}, "b": {"x": 350.0, "y": 240.0}, "rgb": [0.0, 0.0, 0.0], "width": 3.0, "heads": "none"},
                    {"id": 9, "op": "highlight", "points": [{"x": 360.0, "y": 200.0}, {"x": 400.0, "y": 200.0}], "rgb": [0.1, 0.75, 0.25], "width": 15.0, "alpha": 0.4},
                    {"id": 10, "op": "text", "origin": {"x": 410.0, "y": 180.0}, "text": "two\nlines", "rgb": [1.0, 1.0, 1.0], "size": 18.0, "chip": true},
                    {"id": 11, "op": "callout", "centre": {"x": 500.0, "y": 200.0}, "n": 3, "rgb": [0.15, 0.45, 1.0], "size": 22.0}
                ]},
                {"id": 12, "clip": {"x": 100.0, "y": 170.0, "w": 560.0, "h": 90.0}, "hidden": true, "ops": [
                    {"id": 13, "op": "filter", "rect": {"x": 100.0, "y": 170.0, "w": 560.0, "h": 90.0}, "kind": "blur", "amount": 4.0, "muted": false},
                    {"id": 14, "op": "filter", "rect": {"x": 520.0, "y": 180.0, "w": 60.0, "h": 40.0}, "kind": "brightness", "amount": -2.0, "muted": true}
                ]}
            ],
            "next_id": 15,
            "measure_scale": {"px": 100.0, "real": 2.5, "unit": "cm", "error": 0.01}
        },
        "layout": {
            "monitors": [{"id": 0, "name": "DP-1", "rect": {"x": 0.0, "y": 0.0, "w": 1920.0, "h": 1080.0}, "scale": 1.0}],
            "windows": [{"id": 7, "rect": {"x": 3.0, "y": 3.0, "w": 955.0, "h": 1074.0}, "monitor": 0, "class": "foot", "title": "~"}],
            "active": 7,
            "focused": 0,
            "bounds": {"x": 0.0, "y": 0.0, "w": 1920.0, "h": 1080.0}
        }
    }"#;

    fn v1() -> ProjectFile {
        use crate::layout::{Layout, Monitor, MonitorId, Window, WindowId};
        let r = LogicalRect::new;
        let pt = LogicalPoint::new;
        let sel = r(100.0, 170.0, 560.0, 90.0);
        let black = [0.0, 0.0, 0.0];
        let ops = |data: Vec<OpData>, first: u64| -> Vec<Op> {
            (first..)
                .zip(data)
                .map(|(id, data)| Op { id: OpId(id), data })
                .collect()
        };
        let line = |a, b, heads| {
            OpData::Line(LineOp {
                a,
                b,
                rgb: black,
                width: 3.0,
                heads,
            })
        };
        let marks = ops(
            vec![
                OpData::Censor(CensorOp {
                    rect: r(110.0, 180.0, 20.0, 20.0),
                    kind: CensorKind::Pixelate,
                    amount: 10.0,
                }),
                OpData::Censor(CensorOp {
                    rect: r(140.0, 180.0, 20.0, 20.0),
                    kind: CensorKind::Bar,
                    amount: 0.5,
                }),
                OpData::Stroke(StrokeOp {
                    points: vec![pt(120.0, 200.0), pt(130.0, 210.0)],
                    rgb: [0.9, 0.12, 0.12],
                    width: 3.0,
                }),
                OpData::Shape(ShapeOp {
                    kind: ShapeKind::Circle,
                    rect: r(200.0, 180.0, 40.0, 30.0),
                    rgb: [1.0, 1.0, 1.0],
                    width: 2.0,
                }),
                line(pt(250.0, 190.0), pt(300.0, 220.0), Heads::End),
                line(pt(250.0, 230.0), pt(300.0, 240.0), Heads::Both),
                line(pt(310.0, 230.0), pt(350.0, 240.0), Heads::None),
                OpData::Highlight(HighlightOp {
                    points: vec![pt(360.0, 200.0), pt(400.0, 200.0)],
                    rgb: [0.1, 0.75, 0.25],
                    width: 15.0,
                    alpha: 0.4,
                }),
                OpData::Text(TextOp {
                    origin: pt(410.0, 180.0),
                    text: "two\nlines".into(),
                    rgb: [1.0, 1.0, 1.0],
                    size: 18.0,
                    chip: true,
                }),
                OpData::Callout(CalloutOp {
                    centre: pt(500.0, 200.0),
                    n: 3,
                    rgb: [0.15, 0.45, 1.0],
                    size: 22.0,
                }),
            ],
            2,
        );
        let filters = ops(
            vec![
                OpData::Filter(FilterOp {
                    rect: sel,
                    kind: FilterKind::Blur,
                    amount: 4.0,
                    muted: false,
                }),
                OpData::Filter(FilterOp {
                    rect: r(520.0, 180.0, 60.0, 40.0),
                    kind: FilterKind::Brightness,
                    amount: -2.0,
                    muted: true,
                }),
            ],
            13,
        );
        let mut layout = Layout::new(
            vec![Monitor {
                id: MonitorId(0),
                name: "DP-1".into(),
                rect: r(0.0, 0.0, 1920.0, 1080.0),
                scale: 1.0,
            }],
            vec![Window {
                id: WindowId(7),
                rect: r(3.0, 3.0, 955.0, 1074.0),
                monitor: Some(MonitorId(0)),
                class: "foot".into(),
                title: "~".into(),
            }],
            MonitorId(0),
        );
        layout.active = Some(WindowId(7));
        let project = Project {
            version: 1,
            selection: sel,
            layers: vec![
                Layer {
                    ops: marks,
                    ..Layer::new(LayerId(1), sel)
                },
                Layer {
                    hidden: true,
                    ops: filters,
                    ..Layer::new(LayerId(12), sel)
                },
            ],
            next_id: 15,
            measure_scale: Some(MeasureScale {
                px: 100.0,
                real: 2.5,
                unit: "cm".into(),
                error: 0.01,
            }),
        };
        ProjectFile::new(project, layout)
    }

    #[test]
    fn a_v1_project_file_still_reads_and_writes_the_same() {
        let json = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
        assert_eq!(ProjectFile::from_json(V1).unwrap(), v1());
        assert_eq!(json(&v1().to_json()), json(V1));
    }

    #[test]
    fn an_older_v1_file_without_the_defaulted_fields_reads() {
        let old = r#"{"version": 1, "selection": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0},
            "layers": [{"id": 1, "clip": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}, "ops": [
                {"id": 2, "op": "filter", "rect": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}, "kind": "invert", "amount": 1.0}
            ]}],
            "next_id": 3}"#;
        let p = Project::from_json(old).unwrap();
        assert!(!p.layers[0].hidden);
        assert!(matches!(p.layers[0].ops[0].data, OpData::Filter(f) if !f.muted));
        assert_eq!(p.measure_scale, None);
    }

    #[test]
    fn a_layer_bounds_its_ops_within_its_clip() {
        let mut l = Layer::new(LayerId(1), LogicalRect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(l.bbox(), None);
        for (id, x) in [(2, 10.0), (3, 90.0)] {
            l.ops.push(Op {
                id: OpId(id),
                data: censor(x, CensorKind::Bar),
            });
        }
        // 10..110 across, cut at the clip's right edge.
        assert_eq!(l.bbox(), Some(LogicalRect::new(10.0, 10.0, 90.0, 20.0)));
    }

    #[test]
    fn layers_describe_their_ops() {
        let p = sample();
        assert_eq!(p.layers[0].describe(), "censor ×2");
        assert_eq!(Layer::new(LayerId(9), p.selection).describe(), "empty");

        let line = |heads| {
            OpData::Line(LineOp {
                a: LogicalPoint::new(0.0, 0.0),
                b: LogicalPoint::new(10.0, 0.0),
                rgb: [0.0, 0.0, 0.0],
                width: 3.0,
                heads,
            })
        };
        let stroke = OpData::Stroke(StrokeOp {
            points: vec![LogicalPoint::new(0.0, 0.0)],
            rgb: [0.0, 0.0, 0.0],
            width: 3.0,
        });
        let mut l = Layer::new(LayerId(1), p.selection);
        for (id, data) in [stroke, line(Heads::None), line(Heads::End)]
            .into_iter()
            .enumerate()
        {
            l.ops.push(Op {
                id: OpId(id as u64 + 2),
                data,
            });
        }
        assert_eq!(l.describe(), "pen, line, arrow");
    }
}
