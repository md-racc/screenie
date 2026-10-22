//! The `?` key legend, spelled from the keymaps so it can't drift from
//! what the keys do.

use crate::annotate::ShapeTool;
use crate::geometry::Dir;
use crate::intent::{Intent, Tool};
use crate::keymap::{Keymap, SESSION};

/// One legend entry: a label and the intents whose keys it lists.
pub type Item = (&'static str, &'static [Intent]);

/// A legend or palette: lines of items.
pub type Lines = &'static [&'static [Item]];

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
            "picture edges",
            &[
                Intent::PictureEdge(Dir::Right),
                Intent::PictureEdge(Dir::Left),
                Intent::PictureEdge(Dir::Down),
                Intent::PictureEdge(Dir::Up),
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
        ("measure", &[Intent::Measure(crate::measure::Prim::Line)]),
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

/// Select's palette: the modes it opens and what it finishes with, so a
/// session can go by mouse alone (docs/mouse.md).
pub const SELECT_PALETTE: &[&[Item]] = &[
    &[
        ("insert", &[Intent::Insert]),
        ("loupe", &[Intent::Loupe]),
        ("enhance", &[Intent::Enhance]),
        ("censor", &[Intent::Tool(Tool::Censor)]),
        ("annotate", &[Intent::Tool(Tool::Annotate)]),
        ("filter", &[Intent::Tool(Tool::Filter)]),
        ("measure", &[Intent::Measure(crate::measure::Prim::Line)]),
        ("layers", &[Intent::Choose(1)]),
    ],
    &[
        ("image", &[Intent::Confirm]),
        ("text", &[Intent::Ocr]),
        ("colour", &[Intent::CopyColour]),
        ("code", &[Intent::DecodeCode]),
        ("pin", &[Intent::Pin]),
    ],
];

/// Censor's palette: the kind of censor, and finding secrets, always shown.
pub const CENSOR_PALETTE: &[&[Item]] = &[&[
    ("pixelate/bar", &[Intent::CycleKind]),
    ("find secrets", &[Intent::Redact]),
]];

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
        (
            "move",
            &[
                Intent::Steer(Dir::Left),
                Intent::Steer(Dir::Down),
                Intent::Steer(Dir::Up),
                Intent::Steer(Dir::Right),
            ],
        ),
        ("Shift ×4  Alt half to edge", &[]),
        (
            "picture edges",
            &[
                Intent::Jump(Dir::Right),
                Intent::Jump(Dir::Left),
                Intent::Jump(Dir::Down),
                Intent::Jump(Dir::Up),
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
    ],
    &[
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
    render_items(map, lines)
        .into_iter()
        .map(|items| {
            items
                .into_iter()
                .map(|(t, _)| t)
                .collect::<Vec<_>>()
                .join(ITEM_GAP)
        })
        .collect()
}

/// What separates a line's items.
pub const ITEM_GAP: &str = "  ";

/// Each line's items, spelled as `render` joins them, with the intent a
/// click on one sends (its first; none for a label alone). Items whose
/// intents have no key here are left out, and so are empty lines.
pub fn render_items(map: &Keymap, lines: &[&[Item]]) -> Vec<Vec<(String, Option<Intent>)>> {
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
                        return Some((label.to_string(), None));
                    }
                    (!keys.is_empty())
                        .then(|| (format!("{} {label}", keys.join(" ")), Some(intents[0])))
                })
                .collect::<Vec<_>>()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

/// A mode's palette (shown always) and legend (under `?`), by name.
pub fn of(name: &str) -> Option<(Lines, Lines)> {
    Some(match name {
        "select" => (SELECT_PALETTE, SELECT),
        "censor" => (CENSOR_PALETTE, CENSOR),
        "annotate" => (ANNOTATE_PALETTE, ANNOTATE),
        "shape" => (SHAPE_PALETTE, SHAPE),
        "filter" => (FILTER_PALETTE, FILTER),
        "layers" => (&[], LAYERS),
        "insert" => (&[], INSERT),
        "loupe" => (&[], LOUPE),
        "enhance" => (&[], ENHANCE),
        "text" => (&[], TEXT),
        "redact" => (&[], REDACT),
        "measure" => (MEASURE_PALETTE, MEASURE),
        _ => return None,
    })
}

/// Every mode's keys, palette then legend, from the keymaps themselves
/// (`screenie help keys`). What the legend already lists is left out of
/// the palette, so each key is listed once, by the legend's name for it.
pub fn reference() -> Vec<(&'static str, Vec<String>)> {
    crate::keymap::all()
        .into_iter()
        .filter_map(|(name, map)| {
            let (palette, legend) = of(name)?;
            let palette = not_in(palette, legend);
            let palette: Vec<&[Item]> = palette.iter().map(Vec::as_slice).collect();
            let mut lines = render(map, &palette);
            lines.extend(render(map, legend));
            Some((name, lines))
        })
        .collect()
}

/// `palette`'s items that `legend` doesn't list already: by intent, by
/// label, or by a note naming it (`1-9 layers` for `layers`).
fn not_in(palette: Lines, legend: Lines) -> Vec<Vec<Item>> {
    let items = || legend.iter().flat_map(|l| l.iter());
    let listed = |&(label, intents): &Item| {
        items().any(|&(l, is)| {
            l == label
                || is.is_empty() && l.ends_with(&format!(" {label}"))
                || intents.first().is_some_and(|i| is.contains(i))
        })
    };
    palette
        .iter()
        .map(|line| line.iter().copied().filter(|i| !listed(i)).collect())
        .filter(|line: &Vec<Item>| !line.is_empty())
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
                "o/Space O/S-Space corner  w b } { edges  C-A-l C-A-h C-A-j C-A-k picture edges  0 $ g G far  c centre  + - scope  . last  u undo",
                "x censor  a annotate  f filter  1-9 layers",
                "i insert  m measure  z loupe  e enhance",
                "Enter capture  y text  p colour  Q code  P pin  Esc cancel  q quit",
            ]
        );
    }

    #[test]
    fn measure_legend() {
        assert_eq!(
            render(&keymap::MEASURE, MEASURE)[1],
            "h j k l move  Shift ×4  Alt half to edge  C-l C-h C-j C-k picture edges  0 $ g G far"
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
    fn the_reference_has_every_mode_and_their_keys() {
        let r = reference();
        assert_eq!(r.len(), keymap::all().len(), "every mode has a legend");
        let measure = &r.iter().find(|(n, _)| *n == "measure").unwrap().1;
        assert!(measure[0].contains("s line"), "palette first: {measure:?}");
        assert!(measure.iter().any(|l| l.contains("C calibrate")));
    }

    #[test]
    fn the_reference_lists_palette_keys_the_legend_already_has_once() {
        let r = reference();
        let get = |m: &str| r.iter().find(|(n, _)| *n == m).unwrap().1.clone();
        assert_eq!(get("select"), render(&keymap::SELECT, SELECT));
        assert_eq!(get("censor"), render(&keymap::CENSOR, CENSOR));
        assert!(
            get("annotate")[0].starts_with("1 white"),
            "palettes not in the legend stay"
        );
    }

    #[test]
    fn every_legend_item_is_bound() {
        for (name, map) in keymap::all() {
            let Some((_, lines)) = of(name) else {
                continue;
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
