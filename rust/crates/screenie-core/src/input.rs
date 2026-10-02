//! Physical input state: modifiers, held keys, and screenie's own key
//! repeat.
//!
//! - A key fires once per physical press. A press of a key that's already
//!   down (a platform's autorepeat) is dropped.
//! - Held directions repeat, with acceleration, at the rates the top mode
//!   gives. Every held direction counts, so `k` + `l` is diagonal.
//! - Keys held across a mode transition go stale: until they're released,
//!   they're not fresh presses in the new mode, and they stop repeating.
//!
//! Time is seconds on any monotonic clock, passed in with each call.

use std::collections::HashSet;

use crate::geometry::Dir;
use crate::intent::DirSet;
use crate::keys::{Key, Mods};

/// Seconds a direction is held before repeat starts.
pub const DELAY: f64 = 0.08;
/// Seconds from a mode's starting repeat rate to its top rate.
pub const RAMP: f64 = 0.6;

/// A mode's repeat speed, in steps per second: at the start, and at the
/// top after `RAMP`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub start: f64,
    pub top: f64,
}

impl Rates {
    /// Steps taken in the first `u` seconds of repeating.
    fn steps_by(self, u: f64) -> f64 {
        let ramp = u.min(RAMP);
        let ramped = self.start * ramp + (self.top - self.start) * ramp * ramp / (2.0 * RAMP);
        ramped + self.top * (u - RAMP).max(0.0)
    }
}

#[derive(Clone, Copy, Debug)]
struct HeldDir {
    code: u32,
    dir: Dir,
    since: f64,
}

/// What a key press turned out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// A fresh press: dispatch it.
    Fresh,
    /// A modifier: the state is updated, nothing to dispatch.
    Modifier,
    /// Autorepeat of a key already down, or a key held since before a mode
    /// transition: ignore it.
    Ignored,
}

#[derive(Clone, Debug, Default)]
pub struct InputState {
    mods: Mods,
    held: Vec<HeldDir>,
    down: HashSet<u32>,
    stale: HashSet<u32>,
    /// When repeat steps were last counted up to.
    counted: Option<f64>,
    /// Fractional repeat steps carried to the next tick.
    acc: f64,
}

impl InputState {
    pub fn mods(&self) -> Mods {
        self.mods
    }

    /// A key went down. `mods` is the modifier state including this key.
    pub fn press(&mut self, code: u32, key: Key, mods: Mods, now: f64) -> Press {
        self.mods = mods;
        if key.is_modifier() {
            return Press::Modifier;
        }
        if self.stale.contains(&code) || self.down.contains(&code) {
            return Press::Ignored;
        }
        self.down.insert(code);
        if let Some(dir) = key.direction() {
            self.held.push(HeldDir {
                code,
                dir,
                since: now,
            });
        }
        Press::Fresh
    }

    /// A key came up. `mods` is the modifier state after it.
    pub fn release(&mut self, code: u32, mods: Mods) {
        self.mods = mods;
        self.down.remove(&code);
        self.stale.remove(&code);
        self.held.retain(|h| h.code != code);
    }

    /// The top mode changed: every key down now goes stale until released.
    pub fn transition(&mut self) {
        self.stale.extend(self.down.drain());
        self.held.clear();
        self.acc = 0.0;
        self.counted = None;
    }

    /// Focus went elsewhere: forget everything held.
    pub fn reset(&mut self) {
        *self = InputState::default();
    }

    /// Repeat steps due by `now` for the held directions, at `rates` (none
    /// for a mode or modifier state that doesn't repeat). Returns the held
    /// directions and how many whole steps to take.
    pub fn repeat(&mut self, now: f64, rates: Option<Rates>) -> Option<(DirSet, u32)> {
        let repeating: Vec<&HeldDir> = self
            .held
            .iter()
            .filter(|h| now - h.since >= DELAY)
            .collect();
        let Some(rates) = rates.filter(|_| !repeating.is_empty()) else {
            self.acc = 0.0;
            self.counted = None;
            return None;
        };
        let start = repeating
            .iter()
            .map(|h| h.since)
            .fold(f64::INFINITY, f64::min)
            + DELAY;
        let dirs: DirSet = repeating.iter().map(|h| h.dir).collect();
        let from = self.counted.map_or(start, |c| c.max(start));
        self.acc += rates.steps_by(now - start) - rates.steps_by(from - start);
        self.counted = Some(now);
        // A hair of slack, so float error never loses a whole step.
        let n = (self.acc + 1e-9).floor();
        self.acc = (self.acc - n).max(0.0);
        (n >= 1.0).then_some((dirs, n as u32))
    }

