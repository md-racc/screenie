//! The `?` key legend, spelled from the keymaps so it can't drift from
//! what the keys do.

use crate::geometry::Dir;
use crate::intent::Intent;
use crate::keymap::{Keymap, SESSION};

/// One legend entry: a label and the intents whose keys it lists.
pub type Item = (&'static str, &'static [Intent]);

/// Select's legend, line by line.
pub const SELECT: &[&[Item]] = &[
    &[
        ("corner", &[Intent::SwapOpposite, Intent::SwapSideways]),
        (
            "edges",
            &[
                Intent::EdgeJump(Dir::Right),
                Intent::EdgeJump(Dir::Left),
                Intent::EdgeJump(Dir::Down),
                Intent::EdgeJump(Dir::Up),
            ],
        ),
        (
            "far",
            &[
                Intent::FarEdge(Dir::Left),
                Intent::FarEdge(Dir::Right),
                Intent::FarEdge(Dir::Up),
                Intent::FarEdge(Dir::Down),
            ],
        ),
        ("centre", &[Intent::ToggleCentre]),
        ("scope", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("last", &[Intent::Recall]),
        ("undo", &[Intent::Undo]),
    ],
    &[
        ("capture", &[Intent::Confirm]),
        ("text", &[Intent::Ocr]),
        ("colour", &[Intent::CopyColour]),
        ("code", &[Intent::DecodeCode]),
        ("pin", &[Intent::Pin]),
        ("cancel", &[Intent::Back]),
        ("quit", &[Intent::Quit]),
    ],
];

/// The lines of a legend, with each item's keys looked up in `map` (then
/// the session keys). An item with no bound keys is left out.
pub fn render(map: &Keymap, lines: &[&[Item]]) -> Vec<String> {
    lines
        .iter()
        .map(|items| {
            items
                .iter()
                .filter_map(|(label, intents)| {
                    let keys: Vec<String> = intents
                        .iter()
                        .filter_map(|&i| map.keys_for(i).or_else(|| SESSION.keys_for(i)))
                        .collect();
                    (!keys.is_empty()).then(|| format!("{} {label}", keys.join(" ")))
                })
                .collect::<Vec<_>>()
                .join("  ")
        })
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap;

    #[test]
    fn select_legend() {
        assert_eq!(
            render(&keymap::SELECT, SELECT),
            [
                "o/Space O/S-Space corner  w b } { edges  0 $ g G far  c centre  + - scope  . last  u undo",
                "Enter capture  y text  p colour  Q code  P pin  Esc cancel  q quit",
            ]
        );
    }

    #[test]
    fn every_legend_item_is_bound() {
        for (name, map) in keymap::all() {
            let lines = match name {
                "select" => SELECT,
                _ => continue,
            };
            for (label, intents) in lines.iter().flat_map(|l| l.iter()) {
                for &i in *intents {
                    assert!(
                        map.keys_for(i).or_else(|| SESSION.keys_for(i)).is_some(),
                        "{name}: {label} lists {i:?}, which has no key"
                    );
                }
            }
        }
    }
}
