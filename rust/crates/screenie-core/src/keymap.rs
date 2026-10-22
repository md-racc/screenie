//! Keymaps as data: chord -> intent tables, one per mode, built from the
//! shared grammars plus each mode's own keys.
//!
//! The same tables drive key dispatch, the `?` legend and collision tests.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::annotate::ShapeTool;
use crate::geometry::Dir;
use crate::intent::{Intent, Scale, Tool};
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

    /// Drop a chord's binding (a mode giving a shared key its own meaning).
    fn unbind(&mut self, chord: Chord) -> &mut Self {
        self.bindings.retain(|b| b.chord != chord);
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
    Mods {
        shift,
        ctrl,
        alt,
        logo: false,
    }
}

/// The modifier grammar on `hjkl` and the arrows, shared by every box.
/// Ctrl+Alt is left unbound here; the box modes give it to picture edges.
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
            m.bind(Chord::new(Key::Char(c), md), intent);
            m.alias(Chord::new(arrow, md), intent);
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
        .bind(Chord::new(Key::Space, Mods::SHIFT), Intent::SwapSideways)
        .extend(&edge_keys())
        .extend(&far_keys())
        .bind(Chord::ch('c'), Intent::ToggleCentre)
        .bind(Chord::ch('u'), Intent::Undo)
        .extend(&lens_keys());
    // Ctrl jumps; with Alt, to what's in the picture rather than the layout.
    let ctrl_alt = mods(false, true, true);
    for (c, arrow, d) in DIR_KEYS {
        m.bind(Chord::new(Key::Char(c), ctrl_alt), Intent::PictureEdge(d))
            .alias(Chord::new(arrow, ctrl_alt), Intent::PictureEdge(d));
    }
    m
}

/// `w b } {`: the cursor to the next window or monitor edge that way.
fn edge_keys() -> Keymap {
    let mut m = Keymap::default();
    m.bind(Chord::ch('w'), Intent::EdgeJump(Dir::Right))
        .bind(Chord::ch('b'), Intent::EdgeJump(Dir::Left))
        .bind(Chord::ch('}'), Intent::EdgeJump(Dir::Down))
        .bind(Chord::ch('{'), Intent::EdgeJump(Dir::Up));
    m
}

/// `0 $ g G`: the cursor to the far left, right, top or bottom.
fn far_keys() -> Keymap {
    let mut m = Keymap::default();
    m.bind(Chord::ch('0'), Intent::FarEdge(Dir::Left))
        .bind(Chord::ch('$'), Intent::FarEdge(Dir::Right))
        .bind(Chord::ch('g'), Intent::FarEdge(Dir::Up))
        .bind(Chord::ch('G'), Intent::FarEdge(Dir::Down));
    m
}

/// `z` (and `Z`) the loupe, `e` enhance: from every mode with a box.
pub fn lens_keys() -> Keymap {
    let mut m = Keymap::default();
    m.bind(Chord::ch('z'), Intent::Loupe)
        .alias(Chord::ch('Z'), Intent::Loupe)
        .bind(Chord::ch('e'), Intent::Enhance);
    m
}

/// `v` / `V` cycle the view filter (the lenses only).
fn view_keys() -> Keymap {
    let mut m = Keymap::default();
    m.bind(Chord::ch('v'), Intent::ViewFilter(1))
        .bind(Chord::ch('V'), Intent::ViewFilter(-1));
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
        .bind(Chord::ch('i'), Intent::Insert)
        .bind(Chord::ch('m'), Intent::Measure(crate::measure::Prim::Line))
        .extend(&build_tool_openers())
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m.extend(&digits());
    m
});

/// `1`-`9`: pick that option (an ink, a filter, a layer).
fn digits() -> Keymap {
    let mut m = Keymap::default();
    for n in 1..=9u8 {
        m.bind(Chord::ch(char::from(b'0' + n)), Intent::Choose(n));
    }
    m
}

/// Layers: the list grammar over the committed layers; `+`/`-` (and the
/// wheel) select too.
pub static LAYERS: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = list_keys();
    m.extend(&digits())
        .extend(&adjust_keys())
        .bind(Chord::ch('u'), Intent::Undo)
        .bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m
});

/// The keys every build tool shares: commit, discard (undo is in the box
/// grammar, and means the open layer's undo there).
fn build_tool_keys() -> Keymap {
    let mut m = Keymap::default();
    m.bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m
}

