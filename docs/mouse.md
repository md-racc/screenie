# Mouse: clickable palettes, and the box by its edges

**Status:** built (2026-10-05). `README.md` (Mouse, Tags) is the spec
now; this page keeps the reasons.

The goal is a whole session by mouse alone: box, a build tool, its kinds,
commit, finish. Today the mouse can draw and snap the box, scroll scope,
capture (right click) and cancel (middle click), but it can't open a
mode, pick a kind, resize the box or move it.

## Clickable palettes

- **Every palette item is a click target.** A left click on an item sends
  its intents, exactly as its key would: palettes are already
  `(label, intents)` data (`legend.rs`), so undo, legends and tests don't
  change.
- **Select gets a palette**, always visible above the box like the build
  modes' (the README rule: palettes are shown when a mode has directly
  selectable things):

  ```text
  i insert  z loupe  e enhance  x censor  a annotate  f filter  m measure  1 layers
  ⏎ image  y OCR  p colour  Q code  P pin
  ```

  The finish row is there because right click only gives the image.
- **Censor gets a palette**: `t pixelate/bar` (the type) and `r find
  secrets` (auto-redact), so both have a mouse way too.
- **A chip takes its clicks.** A press on any chip doesn't reach the
  picture or the box; a drag starting on one does nothing.
- **Hover shows the target:** the item under the pointer is drawn
  inverted, as Measure's selected chip is.
- Leaving a build mode stays the buttons: right click is Enter (commit),
  middle click Esc. No `commit`/`cancel` chips.

## The box by its edges

In Select, Censor and Filter (`boxes::apply` with handles). Not the pen
or the highlighter, where a drag is a stroke, and not Annotate's shapes,
whose cursor corner follows the pointer: every press would land on it,
and a plain drag there draws a new shape:

- **Grab:** a press within 6 px of the box's edge, inside or out, grabs
  that edge; within 6 px of a corner, the corner. Dragging an edge moves
  only it; a corner moves, its opposite stays.
- **Hover:** the pointer becomes the matching resize cursor
  (wp-cursor-shape: `ns-resize`, `ew-resize`, `nwse-resize`,
  `nesw-resize`), so a grab is visible before the press.
- **Move:** Alt+drag inside the box moves it whole (cursor `move`), as
  Alt+drag moves a measurement's point: Alt is about objects, and here the
  box is the object.
- **Centre mode** (`c`), when on, makes an edge drag resize about the
  centre. No new modifier.
- An edge may cross the opposite one, as the cursor crosses the anchor.
- **One drag is one undo step**, like a drawn region.
- **Precedence:** chip, then Alt+drag (move), then corner, then edge, then
  the mode's own press: a plain drag elsewhere still draws a new region
  and a click still snaps to a window.

## Why

- **Palettes, not a pop-up menu or a toolbar.** A gesture menu would
  need a button that isn't taken (right click is Enter, middle is Esc)
  and is invisible to a mouse-only user; a fixed toolbar sits away from
  the box and needs its own placement rules. Palettes already exist,
  are placed by the tag rules, and are data.
- **A plain drag inside the box draws, it doesn't move.** The box starts
  on the active window, so most fresh drags begin inside it.
- **The cost of edge grabs:** the starting box's edges are the window's,
  so a drag meant to start a new region exactly on the frame grabs the
  edge. The zone is small and the cursor shows it; start a few pixels off.

## Testing

Headless, through `Session::handle` with pointer events on the fixture:
a palette click equals its key (for each palette); a press on a chip
doesn't move the box; edge, corner, Alt and centre-mode drags; crossing;
one undo step a drag; precedence at a corner on a chip. The hover cursor
and inversion are platform and render tests.
