//! Filter (`f`): an ordered stack of image filters in the open layer.
//!
//! A digit inserts a filter just above the selected one (on top if none
//! is) and selects it. `+`/`-` adjust the selected filter's amount. Each
//! filter has its own box, starting as the whole main box; the box shows
//! the selected filter's, and the box grammar reshapes it. The list keys
//! select, move, delete and mute; `f` duplicates. A run of the same edit to
//! the same filter (holding `+`, repeated taps, a drag) is one undo step.

use crate::anchor_cursor::AnchorCursor;
use crate::boxes::{self, BoxOutcome, Drag};
use crate::geometry::LogicalRect;
use crate::history::LayerUndo;
use crate::intent::{Intent, Tool};
use crate::lists;
use crate::model::{FilterKind, FilterOp, OpData, OpId};
use crate::session::{Session, Transition};

/// The filter kinds by key: kind, starting amount, step, min, max.
pub const KINDS: [(char, FilterKind, f64, f64, f64, f64); 5] = [
    ('1', FilterKind::Grey, 1.0, 0.25, 0.25, 1.0),
    ('2', FilterKind::Invert, 1.0, 0.0, 1.0, 1.0),
    ('3', FilterKind::Brightness, 1.0, 1.0, -5.0, 5.0),
    ('4', FilterKind::Contrast, 1.0, 1.0, -4.0, 4.0),
    ('5', FilterKind::Blur, 4.0, 2.0, 2.0, 20.0),
];

fn spec(kind: FilterKind) -> (f64, f64, f64, f64) {
    let (_, _, start, step, lo, hi) = KINDS
        .iter()
        .copied()
        .find(|k| k.1 == kind)
        .expect("every kind is listed");
    (start, step, lo, hi)
}

/// "grey 75%", "brightness -2", "blur 6px", "invert  (muted)".
pub fn describe(f: &FilterOp) -> String {
    let v = f.amount;
    let text = match f.kind {
        FilterKind::Grey => format!("grey {}%", (v * 100.0).round()),
        FilterKind::Invert => "invert".into(),
        FilterKind::Brightness => format!("brightness {:+}", v as i64),
        FilterKind::Contrast => format!("contrast {:+}", v as i64),
        FilterKind::Blur => format!("blur {v}px"),
    };
    if f.muted { text + "  (muted)" } else { text }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    Amount,
    Rect,
}

#[derive(Clone, Debug)]
pub struct FilterMode {
    /// The selected filter's box (the whole main box with none selected).
    pub sel: AnchorCursor,
    pub selected: Option<OpId>,
    /// An ongoing run of one edit to one filter: (filter, edit, the layer
    /// log's length after its entry).
    run: Option<(OpId, Edit, usize)>,
    pub(crate) drag: Option<Drag>,
}

impl FilterMode {
    pub fn new(main: LogicalRect) -> Self {
        Self {
            sel: AnchorCursor::new(main, main),
            selected: None,
            run: None,
            drag: None,
        }
    }
}

impl Session {
    /// The open layer's filters, bottom first.
    pub fn filter_stack(&self) -> Vec<OpId> {
        self.open_layer().map_or_else(Vec::new, |l| {
            l.ops
                .iter()
                .filter(|o| matches!(o.data, OpData::Filter(_)))
                .map(|o| o.id)
                .collect()
        })
    }

    pub fn filter(&self, id: OpId) -> Option<&FilterOp> {
        let layer = self.open_layer()?;
        match &layer.ops[layer.op_index(id)?].data {
            OpData::Filter(f) => Some(f),
            _ => None,
        }
    }

    fn filter_mut(&mut self, id: OpId) -> Option<&mut FilterOp> {
        let layer = &mut self.open.as_mut()?.layer;
        let i = layer.op_index(id)?;
        match &mut layer.ops[i].data {
            OpData::Filter(f) => Some(f),
            _ => None,
        }
    }

    /// Select a filter: the box shows its rect.
    fn select_filter(&self, f: &mut FilterMode, id: Option<OpId>) {
        f.selected = id;
        if let Some(op) = id.and_then(|id| self.filter(id)) {
            f.sel.set_rect(op.rect);
        }
    }

    /// Change a filter, as one undo step per run of the same edit.
    fn edit_filter(&mut self, f: &mut FilterMode, edit: Edit, change: impl FnOnce(&mut FilterOp)) {
        let Some(id) = f.selected else { return };
        let Some(before) = self.filter(id).copied() else {
            return;
        };
        let mut after = before;
        change(&mut after);
        if after == before {
            return;
        }
        *self.filter_mut(id).expect("just read") = after;
        let len = self.layer_log_len();
        if f.run != Some((id, edit, len)) {
            self.log_layer(LayerUndo::Revert {
                id,
                data: OpData::Filter(before),
            });
            f.run = Some((id, edit, self.layer_log_len()));
        }
    }

    fn insert_filter(&mut self, f: &mut FilterMode, op: FilterOp) {
        let id = self.insert_op(OpData::Filter(op), f.selected);
        f.run = None;
        self.select_filter(f, Some(id));
    }

