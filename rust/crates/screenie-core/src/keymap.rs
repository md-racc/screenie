//! Keymaps as data: chord -> intent tables, one per mode, built from the
//! shared grammars plus each mode's own keys.
//!
//! The same tables drive key dispatch, the `?` legend and collision tests.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::geometry::Dir;
use crate::intent::{Intent, Scale};
use crate::keys::{Chord, Key, Mods};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Binding {
    pub chord: Chord,
    pub intent: Intent,
    /// An alternative spelling (the arrows, `=` for `+`): bound, but left
    /// out of the legend.
    pub alias: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    pub fn lookup(&self, chord: Chord) -> Option<Intent> {
        self.bindings
            .iter()
            .find(|b| b.chord == chord)
            .map(|b| b.intent)
    }

    fn bind(&mut self, chord: Chord, intent: Intent) -> &mut Self {
        self.bindings.push(Binding {
            chord,
            intent,
            alias: false,
        });
        self
    }

    fn alias(&mut self, chord: Chord, intent: Intent) -> &mut Self {
        self.bindings.push(Binding {
            chord,
            intent,
            alias: true,
        });
        self
    }

    fn extend(&mut self, other: &Keymap) -> &mut Self {
        self.bindings.extend_from_slice(&other.bindings);
        self
    }

    /// The chords that are bound more than once.
    pub fn collisions(&self) -> Vec<Chord> {
        let mut seen: HashMap<Chord, usize> = HashMap::new();
        for b in &self.bindings {
            *seen.entry(b.chord).or_default() += 1;
        }
        let mut dup: Vec<_> = seen
            .into_iter()
            .filter(|&(_, n)| n > 1)
            .map(|(c, _)| c)
            .collect();
        dup.sort();
        dup
    }

    /// The legend's spelling of an intent: its non-alias chords, `/`-joined.
    pub fn keys_for(&self, intent: Intent) -> Option<String> {
        let keys: Vec<String> = self
            .bindings
            .iter()
            .filter(|b| b.intent == intent && !b.alias)
            .map(|b| b.chord.to_string())
            .collect();
        (!keys.is_empty()).then(|| keys.join("/"))
    }
}

const DIR_KEYS: [(char, Key, Dir); 4] = [
    ('h', Key::Left, Dir::Left),
    ('j', Key::Down, Dir::Down),
    ('k', Key::Up, Dir::Up),
    ('l', Key::Right, Dir::Right),
];

fn mods(shift: bool, ctrl: bool, alt: bool) -> Mods {
    Mods { shift, ctrl, alt }
}

/// The modifier grammar on `hjkl` and the arrows, shared by every box.
/// Ctrl+Alt is meaningless, so it's left unbound.
pub fn direction_grammar() -> Keymap {
    let mut m = Keymap::default();
    for (c, arrow, d) in DIR_KEYS {
        for (shift, ctrl, alt, intent) in [
            (false, false, false, Intent::Steer(d)),
            (true, false, false, Intent::Translate(d)),
            (false, true, false, Intent::Jump(d)),
            (true, true, false, Intent::Slide(d)),
            (false, false, true, Intent::Scale(d, Scale::Half)),
            (true, false, true, Intent::Scale(d, Scale::Double)),
        ] {
            let md = mods(shift, ctrl, alt);
            m.bind(Chord::with(Key::Char(c), md), intent);
            m.alias(Chord::with(arrow, md), intent);
        }
    }
    m
}

/// The box keys: corners, edges, centre mode, undo.
pub fn box_grammar() -> Keymap {
    let mut m = direction_grammar();
    m.bind(Chord::ch('o'), Intent::SwapOpposite)
        .bind(Chord::plain(Key::Space), Intent::SwapOpposite)
        .bind(Chord::ch('O'), Intent::SwapSideways)
        .bind(Chord::with(Key::Space, Mods::SHIFT), Intent::SwapSideways)
        .bind(Chord::ch('w'), Intent::EdgeJump(Dir::Right))
        .bind(Chord::ch('b'), Intent::EdgeJump(Dir::Left))
        .bind(Chord::ch('}'), Intent::EdgeJump(Dir::Down))
        .bind(Chord::ch('{'), Intent::EdgeJump(Dir::Up))
        .bind(Chord::ch('0'), Intent::FarEdge(Dir::Left))
        .bind(Chord::ch('$'), Intent::FarEdge(Dir::Right))
        .bind(Chord::ch('g'), Intent::FarEdge(Dir::Up))
        .bind(Chord::ch('G'), Intent::FarEdge(Dir::Down))
        .bind(Chord::ch('c'), Intent::ToggleCentre)
        .bind(Chord::ch('u'), Intent::Undo);
    m
}

