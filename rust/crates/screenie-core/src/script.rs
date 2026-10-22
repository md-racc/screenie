//! `--keys` scripts: the picker driven by text, through the same raw events
//! a platform sends (`docs/cli.md`, PICK OPTIONS).
//!
//! A script is space-separated tokens:
//! - key names: `x`, `L`, `Return`, `dollar`, `space`;
//! - vim-style modifiers: `<C-l>`, `<S-space>`, `<A-S-h>`;
//! - the mouse: `click:X,Y`, `drag:X,Y:X,Y`, `rclick`, `wheel:±N`.

use std::fmt;

use crate::geometry::LogicalPoint;
use crate::keys::{Key, Mods};
use crate::session::{Button, RawEvent, Session};

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// A key pressed and released with modifiers held around it.
    Key {
        key: Key,
        mods: Mods,
    },
    Click(LogicalPoint),
    Drag(LogicalPoint, LogicalPoint),
    RightClick,
    Wheel(i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub token: String,
    pub reason: &'static str,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "--keys: {:?}: {}", self.token, self.reason)
    }
}

impl std::error::Error for ScriptError {}

fn point(s: &str) -> Option<LogicalPoint> {
    let (x, y) = s.split_once(',')?;
    Some(LogicalPoint::new(
        x.trim().parse().ok()?,
        y.trim().parse().ok()?,
    ))
}

fn parse_token(tok: &str) -> Result<Step, &'static str> {
    if let Some(inner) = tok.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        let mut mods = Mods::NONE;
        let mut rest = inner;
        loop {
            let m = match rest.get(..2) {
                Some("C-") => &mut mods.ctrl,
                Some("S-") => &mut mods.shift,
                Some("A-" | "M-") => &mut mods.alt,
                _ => break,
            };
            *m = true;
            rest = &rest[2..];
        }
        let key = Key::from_name(rest);
        if rest.is_empty() || key == Key::Other {
            return Err("unknown key");
        }
        return Ok(Step::Key { key, mods });
    }
    if let Some(p) = tok.strip_prefix("click:") {
        return point(p).map(Step::Click).ok_or("expected click:X,Y");
    }
    if let Some(p) = tok.strip_prefix("drag:") {
        let (a, b) = p.split_once(':').ok_or("expected drag:X,Y:X,Y")?;
        return match (point(a), point(b)) {
            (Some(a), Some(b)) => Ok(Step::Drag(a, b)),
            _ => Err("expected drag:X,Y:X,Y"),
        };
    }
    if let Some(n) = tok.strip_prefix("wheel:") {
        return n.parse().map(Step::Wheel).map_err(|_| "expected wheel:±N");
    }
    if tok == "rclick" {
        return Ok(Step::RightClick);
    }
    match Key::from_name(tok) {
        Key::Other => Err("unknown key"),
        key => Ok(Step::Key {
            key,
            mods: Mods::NONE,
        }),
    }
}

pub fn parse(script: &str) -> Result<Vec<Step>, ScriptError> {
    script
        .split_whitespace()
        .map(|tok| {
            parse_token(tok).map_err(|reason| ScriptError {
                token: tok.into(),
                reason,
            })
        })
        .collect()
}

/// Keycodes for scripted keys: modifiers get fixed ones, and every other
/// key a fresh one per press.
const SHIFT: u32 = 50;
const CTRL: u32 = 37;
const ALT: u32 = 64;

fn set_modifier(m: &mut Mods, key: Key, on: bool) {
    match key {
        Key::Shift => m.shift = on,
        Key::Control => m.ctrl = on,
        _ => m.alt = on,
    }
}

/// The raw events a step stands for.
pub fn events(step: &Step, pointer: LogicalPoint, code: &mut u32) -> Vec<RawEvent> {
    let button = |button, at, pressed| {
        if pressed {
            RawEvent::ButtonPress { button, at }
        } else {
            RawEvent::ButtonRelease { button, at }
        }
    };
    match *step {
        Step::Key { key, mods } => {
            let modifiers = [
                (mods.shift, SHIFT, Key::Shift),
                (mods.ctrl, CTRL, Key::Control),
                (mods.alt, ALT, Key::Alt),
            ];
            let mut out = vec![];
            let mut held = Mods::NONE;
            for &(on, code, key) in &modifiers {
                if on {
                    set_modifier(&mut held, key, true);
                    out.push(RawEvent::KeyPress {
                        code,
                        key,
                        mods: held,
                    });
                }
            }
            *code += 1;
            out.push(RawEvent::KeyPress {
                code: *code,
                key,
                mods,
            });
            out.push(RawEvent::KeyRelease {
                code: *code,
                key,
                mods,
            });
            for &(on, code, key) in modifiers.iter().rev() {
                if on {
                    set_modifier(&mut held, key, false);
                    out.push(RawEvent::KeyRelease {
                        code,
                        key,
                        mods: held,
                    });
                }
            }
            out
        }
        Step::Click(at) => vec![
            RawEvent::Motion { at },
            button(Button::Left, at, true),
            button(Button::Left, at, false),
        ],
        Step::Drag(a, b) => vec![
            RawEvent::Motion { at: a },
            button(Button::Left, a, true),
            RawEvent::Motion { at: b },
            button(Button::Left, b, false),
        ],
        Step::RightClick => vec![
            button(Button::Right, pointer, true),
            button(Button::Right, pointer, false),
        ],
        Step::Wheel(n) => vec![RawEvent::Wheel { steps: n }],
    }
}

impl Session {
    /// Play a `--keys` script into the session, at time `now`. Stops early
    /// once the session finishes.
    pub fn play(&mut self, script: &str, now: f64) -> Result<(), ScriptError> {
        let steps = parse(script)?;
        let mut code = 1000;
        for step in &steps {
            for ev in events(step, self.pointer(), &mut code) {
                if self.is_finished() {
                    return Ok(());
                }
                self.handle(ev, now);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keys_modifiers_and_the_mouse() {
        assert_eq!(
            parse("x <S-l> <A-S-h> Return click:10,20.5 drag:1,2:3,4 rclick wheel:-2").unwrap(),
            [
                Step::Key {
                    key: Key::Char('x'),
                    mods: Mods::NONE
                },
                Step::Key {
                    key: Key::Char('l'),
                    mods: Mods::SHIFT
                },
                Step::Key {
                    key: Key::Char('h'),
                    mods: Mods {
                        alt: true,
                        shift: true,
                        ..Mods::NONE
                    }
                },
                Step::Key {
                    key: Key::Return,
                    mods: Mods::NONE
                },
                Step::Click(LogicalPoint::new(10.0, 20.5)),
                Step::Drag(LogicalPoint::new(1.0, 2.0), LogicalPoint::new(3.0, 4.0)),
                Step::RightClick,
                Step::Wheel(-2),
            ]
        );
        assert_eq!(
            parse("<S-space>").unwrap(),
            [Step::Key {
                key: Key::Space,
                mods: Mods::SHIFT
            }]
        );
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        for bad in [
            "nosuchkey",
            "<C-nosuchkey>",
            "click:1",
            "drag:1,2",
            "wheel:up",
            "<C->",
        ] {
            let err = parse(bad).unwrap_err();
            assert_eq!(err.token, bad);
        }
    }
}
