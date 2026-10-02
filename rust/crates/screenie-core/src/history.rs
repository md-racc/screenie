//! Undo logs. They record semantic actions, not input, and are runtime
//! state: never serialized.
//!
//! - Project history (Select and project management): one entry per
//!   selection change (a tap, a whole held-key repeat, a whole drag), and
//!   per committed layer; later, deleted and moved layers.
//! - Open-layer history (the build tools, shared): one entry per action on
//!   the open layer, whichever tool made it.

use crate::geometry::LogicalRect;
use crate::model::{Layer, LayerId, Op, OpData, OpId};

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectEntry {
    /// The selection before a change.
    Selection(LogicalRect),
    /// A layer was committed, with the selection as it was then.
    LayerCommitted { layer: LayerId, rect: LogicalRect },
    /// A layer was deleted from this index.
    LayerDeleted { layer: Layer, index: usize },
    /// A layer moved from this index.
    LayerMoved { layer: LayerId, from: usize },
    /// A layer was hidden or shown (undo flips it back).
    LayerToggled { layer: LayerId },
}

impl ProjectEntry {
    /// Whether it's a layer-management action (`u` in Layers).
    pub fn is_layer(&self) -> bool {
        !matches!(self, ProjectEntry::Selection(_))
    }
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

    /// Take out the most recent layer-management entry, wherever it is.
    pub fn pop_layer(&mut self) -> Option<ProjectEntry> {
        let i = self.entries.iter().rposition(ProjectEntry::is_layer)?;
        Some(self.entries.remove(i))
    }
}

/// How to reverse one action on the open layer.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerUndo {
    /// An op was added: take it out.
    Remove(OpId),
    /// An op was deleted from this index: put it back.
    Restore { index: usize, op: Op },
    /// An op was changed (amount, rect, mute): this is what it was.
    Revert { id: OpId, data: OpData },
    /// Two ops swapped places: swap them back.
    Swap(OpId, OpId),
    /// Several ops added as one action (accepted suggestions).
    RemoveMany(Vec<OpId>),
}
