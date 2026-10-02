//! Semantic intents: what the user means, not which key they pressed.
//!
//! The keymap turns chords into intents, the pointer mapping turns buttons
//! and the wheel into the same intents, and the top mode resolves them.

use crate::geometry::{Dir, LogicalPoint};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scale {
    Half,
    Double,
}

/// A set of held directions (`k` + `l` is diagonal).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DirSet(u8);

impl DirSet {
    fn bit(d: Dir) -> u8 {
        match d {
            Dir::Left => 1,
            Dir::Right => 2,
            Dir::Up => 4,
            Dir::Down => 8,
        }
    }

    pub fn insert(&mut self, d: Dir) {
        self.0 |= Self::bit(d);
    }

    pub fn contains(self, d: Dir) -> bool {
        self.0 & Self::bit(d) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Dir> {
        Dir::ALL.into_iter().filter(move |&d| self.contains(d))
    }

    /// The held directions as one vector: opposite keys cancel.
    pub fn vector(self) -> (f64, f64) {
        self.iter().fold((0.0, 0.0), |(x, y), d| {
            let (dx, dy) = d.vec();
            (x + dx, y + dy)
        })
    }
}

impl FromIterator<Dir> for DirSet {
    fn from_iter<I: IntoIterator<Item = Dir>>(iter: I) -> Self {
        let mut s = DirSet::default();
        for d in iter {
            s.insert(d);
        }
        s
    }
}

/// The build tools, by the key that opens them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    /// `x`
    Censor,
    /// `a`
    Annotate,
    /// `f`
    Filter,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Intent {
    // The box grammar on hjkl and its modifiers.
    /// `hjkl`: steer the active thing (the cursor corner).
    Steer(Dir),
    /// `Shift+hjkl`: move the whole box.
    Translate(Dir),
    /// `Ctrl+hjkl`: jump to the next meaningful target.
    Jump(Dir),
    /// `Ctrl+Shift+hjkl`: slide the box to the next meaningful boundary.
    Slide(Dir),
    /// `Alt+hjkl` (half) and `Alt+Shift+hjkl` (double).
    Scale(Dir, Scale),
    /// Held directions repeating: `steps` more steps of steering.
    HeldSteer(DirSet, u32),
    /// Held directions repeating with Shift: `steps` more steps of moving.
    HeldTranslate(DirSet, u32),

    // Box keys.
    /// `o` / `Space`.
    SwapOpposite,
    /// `O` / `Shift+Space`.
    SwapSideways,
    /// `w b } {`: the next nearby edge that way.
    EdgeJump(Dir),
    /// `0 $ g G`: the far edge that way.
    FarEdge(Dir),
    /// `c`: centre mode.
    ToggleCentre,

    // Common.
    /// `+` / `-` and the wheel: the current amount.
    Adjust(i32),
    Undo,
    Confirm,
    Back,
    Quit,
    ToggleHelp,

    // Build tools.
    /// A build tool's key: from Select it opens the tool (and a layer);
    /// in that tool it locks the item and begins another.
    Tool(Tool),
    /// `t`: the next kind (censor type, shape).
    CycleKind,
    /// `1`-`9` in a palette: pick that option (an ink colour).
    Choose(u8),
    /// `p` in Annotate and Shape: the colour under the pointer becomes the
    /// ink.
    Eyedropper,
    /// `r` / `c` in Annotate: a shape, as a box of its own.
    Shape(crate::model::ShapeKind),

    // Lists (Filter, Layers).
    /// `[` / `]`: select the item below (-1) or above (+1).
    ListStep(i32),
    /// `<` / `>`: move the selected item down (-1) or up (+1).
    ListMove(i32),
    /// `d`: delete the selected item.
    ListDelete,
    /// `Space`: mute or hide the selected item.
    ListToggle,

    // Select.
    /// `.`: the previously accepted boxes.
    Recall,
    /// `y`: OCR.
    Ocr,
    /// `p`: the cursor pixel's colour.
    CopyColour,
    /// `Q`: decode a QR code or barcode.
    DecodeCode,
    /// `P`: pin.
    Pin,

    // The pointer, in logical coordinates.
    /// The left button went down.
    Press(LogicalPoint),
    /// The pointer moved (with or without a button down).
    Motion(LogicalPoint),
    /// The left button came up.
    Release(LogicalPoint),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_held_directions_cancel() {
        let s: DirSet = [Dir::Left, Dir::Right, Dir::Down].into_iter().collect();
        assert_eq!(s.vector(), (0.0, 1.0));
        assert_eq!(
            s.iter().collect::<Vec<_>>(),
            [Dir::Left, Dir::Right, Dir::Down]
        );
    }
}
