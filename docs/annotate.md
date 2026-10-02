# Annotate: text, arrows, callouts, the highlighter

**Status:** implemented in the Rust port (milestone 10). `README.md`
(Annotate, Annotation extensions) is the reference now; this page is the
design history. Where they differ, the README wins: line is `s` and the
highlighter is `m` (a pen mode), because Shift+hjkl move the keyboard pen;
the highlight is a trace, not a box; clicking text to re-edit it isn't
built.

> **Superseded: the highlighter.** The README and the architecture define
> the highlighter as a translucent **trace**, drawn like the pen
> (`Trace → HighlightOp`). The box-shaped highlight below (`H`, a
> multiply-blended box; `HighlightOp(rect, …)`) is the older design. Its
> key and exact op fields are still open.

What screenshots for docs and tickets actually need:
- arrows, pointing at the thing;
- numbered callouts, for step 1, step 2, step 3;
- text labels;
- a highlighter, to mark without hiding.

All four fit the existing model: box shapes are Shape (anchor and cursor),
and point things are placed at the pointer.

## Shapes: t cycles, and letters open one

Shape's `t` cycles rect → circle → line → arrow → highlight, keeping the
box. Each also has its own key in Annotate:

| Key (in Annotate) | Shape | Anchor → cursor means |
|---|---|---|
| `r` | rectangle | corners (as today) |
| `c` | circle | the bounding box (as today) |
| `L` | line | the two ends |
| `v` | arrow | from the tail to the head, with the head at the cursor |
| `H` | highlight | a translucent box, multiply-blended (marks without hiding text) |

- Arrow and line are the anchor/cursor pair, **drawn diagonally** in the
  box rather than as its outline, so every box key works on them: `o`
  swaps the ends (which flips the arrow's direction), Shift moves it
  whole, and Ctrl jumps to window edges.
- The arrowhead scales with the width (`+`/`-`). `V` in the Shape toggles
  a head at both ends.
- The highlight defaults to yellow at ~40%, and 1–5 still pick its colour.
  Its fill is the ink at a fixed opacity, never opaque (it's not a censor).
- `L`, `v` and `H` are capitals or unused letters, because hjkl stay
  movement keys.

## Numbered callouts: n

- `n` drops a numbered disc at the pointer: 1, then 2, 3… Each is an op
  in the layer, so `u` removes the last one and the next `n` reuses its
  number.
- A click while holding `n` places one at the click, for quick mouse use.
- Size follows the width; the colour is the ink, with the number in black
  or white, whichever contrasts.
- `N` sets the next number (typed), so a second shot can continue the
  first's steps: `N 4`.
- Numbering spans the layer. A new layer starts again at 1, unless
  `[annotate] callouts = "continue"` is set.

## Text: i

- `i` opens a text entry at the pointer: a caret, and typing puts text
  there. It's vim's insert: in Annotate, `i` means text, because Annotate
  has no Insert mode of its own.
- While typing, every key is text, so a text entry is a **mode of its own**
  on the stack (TEXT in the status tag):

  | Key | Does |
  |---|---|
  | Enter | lock the text in (Shift+Enter adds a line) |
  | Esc | drop it |
  | Ctrl+hjkl | move it, 1 px (+Shift: 10 px) |
  | Ctrl+`+`/`-` | size |
  | Ctrl+1–5 | colour |
  | Ctrl+b | toggle a chip behind it (black or white, the tags' look), for text over busy pictures |

- The font is the tags' monospace by default; `[annotate] font = "…"` sets
  another.
- Clicking the text later, in Annotate, reopens it for editing. That's
  something the stroke ops can't do.

## Ops

These are new layer ops, each with `key()` and `bbox()` like the others:
- `LineOp(a, b, rgb, width, heads)` covers lines and arrows.
- `HighlightOp(rect, rgb, alpha)`.
- `CalloutOp(centre, n, rgb, size)`.
- `TextOp(origin, text, rgb, size, chip)`.

Measure's bake step (`docs/measure.md`) reuses `LineOp` (dimension arrows)
and `TextOp` (labels).

## Open questions

- ~~Should the prototype get these before the port?~~ No: the prototype
  is frozen, and these come after parity (`TODO.md`).
- `v` for arrow, or `A`? (`a` itself is taken: lock and next.)
- Should a text entry's chip default on? Screenshots are busy, and
  unreadable annotations are worse than slightly heavy ones.
