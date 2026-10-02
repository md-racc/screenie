//! The durable project: what exists now, as plain data.
//!
//! A project is the selection and an ordered stack of layers, each an
//! ordered list of operations clipped to the box it was opened in. Layers
//! and operations have stable IDs: reordering changes position, never
//! identity. The source capture and layout travel with it in the project
//! file (milestone 8); undo history and other runtime state never do.

use serde::{Deserialize, Serialize};

use crate::geometry::LogicalRect;

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

/// What an operation does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum OpData {
    Censor(CensorOp),
}

impl OpData {
    /// A conservative box of what the operation can change.
    pub fn bbox(&self) -> LogicalRect {
        match self {
            OpData::Censor(c) => c.rect,
        }
    }

    /// The layer list's word for it.
    pub fn noun(&self) -> &'static str {
        match self {
            OpData::Censor(_) => "censor",
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
    fn layers_describe_their_ops() {
        let p = sample();
        assert_eq!(p.layers[0].describe(), "censor ×2");
        assert_eq!(Layer::new(LayerId(9), p.selection).describe(), "empty");
    }
}
