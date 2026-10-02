//! Redact (`r` in Censor): review auto-redact's suggestions, then accept
//! the ticked ones as ordinary censors. Scanning runs as an analysis job,
//! so the picker stays live while OCR works.

use crate::geometry::{LogicalRect, PixelRect};
use crate::history::LayerUndo;
use crate::intent::Intent;
use crate::model::{CensorKind, CensorOp, OpData};
use crate::redact::{Category, Proposal};
use crate::session::{JobKind, JobResult, Mode, Session, Transition};

/// Padding of a suggestion, as a share of its line's height.
pub const PAD: f64 = 0.2;

/// A suggestion, in logical coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub rect: LogicalRect,
    pub category: Category,
    pub label: String,
    pub ticked: bool,
}

#[derive(Clone, Debug)]
pub struct RedactMode {
    pub suggestions: Vec<Suggestion>,
    pub selected: usize,
    /// The scan in flight, if any.
    pub scanning: Option<u64>,
    /// What went wrong with the last scan.
    pub failed: Option<String>,
}

impl Session {
    /// A pixel rect as logical, within the main box.
    fn logical(&self, r: PixelRect) -> LogicalRect {
        let a = self.pixels.from_device(f64::from(r.x), f64::from(r.y));
        let b = self
            .pixels
            .from_device(f64::from(r.x + r.w), f64::from(r.y + r.h));
        let m = self.sel.rect();
        let (x0, y0) = (a.x.max(m.x), a.y.max(m.y));
        let (x1, y1) = (b.x.min(m.right()), b.y.min(m.bottom()));
        LogicalRect::from_edges(x0, y0, x1.max(x0), y1.max(y0))
    }

    /// Start scanning the box. A synchronous analyzer answers at once.
    pub(crate) fn scan(&mut self, m: &mut RedactMode) {
        let rect = self.pixels.to_pixels(self.sel.rect());
        m.failed = None;
        match self.start_job(JobKind::Ocr { rect }) {
            (id, None) => m.scanning = Some(id),
            (_, Some(result)) => {
                m.scanning = None;
                self.take_scan(m, result);
            }
        }
    }

    pub(crate) fn take_scan(&mut self, m: &mut RedactMode, result: JobResult) {
        match result {
            JobResult::Ocr(Ok(ocr)) => {
                let found: Vec<Proposal> = crate::redact::propose(&ocr, &self.redact_env, PAD);
                m.suggestions = found
                    .into_iter()
                    .map(|p| Suggestion {
                        rect: self.logical(p.rect),
                        category: p.category,
                        label: p.label,
                        ticked: p.ticked,
                    })
                    .filter(|s| s.rect.w > 0.0 && s.rect.h > 0.0)
                    .collect();
                // In reading order, which is how the list is shown.
                m.suggestions.sort_by(|a, b| {
                    (a.rect.y, a.rect.x)
                        .partial_cmp(&(b.rect.y, b.rect.x))
                        .unwrap()
                });
                m.selected = 0;
            }
            JobResult::Ocr(Err(e)) => m.failed = Some(e),
        }
    }

    /// An analysis job finished (from the host, for an asynchronous
    /// analyzer). Results nobody waits for any more are dropped.
    pub fn job_done(&mut self, id: u64, result: JobResult) {
        let Some(i) = self
            .stack_mut()
            .iter()
            .position(|m| matches!(m, Mode::Redact(r) if r.scanning == Some(id)))
        else {
            return; // stale: rescanned, or the review is gone
        };
        let Mode::Redact(mut m) = self.stack_mut().remove(i) else {
            unreachable!()
        };
        m.scanning = None;
        self.take_scan(&mut m, result);
        self.stack_mut().insert(i, Mode::Redact(m));
    }

    /// Accept the ticked suggestions as black-bar censors: one undo step.
    fn accept(&mut self, m: &mut RedactMode) {
        let mut ids = Vec::new();
        for s in m.suggestions.iter().filter(|s| s.ticked) {
            let id = self.insert_op(
                OpData::Censor(CensorOp {
                    rect: s.rect,
                    kind: CensorKind::Bar,
                    amount: 0.0,
                }),
                None,
            );
            ids.push(id);
            self.redacted.push((id, s.category.name(), s.rect));
        }
        if !ids.is_empty() {
            self.merge_layer_log(ids.len(), LayerUndo::RemoveMany(ids));
        }
        m.suggestions.retain(|s| !s.ticked);
    }

    /// `--redact suggest|auto`: open Censor on the box and scan it. With
    /// `auto`, accept what's ticked and commit the layer, so the analyzer
    /// must answer at once. A failed scan is an error for `auto`.
    pub fn start_redact(&mut self, auto: bool) -> Result<(), String> {
        self.dispatch(Intent::Tool(crate::intent::Tool::Censor));
        self.dispatch(Intent::Redact);
        if !auto {
            return Ok(());
        }
        if let Some(Mode::Redact(r)) = self.modes().last() {
            if let Some(e) = &r.failed {
                return Err(e.clone());
            }
            if r.scanning.is_some() {
                return Err("auto-redact needs an analyzer that answers at once".into());
            }
        }
        self.dispatch(Intent::Accept);
        let t = self.close_layer(true);
        self.apply(t);
        Ok(())
    }

    pub(crate) fn redact_intent(&mut self, m: &mut RedactMode, intent: Intent) -> Transition {
        let n = m.suggestions.len();
        match intent {
            Intent::ListStep(d) if n > 0 => {
                // Listed top to bottom: ] (up) is towards the first.
                m.selected = (m.selected as i64 - i64::from(d)).clamp(0, n as i64 - 1) as usize;
            }
            Intent::ListToggle if n > 0 => {
                let s = &mut m.suggestions[m.selected];
                s.ticked = !s.ticked;
            }
            Intent::ListDelete if n > 0 => {
                // Listed top first, so the next row down takes its index.
                m.suggestions.remove(m.selected);
                m.selected = m.selected.min(m.suggestions.len().saturating_sub(1));
            }
            Intent::Accept | Intent::Confirm => {
                self.accept(m);
                return Transition::Pop;
            }
            Intent::Redact => self.scan(m),
            Intent::Back => return Transition::Pop,
            _ => {}
        }
        Transition::Stay
    }

    pub(crate) fn redact_status(&self, m: &RedactMode) -> String {
        if m.scanning.is_some() {
            return "scanning…".into();
        }
        if let Some(e) = &m.failed {
            return format!("scan failed: {e}");
        }
        let ticked = m.suggestions.iter().filter(|s| s.ticked).count();
        format!("{} found ({ticked} ticked)", m.suggestions.len())
    }

    pub(crate) fn redact_list(&self, m: &RedactMode) -> Vec<String> {
        m.suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mark = if i == m.selected { ">" } else { " " };
                let tick = if s.ticked { "✓" } else { "·" };
                format!("{mark} {tick} {:<10} {}", s.category.name(), s.label)
            })
            .collect()
    }
}
