//! What happened, for `--json` (docs/cli.md, JSON): one line on success,
//! and also on cancel or failure, so scripts can tell them apart.

use serde_json::{Map, Value, json};

use screenie_core::geometry::{LogicalRect, PixelRect};
use screenie_core::layout::Layout;

use crate::deliver::Delivered;

/// The schema's version: bump on any breaking change.
pub const VERSION: u32 = 1;

pub struct Report {
    pub cancelled: bool,
    fields: Map<String, Value>,
}

impl Report {
    pub fn done(kind: &str) -> Self {
        let mut fields = Map::new();
        fields.insert("version".into(), json!(VERSION));
        fields.insert("status".into(), json!("done"));
        fields.insert("kind".into(), json!(kind));
        Self {
            cancelled: false,
            fields,
        }
    }

    pub fn cancelled() -> Self {
        let mut fields = Map::new();
        fields.insert("version".into(), json!(VERSION));
        fields.insert("status".into(), json!("cancelled"));
        Self {
            cancelled: true,
            fields,
        }
    }

    pub fn error(code: u8, message: &str) -> Value {
        json!({"version": VERSION, "status": "error", "code": code, "message": message})
    }

    pub fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    pub fn rect(self, r: LogicalRect, layout: &Layout) -> Self {
        let monitor = layout.monitor_at(r.centre()).map(|m| m.name.clone());
        let s = self.set("rect", json!({"x": r.x, "y": r.y, "w": r.w, "h": r.h}));
        match monitor {
            Some(m) => s.set("monitor", m),
            None => s,
        }
    }

    pub fn px(self, p: PixelRect) -> Self {
        self.set("px", json!({"w": p.w, "h": p.h}))
    }

    pub fn delivered(self, d: &Delivered) -> Self {
        let mut s = self.set("copied", d.copied);
        if let Some(p) = d.saved.as_ref().or(d.shot.as_ref()) {
            s = s.set("path", p.display().to_string());
        }
        s
    }

    pub fn line(&self) -> String {
        Value::Object(self.fields.clone()).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shapes_cli_md_gives() {
        assert_eq!(
            Report::cancelled().line(),
            r#"{"status":"cancelled","version":1}"#
        );
        let d = Report::done("colour")
            .set("text", "#ff0000")
            .set("rgb", json!([255, 0, 0]));
        let v: Value = serde_json::from_str(&d.line()).unwrap();
        assert_eq!(v["kind"], "colour");
        assert_eq!(v["rgb"][0], 255);
        assert_eq!(Report::error(3, "no window")["code"], 3);
    }
}
