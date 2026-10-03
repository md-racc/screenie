//! The `?` key legend, spelled from the keymaps so it can't drift from
//! what the keys do.

use crate::annotate::ShapeTool;
use crate::geometry::Dir;
use crate::intent::{Intent, Tool};
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
        ("censor", &[Intent::Tool(Tool::Censor)]),
        ("annotate", &[Intent::Tool(Tool::Annotate)]),
        ("filter", &[Intent::Tool(Tool::Filter)]),
        ("1-9 layers", &[]),
    ],
    &[
        ("insert", &[Intent::Insert]),
        ("loupe", &[Intent::Loupe]),
        ("enhance", &[Intent::Enhance]),
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
        ("find secrets", &[Intent::Redact]),
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
    ("rectangle", &[Intent::Shape(ShapeTool::Rect)]),
    ("circle", &[Intent::Shape(ShapeTool::Circle)]),
    ("arrow", &[Intent::Shape(ShapeTool::Arrow)]),
]];

pub const ANNOTATE: &[&[Item]] = &[
    &[
        ("width", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("pick colour", &[Intent::Eyedropper]),
        ("highlighter", &[Intent::ToggleHighlighter]),
        ("text", &[Intent::Insert]),
        ("callout", &[Intent::Callout]),
        ("callout number", &[Intent::CalloutNumber]),
        ("drag draw  S-drag straight  C-drag arc", &[]),
        ("arc bulge", &[Intent::Bulge(1), Intent::Bulge(-1)]),
    ],
    &[
        (
            "move pen  Shift ×4",
            &[
                Intent::Steer(Dir::Left),
                Intent::Steer(Dir::Down),
                Intent::Steer(Dir::Up),
                Intent::Steer(Dir::Right),
            ],
        ),
        ("pen down/up", &[Intent::PenToggle]),
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
    ("rect/circle/line/arrow", &[Intent::CycleKind]),
]];

pub const SHAPE: &[&[Item]] = &[
    &[
        ("width", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("pick colour", &[Intent::Eyedropper]),
        ("arrow both ends", &[Intent::ToggleHeads]),
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

pub const INSERT: &[&[Item]] = &[
    &[
        (
            "move",
            &[
                Intent::Steer(Dir::Left),
                Intent::Steer(Dir::Down),
                Intent::Steer(Dir::Up),
                Intent::Steer(Dir::Right),
            ],
        ),
        ("step", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("Shift ×4  Alt half to edge", &[]),
        (
            "edges",
            &[
                Intent::EdgeJump(Dir::Right),
                Intent::EdgeJump(Dir::Left),
                Intent::EdgeJump(Dir::Down),
                Intent::EdgeJump(Dir::Up),
            ],
        ),
    ],
    &[
        (
            "box sides",
            &[
                Intent::FarEdge(Dir::Left),
                Intent::FarEdge(Dir::Right),
                Intent::FarEdge(Dir::Up),
                Intent::FarEdge(Dir::Down),
            ],
        ),
        ("middle", &[Intent::Middle]),
        ("back", &[Intent::Undo]),
        ("click place", &[]),
    ],
    &[
        ("loupe", &[Intent::Loupe]),
        ("enhance", &[Intent::Enhance]),
        ("censor", &[Intent::Tool(Tool::Censor)]),
        ("annotate", &[Intent::Tool(Tool::Annotate)]),
        ("filter", &[Intent::Tool(Tool::Filter)]),
        ("colour", &[Intent::CopyColour]),
        ("back", &[Intent::Confirm, Intent::Back]),
    ],
];

pub const LOUPE: &[&[Item]] = &[
    &[
        ("1px  Shift a loupe-width", &[]),
        ("size", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        (
            "view filter",
            &[Intent::ViewFilter(1), Intent::ViewFilter(-1)],
        ),
        ("click/drag place", &[]),
        ("Measure: point", &[Intent::DropPoint]),
    ],
    &[
        ("colour", &[Intent::CopyColour]),
        ("enhance", &[Intent::Enhance]),
        ("censor", &[Intent::Tool(Tool::Censor)]),
        ("annotate", &[Intent::Tool(Tool::Annotate)]),
        ("filter", &[Intent::Tool(Tool::Filter)]),
        ("off", &[Intent::Loupe, Intent::Confirm, Intent::Back]),
    ],
];

pub const ENHANCE: &[&[Item]] = &[
    &[
        (
            "move lens  Shift faster  Alt / Alt+Shift halve / double",
            &[],
        ),
        ("zoom", &[Intent::Adjust(1), Intent::Adjust(-1)]),
    ],
    &[
        (
            "view filter",
            &[Intent::ViewFilter(1), Intent::ViewFilter(-1)],
        ),
        ("sharp/smooth", &[Intent::ToggleSharp]),
        ("drag new lens  click centre", &[]),
        ("loupe", &[Intent::Loupe]),
        ("back", &[Intent::Confirm, Intent::Back]),
    ],
];

pub const TEXT: &[&[Item]] = &[
    &[
        ("type text  S-Enter new line", &[]),
        ("done", &[Intent::Confirm]),
        ("drop", &[Intent::Back]),
    ],
    &[
        (
            "move",
            &[
                Intent::Jump(Dir::Left),
                Intent::Jump(Dir::Down),
                Intent::Jump(Dir::Up),
                Intent::Jump(Dir::Right),
            ],
        ),
        ("size", &[Intent::Adjust(1), Intent::Adjust(-1)]),
        ("C-1-5 colour", &[]),
        ("chip", &[Intent::ToggleChip]),
    ],
];

pub const REDACT: &[&[Item]] = &[&[
    ("select", &[Intent::ListStep(-1), Intent::ListStep(1)]),
    ("tick", &[Intent::ListToggle]),
    ("drop", &[Intent::ListDelete]),
    ("accept ticked", &[Intent::Accept, Intent::Confirm]),
    ("rescan", &[Intent::Redact]),
    ("back", &[Intent::Back]),
]];

/// Measure's primitives, always shown (README: palettes for directly
/// selectable kinds).
pub const MEASURE_PALETTE: &[&[Item]] = &[&[
    ("line", &[Intent::Measure(crate::measure::Prim::Line)]),
    ("circle", &[Intent::Measure(crate::measure::Prim::Circle)]),
    ("area", &[Intent::Measure(crate::measure::Prim::Area)]),
    ("rect", &[Intent::Measure(crate::measure::Prim::Rect)]),
    ("angle", &[Intent::Measure(crate::measure::Prim::Angle)]),
]];

pub const MEASURE: &[&[Item]] = &[
    &[
        ("point", &[Intent::DropPoint]),
        ("path point", &[Intent::AddPoint]),
        ("finish", &[Intent::Confirm]),
        ("undo", &[Intent::Undo]),
        (
            "edit points  A-click select, A-drag move",
            &[Intent::SwapOpposite],
        ),
    ],
    &[
        ("edge", &[Intent::Jump(crate::geometry::Dir::Right)]),
        (
            "step, or bend a line",
            &[Intent::Adjust(1), Intent::Adjust(-1)],
        ),
        ("loupe", &[Intent::Loupe]),
        ("enhance", &[Intent::Enhance]),
        ("calibrate", &[Intent::Calibrate]),
        ("m/ft", &[Intent::UnitSystem]),
        ("px", &[Intent::ShowPixels]),
    ],
    &[
        ("select", &[Intent::ListStep(-1), Intent::ListStep(1)]),
        ("move", &[Intent::ListMove(-1), Intent::ListMove(1)]),
        ("delete", &[Intent::ListDelete]),
        ("total", &[Intent::ListToggle]),
        ("copy", &[Intent::CopyMeasurements]),
        ("back", &[Intent::Back]),
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
                "i insert  z loupe  e enhance",
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
            ["1 white  2 black  3 red  4 green  5 blue  r rectangle  c circle  v arrow"]
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
                "insert" => INSERT,
                "loupe" => LOUPE,
                "enhance" => ENHANCE,
                "text" => TEXT,
                "redact" => REDACT,
                "measure" => MEASURE,
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