    pub(crate) fn filter_intent(&mut self, f: &mut FilterMode, intent: Intent) -> Transition {
        match intent {
            Intent::Undo => {
                self.undo_layer();
                f.run = None;
                // The selected filter may be gone: then the top one.
                let stack = self.filter_stack();
                let keep = f.selected.filter(|id| stack.contains(id));
                self.select_filter(f, keep.or(stack.last().copied()));
                return Transition::Stay;
            }
            Intent::Tool(Tool::Filter) => {
                // Duplicate the selected filter, just above it.
                if let Some(op) = f.selected.and_then(|id| self.filter(id)).copied() {
                    self.insert_filter(f, op);
                }
                return Transition::Stay;
            }
            Intent::Tool(other) => return Transition::SwitchTool(self.tool_mode(other)),
            Intent::Confirm => return self.close_layer(true),
            Intent::Back => return self.back_from_tool(),
            _ => {}
        }
        if let Some(t) = self.lens_at(intent, f.sel.rect().centre()) {
            return t;
        }
        let mods = Some(self.input_mods());
        match boxes::apply(&mut f.sel, &self.snap, &mut f.drag, intent, mods) {
            BoxOutcome::NotMine => {}
            outcome => {
                if let BoxOutcome::Click(at) = outcome {
                    f.sel.place_cursor(at);
                }
                // Box edits reshape the selected filter.
                let rect = f.sel.rect();
                self.edit_filter(f, Edit::Rect, |op| op.rect = rect);
                return Transition::Stay;
            }
        }
        match intent {
            Intent::Choose(n) => {
                if let Some(&(_, kind, amount, ..)) = KINDS.get(usize::from(n).wrapping_sub(1)) {
                    let rect = f.sel.rect();
                    self.insert_filter(
                        f,
                        FilterOp {
                            rect,
                            kind,
                            amount,
                            muted: false,
                        },
                    );
                }
            }
            Intent::Adjust(n) => {
                let Some(op) = f.selected.and_then(|id| self.filter(id)).copied() else {
                    self.note = Some("pick a filter: 1-5".into());
                    return Transition::Stay;
                };
                let (_, step, lo, hi) = spec(op.kind);
                let amount = (op.amount + step * f64::from(n)).clamp(lo, hi);
                self.edit_filter(f, Edit::Amount, |op| op.amount = amount);
            }
            Intent::ListStep(d) => {
                let stack = self.filter_stack();
                let next = lists::step(&stack, f.selected, d);
                if next.is_some() {
                    f.run = None;
                    self.select_filter(f, next);
                }
            }
            Intent::ListMove(d) => {
                let stack = self.filter_stack();
                if let (Some(a), Some(b)) = (f.selected, lists::swap_with(&stack, f.selected, d)) {
                    self.swap_ops(a, b);
                    f.run = None;
                }
            }
            Intent::ListDelete => {
                let stack = self.filter_stack();
                if let Some(i) = f
                    .selected
                    .and_then(|id| stack.iter().position(|&x| x == id))
                {
                    self.delete_op(stack[i]);
                    f.run = None;
                    self.select_filter(f, lists::after_delete(&stack, i));
                }
            }
            Intent::ListToggle => {
                if let Some(id) = f.selected {
                    if let Some(before) = self.filter(id).copied() {
                        self.filter_mut(id).expect("just read").muted = !before.muted;
                        self.log_layer(LayerUndo::Revert {
                            id,
                            data: OpData::Filter(before),
                        });
                        f.run = None;
                    }
                }
            }
            _ => {}
        }
        Transition::Stay
    }

    /// The stack for the list tag, top first, the selected one marked.
    pub(crate) fn filter_list(&self, f: &FilterMode) -> Vec<String> {
        self.filter_stack()
            .iter()
            .rev()
            .filter_map(|&id| {
                let op = self.filter(id)?;
                let mark = if Some(id) == f.selected { ">" } else { " " };
                Some(format!("{mark} {}", describe(op)))
            })
            .collect()
    }

    pub(crate) fn filter_status(&self, f: &FilterMode) -> String {
        let stack = self.filter_stack();
        let Some((i, op)) = f
            .selected
            .and_then(|id| Some((stack.iter().position(|&x| x == id)?, self.filter(id)?)))
        else {
            return "raw".into();
        };
        let b = self.pixels.to_pixels(f.sel.rect());
        format!(
            "{}/{} {}  {}×{}",
            i + 1,
            stack.len(),
            describe(op),
            b.w,
            b.h
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::LogicalRect;

    fn op(kind: FilterKind, amount: f64) -> FilterOp {
        FilterOp {
            rect: LogicalRect::default(),
            kind,
            amount,
            muted: false,
        }
    }

    #[test]
    fn descriptions() {
        assert_eq!(describe(&op(FilterKind::Grey, 0.75)), "grey 75%");
        assert_eq!(describe(&op(FilterKind::Brightness, -2.0)), "brightness -2");
        assert_eq!(describe(&op(FilterKind::Contrast, 4.0)), "contrast +4");
        assert_eq!(describe(&op(FilterKind::Blur, 6.0)), "blur 6px");
        let mut i = op(FilterKind::Invert, 1.0);
        i.muted = true;
        assert_eq!(describe(&i), "invert  (muted)");
    }
}
