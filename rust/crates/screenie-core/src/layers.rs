//! Layers (`1`-`9` in Select): inspect and manage the committed layers.
//!
//! The selected layer is shown on its own, in its clip: the capture with
//! only that layer applied. The list grammar picks, moves, deletes and
//! hides; every change is one entry in project history, and `u` here
//! undoes the last layer-management action.

use crate::history::ProjectEntry;
use crate::intent::Intent;
use crate::lists;
use crate::model::{Layer, LayerId};
use crate::session::{Session, Transition};

#[derive(Clone, Debug)]
pub struct LayersMode {
    pub selected: Option<LayerId>,
}

impl Session {
    fn layer_ids(&self) -> Vec<LayerId> {
        self.project.layers.iter().map(|l| l.id).collect()
    }

    /// Layers opened at layer `index` (0 is the bottom), clamped.
    pub(crate) fn layers_at(&self, index: usize) -> LayersMode {
        let ids = self.layer_ids();
        LayersMode {
            selected: ids.get(index.min(ids.len().saturating_sub(1))).copied(),
        }
    }

    /// The layer Layers is showing on its own, if it's the top mode.
    pub fn inspected(&self) -> Option<&Layer> {
        let crate::session::Mode::Layers(m) = self.modes().last()? else {
            return None;
        };
        let i = self.project.layer_index(m.selected?)?;
        Some(&self.project.layers[i])
    }

    /// Reverse one project-history entry. Returns whether it changed
    /// anything. `in_order` is false when Layers reaches past later
    /// selection changes: then the box stays as it is.
    pub(crate) fn undo_layer_entry(&mut self, entry: ProjectEntry, in_order: bool) -> bool {
        let layers = &mut self.project.layers;
        match entry {
            ProjectEntry::Selection(r) => {
                let changed = r != self.sel.rect();
                self.sel.set_rect(r);
                changed
            }
            ProjectEntry::LayerCommitted { layer, rect } => {
                // (it may have been deleted since)
                let i = layers.iter().position(|l| l.id == layer);
                if let Some(i) = i {
                    layers.remove(i);
                }
                let moved = in_order && rect != self.sel.rect();
                if in_order {
                    self.sel.set_rect(rect);
                }
                i.is_some() || moved
            }
            ProjectEntry::LayerDeleted { layer, index } => {
                layers.insert(index.min(layers.len()), layer);
                true
            }
            ProjectEntry::LayerMoved { layer, from } => {
                let Some(i) = layers.iter().position(|l| l.id == layer) else {
                    return false;
                };
                let l = layers.remove(i);
                layers.insert(from.min(layers.len()), l);
                i != from
            }
            ProjectEntry::LayerToggled { layer } => {
                let Some(l) = layers.iter_mut().find(|l| l.id == layer) else {
                    return false;
                };
                l.hidden = !l.hidden;
                true
            }
        }
    }

    pub(crate) fn layers_intent(&mut self, m: &mut LayersMode, intent: Intent) -> Transition {
        let ids = self.layer_ids();
        let index = m.selected.and_then(|id| ids.iter().position(|&x| x == id));
        match intent {
            Intent::Choose(n) if n >= 1 && !ids.is_empty() => {
                m.selected = ids.get((usize::from(n) - 1).min(ids.len() - 1)).copied();
            }
            Intent::ListStep(d) | Intent::Adjust(d) => {
                m.selected = lists::step(&ids, m.selected, d).or(m.selected);
            }
            Intent::Press(at) => {
                // The topmost layer under the pointer.
                let hit = self
                    .project
                    .layers
                    .iter()
                    .rev()
                    .find(|l| l.bbox().is_some_and(|b| b.contains(at)));
                if let Some(l) = hit {
                    m.selected = Some(l.id);
                }
            }
            Intent::ListMove(d) => {
                if let (Some(i), Some(other)) = (index, lists::swap_with(&ids, m.selected, d)) {
                    let j = ids.iter().position(|&x| x == other).expect("listed");
                    self.project.layers.swap(i, j);
                    self.history.push(ProjectEntry::LayerMoved {
                        layer: ids[i],
                        from: i,
                    });
                }
            }
            Intent::ListDelete => {
                if let Some(i) = index {
                    let layer = self.project.layers.remove(i);
                    self.history
                        .push(ProjectEntry::LayerDeleted { layer, index: i });
                    m.selected = lists::after_delete(&ids, i);
                    if m.selected.is_none() {
                        return Transition::Pop; // the last layer: back out
                    }
                }
            }
            Intent::ListToggle => {
                if let Some(i) = index {
                    let l = &mut self.project.layers[i];
                    l.hidden = !l.hidden;
                    self.history
                        .push(ProjectEntry::LayerToggled { layer: l.id });
                }
            }
            Intent::Undo => {
                // The last layer-management action, wherever it is.
                if let Some(entry) = self.history.pop_layer() {
                    self.undo_layer_entry(entry, false);
                }
                let ids = self.layer_ids();
                if ids.is_empty() {
                    return Transition::Pop;
                }
                if !m.selected.is_some_and(|id| ids.contains(&id)) {
                    m.selected = ids.get(index.unwrap_or(0).min(ids.len() - 1)).copied();
                }
            }
            Intent::Confirm | Intent::Back => return Transition::Pop,
            _ => {}
        }
        Transition::Stay
    }

    /// The layer list, top first, the selected one marked.
    pub(crate) fn layers_list(&self, m: &LayersMode) -> Vec<String> {
        self.project
            .layers
            .iter()
            .enumerate()
            .rev()
            .map(|(i, l)| {
                let mark = if Some(l.id) == m.selected { ">" } else { " " };
                let hidden = if l.hidden { "  (hidden)" } else { "" };
                format!("{mark} {}  {}{hidden}", i + 1, l.describe())
            })
            .collect()
    }

    pub(crate) fn layers_status(&self, m: &LayersMode) -> String {
        let n = self.project.layers.len();
        match m.selected.and_then(|id| self.project.layer_index(id)) {
            Some(i) => {
                let l = &self.project.layers[i];
                let hidden = if l.hidden { "  hidden" } else { "" };
                format!("{}/{n}  {}{hidden}", i + 1, l.describe())
            }
            None => "no layers".into(),
        }
    }
}