/// `x`, `a`, `f`: open (or switch to) a build tool, from every mode that can.
fn build_tool_openers() -> Keymap {
    let mut m = Keymap::default();
    m.bind(Chord::ch('x'), Intent::Tool(Tool::Censor))
        .bind(Chord::ch('a'), Intent::Tool(Tool::Annotate))
        .bind(Chord::ch('f'), Intent::Tool(Tool::Filter));
    m
}

pub static CENSOR: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = box_grammar();
    m.extend(&adjust_keys())
        .extend(&build_tool_keys())
        .bind(Chord::ch('t'), Intent::CycleKind)
        .bind(Chord::ch('r'), Intent::Redact)
        .extend(&build_tool_openers());
    m
});

/// The ink keys, `1`-`5`, and the eyedropper.
fn ink_keys() -> Keymap {
    let mut m = Keymap::default();
    for (i, (key, _, _)) in crate::annotate::INKS.iter().enumerate() {
        m.bind(Chord::ch(*key), Intent::Choose(i as u8 + 1));
    }
    m.bind(Chord::ch('p'), Intent::Eyedropper);
    m
}

/// The pen: no box. hjkl and the arrows move the pen point (Shift: a
/// larger step); with Ctrl or Alt they mean nothing.
pub static ANNOTATE: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = direction_grammar();
    m.bindings
        .retain(|b| matches!(b.intent, Intent::Steer(_) | Intent::Translate(_)));
    m.extend(&adjust_keys())
        .extend(&ink_keys())
        .extend(&build_tool_keys())
        .bind(Chord::ch('r'), Intent::Shape(ShapeTool::Rect))
        .bind(Chord::ch('c'), Intent::Shape(ShapeTool::Circle))
        .bind(Chord::ch('v'), Intent::Shape(ShapeTool::Arrow))
        .bind(Chord::ch('m'), Intent::ToggleHighlighter)
        .bind(Chord::new(Key::Char('+'), Mods::CTRL), Intent::Bulge(1))
        .alias(Chord::new(Key::Char('='), Mods::CTRL), Intent::Bulge(1))
        .bind(Chord::new(Key::Char('-'), Mods::CTRL), Intent::Bulge(-1))
        .bind(Chord::plain(Key::Space), Intent::PenToggle)
        .bind(Chord::ch('i'), Intent::Insert)
        .bind(Chord::ch('n'), Intent::Callout)
        .bind(Chord::ch('N'), Intent::CalloutNumber)
        .extend(&build_tool_openers())
        .bind(Chord::ch('u'), Intent::Undo)
        .extend(&lens_keys());
    m
});

/// A shape is a box: the box grammar (`c` is centre mode here), plus ink.
pub static SHAPE: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = box_grammar();
    m.extend(&adjust_keys())
        .extend(&ink_keys())
        .extend(&build_tool_keys())
        .bind(Chord::ch('t'), Intent::CycleKind)
        .bind(Chord::ch('V'), Intent::ToggleHeads)
        .extend(&build_tool_openers());
    m
});

/// The list keys (Filter, Layers): `[ ]` select, `< >` move, `d` delete,
/// `Space` mute or hide.
fn list_keys() -> Keymap {
    let mut m = Keymap::default();
    m.bind(Chord::ch('['), Intent::ListStep(-1))
        .bind(Chord::ch(']'), Intent::ListStep(1))
        .bind(Chord::ch('<'), Intent::ListMove(-1))
        .bind(Chord::ch('>'), Intent::ListMove(1))
        .bind(Chord::ch('d'), Intent::ListDelete)
        .bind(Chord::plain(Key::Space), Intent::ListToggle);
    m
}

/// Filter: a box (the selected filter's), the filter kinds, and a list.
/// Space mutes here; Shift+Space is still the sideways corner.
pub static FILTER: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = box_grammar();
    m.unbind(Chord::plain(Key::Space))
        .extend(&adjust_keys())
        .extend(&build_tool_keys())
        .extend(&list_keys())
        .extend(&build_tool_openers());
    for (i, (key, ..)) in crate::filter::KINDS.iter().enumerate() {
        m.bind(Chord::ch(*key), Intent::Choose(i as u8 + 1));
    }
    m
});

