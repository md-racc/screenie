//! The session: one picker run, from raw input to a finished action.
//!
//! The host (an overlay, a test, a `--keys` script) feeds raw events and
//! clock ticks in, and reads the outcome, the project, the boxes and the
//! status line out. Nothing here draws or touches the desktop.
//!
//! Select is always the base mode; other modes sit on a stack above it,
//! and only the top one gets input. Modes don't touch the stack: they
//! return a `Transition`, which the session carries out. The open layer,
//! both undo logs and the Esc/q warnings belong to the session, so they
//! survive tool changes.

use crate::anchor_cursor::AnchorCursor;
use crate::censor::CensorMode;
use crate::geometry::{Axis, End, LogicalPoint, LogicalRect, PixelMap, PixelPoint};
use crate::history::{LayerUndo, ProjectEntry, ProjectHistory};
use crate::input::{InputState, Press};
use crate::intent::Intent;
use crate::keymap::{self, Keymap};
use crate::keys::{Chord, Key, Mods};
use crate::layout::Layout;
use crate::model::{Layer, Op, OpData, Project};
use crate::select::SelectState;
use crate::targets::SnapEnv;

pub use crate::boxes::STEP;

/// Physical input, as a platform reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RawEvent {
    /// `code` identifies the physical key (for held-key tracking); `key`
    /// is what it produced; `mods` is the modifier state including it.
    KeyPress {
        code: u32,
        key: Key,
        mods: Mods,
    },
    /// `mods` is the modifier state after the release.
    KeyRelease {
        code: u32,
        key: Key,
        mods: Mods,
    },
    /// The modifier state changed (Wayland reports it apart from keys).
    Modifiers {
        mods: Mods,
    },
    /// The keyboard's repeat settings, for keys whose action repeats.
    RepeatInfo {
        rate: f64,
        delay: f64,
    },
    ButtonPress {
        button: Button,
        at: LogicalPoint,
    },
    ButtonRelease {
        button: Button,
        at: LogicalPoint,
    },
    Motion {
        at: LogicalPoint,
    },
    /// Wheel notches: positive is up (`+`), negative down (`-`).
    Wheel {
        steps: i32,
    },
    /// The overlay lost the keyboard: forget held keys.
    FocusLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    Other(u32),
}

/// What the picker produced. The front end delivers it.
#[derive(Clone, Debug, PartialEq)]
pub enum FinishedAction {
    Image { rect: LogicalRect },
    Ocr { rect: LogicalRect },
    Colour { pixel: PixelPoint },
    Code { rect: LogicalRect },
    Pin { rect: LogicalRect },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Done(FinishedAction),
    /// Esc or q: the user's decision, not an error.
    Cancelled,
}

/// Everything a session starts from.
#[derive(Clone, Debug)]
pub struct Setup {
    pub layout: Layout,
    pub pixels: PixelMap,
    /// The initial box; `None` starts on the active window.
    pub start: Option<LogicalRect>,
    /// The recall ring (`.`), newest first, resolved to this layout.
    pub recall: Vec<LogicalRect>,
}

/// The modes that sit above Select.
#[derive(Clone, Debug)]
pub enum Mode {
    Censor(CensorMode),
}

impl Mode {
    pub fn name(&self) -> &'static str {
        match self {
            Mode::Censor(_) => "censor",
        }
    }
}

/// What a mode asks the session to do after an intent.
#[derive(Debug)]
pub enum Transition {
    Stay,
    /// Open a build tool: the open layer is created first if there's none.
    OpenTool(Mode),
    /// Back to the mode at this depth (0 is Select).
    PopTo(usize),
    Finish(Outcome),
}

/// The layer the build tools are adding to, until Enter or Esc.
#[derive(Clone, Debug)]
pub(crate) struct OpenLayer {
    pub(crate) layer: Layer,
    /// The stack depth the first tool was opened at: Enter and Esc return
    /// to the mode below it.
    depth: usize,
    log: Vec<LayerUndo>,
}

/// A warning waiting for its confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Esc once with locked work: Esc again throws it away.
    Discard,
    /// q once with locked work: q again quits.
    Quit,
}

/// A box drawn by a mode above Select, for the host to show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxView {
    pub rect: LogicalRect,
    pub cursor: LogicalPoint,
    pub active: bool,
}

pub struct Session {
    pub(crate) layout: Layout,
    pub(crate) pixels: PixelMap,
    pub(crate) snap: SnapEnv,
    pub(crate) sel: AnchorCursor,
    pub(crate) select: SelectState,
    pub(crate) history: ProjectHistory,
    pub(crate) recall: Vec<LogicalRect>,
    pub(crate) note: Option<String>,
    pub(crate) project: Project,
    pub(crate) open: Option<OpenLayer>,
    stack: Vec<Mode>,
    pending: Option<Pending>,
    input: InputState,
    show_help: bool,
    outcome: Option<Outcome>,
    pointer: LogicalPoint,
}

