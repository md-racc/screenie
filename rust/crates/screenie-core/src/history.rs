//! Undo logs. They record semantic actions, not input, and are runtime
//! state: never serialized.
//!
//! - Project history (Select and project management): one entry per
//!   selection change (a tap, a whole held-key repeat, a whole drag), and
//!   per committed layer; later, deleted and moved layers.
//! - Open-layer history (the build tools, shared): one entry per action on
//!   the open layer, whichever tool made it.

use crate::geometry::LogicalRect;
use crate::model::{LayerId, OpId};

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectEntry {
    /// The selection before a change.
    Selection(LogicalRect),
    /// A layer was committed, with the selection as it was then.
    LayerCommitted { layer: LayerId, rect: LogicalRect },
}

#[derive(Clone, Debug, Default)]
pub struct ProjectHistory {
    entries: Vec<ProjectEntry>,
}

impl ProjectHistory {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record the selection as it is before a change, unless the last
    /// entry already holds exactly that.
    pub fn remember_selection(&mut self, rect: LogicalRect) {
        if self.entries.last() != Some(&ProjectEntry::Selection(rect)) {
            self.entries.push(ProjectEntry::Selection(rect));
        }
    }

    pub fn push(&mut self, e: ProjectEntry) {
        self.entries.push(e);
    }

    pub fn pop(&mut self) -> Option<ProjectEntry> {
        self.entries.pop()
    }
}

/// How to reverse one action on the open layer.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerUndo {
    /// An op was locked in: take it out.
    Remove(OpId),
}