/// Insert: a pixel cursor. hjkl step (Shift: four steps; Alt: halfway to
/// the box edge); Ctrl and Alt+Shift mean nothing here.
pub static INSERT: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = direction_grammar();
    m.bindings.retain(|b| {
        !matches!(
            b.intent,
            Intent::Jump(_) | Intent::Slide(_) | Intent::Scale(_, Scale::Double)
        )
    });
    m.extend(&adjust_keys())
        .extend(&lens_keys())
        .extend(&build_tool_openers())
        .extend(&edge_keys())
        .extend(&far_keys())
        .bind(Chord::ch('m'), Intent::Middle)
        .bind(Chord::ch('u'), Intent::Undo)
        .bind(Chord::ch('p'), Intent::CopyColour)
        .bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m
});

/// Loupe: one pixel a tap, a loupe-width with Shift; z, Enter or Esc close.
pub static LOUPE: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = direction_grammar();
    m.bindings
        .retain(|b| matches!(b.intent, Intent::Steer(_) | Intent::Translate(_)));
    m.extend(&adjust_keys())
        .extend(&view_keys())
        .extend(&build_tool_openers())
        .extend(&lens_keys())
        .bind(Chord::ch('p'), Intent::CopyColour)
        // Over Measure: a point at the loupe's pixel (Shift: a path's).
        .bind(Chord::plain(Key::Space), Intent::DropPoint)
        .bind(Chord::new(Key::Space, Mods::ALT), Intent::DropPoint)
        .bind(Chord::new(Key::Space, Mods::SHIFT), Intent::AddPoint)
        .bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m
});

/// Enhance: hjkl move the lens (Shift: bigger steps), Alt / Alt+Shift halve
/// or double it; Ctrl means nothing.
pub static ENHANCE: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = direction_grammar();
    m.bindings
        .retain(|b| !matches!(b.intent, Intent::Jump(_) | Intent::Slide(_)));
    m.extend(&adjust_keys())
        .extend(&view_keys())
        .bind(Chord::ch('s'), Intent::ToggleSharp)
        .bind(Chord::ch('z'), Intent::Loupe)
        .alias(Chord::ch('Z'), Intent::Loupe)
        .bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m
});

/// Text input: plain keys are typed (the session routes them before this
/// table); these are the rest. Ctrl+hjkl (or arrows) nudge, Ctrl+Shift
/// further, Ctrl+`+`/`-` size, Ctrl+1-5 ink, Ctrl+b chip.
pub static TEXT: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = Keymap::default();
    m.bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::plain(Key::Escape), Intent::Back)
        .bind(Chord::new(Key::Char('b'), Mods::CTRL), Intent::ToggleChip)
        .bind(Chord::new(Key::Char('+'), Mods::CTRL), Intent::Adjust(1))
        .alias(Chord::new(Key::Char('='), Mods::CTRL), Intent::Adjust(1))
        .bind(Chord::new(Key::Char('-'), Mods::CTRL), Intent::Adjust(-1));
    for (c, arrow, d) in DIR_KEYS {
        m.bind(Chord::new(Key::Char(c), Mods::CTRL), Intent::Jump(d))
            .alias(Chord::new(arrow, Mods::CTRL), Intent::Jump(d))
            .bind(
                Chord::new(Key::Char(c), Mods::CTRL.with_shift()),
                Intent::Slide(d),
            )
            .alias(Chord::new(arrow, Mods::CTRL.with_shift()), Intent::Slide(d));
    }
    for n in 1..=5u8 {
        m.bind(
            Chord::new(Key::Char(char::from(b'0' + n)), Mods::CTRL),
            Intent::Choose(n),
        );
    }
    m
});

/// Reviewing suggestions: the list keys, `a` (or Enter) accepts the ticked
/// ones, `r` rescans, Esc drops them.
pub static REDACT: LazyLock<Keymap> = LazyLock::new(|| {
    let mut m = list_keys();
    m.unbind(Chord::ch('<'))
        .unbind(Chord::ch('>'))
        .bind(Chord::ch('a'), Intent::Accept)
        .bind(Chord::ch('r'), Intent::Redact)
        .bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m
});

