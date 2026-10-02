//! The `?` key legend, spelled from the keymaps so it can't drift from
//! what the keys do.

use crate::geometry::Dir;
use crate::intent::{Intent, Tool};
use crate::keymap::{Keymap, SESSION};
use crate::model::ShapeKind;

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
        ("censor", &[Intent::Tool(Tool::Censor)]),
        ("annotate", &[Intent::Tool(Tool::Annotate)]),
        ("filter", &[Intent::Tool(Tool::Filter)]),
        ("1-9 layers", &[]),
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

/// Censor's legend.
pub const CENSOR: &[&[Item]] = &[
    &[
        ("pixelate/bar", &[Intent::CycleKind]),
        ("amount", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("next", &[Intent::Tool(Tool::Censor)]),
        ("annotate", &[Intent::Tool(Tool::Annotate)]),
        ("filter", &[Intent::Tool(Tool::Filter)]),
    ],
    &[
        ("undo", &[Intent::Undo]),
        ("commit", &[Intent::Confirm]),
        ("discard", &[Intent::Back]),
        ("(box keys as in Select)", &[]),
    ],
];

const INKS: &[Item] = &[
    ("white", &[Intent::Choose(1)]),
    ("black", &[Intent::Choose(2)]),
    ("red", &[Intent::Choose(3)]),
    ("green", &[Intent::Choose(4)]),
    ("blue", &[Intent::Choose(5)]),
];

/// Annotate's palette: the inks and the shapes, always shown.
pub const ANNOTATE_PALETTE: &[&[Item]] = &[&[
    INKS[0],
    INKS[1],
    INKS[2],
    INKS[3],
    INKS[4],
    ("rectangle", &[Intent::Shape(ShapeKind::Rect)]),
    ("circle", &[Intent::Shape(ShapeKind::Circle)]),
]];

pub const ANNOTATE: &[&[Item]] = &[
    &[
        ("width", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("pick colour", &[Intent::Eyedropper]),
        ("drag draw", &[]),
    ],
    &[
        ("undo", &[Intent::Undo]),
        ("censor", &[Intent::Tool(Tool::Censor)]),
        ("filter", &[Intent::Tool(Tool::Filter)]),
        ("commit", &[Intent::Confirm]),
        ("discard", &[Intent::Back]),
    ],
];

pub const SHAPE_PALETTE: &[&[Item]] = &[&[
    INKS[0],
    INKS[1],
    INKS[2],
    INKS[3],
    INKS[4],
    ("rectangle/circle", &[Intent::CycleKind]),
]];

pub const SHAPE: &[&[Item]] = &[
    &[
        ("width", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("pick colour", &[Intent::Eyedropper]),
    ],
    &[
        ("next shape", &[Intent::Tool(Tool::Annotate)]),
        ("drop this one", &[Intent::Back]),
        ("undo", &[Intent::Undo]),
        ("commit", &[Intent::Confirm]),
    ],
    &[
        ("centre", &[Intent::ToggleCentre]),
        ("(box keys as in Select)", &[]),
    ],
];

pub const FILTER_PALETTE: &[&[Item]] = &[&[
    ("grey", &[Intent::Choose(1)]),
    ("invert", &[Intent::Choose(2)]),
    ("bright", &[Intent::Choose(3)]),
    ("contrast", &[Intent::Choose(4)]),
    ("blur", &[Intent::Choose(5)]),
]];

pub const FILTER: &[&[Item]] = &[
    &[
        ("amount", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("select", &[Intent::ListStep(-1), Intent::ListStep(1)]),
        ("move", &[Intent::ListMove(-1), Intent::ListMove(1)]),
        ("delete", &[Intent::ListDelete]),
        ("mute", &[Intent::ListToggle]),
        ("duplicate", &[Intent::Tool(Tool::Filter)]),
        ("undo", &[Intent::Undo]),
    ],
    &[
        ("censor", &[Intent::Tool(Tool::Censor)]),
        ("annotate", &[Intent::Tool(Tool::Annotate)]),
        ("commit", &[Intent::Confirm]),
        ("discard", &[Intent::Back]),
    ],
];

pub const LAYERS: &[&[Item]] = &[
    &[
        ("1-9 pick", &[]),
        ("select", &[Intent::ListStep(-1), Intent::ListStep(1)]),
        ("move", &[Intent::ListMove(-1), Intent::ListMove(1)]),
        ("delete", &[Intent::ListDelete]),
        ("hide", &[Intent::ListToggle]),
    ],
    &[
        ("undo", &[Intent::Undo]),
        ("click pick", &[]),
        ("back", &[Intent::Confirm, Intent::Back]),
    ],
];

/// The lines of a legend, with each item's keys looked up in `map` (then
/// the session keys). An item listing intents none of which is bound is
/// left out; one listing none is a plain note.
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
                    if intents.is_empty() {
                        return Some(label.to_string());
                    }
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
                "x censor  a annotate  f filter  1-9 layers",
                "Enter capture  y text  p colour  Q code  P pin  Esc cancel  q quit",
            ]
        );
    }

    #[test]
    fn the_ink_palette_is_the_ink_table() {
        let names: Vec<&str> = crate::annotate::INKS.iter().map(|(_, n, _)| *n).collect();
        let shown: Vec<&str> = INKS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, shown);
        assert_eq!(
            render(&keymap::ANNOTATE, ANNOTATE_PALETTE),
            ["1 white  2 black  3 red  4 green  5 blue  r rectangle  c circle"]
        );
    }

    #[test]
    fn every_legend_item_is_bound() {
        for (name, map) in keymap::all() {
            let lines = match name {
                "select" => SELECT,
                "censor" => CENSOR,
                "annotate" => ANNOTATE,
                "shape" => SHAPE,
                "filter" => FILTER,
                "layers" => LAYERS,
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
