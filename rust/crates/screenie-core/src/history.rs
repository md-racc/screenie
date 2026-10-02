//! Project history: the undo log of Select and project management.
//!
//! It records semantic actions, not input: one entry per selection change
//! (a tap, a whole held-key repeat, a whole drag), and later committed,
//! deleted and moved layers. It is runtime state, never serialized.

use crate::geometry::LogicalRect;

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectEntry {
    /// The selection before a change.
    Selection(LogicalRect),
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

    pub fn pop(&mut self) -> Option<ProjectEntry> {
        self.entries.pop()
    }
}