/// Measure: Insert's pixel cursor (Ctrl+hjkl: the next edge in the
/// picture), the primitives, points, the list, units and output.
pub static MEASURE: LazyLock<Keymap> = LazyLock::new(|| {
    use crate::measure::Prim;
    let mut m = direction_grammar();
    m.bindings
        .retain(|b| !matches!(b.intent, Intent::Slide(_) | Intent::Scale(_, Scale::Double)));
    m.extend(&adjust_keys())
        .extend(&lens_keys())
        .extend(&far_keys())
        .bind(Chord::ch('s'), Intent::Measure(Prim::Line))
        .bind(Chord::ch('c'), Intent::Measure(Prim::Circle))
        .bind(Chord::ch('A'), Intent::Measure(Prim::Area))
        .bind(Chord::ch('r'), Intent::Measure(Prim::Rect))
        .bind(Chord::ch('^'), Intent::Measure(Prim::Angle))
        .bind(Chord::plain(Key::Space), Intent::DropPoint)
        .bind(Chord::new(Key::Space, Mods::ALT), Intent::DropPoint)
        .bind(Chord::new(Key::Space, Mods::SHIFT), Intent::AddPoint)
        .bind(Chord::ch('u'), Intent::Undo)
        .bind(Chord::ch('C'), Intent::Calibrate)
        .bind(Chord::ch('o'), Intent::SwapOpposite)
        .bind(Chord::ch('t'), Intent::UnitSystem)
        .bind(Chord::ch('T'), Intent::ShowPixels)
        .bind(Chord::ch('['), Intent::ListStep(-1))
        .bind(Chord::ch(']'), Intent::ListStep(1))
        .bind(Chord::ch('<'), Intent::ListMove(-1))
        .bind(Chord::ch('>'), Intent::ListMove(1))
        .bind(Chord::ch('d'), Intent::ListDelete)
        .bind(Chord::ch('x'), Intent::ListToggle)
        .bind(Chord::ch('y'), Intent::CopyMeasurements)
        .bind(Chord::plain(Key::Return), Intent::Confirm)
        .bind(Chord::plain(Key::Escape), Intent::Back);
    m
});

/// Every mode's keymap, by mode name.
pub fn all() -> Vec<(&'static str, &'static Keymap)> {
    vec![
        ("select", &*SELECT),
        ("censor", &*CENSOR),
        ("annotate", &*ANNOTATE),
        ("shape", &*SHAPE),
        ("filter", &*FILTER),
        ("layers", &*LAYERS),
        ("insert", &*INSERT),
        ("loupe", &*LOUPE),
        ("enhance", &*ENHANCE),
        ("text", &*TEXT),
        ("redact", &*REDACT),
        ("measure", &*MEASURE),
    ]
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
        let l = |m: Mods| SELECT.lookup(Chord::new(Key::Char('l'), m));
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
            SELECT.lookup(Chord::new(Key::Right, Mods::SHIFT)),
            Some(Intent::Translate(Dir::Right))
        );
    }

    #[test]
    fn a_meaningless_modifier_does_not_fall_through() {
        let all = Mods {
            ctrl: true,
            alt: true,
            shift: true,
            logo: false,
        };
        assert_eq!(SELECT.lookup(Chord::new(Key::Char('l'), all)), None);
        // Ctrl+Alt means something only in the box modes: picture edges.
        let ctrl_alt = Mods {
            shift: false,
            ..all
        };
        assert_eq!(
            SELECT.lookup(Chord::new(Key::Char('l'), ctrl_alt)),
            Some(Intent::PictureEdge(Dir::Right))
        );
        assert_eq!(INSERT.lookup(Chord::new(Key::Char('l'), ctrl_alt)), None);
        assert_eq!(SELECT.lookup(Chord::new(Key::Char('o'), Mods::CTRL)), None);
        let logo = Mods {
            logo: true,
            ..Mods::NONE
        };
        assert_eq!(SELECT.lookup(Chord::new(Key::Char('l'), logo)), None);
        assert_eq!(SESSION.lookup(Chord::new(Key::Return, logo)), None);
        assert_eq!(SELECT.lookup(Chord::new(Key::Return, Mods::ALT)), None);
        assert_eq!(SESSION.lookup(Chord::new(Key::Char('q'), Mods::CTRL)), None);
    }

    #[test]
    fn text_nudges_with_arrows_as_with_hjkl() {
        for (c, arrow, d) in DIR_KEYS {
            for mods in [Mods::CTRL, Mods::CTRL.with_shift()] {
                assert_eq!(
                    TEXT.lookup(Chord::new(arrow, mods)),
                    TEXT.lookup(Chord::new(Key::Char(c), mods)),
                    "{d:?} {mods:?}"
                );
            }
        }
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
