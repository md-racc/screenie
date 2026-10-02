//! Keys, modifiers and chords.
//!
//! Platforms report keys by keysym name (xkb's names, which X11 shares).
//! A `Chord` is a key plus the modifiers that matter, normalised so the
//! keymap can match it exactly:
//! - an upper-case letter is its lower-case letter with Shift (`O` is
//!   `Shift+o`, `H` is `Shift+h`);
//! - a symbol has its Shift consumed (`$`, `{`, `?` and `+` already say it);
//! - named keys keep Shift (`Shift+Space` is not `Space`).
//!
//! A chord with a modifier the mode gives no meaning simply isn't bound,
//! so it never falls through to the unmodified action.

use std::fmt;

use crate::geometry::Dir;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// A printable character. Letters are lower case once in a `Chord`.
    Char(char),
    Return,
    Escape,
    Space,
    Tab,
    BackSpace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Shift,
    Control,
    Alt,
    Super,
    /// Anything else: never bound.
    Other,
}

impl Key {
    /// A key from its xkb keysym name (`h`, `H`, `dollar`, `Return`,
    /// `KP_Add`...) or a single character.
    pub fn from_name(name: &str) -> Key {
        let mut chars = name.chars();
        if let (Some(c), None) = (chars.next(), chars.clone().next()) {
            return if c == ' ' { Key::Space } else { Key::Char(c) };
        }
        let c = match name {
            "Return" | "KP_Enter" | "Enter" => return Key::Return,
            "Escape" | "Esc" => return Key::Escape,
            "space" | "Space" => return Key::Space,
            "Tab" => return Key::Tab,
            "BackSpace" => return Key::BackSpace,
            "Delete" => return Key::Delete,
            "Left" => return Key::Left,
            "Right" => return Key::Right,
            "Up" => return Key::Up,
            "Down" => return Key::Down,
            "Shift_L" | "Shift_R" => return Key::Shift,
            "Control_L" | "Control_R" => return Key::Control,
            "Alt_L" | "Alt_R" | "Meta_L" | "Meta_R" => return Key::Alt,
            "Super_L" | "Super_R" | "ISO_Level3_Shift" | "ISO_Level5_Shift" => {
                return Key::Super;
            }
            "plus" | "KP_Add" => '+',
            "minus" | "KP_Subtract" => '-',
            "equal" => '=',
            "period" => '.',
            "comma" => ',',
            "dollar" => '$',
            "braceleft" => '{',
            "braceright" => '}',
            "bracketleft" => '[',
            "bracketright" => ']',
            "less" => '<',
            "greater" => '>',
            "question" => '?',
            "slash" => '/',
            "colon" => ':',
            "semicolon" => ';',
            "exclam" => '!',
            "at" => '@',
            "numbersign" => '#',
            "percent" => '%',
            "asciicircum" => '^',
            "ampersand" => '&',
            "asterisk" | "KP_Multiply" => '*',
            "parenleft" => '(',
            "parenright" => ')',
            "underscore" => '_',
            "apostrophe" => '\'',
            "quotedbl" => '"',
            "grave" => '`',
            "asciitilde" => '~',
            "bar" => '|',
            "backslash" => '\\',
            _ => {
                if let Some(d) = name.strip_prefix("KP_") {
                    if let Ok(n) = d.parse::<u8>() {
                        if n < 10 {
                            return Key::Char(char::from(b'0' + n));
                        }
                    }
                }
                return Key::Other;
            }
        };
        Key::Char(c)
    }

    pub fn is_modifier(self) -> bool {
        matches!(self, Key::Shift | Key::Control | Key::Alt | Key::Super)
    }

    /// `h j k l` and the arrows.
    pub fn direction(self) -> Option<Dir> {
        match self {
            Key::Char('h' | 'H') | Key::Left => Some(Dir::Left),
            Key::Char('j' | 'J') | Key::Down => Some(Dir::Down),
            Key::Char('k' | 'K') | Key::Up => Some(Dir::Up),
            Key::Char('l' | 'L') | Key::Right => Some(Dir::Right),
            _ => None,
        }
    }
}

/// Modifier state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
    };
    pub const SHIFT: Mods = Mods {
        shift: true,
        ..Mods::NONE
    };
    pub const CTRL: Mods = Mods {
        ctrl: true,
        ..Mods::NONE
    };
    pub const ALT: Mods = Mods {
        alt: true,
        ..Mods::NONE
    };

    pub fn with_shift(self) -> Mods {
        Mods {
            shift: true,
            ..self
        }
    }
}