    /// Whether any direction is held (the host should keep ticking).
    pub fn repeating(&self) -> bool {
        !self.held.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: Rates = Rates {
        start: 20.0,
        top: 300.0,
    };

    fn tick_until(s: &mut InputState, from: f64, to: f64, rates: Rates) -> u32 {
        let mut steps = 0;
        let mut t = from;
        while t < to {
            t += 1.0 / 60.0;
            if let Some((_, n)) = s.repeat(t.min(to), Some(rates)) {
                steps += n;
            }
        }
        steps
    }

    #[test]
    fn autorepeat_of_a_held_key_is_ignored() {
        let mut s = InputState::default();
        assert_eq!(s.press(1, Key::Char('z'), Mods::NONE, 0.0), Press::Fresh);
        assert_eq!(s.press(1, Key::Char('z'), Mods::NONE, 0.5), Press::Ignored);
        s.release(1, Mods::NONE);
        assert_eq!(s.press(1, Key::Char('z'), Mods::NONE, 0.6), Press::Fresh);
    }

    #[test]
    fn nothing_repeats_before_the_delay() {
        let mut s = InputState::default();
        s.press(1, Key::Char('l'), Mods::NONE, 0.0);
        assert_eq!(s.repeat(DELAY - 0.01, Some(BOX)), None);
    }

    #[test]
    fn repeat_ramps_from_the_start_rate_to_the_top_rate() {
        let mut s = InputState::default();
        s.press(1, Key::Char('l'), Mods::NONE, 0.0);
        // The ramp: 20/s rising to 300/s over 0.6s is 96 steps.
        assert_eq!(tick_until(&mut s, 0.0, DELAY + RAMP, BOX), 96);
        // Then the top rate.
        assert_eq!(
            tick_until(&mut s, DELAY + RAMP, DELAY + RAMP + 1.0, BOX),
            300
        );
    }

    #[test]
    fn the_count_is_the_same_however_often_the_host_ticks() {
        let mut a = InputState::default();
        a.press(1, Key::Char('l'), Mods::NONE, 0.0);
        let mut b = a.clone();
        let fine = tick_until(&mut a, 0.0, 1.0, BOX);
        let coarse = b.repeat(1.0, Some(BOX)).map_or(0, |(_, n)| n);
        assert_eq!(fine, coarse);
    }

    #[test]
    fn held_directions_combine() {
        let mut s = InputState::default();
        s.press(1, Key::Char('k'), Mods::NONE, 0.0);
        s.press(2, Key::Char('l'), Mods::NONE, 0.01);
        let (dirs, _) = s.repeat(0.5, Some(BOX)).unwrap();
        assert_eq!(dirs.iter().collect::<Vec<_>>(), [Dir::Right, Dir::Up]);
        s.release(1, Mods::NONE);
        let (dirs, _) = s.repeat(0.7, Some(BOX)).unwrap();
        assert_eq!(dirs.iter().collect::<Vec<_>>(), [Dir::Right]);
    }

    #[test]
    fn no_rates_no_repeat() {
        let mut s = InputState::default();
        s.press(1, Key::Char('l'), Mods::CTRL, 0.0);
        assert_eq!(s.repeat(1.0, None), None);
    }

    #[test]
    fn keys_held_across_a_transition_are_stale_until_released() {
        let mut s = InputState::default();
        s.press(1, Key::Char('h'), Mods::NONE, 0.0);
        s.transition();
        assert_eq!(s.repeat(1.0, Some(BOX)), None, "a stale key doesn't repeat");
        assert_eq!(s.press(1, Key::Char('h'), Mods::NONE, 1.0), Press::Ignored);
        s.release(1, Mods::NONE);
        assert_eq!(s.press(1, Key::Char('h'), Mods::NONE, 1.1), Press::Fresh);
    }

    #[test]
    fn modifiers_update_the_state_and_dispatch_nothing() {
        let mut s = InputState::default();
        assert_eq!(s.press(50, Key::Shift, Mods::SHIFT, 0.0), Press::Modifier);
        assert!(s.mods().shift);
        s.release(50, Mods::NONE);
        assert!(!s.mods().shift);
    }
}
