//! What happened, for `--json` (docs/cli.md, JSON): one line on success,
//! and also on cancel or failure, so scripts can tell them apart.

use serde_json::{Map, Value, json};

use screenie_core::geometry::LogicalRect;
use screenie_core::layout::Layout;

use crate::deliver::Delivered;

/// The schema's version: bump on any breaking change.
pub const VERSION: u32 = 1;

/// A box as the JSON has it (docs/cli.md, JSON): logical, unrounded.
pub fn rect_json(r: LogicalRect) -> Value {
    json!({"x": r.x, "y": r.y, "w": r.w, "h": r.h})
}

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
        let s = self.set("rect", rect_json(r));
        match monitor {
            Some(m) => s.set("monitor", m),
            None => s,
        }
    }

    /// The delivered image's size, as `px`.
    pub fn size(self, img: &screenie_image::Image) -> Self {
        self.set("px", json!({"w": img.width(), "h": img.height()}))
    }

    pub fn delivered(self, d: &Delivered) -> Self {
        let mut s = self.set("copied", d.copied);
        if let Some(p) = d.saved.as_ref().or(d.shot.as_ref()) {
            s = s.set("path", p.display().to_string());
        }
        if let Some(p) = &d.project {
            s = s.set("project", p.display().to_string());
        }
        s
    }

    /// What auto-redact censored: when asked for, or when it did.
    pub fn redacted(
        self,
        out: &crate::cli::Output,
        what: &[(impl AsRef<str>, LogicalRect)],
    ) -> Self {
        if out.redact.is_none() && what.is_empty() {
            return self;
        }
        let list: Vec<Value> = what
            .iter()
            .map(|(w, r)| json!({"what": w.as_ref(), "rect": rect_json(*r)}))
            .collect();
        self.set("redacted", list)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
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

    #[test]
    fn an_image_report_says_where_its_file_and_project_went() {
        let d = Delivered {
            saved: Some("/x/a.png".into()),
            shot: Some("/run/s.png".into()),
            project: Some("/x/a.screenie".into()),
            copied: true,
        };
        let v: Value = serde_json::from_str(&Report::done("image").delivered(&d).line()).unwrap();
        assert_eq!(
            (&v["path"], &v["project"]),
            (&json!("/x/a.png"), &json!("/x/a.screenie"))
        );
        let v: Value = serde_json::from_str(
            &Report::done("image")
                .delivered(&Delivered::default())
                .line(),
        )
        .unwrap();
        assert!(v.get("project").is_none() && v.get("path").is_none());
    }
}
