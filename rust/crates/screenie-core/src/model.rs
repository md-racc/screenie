//! The durable project: what exists now, as plain data.
//!
//! A project is the selection and an ordered stack of layers, each an
//! ordered list of operations clipped to the box it was opened in. Layers
//! and operations have stable IDs: reordering changes position, never
//! identity. The source capture and layout travel with it in the project
//! file (milestone 8); undo history and other runtime state never do.

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

fn grown(r: LogicalRect, m: f64) -> LogicalRect {
    LogicalRect::new(r.x - m, r.y - m, r.w + 2.0 * m, r.h + 2.0 * m)
}

/// What an operation does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum OpData {
    Censor(CensorOp),
    Stroke(StrokeOp),
    Shape(ShapeOp),
    Filter(FilterOp),
}

impl OpData {
    /// A conservative box of what the operation can change.
    pub fn bbox(&self) -> LogicalRect {
        match self {
            OpData::Censor(c) => c.rect,
            OpData::Stroke(s) => {
                if s.points.is_empty() {
                    return LogicalRect::default();
                }
                let xs = s.points.iter().map(|p| p.x);
                let ys = s.points.iter().map(|p| p.y);
                let (x0, x1) = (
                    xs.clone().fold(f64::INFINITY, f64::min),
                    xs.fold(f64::NEG_INFINITY, f64::max),
                );
                let (y0, y1) = (
                    ys.clone().fold(f64::INFINITY, f64::min),
                    ys.fold(f64::NEG_INFINITY, f64::max),
                );
                grown(LogicalRect::from_edges(x0, y0, x1, y1), s.width)
            }
            OpData::Shape(s) => grown(s.rect, s.width),
            OpData::Filter(f) => f.rect,
        }
    }

    /// The layer list's word for it.
    pub fn noun(&self) -> &'static str {
        match self {
            OpData::Censor(_) => "censor",
            OpData::Stroke(_) => "line",
            OpData::Shape(_) => "shape",
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
        let mut it = self.ops.iter().map(|o| o.data.bbox());
        let first = it.next()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.right(), first.bottom());
        for b in it {
            x0 = x0.min(b.x);
            y0 = y0.min(b.y);
            x1 = x1.max(b.right());
            y1 = y1.max(b.bottom());
        }
        let c = self.clip;
        let (x0, y0) = (x0.max(c.x), y0.max(c.y));
        let (x1, y1) = (x1.min(c.right()), y1.min(c.bottom()));
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
}

impl Project {
    pub fn new(selection: LogicalRect) -> Self {
        Self {
            version: VERSION,
            selection,
            layers: Vec::new(),
            next_id: 1,
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

    #[test]
    fn layers_describe_their_ops() {
        let p = sample();
        assert_eq!(p.layers[0].describe(), "censor ×2");
        assert_eq!(Layer::new(LayerId(9), p.selection).describe(), "empty");
    }
}
