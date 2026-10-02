//! The session: one picker run, from raw input to a finished action.
//!
//! The host (an overlay, a test, a `--keys` script) feeds raw events and
//! clock ticks in, and reads the outcome, the selection and the status
//! line out. Nothing here draws or touches the desktop.

use crate::anchor_cursor::AnchorCursor;
use crate::geometry::{LogicalPoint, LogicalRect, PixelMap, PixelPoint};
use crate::history::{ProjectEntry, ProjectHistory};
use crate::input::{InputState, Press};
use crate::intent::Intent;
use crate::keymap::{self, Keymap};
use crate::keys::{Chord, Key, Mods};
use crate::layout::Layout;
use crate::select::SelectState;
use crate::targets::SnapEnv;

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

/// A box step (a tap), in logical pixels.
pub const STEP: f64 = 5.0;

pub struct Session {
    pub(crate) layout: Layout,
    pub(crate) pixels: PixelMap,
    pub(crate) snap: SnapEnv,
    pub(crate) sel: AnchorCursor,
    pub(crate) select: SelectState,
    pub(crate) history: ProjectHistory,
    pub(crate) recall: Vec<LogicalRect>,
    pub(crate) note: Option<String>,
    input: InputState,
    show_help: bool,
    outcome: Option<Outcome>,
    pointer: LogicalPoint,
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
        Self {
            snap: SnapEnv::from_layout(&layout),
            sel: AnchorCursor::new(start, layout.bounds),
            select: SelectState::default(),
            history: ProjectHistory::default(),
            input: InputState::default(),
            layout,
            pixels,
            recall,
            note: None,
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

    /// The top mode's name.
    pub fn mode_name(&self) -> &'static str {
        "select"
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
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
        let s = &self.sel;
        if s.is_centred() {
            return self.pixels.pixel_under(s.rect().centre());
        }
        use crate::geometry::{Axis, End};
        self.pixels.edge_pixel(
            s.cursor_point(),
            s.cursor_end(Axis::X) == End::High,
            s.cursor_end(Axis::Y) == End::High,
        )
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
        if let Some(n) = &self.note {
            s.push_str(&format!("  ({n})"));
        }
        s
    }

    /// The top mode's keymap.
    pub fn keymap(&self) -> &'static Keymap {
        &keymap::SELECT
    }

    /// The top mode's key legend, as shown behind `?`.
    pub fn legend(&self) -> Vec<String> {
        crate::legend::render(self.keymap(), crate::legend::SELECT)
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
                self.user_acted();
                let chord = Chord::new(key, mods);
                if let Some(intent) = keymap::SESSION.lookup(chord) {
                    self.session_intent(intent);
                } else if let Some(intent) = self.keymap().lookup(chord) {
                    self.dispatch(intent);
                }
            }
            RawEvent::KeyRelease { code, mods, .. } => self.input.release(code, mods),
            RawEvent::ButtonPress { button, at } => {
                self.pointer = at;
                self.user_acted();
                match button {
                    Button::Left => self.dispatch(Intent::Press(at)),
                    Button::Right => self.dispatch(Intent::Confirm),
                    Button::Middle => self.dispatch(Intent::Back),
                    Button::Other(_) => {}
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
                self.user_acted();
                for _ in 0..steps.unsigned_abs() {
                    self.dispatch(Intent::Adjust(steps.signum()));
                }
            }
            RawEvent::FocusLost => self.input.reset(),
        }
    }

    /// Advance held-key repeat to `now`. Returns whether anything moved.
    pub fn tick(&mut self, now: f64) -> bool {
        if self.is_finished() {
            return false;
        }
        let mods = self.input.mods();
        let rates = if mods.ctrl || mods.alt {
            None // jumps and scaling are one-shot
        } else {
            Some(crate::select::RATES)
        };
        match self.input.repeat(now, rates) {
            Some((dirs, n)) if mods.shift => self.dispatch(Intent::HeldTranslate(dirs, n)),
            Some((dirs, n)) => self.dispatch(Intent::HeldSteer(dirs, n)),
            None => return false,
        }
        true
    }

    /// Any fresh input clears the one-off note.
    fn user_acted(&mut self) {
        self.note = None;
    }

    fn session_intent(&mut self, intent: Intent) {
        match intent {
            Intent::Quit => self.finish(Outcome::Cancelled),
            Intent::ToggleHelp => self.show_help = !self.show_help,
            _ => {}
        }
    }

    /// Send an intent to the top mode.
    pub fn dispatch(&mut self, intent: Intent) {
        if !self.is_finished() {
            self.select_intent(intent);
        }
    }

    pub(crate) fn finish(&mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
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
            match entry {
                ProjectEntry::Selection(r) => {
                    if r == self.sel.rect() {
                        continue;
                    }
                    self.sel.set_rect(r);
                    return;
                }
            }
        }
    }
}