/// A normalised key plus modifiers: what the keymap matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    pub key: Key,
    pub mods: Mods,
}

impl Chord {
    /// Normalise a key as reported with the modifiers held.
    pub fn new(key: Key, mods: Mods) -> Chord {
        match key {
            Key::Char(c) if c.is_ascii_uppercase() => Chord {
                key: Key::Char(c.to_ascii_lowercase()),
                mods: mods.with_shift(),
            },
            Key::Char(c) if !c.is_ascii_alphanumeric() => Chord {
                key,
                mods: Mods {
                    shift: false,
                    ..mods
                },
            },
            _ => Chord { key, mods },
        }
    }

    pub fn plain(key: Key) -> Chord {
        Chord::new(key, Mods::NONE)
    }

    /// A single character as typed: `'O'` is `Shift+o`.
    pub fn ch(c: char) -> Chord {
        Chord::plain(if c == ' ' { Key::Space } else { Key::Char(c) })
    }

    pub fn with(key: Key, mods: Mods) -> Chord {
        Chord::new(key, mods)
    }
}

/// How the legend writes a chord: `O`, `$`, `Enter`, `S-Space`, `C-h`.
impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Mods { shift, ctrl, alt } = self.mods;
        if ctrl {
            f.write_str("C-")?;
        }
        if alt {
            f.write_str("A-")?;
        }
        let name = match self.key {
            Key::Char(c) if c.is_ascii_lowercase() && shift => {
                return write!(f, "{}", c.to_ascii_uppercase());
            }
            Key::Char(c) => {
                if shift {
                    f.write_str("S-")?;
                }
                return write!(f, "{c}");
            }
            Key::Return => "Enter",
            Key::Escape => "Esc",
            Key::Space => "Space",
            Key::Tab => "Tab",
            Key::BackSpace => "BackSpace",
            Key::Delete => "Delete",
            Key::Left => "Left",
            Key::Right => "Right",
            Key::Up => "Up",
            Key::Down => "Down",
            Key::Shift => "Shift",
            Key::Control => "Ctrl",
            Key::Alt => "Alt",
            Key::Super => "Super",
            Key::Other => "?",
        };
        if shift {
            f.write_str("S-")?;
        }
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_case_letters_are_shifted_lower_case() {
        assert_eq!(
            Chord::new(Key::from_name("O"), Mods::SHIFT),
            Chord::with(Key::Char('o'), Mods::SHIFT)
        );
        assert_eq!(Chord::ch('G'), Chord::with(Key::Char('g'), Mods::SHIFT));
        assert_eq!(Chord::ch('H').key.direction(), Some(Dir::Left));
    }

    #[test]
    fn symbols_consume_shift_but_named_keys_keep_it() {
        assert_eq!(
            Chord::new(Key::from_name("dollar"), Mods::SHIFT),
            Chord::ch('$')
        );
        assert_eq!(
            Chord::new(Key::from_name("question"), Mods::SHIFT),
            Chord::ch('?')
        );
        assert_ne!(
            Chord::new(Key::from_name("space"), Mods::SHIFT),
            Chord::ch(' ')
        );
        // Ctrl is never consumed.
        assert_ne!(Chord::new(Key::Char('$'), Mods::CTRL), Chord::ch('$'));
    }

    #[test]
    fn keysym_names() {
        assert_eq!(Key::from_name("plus"), Key::Char('+'));
        assert_eq!(Key::from_name("KP_Add"), Key::Char('+'));
        assert_eq!(Key::from_name("KP_Enter"), Key::Return);
        assert_eq!(Key::from_name("braceright"), Key::Char('}'));
        assert_eq!(Key::from_name("KP_7"), Key::Char('7'));
        assert_eq!(Key::from_name("XF86AudioMute"), Key::Other);
        assert!(Key::from_name("Shift_L").is_modifier());
    }

    #[test]
    fn chords_display_as_in_the_legend() {
        assert_eq!(Chord::ch('O').to_string(), "O");
        assert_eq!(Chord::with(Key::Space, Mods::SHIFT).to_string(), "S-Space");
        assert_eq!(Chord::plain(Key::Return).to_string(), "Enter");
        assert_eq!(
            Chord::with(
                Key::Char('h'),
                Mods {
                    ctrl: true,
                    shift: true,
                    alt: false
                }
            )
            .to_string(),
            "C-H"
        );
    }
}