fn corner_pixel(pixels: &PixelMap, s: &AnchorCursor) -> PixelPoint {
    if s.is_centred() {
        return pixels.pixel_under(s.rect().centre());
    }
    pixels.edge_pixel(
        s.cursor_point(),
        s.cursor_end(Axis::X) == End::High,
        s.cursor_end(Axis::Y) == End::High,
    )
}

impl Session {
    pub fn new(setup: Setup) -> Self {
        let Setup {
            layout,
            pixels,
            start,
            recall,
        } = setup;
        let start = start.unwrap_or_else(|| layout.start_rect());
        let sel = AnchorCursor::new(start, layout.bounds);
        Self {
            snap: SnapEnv::from_layout(&layout),
            project: Project::new(sel.rect()),
            sel,
            select: SelectState::default(),
            history: ProjectHistory::default(),
            input: InputState::default(),
            layout,
            pixels,
            recall,
            note: None,
            open: None,
            stack: Vec::new(),
            pending: None,
            show_help: false,
            outcome: None,
            pointer: LogicalPoint::default(),
        }
    }

    // --- what the host reads ------------------------------------------------

    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    pub fn is_finished(&self) -> bool {
        self.outcome.is_some()
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn pixels(&self) -> &PixelMap {
        &self.pixels
    }

    /// The main box.
    pub fn selection(&self) -> &AnchorCursor {
        &self.sel
    }

    pub fn rect(&self) -> LogicalRect {
        self.sel.rect()
    }

    /// The project: the committed layers, with the selection as it is now.
    pub fn project(&self) -> Project {
        let mut p = self.project.clone();
        p.selection = self.sel.rect();
        p
    }

    /// The committed layers, bottom first.
    pub fn layers(&self) -> &[Layer] {
        &self.project.layers
    }

    /// The open layer's locked work, if a layer is open.
    pub fn open_layer(&self) -> Option<&Layer> {
        self.open.as_ref().map(|o| &o.layer)
    }

    /// The item the top build tool is editing, not yet locked in.
    pub fn editing(&self) -> Option<OpData> {
        match self.stack.last()? {
            Mode::Censor(c) => c.current(),
        }
    }

    /// The modes above Select, bottom first.
    pub fn modes(&self) -> &[Mode] {
        &self.stack
    }

    /// The boxes of the modes above Select, bottom first.
    pub fn boxes(&self) -> Vec<BoxView> {
        let n = self.stack.len();
        self.stack
            .iter()
            .enumerate()
            .map(|(i, m)| match m {
                Mode::Censor(c) => BoxView {
                    rect: c.sel.rect(),
                    cursor: c.sel.cursor_point(),
                    active: i + 1 == n,
                },
            })
            .collect()
    }

    /// Whether Select is the top mode.
    pub fn select_is_top(&self) -> bool {
        self.stack.is_empty()
    }

    /// The top mode's name.
    pub fn mode_name(&self) -> &'static str {
        self.stack.last().map_or("select", Mode::name)
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn pending(&self) -> Option<Pending> {
        self.pending
    }

    /// Whether `?` has the key legend up.
    pub fn show_help(&self) -> bool {
        self.show_help
    }

    /// Whether held keys need `tick` calls.
    pub fn wants_ticks(&self) -> bool {
        self.input.repeating()
    }

    /// The pointer's last position.
    pub fn pointer(&self) -> LogicalPoint {
        self.pointer
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// The pixel the active cursor is on, for the status line.
    pub fn cursor_pixel(&self) -> PixelPoint {
        match self.stack.last() {
            None => corner_pixel(&self.pixels, &self.sel),
            Some(Mode::Censor(c)) => corner_pixel(&self.pixels, &c.sel),
        }
    }

    /// The status tag: the mode (vim-style), the box size in output
    /// pixels, the active cursor's pixel, then whatever's worth knowing.
    pub fn status(&self) -> String {
        let px = self.pixels.to_pixels(self.sel.rect());
        let c = self.cursor_pixel();
        let mut s = format!(
            "{}  {}×{}  ({},{})",
            self.mode_name().to_uppercase(),
            px.w,
            px.h,
            c.x,
            c.y
        );
        if self.sel.is_centred() {
            s.push_str("  centre");
        }
        let locked = self.open.as_ref().map_or(0, |o| o.layer.ops.len());
        if self.open.is_some() {
            if locked > 0 {
                s.push_str(&format!("  layer +{locked}"));
            } else {
                s.push_str("  layer");
            }
        }
        if let Some(Mode::Censor(c)) = self.stack.last() {
            let b = self.pixels.to_pixels(c.sel.rect());
            s.push_str(&format!("  {}  {}×{}", c.describe(), b.w, b.h));
            if c.sel.is_centred() {
                s.push_str("  centre");
            }
        }
        match self.pending {
            Some(Pending::Discard) => s.push_str(&format!("  Esc again to discard {locked}")),
            Some(Pending::Quit) => s.push_str(&format!("  q again to quit, discarding {locked}")),
            None => {}
        }
        if let Some(n) = &self.note {
            s.push_str(&format!("  ({n})"));
        }
        s
    }

    /// The top mode's keymap.
    pub fn keymap(&self) -> &'static Keymap {
        match self.stack.last() {
            None => &keymap::SELECT,
            Some(Mode::Censor(_)) => &keymap::CENSOR,
        }
    }

    /// The top mode's key legend, as shown behind `?`.
    pub fn legend(&self) -> Vec<String> {
        let lines = match self.stack.last() {
            None => crate::legend::SELECT,
            Some(Mode::Censor(_)) => crate::legend::CENSOR,
        };
        crate::legend::render(self.keymap(), lines)
    }

    // --- input ------------------------------------------------------------------

    /// Feed one raw event in, at time `now` (seconds, monotonic).
    pub fn handle(&mut self, ev: RawEvent, now: f64) {
        if self.is_finished() {
            return;
        }
        match ev {
            RawEvent::KeyPress { code, key, mods } => {
                if self.input.press(code, key, mods, now) != Press::Fresh {
                    return;
                }
                let chord = Chord::new(key, mods);
                let session = keymap::SESSION.lookup(chord);
                let intent = session.or_else(|| self.keymap().lookup(chord));
                self.user_acted(intent);
                match (session, intent) {
                    (Some(i), _) => self.session_intent(i),
                    (None, Some(i)) => {
                        let repeats = self.repeats(i);
                        self.dispatch(i);
                        if repeats && !self.is_finished() {
                            self.input.repeat_key(code, i, now);
                        }
                    }
                    (None, None) => {}
                }
            }
            RawEvent::KeyRelease { code, mods, .. } => self.input.release(code, mods),
            RawEvent::Modifiers { mods } => self.input.set_mods(mods),
            RawEvent::RepeatInfo { rate, delay } => {
                self.input
                    .set_key_repeat(crate::input::KeyRepeat { rate, delay });
            }
            RawEvent::ButtonPress { button, at } => {
                self.pointer = at;
                let intent = match button {
                    Button::Left => Some(Intent::Press(at)),
                    Button::Right => Some(Intent::Confirm),
                    Button::Middle => Some(Intent::Back),
                    Button::Other(_) => None,
                };
                self.user_acted(intent);
                if let Some(i) = intent {
                    self.dispatch(i);
                }
            }
            RawEvent::ButtonRelease { button, at } => {
                self.pointer = at;
                if button == Button::Left {
                    self.dispatch(Intent::Release(at));
                }
            }
            RawEvent::Motion { at } => {
                self.pointer = at;
                self.dispatch(Intent::Motion(at));
            }
            RawEvent::Wheel { steps } => {
                self.user_acted(Some(Intent::Adjust(steps.signum())));
                for _ in 0..steps.unsigned_abs() {
                    self.dispatch(Intent::Adjust(steps.signum()));
                }
            }
            RawEvent::FocusLost => self.input.reset(),
        }
    }

    /// Whether holding the key for this intent repeats it, in the top mode.
    fn repeats(&self, intent: Intent) -> bool {
        matches!(
            (self.stack.last(), intent),
            (Some(Mode::Censor(_)), Intent::Adjust(_))
        )
    }

    /// Advance held-key repeat to `now`. Returns whether anything moved.
    pub fn tick(&mut self, now: f64) -> bool {
        if self.is_finished() {
            return false;
        }
        if let Some((intent, n)) = self.input.key_repeats(now) {
            for _ in 0..n {
                self.dispatch(intent);
            }
            return true;
        }
        let mods = self.input.mods();
        let rates = if mods.ctrl || mods.alt {
            None // jumps and scaling are one-shot
        } else {
            Some(crate::boxes::RATES)
        };
        match self.input.repeat(now, rates) {
            Some((dirs, n)) if mods.shift => self.dispatch(Intent::HeldTranslate(dirs, n)),
            Some((dirs, n)) => self.dispatch(Intent::HeldSteer(dirs, n)),
            None => return false,
        }
        true
    }

    /// Any fresh input clears the one-off note, and takes back a pending
    /// warning unless it's the same key again.
    fn user_acted(&mut self, intent: Option<Intent>) {
        self.note = None;
        let keeps = match self.pending {
            Some(Pending::Discard) => intent == Some(Intent::Back),
            Some(Pending::Quit) => intent == Some(Intent::Quit),
            None => true,
        };
        if !keeps {
            self.pending = None;
        }
    }

    fn locked_work(&self) -> bool {
        self.open.as_ref().is_some_and(|o| !o.layer.ops.is_empty())
    }

    fn session_intent(&mut self, intent: Intent) {
        match intent {
            Intent::Quit => {
                // With locked work in the open layer, only on a second q.
                if self.locked_work() && self.pending != Some(Pending::Quit) {
                    self.pending = Some(Pending::Quit);
                } else {
                    self.finish(Outcome::Cancelled);
                }
            }
            Intent::ToggleHelp => self.show_help = !self.show_help,
            _ => {}
        }
    }

    /// Send an intent to the top mode, and carry out its transition.
    pub fn dispatch(&mut self, intent: Intent) {
        if self.is_finished() {
            return;
        }
        let t = match self.stack.pop() {
            None => self.select_intent(intent),
            Some(mut mode) => {
                let t = match &mut mode {
                    Mode::Censor(c) => self.censor_intent(c, intent),
                };
                self.stack.push(mode);
                t
            }
        };
        self.apply(t);
    }

    fn apply(&mut self, t: Transition) {
        match t {
            Transition::Stay => {}
            Transition::OpenTool(mode) => {
                if self.open.is_none() {
                    let id = self.project.new_layer_id();
                    self.open = Some(OpenLayer {
                        layer: Layer::new(id, self.sel.rect()),
                        depth: self.stack.len(),
                        log: Vec::new(),
                    });
                }
                self.leave_top();
                self.stack.push(mode);
                self.input.transition();
            }
            Transition::PopTo(depth) => {
                self.stack.truncate(depth);
                self.input.transition();
            }
            Transition::Finish(o) => self.finish(o),
        }
    }

    /// The top mode is about to lose input: a drag in progress ends there.
    fn leave_top(&mut self) {
        if self.stack.is_empty() {
            self.select.drag = None;
        }
    }

    pub(crate) fn finish(&mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
    }

    // --- the open layer -----------------------------------------------------------

    /// Lock an item into the open layer, on top. New work also takes back
    /// a pending Esc or q warning.
    pub(crate) fn lock(&mut self, data: Option<OpData>) {
        let Some(data) = data else { return };
        let id = self.project.new_op_id();
        let Some(open) = &mut self.open else { return };
        open.layer.ops.push(Op { id, data });
        open.log.push(LayerUndo::Remove(id));
        self.pending = None;
    }

    /// `u` in a build tool: the last action on the open layer, whichever
    /// tool made it. With nothing left, nothing: it never reaches into
    /// project history.
    pub(crate) fn undo_layer(&mut self) {
        let Some(open) = &mut self.open else { return };
        match open.log.pop() {
            Some(LayerUndo::Remove(id)) => open.layer.ops.retain(|o| o.id != id),
            None => {}
        }
    }

    /// Enter (commit) or Esc (discard) in a build tool: back to wherever
    /// the layer was opened from.
    pub(crate) fn close_layer(&mut self, commit: bool) -> Transition {
        self.pending = None;
        let Some(open) = self.open.take() else {
            return Transition::PopTo(0);
        };
        if commit && !open.layer.ops.is_empty() {
            self.history.push(ProjectEntry::LayerCommitted {
                layer: open.layer.id,
                rect: self.sel.rect(),
            });
            self.project.layers.push(open.layer);
        }
        Transition::PopTo(open.depth)
    }

    /// Esc in a build tool: with locked work, a warning first.
    pub(crate) fn back_from_tool(&mut self) -> Transition {
        if self.locked_work() && self.pending != Some(Pending::Discard) {
            self.pending = Some(Pending::Discard);
            return Transition::Stay;
        }
        self.close_layer(false)
    }

    // --- project history ----------------------------------------------------------

    /// Before a selection change: record where it was.
    pub(crate) fn remember(&mut self) {
        self.history.remember_selection(self.sel.rect());
    }

    /// `u` in Select: the last project action. Entries that would change
    /// nothing are skipped, so every `u` visibly does something.
    pub(crate) fn undo(&mut self) {
        while let Some(entry) = self.history.pop() {
            let changed = match entry {
                ProjectEntry::Selection(r) => {
                    let changed = r != self.sel.rect();
                    self.sel.set_rect(r);
                    changed
                }
                ProjectEntry::LayerCommitted { layer, rect } => {
                    // (it may have been deleted since)
                    let i = self.project.layer_index(layer);
                    if let Some(i) = i {
                        self.project.layers.remove(i);
                    }
                    let moved = rect != self.sel.rect();
                    self.sel.set_rect(rect);
                    i.is_some() || moved
                }
            };
            if changed {
                return;
            }
        }
    }
}