/// `+` and `-`, with their keypad and unshifted (`=`) spellings.
pub fn adjust_keys() -> Keymap {
    let mut m = Keymap::default();
    m.bind(Chord::ch('+'), Intent::Adjust(1))
        .alias(Chord::ch('='), Intent::Adjust(1))
        .bind(Chord::ch('-'), Intent::Adjust(-1));
    m
}

/// Keys every mode shares, handled by the session before the mode.
pub static SESSION: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = Keymap::default();
    m.bind(Chord::ch('q'), Intent::Quit)
        .bind(Chord::ch('?'), Intent::ToggleHelp);
    m
});

pub static SELECT: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = box_grammar();
    m.extend(&adjust_keys())
        .bind(Chord::ch('.'), Intent::Recall)
        .bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::ch('y'), Intent::Ocr)
        .bind(Chord::ch('p'), Intent::CopyColour)
        .bind(Chord::ch('Q'), Intent::DecodeCode)
        .bind(Chord::ch('P'), Intent::Pin)
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m
});

/// Every mode's keymap, by mode name.
pub fn all() -> Vec<(&'static str, &'static Keymap)> {
    vec![("select", &*SELECT)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_mode_binds_a_chord_twice_or_shadows_the_session_keys() {
        for (name, map) in all() {
            assert_eq!(map.collisions(), [], "{name}");
            let mut both = map.clone();
            both.extend(&SESSION);
            assert_eq!(both.collisions(), [], "{name} shadows a session key");
        }
    }

    #[test]
    fn the_modifier_grammar() {
        let l = |m: Mods| SELECT.lookup(Chord::with(Key::Char('l'), m));
        assert_eq!(l(Mods::NONE), Some(Intent::Steer(Dir::Right)));
        assert_eq!(l(Mods::SHIFT), Some(Intent::Translate(Dir::Right)));
        assert_eq!(l(Mods::CTRL), Some(Intent::Jump(Dir::Right)));
        assert_eq!(l(Mods::CTRL.with_shift()), Some(Intent::Slide(Dir::Right)));
        assert_eq!(l(Mods::ALT), Some(Intent::Scale(Dir::Right, Scale::Half)));
        assert_eq!(
            l(Mods::ALT.with_shift()),
            Some(Intent::Scale(Dir::Right, Scale::Double))
        );
        assert_eq!(
            SELECT.lookup(Chord::with(Key::Right, Mods::SHIFT)),
            Some(Intent::Translate(Dir::Right))
        );
    }

    #[test]
    fn a_meaningless_modifier_does_not_fall_through() {
        let ctrl_alt = Mods {
            ctrl: true,
            alt: true,
            shift: false,
        };
        assert_eq!(SELECT.lookup(Chord::with(Key::Char('l'), ctrl_alt)), None);
        assert_eq!(SELECT.lookup(Chord::with(Key::Char('o'), Mods::CTRL)), None);
        assert_eq!(SELECT.lookup(Chord::with(Key::Return, Mods::ALT)), None);
        assert_eq!(
            SESSION.lookup(Chord::with(Key::Char('q'), Mods::CTRL)),
            None
        );
    }

    #[test]
    fn legend_spellings_leave_out_aliases() {
        assert_eq!(
            SELECT.keys_for(Intent::SwapOpposite).as_deref(),
            Some("o/Space")
        );
        assert_eq!(
            SELECT.keys_for(Intent::SwapSideways).as_deref(),
            Some("O/S-Space")
        );
        assert_eq!(SELECT.keys_for(Intent::Adjust(1)).as_deref(), Some("+"));
        assert_eq!(
            SELECT.keys_for(Intent::Steer(Dir::Left)).as_deref(),
            Some("h")
        );
    }
}
