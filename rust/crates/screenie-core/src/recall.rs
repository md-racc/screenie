//! The recall ring (`.` and `last-region`): the last accepted boxes,
//! newest first.
//!
//! Each box is kept relative to the monitor its centre was on, by name, so
//! it follows that monitor if the layout is rearranged. With the monitor
//! gone, the global rect is used (and the selection clamps it).
//!
//! This is only the data. The front end reads and writes the file
//! (`$XDG_STATE_HOME/screenie/regions.json`), in the same format as the
//! Python prototype, so the two can share it during the port.

use serde::{Deserialize, Serialize};

use crate::geometry::LogicalRect;
use crate::layout::Monitor;

/// Boxes kept.
pub const SIZE: usize = 10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Entry {
    rect: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monitor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offset: Option<[f64; 2]>,
}

impl Entry {
    fn global(&self) -> LogicalRect {
        let [x, y, w, h] = self.rect;
        LogicalRect::new(x, y, w, h)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ring {
    entries: Vec<Entry>,
}

impl Ring {
    /// The ring from its file's contents. Anything unreadable is an empty
    /// ring, and unreadable entries are dropped: a broken file never
    /// breaks the picker.
    pub fn parse(json: &str) -> Ring {
        let Ok(serde_json::Value::Array(items)) = serde_json::from_str(json) else {
            return Ring::default();
        };
        Ring {
            entries: items
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.entries).unwrap_or_else(|_| "[]".into())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Put an accepted box at the front, dropping an older copy of it.
    pub fn remember(&mut self, monitors: &[Monitor], rect: LogicalRect) {
        let home = monitors
            .iter()
            .find(|m| m.rect.contains(rect.centre()) && !m.name.is_empty());
        let entry = Entry {
            rect: [rect.x, rect.y, rect.w, rect.h],
            monitor: home.map(|m| m.name.clone()),
            offset: home.map(|m| [rect.x - m.rect.x, rect.y - m.rect.y]),
        };
        self.entries.retain(|e| e.rect != entry.rect);
        self.entries.insert(0, entry);
        self.entries.truncate(SIZE);
    }

    /// The ring, newest first, as boxes on today's layout.
    pub fn resolve(&self, monitors: &[Monitor]) -> Vec<LogicalRect> {
        self.entries
            .iter()
            .map(|e| {
                let home = e
                    .monitor
                    .as_ref()
                    .and_then(|name| monitors.iter().find(|m| &m.name == name));
                match (home, e.offset) {
                    (Some(m), Some([ox, oy])) => {
                        LogicalRect::new(m.rect.x + ox, m.rect.y + oy, e.rect[2], e.rect[3])
                    }
                    _ => e.global(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::MonitorId;

    fn mon(name: &str, x: f64) -> Monitor {
        Monitor {
            id: MonitorId(0),
            name: name.into(),
            rect: LogicalRect::new(x, 0.0, 1920.0, 1080.0),
            scale: 1.0,
        }
    }

    fn both() -> Vec<Monitor> {
        vec![mon("DP-1", 0.0), mon("DP-2", 1920.0)]
    }

    fn r(x: f64, y: f64, w: f64, h: f64) -> LogicalRect {
        LogicalRect::new(x, y, w, h)
    }

    #[test]
    fn empty_until_something_is_remembered() {
        assert!(Ring::parse("").resolve(&both()).is_empty());
    }

    #[test]
    fn newest_first_without_repeats_and_capped() {
        let mut ring = Ring::default();
        let n = SIZE as f64;
        for i in 0..SIZE + 3 {
            ring.remember(&both(), r(i as f64, 0.0, 50.0, 50.0));
        }
        ring.remember(&both(), r(n + 1.0, 0.0, 50.0, 50.0)); // again: to the front
        let rects = ring.resolve(&both());
        assert_eq!(rects.len(), SIZE);
        assert_eq!(rects[0].x, n + 1.0);
        assert_eq!(rects[1].x, n + 2.0);
        let unique: std::collections::HashSet<_> = rects.iter().map(|r| r.x as i64).collect();
        assert_eq!(unique.len(), SIZE);
    }

    #[test]
    fn a_box_follows_its_monitor() {
        let mut ring = Ring::default();
        ring.remember(&both(), r(2000.0, 100.0, 300.0, 200.0)); // on DP-2
        let swapped = [mon("DP-2", 0.0), mon("DP-1", 1920.0)]; // rearranged
        assert_eq!(ring.resolve(&swapped), [r(80.0, 100.0, 300.0, 200.0)]);
    }

    #[test]
    fn with_its_monitor_gone_the_global_rect_is_used() {
        let mut ring = Ring::default();
        ring.remember(&both(), r(2000.0, 100.0, 300.0, 200.0));
        assert_eq!(
            ring.resolve(&[mon("DP-1", 0.0)]),
            [r(2000.0, 100.0, 300.0, 200.0)]
        );
    }

    #[test]
    fn a_broken_file_is_an_empty_ring_and_bad_entries_are_dropped() {
        assert!(Ring::parse("{not json").is_empty());
        assert!(Ring::parse(r#"{"rect": [1, 2, 3, 4]}"#).is_empty());
        let ring = Ring::parse(r#"[{"rect": [1, 2, 3]}, 7, {"rect": [1, 2, 3, 4]}]"#);
        assert_eq!(ring.resolve(&both()), [r(1.0, 2.0, 3.0, 4.0)]);
    }

    #[test]
    fn reads_and_writes_the_prototypes_format() {
        let py = r#"[{"rect": [2000.0, 100.0, 300.0, 200.0], "monitor": "DP-2", "offset": [80.0, 100.0]}, {"rect": [5.0, 5.0, 10.0, 10.0]}]"#;
        let ring = Ring::parse(py);
        assert_eq!(ring.len(), 2);
        assert_eq!(Ring::parse(&ring.to_json()), ring);
        assert_eq!(
            ring.to_json(),
            r#"[{"rect":[2000.0,100.0,300.0,200.0],"monitor":"DP-2","offset":[80.0,100.0]},{"rect":[5.0,5.0,10.0,10.0]}]"#
        );
    }
}
