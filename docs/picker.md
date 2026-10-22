# The picker

This is the specification of screenie's picker: what every key, click and
mode does. When the code and this document disagree, this document wins.
The command line is in [cli.md](cli.md); auto-redact and Measure have
their own references, [auto-redact.md](auto-redact.md) and
[measure.md](measure.md).

`screenie help keys` prints every mode's keys, and `?` in the picker shows
the current mode's.

## Contents

- [The frozen frame](#the-frozen-frame)
- [The grammar](#the-grammar): directions, adjustment, universal keys
- [Mouse](#mouse): clicks, palettes, the box by its edges
- [Tags](#tags): status, palettes, legends
- [Mode families](#mode-families)
- [Layers](#layers): the open layer, locking and committing
- [Select](#select): scope, recall, picture edges
- [Insert](#insert)
- [Loupe](#loupe)
- [Enhance](#enhance)
- [Censor](#censor) and [auto-redact](#auto-redact)
- [Filter](#filter)
- [Annotate](#annotate): pen, highlighter, text, callouts, shapes
- [Lists](#lists)
- [Layers view](#layers-view)
- [Undo](#undo)
- [Measure](#measure)
- [Output](#output): image, OCR, colour, codes, pins
- [Projects and history](#projects-and-history)
- [Coordinates and scaling](#coordinates-and-scaling)

## The frozen frame

screenie captures the visible desktop once and covers every monitor with
an overlay showing that frozen frame. Everything happens on that frame:
changes to the live desktop after the capture never reach the output.

The capture itself is never modified. Edits are ordered operations in
layers over it, so they stay editable until the shot is delivered, and
afterwards through its project.

## The grammar

The picker is modal. **Select** is the base mode. Other modes take over
input for a while, keeping the same movement and adjustment keys wherever
they make sense. Learn Select, and the rest follows.

### Directions

`h j k l` and the arrow keys are directions everywhere. Held directions
repeat with acceleration and combine: holding `k` and `l` moves
diagonally.

| Input             | Meaning                                                     |
| ----------------- | ----------------------------------------------------------- |
| `hjkl` / arrows   | Steer the active thing                                      |
| `Shift+hjkl`      | Move the whole box, or take a larger step in point modes    |
| `Ctrl+hjkl`       | Jump to the next meaningful target                          |
| `Ctrl+Shift+hjkl` | Slide or jump the whole box to the next meaningful boundary |
| `Alt+hjkl`        | Halve, or scale toward that side                            |
| `Alt+Shift+hjkl`  | Double, or scale outward toward that side                   |
| `Ctrl+Alt+hjkl`   | Jump to the next edge in the picture (box modes)            |

What a jump reaches depends on the mode: in Select and the other box
modes the box becomes the neighbouring window (or monitor) that way; in
Measure the cursor goes to the next edge in the picture. A mode where a modifier
means nothing ignores that modified key completely: a meaningless
modifier never falls through to the unmodified action.

### Adjustment

`+` and `-` (`=` works for `+`) adjust **the current amount**, and the
mouse wheel does the same. Depending on the mode, that is the selection
scope, the cursor step, the line width, the censor strength, the filter
amount, the loupe size, the zoom or the selected layer. There is at most
one adjustable amount at a time.

### Universal keys

| Input   | Meaning                                                     |
| ------- | ----------------------------------------------------------- |
| `z`     | Loupe                                                       |
| `e`     | Enhance                                                     |
| `u`     | Undo the last completed action in the current editing scope |
| `Enter` | Done, confirm                                               |
| `Esc`   | Back                                                        |
| `q`     | Quit                                                        |
| `?`     | Show or hide the current mode's key legend                  |

Keys held across a mode change don't count as fresh presses in the new
mode. Other command keys fire once per physical press, unless their
action is one that repeats.

## Mouse

The mouse covers the whole screenshot workflow on its own. Keyboard and
mouse needn't take the same physical steps, but they reach the same
operations and the same editable model.

| Input                         | Meaning                                               |
| ----------------------------- | ----------------------------------------------------- |
| left drag                     | Draw or change the active geometry                    |
| left click                    | Place, select or snap the active thing                |
| wheel                         | `+` / `-`                                             |
| right click                   | `Enter`                                               |
| middle click                  | `Esc`                                                 |
| click a palette item          | What its key does                                     |
| drag the box's edge or corner | Move that edge or corner (Select, Censor, Filter)     |
| `Alt`+drag inside the box     | Move the box whole (Select, Censor, Filter)           |

A session by mouse alone:

1. click a window, or drag a region;
2. scroll for a wider scope;
3. click a mode in Select's palette (censor, annotate…) and work in it;
   right click commits, middle click discards;
4. right click for the image, or click a finish in Select's palette (text,
   colour, code, pin).

### Palettes

Every palette item is a click target, and a click sends exactly what its
key would. Clicks on any chip belong to the chip: they never reach the
picture or the box, and a drag that starts on a chip does nothing. The
item under the pointer is drawn inverted. There are no commit or cancel
chips: those are the right and middle buttons.

### The box by its edges

In Select, Censor and Filter:

- A press within 6px of the box's edge, inside or out, takes hold of that
  edge; within 6px of a corner, of the corner. Hovering there first shows
  the matching resize pointer, and Alt over the box shows the move
  pointer.
- Dragging an edge moves only that edge. Dragging a corner moves it while
  the opposite corner stays. An edge can cross the opposite one.
- Centre mode (`c`) makes an edge drag resize about the centre.
- `Alt`+drag inside the box moves it whole.
- One drag is one undo step.
- A press is taken by, in order: a chip, `Alt`+drag (move), a corner, an
  edge, and then the mode's own press. Anywhere else, a drag still draws a
  new box and a click still does what the mode's click does.

The box starts on the active window, so its edges are the window's: to
draw a new region from exactly the window's frame, start a few pixels off
it.

The pen, the highlighter and Annotate's shapes have no edge handles: a
drag there draws (a shape's cursor corner is always under the pointer).

## Tags

Tags are the small text chips around the active box: near-opaque black
chips, with white borders.

Each kind has a preferred side: the status at the bottom, the palette and
key legend at the top, lists (Filter, Layers, auto-redact, Measure) at the
sides. A tag goes outside the box on its preferred side, else inside the
box on that side, else on another side.

Tags stay wholly on one monitor and never cross a monitor seam. They avoid
each other, avoid mode-owned UI such as the loupe, and avoid covering the
active box where they can.

The status tag is dense and vim-like: the mode, the box's size in capture
pixels, the active cursor's pixel, then whatever is worth knowing.

```text
CENSOR  560×90  (659,259)  layer +1  pixelate 10px  20×20
```

A **palette** is always shown when a mode has directly selectable kinds:

| Mode     | Palette                                                         |
| -------- | --------------------------------------------------------------- |
| Select   | the modes it opens, and its finishes (below)                    |
| Censor   | `t pixelate/bar`, `r find secrets`                              |
| Annotate | the five inks; `r rectangle`, `c circle`, `v arrow`             |
| Shape    | the five inks; `t rect/circle/line/arrow`                       |
| Filter   | `1 grey`, `2 invert`, `3 bright`, `4 contrast`, `5 blur`        |
| Measure  | `s line`, `c circle`, `A area`, `r rect`, `^ angle`             |

Select's palette:

```text
i insert  z loupe  e enhance  x censor  a annotate  f filter  m measure  1 layers
Enter image  y text  p colour  Q code  P pin
```

The full key legend is behind `?`.

## Mode families

| Kind                | Modes                           | Purpose                               | Changes output?   |
| ------------------- | ------------------------------- | ------------------------------------- | ----------------- |
| **Target**          | Select, Insert                  | Choose a region or a point            | No                |
| **Lens**            | Loupe, Enhance                  | Change how the frozen image is viewed | Never             |
| **Build**           | Censor, Annotate, Shape, Filter | Add operations to the open layer      | When committed    |
| **Manage**          | Layers                          | Inspect and change committed layers   | Yes               |
| **Measure**         | Measure                         | Measure the frozen picture            | Never             |
| **Temporary input** | Text entry, auto-redact review  | Collect a value or temporary geometry | Only if confirmed |
| **Finish**          | image, text, colour, code, pin, measurements | Produce an output        | —                 |

Modes form a stack. Only the top mode receives input, and modes draw
bottom to top, so a temporary child appears over the mode that opened it.
A lens may hand a position back to its parent; it never hands back
changed pixels.

## Layers

The captured frame is immutable. The first build tool opened creates an
implicit **open layer**, clipped to the current selection. Every build
operation made before the layer is committed goes into that layer, in
order, and order matters: a filter added after an annotation filters the
annotation too.

A layer holds editable operations: censors, pen strokes, highlights,
shapes, lines and arrows, text, callouts and filters.

### Locking and committing

Build tools tell apart the item being edited and the operations already
locked into the open layer.

| Input                  | Meaning                                                  |
| ---------------------- | -------------------------------------------------------- |
| the current tool's key | Lock the current item and begin another                  |
| another build tool key | Lock the current item and switch tools in the same layer |
| `u`                    | Undo the last completed action in the open layer         |
| `Enter`                | Lock the current item and commit the layer               |
| `Esc`                  | Discard the open layer                                   |

The build tool keys are `x` (Censor), `a` (Annotate) and `f` (Filter),
in every build tool and in the loupe opened over one. In Filter, `f`
duplicates the selected filter instead; in a Shape, `a` locks it and goes
back to the pen.

- With locked work in the open layer, the first `Esc` warns (`Esc again
  to discard …`) and a second discards. Any other action takes the warning
  back.
- `q` with locked work warns the same way (`q again to quit, discarding
  …`).
- Committing an empty layer adds nothing. Committing or discarding goes
  back to the mode the layer was opened from.
- Undo never crosses an editing-scope boundary: when the open layer has
  nothing left to undo, `u` does nothing rather than undo committed
  project history.

## Select

Select chooses the output region and is the base of the picker. The box is
an **anchor** corner and a **cursor** corner, as in vim's visual-block
mode, and the cursor may cross the anchor.

The box starts on the active window (or on `--region`; see
[cli.md](cli.md)).

| Input               | Action                                                         |
| ------------------- | -------------------------------------------------------------- |
| `h j k l`           | Move the cursor corner                                         |
| `Shift+hjkl`        | Move the whole box                                             |
| `Ctrl+hjkl`         | Select the nearest window that way (one beside the box, else one the box sits inside, else a monitor) |
| `Ctrl+Shift+hjkl`   | Slide the whole box until an edge lands on the next window or monitor edge, or a monitor's quarter line (lines within 12px are skipped) |
| `Alt+hjkl`          | Halve the box toward that side                                 |
| `Alt+Shift+hjkl`    | Double the box toward that side                                |
| `Ctrl+Alt+hjkl`     | Cursor edge to the next edge in the picture                    |
| `o` / `Space`       | Make the opposite corner the cursor                            |
| `O` / `Shift+Space` | Make the sideways corner the cursor                            |
| `w b } {`           | Next nearby window or monitor edge (right, left, down, up)     |
| `0 $ g G`           | Far left, right, top, bottom                                   |
| `c`                 | Centre mode: resize symmetrically about the centre             |
| `+` / `-`, wheel    | Change the selection scope                                     |
| `.`                 | Recall previously accepted boxes                               |
| `u`                 | Undo the last Select or project action                         |
| click               | Snap to the window under the pointer, or the nearest monitor   |
| drag                | Draw a region                                                  |
| `i`                 | Insert                                                         |
| `m`                 | Measure                                                        |
| `z`                 | Loupe                                                          |
| `e`                 | Enhance                                                        |
| `x`                 | Censor                                                         |
| `a`                 | Annotate                                                       |
| `f`                 | Filter                                                         |
| `1`–`9`             | Layers view, at that layer                                     |
| `Enter`             | Capture the image                                              |
| `y`                 | OCR the box                                                    |
| `p`                 | Copy the colour of the cursor corner's pixel                   |
| `Q`                 | Decode a QR code or barcode                                    |
| `P`                 | Pin                                                            |
| `Esc`               | Cancel                                                         |

- A box that covers no output pixel says `empty box` and refuses `Enter`,
  `y`, `Q`, `P`, the build tools and Insert (`p` and Measure still work).
- `--result` changes what `Enter` produces, and `--lock-box` refuses every
  key that would change the box (`the box is locked`). See [cli.md](cli.md).

### Scope

The box is always plain anchor/cursor geometry. The scope says which
objects snapping and widening care about:

```text
window → monitor → everything
```

`+`, `-` and the wheel change scope. A hand-drawn box may first widen to
the window that contains it, then out through the broader scopes.

### Recall

The last ten accepted boxes are kept, each tied to its monitor so it comes
back in the same place when it can. `.` recalls the last one, and `.`
again, while the box is still the one it gave, steps back to older ones
and round. The status shows `last 2/5`; with none, `no last selection`.
The ring is shared with `--region last-region` (see [cli.md](cli.md)).

### Picture edges

`Ctrl+Alt+hjkl` treats the box's cursor edge as a line between pixels. It
looks from the pixel just ahead of the edge, along the cursor corner's row
or column, and stops just before the first pixel that differs. So each
press stops at a boundary (up to a thing, then past it), the same stops
come back the other way, and the edge stays on the corner's monitor. In
centre mode the edges move mirrored about the centre. With no edge that
way, the box stays and the status says so. Censor, Filter and Shape have
the same keys.

## Insert

Insert picks a single pixel inside the box. The box stays put and a free
cursor appears inside it, drawn to stay visible against any colour.

| Input           | Action                                                   |
| --------------- | -------------------------------------------------------- |
| `h j k l`       | Move by the step                                         |
| `Shift+hjkl`    | Move four steps                                          |
| `Alt+hjkl`      | Move halfway to the box's edge                           |
| `+` / `-`       | Change the step (1–20px, starting at 5)                  |
| `w b } {`       | Next window or monitor edge, else the box's edge         |
| `0 $ g G`       | The box's left, right, top, bottom                       |
| `m`             | The box's middle                                         |
| `u`             | Undo the cursor's last move                              |
| click           | Place the cursor                                         |
| `z`             | Loupe at the cursor                                      |
| `e`             | Enhance centred on the cursor                            |
| `x a f`         | Open a build tool from the cursor                        |
| `p`             | Copy the cursor pixel's colour                           |
| `Enter` / `Esc` | Back to Select                                           |

A censor opened from Insert has its top-left corner on the cursor's
pixel. Annotate and Filter take the whole box. After the layer is
committed or discarded, the picker returns to Insert.

## Loupe

The loupe is a precision pixel lens. It shows a magnified crop with a
pixel grid, the active pixel, its colour and its coordinates. The
magnifier is placed like a tag, beside the cursor and off the box where it
fits, and stays on the cursor's monitor.

| Input                   | Action                                                    |
| ----------------------- | --------------------------------------------------------- |
| `h j k l`               | Move one pixel                                            |
| `Shift+hjkl`            | Move about one loupe width                                |
| `+` / `-`               | Change the loupe's size (9–61 pixels across in steps of 4, starting at 21) |
| `v` / `V`               | Next or previous view filter                              |
| click / drag            | Place or move the cursor                                  |
| `Space` / `Alt+Space`   | Over Measure: drop a point at the loupe's pixel, and stay |
| `Shift+Space`           | Over Measure: drop a path's point there                   |
| `p`                     | Copy the pixel's colour                                   |
| `e`                     | Enhance at the loupe's position                           |
| `x a f`                 | Open a build tool at the loupe's position                 |
| `z` / `Enter` / `Esc`   | Close the loupe                                           |

- When the loupe closes, its parent may take its position.
- The loupe remembers its last pixel for the rest of the run (it isn't
  saved). From Select, `z` reopens it there, clamped into the box if the
  box has changed, and Annotate's shapes start there.
- A censor opened from the loupe is centred on its pixel. Opened over a
  build tool, `x a f` lock that tool's item and switch tools in the same
  layer.
- Points dropped through the loupe land exactly on its pixel and never
  snap.
- The loupe never changes image pixels.

## Enhance

Enhance is a movable magnified view inside the box. It never shows pixels
outside the box.

| Input            | Action                                              |
| ---------------- | --------------------------------------------------- |
| `h j k l`        | Move the lens                                       |
| `Shift+hjkl`     | Move by larger steps                                |
| `Alt+hjkl`       | Halve the lens toward that side                     |
| `Alt+Shift+hjkl` | Double the lens toward that side                    |
| `+` / `-`        | Zoom, in 0.5 steps (1× to 64×, starting at 2×)      |
| `v` / `V`        | Next or previous view filter                        |
| `s`              | Sharp or smooth scaling                             |
| drag             | Draw a new lens                                     |
| click            | Centre the lens there                               |
| `z`              | Loupe at the lens's centre                          |
| `Enter` / `Esc`  | Close Enhance                                       |

The view filters are off, greyscale, invert and contrast (for
readability). They change only what you see and never reach the output:
to change the image, use [Filter](#filter).

## Censor

Censor makes obscuring rectangles inside the box, with the same
anchor/cursor grammar as Select. A censor starts as a 20px square at the
box's centre.

| Input                 | Action                                 |
| --------------------- | -------------------------------------- |
| `t`                   | Pixelate or solid bar                  |
| `+` / `-`             | The current kind's amount              |
| `x`                   | Lock this censor and begin another     |
| `r`                   | Find secrets ([auto-redact](#auto-redact)) |
| `a` / `f`             | Lock it, and switch to Annotate / Filter |
| box keys              | Resize, move, scale, as in Select      |
| `u` / `Enter` / `Esc` | Undo, commit, discard                  |

- Pixelate's amount is its block size: 10px by default, 4–40px in 2px
  steps. A bar's amount is its shade, from black (the default) to white
  in tenths. Each kind keeps its own amount across `t` and `x`.
- `x` starts the next censor where the last one was.
- Pixelation obscures; it is not secure redaction (pixelated text can
  sometimes be recovered). It stays the default manual censor.

### Auto-redact

Auto-redact finds likely secrets in the box and proposes censors over
them. You review and accept them: it never decides on its own, except with
`--redact auto`.

`r` in Censor scans the box, in the background, and opens a review over
Censor. Suggestions draw as dashed boxes with their category in a chip,
and the side list shows them with their text elided. Ticked ones are
filled dark, with the text faintly through, as a preview; accepted, they
become black bars, or pixelation with `[redact] kind = "pixelate"`.
Low-confidence matches are proposed unticked.

In the review, `[` / `]` select a suggestion, `Space` ticks it, `d` drops
it, `a` or `Enter` accepts the ticked ones as censors (one undo step: `u`
in Censor takes them back), `r` rescans and `Esc` drops them all. The
review, the detectors (emails, IP and MAC addresses, tokens, card numbers,
IBANs, private keys, codes and more), the configuration and the limits are
in [auto-redact.md](auto-redact.md#reviewing-suggestions).

## Filter

Filter keeps an ordered stack of image filters in the open layer. Each
filter has a kind, its own region, an amount where it has one, a mute
state and a place in the order.

| Key | Filter     | Amount           | Starts at |
| --- | ---------- | ---------------- | --------- |
| `1` | Greyscale  | 25%–100%, by 25% | 100%      |
| `2` | Invert     | none             |           |
| `3` | Brightness | −5…+5            | +1        |
| `4` | Contrast   | −4…+4            | +1        |
| `5` | Blur       | 2–20px radius, by 2 | 4px    |

| Input                     | Action                                       |
| ------------------------- | -------------------------------------------- |
| `1`–`5`                   | Insert that filter                           |
| `+` / `-`                 | The selected filter's amount                 |
| `f`                       | Duplicate the selected filter                |
| `[ ] < > d Space`         | Select, reorder, delete, mute ([Lists](#lists)) |
| box keys                  | Edit the selected filter's region            |
| `x` / `a`                 | Switch to Censor / Annotate                  |
| `u` / `Enter` / `Esc`     | Undo, commit, discard                        |

- A new filter goes just above the selected one (on top if none is) and
  becomes selected.
- A new filter takes the box's current region: the selected filter's, or,
  with none selected, the whole box at first and afterwards the last
  selected filter's. The box shows the selected filter's region.
- `Space` mutes here; `Shift+Space` is still the sideways corner.
- A run of the same edit to the same filter (holding `+`, repeated taps,
  one drag) is one undo step.

## Annotate

Annotate adds visible markup. It has no box of its own: the pen is the
default tool.

The ink is a colour and a width. The colours are `1` white, `2` black,
`3` red (the default), `4` green and `5` blue, and `p` takes the colour
under the pen point instead. The width starts at 3px and runs 1–40px. The
pen, shapes, text and callouts share the ink, and it lasts across layers
for the rest of the run.

| Input                          | Action                                         |
| ------------------------------ | ---------------------------------------------- |
| drag                           | Draw a freehand pen stroke                     |
| `Shift`+drag                   | Hold the line straight                         |
| `Ctrl`+drag                    | Hold the line as an arc                        |
| `Ctrl`+`+`/`-`, `Ctrl`+wheel   | The arc's bulge, 2px a step                    |
| `h j k l`                      | Move the pen point 5px (`Shift`: 20px)         |
| `Space`                        | Pen down / up (up locks the stroke)            |
| `1`–`5`                        | Colour                                         |
| `p`                            | Eyedropper                                     |
| `+` / `-`                      | Width                                          |
| `m`                            | Highlighter on / off                           |
| `r`                            | Rectangle                                      |
| `c`                            | Circle                                         |
| `v`                            | Arrow                                          |
| `i`                            | Text at the pen point                          |
| `n`                            | Numbered callout at the pen point              |
| `N`                            | Set the next callout number                    |
| `z` / `e`                      | Loupe / Enhance at the pen point               |
| `x` / `f`                      | Switch to Censor / Filter                      |
| `u` / `Enter` / `Esc`          | Undo, commit, discard                          |

Every extension reuses the grammar above: the point, the anchor/cursor
box, the trace, the ink and the one adjustable amount.

### The pen

The pen works with the mouse or the keys. The keys move a pen point, drawn
as the pixel cursor and kept inside the box, and `Space` puts the pen down
and lifts it. A stroke is locked when the pen lifts, when the mouse button
is let go, and when anything else ends it (another tool, text, the
highlighter, commit). A mouse press while a keyboard line is down locks
that line first.

While drawing with the mouse, holding `Shift` holds the piece from where
it was pressed straight to the pointer, and holding `Ctrl` holds it as an
arc over that chord. The arc's bulge is its height off the chord: it
starts at 24px, `Ctrl`+`+`/`-` or `Ctrl`+wheel change it by 2px, below
zero it bulges the other way, and it is kept for the rest of the run.
Letting go of the modifier carries the line on freehand from the end of
the piece, and pressing it again starts a new piece from there. The status
says `straight` or `arc 24px` while one is held.

### Highlighter

`m` swaps the pen for the highlighter: a translucent trace drawn exactly
like the pen (straight and arc pieces too), in the ink at 40%, multiplied,
five times the line width, so text under it stays readable. `m` again
goes back to the pen.

### Text

`i` opens a text entry at the pen point. A text entry is a mode of its own
(TEXT in the status): typed characters are literal text, and letter
commands are suspended until it is locked or dropped.

| Input                 | Action                                    |
| --------------------- | ----------------------------------------- |
| characters            | Type                                      |
| `Shift+Enter`         | New line                                  |
| `Backspace`           | Delete the last character                 |
| `Enter`               | Lock the text                             |
| `Esc`                 | Drop it                                   |
| `Ctrl+hjkl`           | Move it 1px (`Ctrl+Shift`: 10px)          |
| `Ctrl+` `+` / `-`     | Size: 18px to start, 8–96px in 2px steps  |
| `Ctrl+1`–`5`          | Colour (the ink)                          |
| `Ctrl+b`              | Background chip on / off (on by default)  |

- The text previews where it will go, with a caret. Empty text locks
  nothing.
- The chip is black behind light ink and white behind dark ink, so text
  stays readable over busy pictures.
- The font is the tags' monospace.
- It is deliberately small: not a rich-text editor.

### Numbered callouts

`n` places a numbered disc at the pen point: the ink, with the number in
black or white, whichever contrasts. Its size follows the width.

The next number is the open layer's highest plus one, so the first is 1,
a new layer starts again at 1, and undoing a callout frees its number.
`N` asks for the next callout's number (`N 4` makes the next one 4, so a
second shot can carry on the first's steps); something that isn't a number
is refused.

### Shapes

`r`, `c` and `v` open a **Shape**: a temporary child of Annotate, with the
same anchor/cursor box as Select.

A shape starts as a point: at the loupe's last pixel if the loupe has been
used this run, else at the pointer. That point is the anchor and stays
put; the cursor corner grows the shape, with the box keys or by following
the mouse, and a click puts the cursor there. `o` makes the opposite
corner the cursor. A drag draws a new shape instead.

| Input        | Action                                                  |
| ------------ | ------------------------------------------------------- |
| `t`          | Rectangle → circle → line → arrow                       |
| `V`          | Arrow: a head at both ends, or at one                   |
| `1`–`5`      | Colour                                                  |
| `+` / `-`    | Width                                                   |
| `p`          | Eyedropper, at the shape's cursor corner                |
| `a`          | Lock the shape and go back to the pen                   |
| `x` / `f`    | Lock it and switch to Censor / Filter                   |
| `Enter`      | Lock it and commit the layer                            |
| `Esc`        | Drop this shape, back to the pen                        |
| `u`          | Undo the open layer's last action                       |
| `c`          | Centre mode                                             |
| box keys     | Change the shape, as in Select                          |
| `z` / `e`    | Loupe / Enhance at the shape's centre                   |

- A circle fills its bounding box, so a square box makes a circle.
- A line or arrow runs from the anchor to the cursor: the tail is pinned
  at the starting point and the tip follows the cursor. It is drawn
  diagonally in the box, so every box key works on it: `o` makes the other
  end the cursor (an arrow's head moves with the cursor, so this flips
  it), `Shift+hjkl` moves it whole, and `Ctrl+hjkl` makes its box the
  neighbouring window's, as in Select.
- A plain line is reached only through `t`.
- Arrowheads scale with the width.
- A shape with no size is not locked.

## Lists

Filter and Layers share one list grammar (auto-redact's review and
Measure use parts of it).

| Input     | Action                          |
| --------- | ------------------------------- |
| `[` / `]` | Select the item below / above   |
| `<` / `>` | Move the selected item down / up |
| `d`       | Delete                          |
| `Space`   | Mute / hide                     |

- Lists show the top of the stack first.
- The selection follows an item when it moves.
- Deleting selects the next row down the list as shown (the item below in
  the stack) where there is one, else the row above.
- The wheel stays `+` / `-`; it doesn't scroll lists.

## Layers view

Layers manages committed layers. `1`–`9` in Select opens it at that layer,
counted from the bottom (the highest there is, if there are fewer); with
no layers, the status says `no layers yet`. The selected layer is shown on
its own: the original capture with only that layer applied, in its own
clip, and the other layers hidden.

| Input             | Action                                   |
| ----------------- | ---------------------------------------- |
| `1`–`9`           | Pick a layer                             |
| click             | Pick the topmost layer under the pointer |
| `[` / `]`         | Select a layer                           |
| `+` / `-`, wheel  | Select a layer                           |
| `<` / `>`         | Reorder                                  |
| `d`               | Delete                                   |
| `Space`           | Hide / show                              |
| `u`               | Undo the last layer-management action    |
| `Enter` / `Esc`   | Back to Select                           |

Every change here is one entry in project history. Committing a layer is
one Select (project) action too.

## Undo

Undo records meaningful actions, not raw input: locking an operation,
adding, deleting or reordering a filter, changing a filter's amount,
committing, deleting or moving a layer, changing the selection.

Repeated key movement and drag motion are not separate undo steps, and a
held adjustment collapses into one step.

There are two scopes, and undo never falls from one into the other.

**Open-layer history** is used while building. Censor, Annotate, Shape
and Filter share it, so undo crosses tools inside the layer:

```text
censor
rectangle
contrast
u → reverts the contrast
u → removes the rectangle
u → removes the censor
```

**Project history** is used from Select and Layers: selection changes,
committed layers, layer deletion, movement and visibility. Entries that
would change nothing are skipped, so every `u` visibly does something.

Insert's `u` undoes its cursor's moves, and Measure has its own scope
([measure.md](measure.md#undo)).

## Measure

Measure (`m` in Select, or `--mode measure`) measures lengths, arcs,
paths, circles, rectangles, areas and angles over the frozen picture: in
capture pixels, or in real units once calibrated. It opens no layer, and
its drawings are overlay only: they are never in the image. How each tool
measures, calibration, precision and output are in
[measure.md](measure.md).

Points are placed with Insert's pixel cursor, starting at the box's
cursor corner and kept on that corner's monitor. The mouse moves it too,
once it moves.

| Input                       | Action                                                    |
| --------------------------- | --------------------------------------------------------- |
| `h j k l`                   | Move the cursor a step (`Shift`: ×4; `Alt`: halfway to the edge) |
| `Ctrl+hjkl`                 | Jump to the next edge in the picture that way             |
| `0 $ g G`                   | The monitor's far edge                                    |
| `+` / `-`, wheel            | Step size; with a line open, bend it into an arc          |
| `s`                         | Line: ends at its second point                            |
| `c`                         | Circle: three points on it                                |
| `A`                         | Area: a point inside measures the region round it         |
| `r`                         | Rectangle: two corners; `Enter` with none measures the box |
| `^`                         | Angle: arm, vertex, arm; or a first point on a vertex of lines, for every angle round it |
| `Space` / click             | Drop a point (snaps to points within 4px)                 |
| `Alt+Space`                 | Drop a point, unsnapped                                   |
| `Shift+Space` / Shift+click | Drop a point and keep the line going, as a path; a Shift+click reaches 12px to land on another measurement's point, else its line |
| hold `Ctrl`                 | Lock the mouse cursor to 0°, 45° or 90° from the last point |
| Alt+click                   | Select the measurement under the pointer                  |
| Alt+drag                    | Move a measurement's point                                |
| `o`                         | Edit the selected measurement's points: `o` the next, the cursor keys move it, `Space`/`Enter` keep, `Esc` put back |
| `Enter`                     | Finish, ending at the cursor; with nothing in progress, deliver (with `r` and no corners: measure the box) |
| `u`                         | Take back the last point, else the last action (a measurement, a delete, a calibration, `x`, a move, an edit) |
| `C`                         | Calibrate against the selected line or arc                |
| `t` / `T`                   | Metric ↔ imperial / show pixels                           |
| `[ ]` `< >` `d`             | Select, move, delete ([Lists](#lists))                    |
| `x`                         | Leave the selected one out of the total                   |
| `z` / `e`                   | Loupe / Enhance at the cursor                             |
| `y`                         | Deliver the measurements as text                          |
| `Esc`                       | Abandon the one in progress; else back (warning first)    |

## Output

The picker finishes with one of:

| Finish | Key (Select)          | Result                                                  |
| ------ | --------------------- | ------------------------------------------------------- |
| image  | `Enter`, right click  | The box, with its committed layers                      |
| text   | `y`                   | The box's text, by OCR                                  |
| colour | `p`                   | A pixel's colour (`#rrggbb` by default)                 |
| code   | `Q`                   | The text of a QR code or barcode in the box             |
| pin    | `P`                   | The box's image, floating on every workspace            |
| measurements | `y` or `Enter` in Measure | The measurements as text                    |

- Results go to the clipboard by default. Where else they go (files,
  stdout, `--json`), and the formats, are in [cli.md](cli.md).
- `p` takes the active cursor's pixel: the box's cursor corner in Select,
  the cursor in Insert, the loupe's pixel in the loupe.
- `Q` with no code in the box says why in the status and stays, so the box
  can be adjusted. Codes are read by zbar's `zbarimg`.
- OCR uses Tesseract (language: `--ocr-lang`, default `eng`). It keeps
  each word's text, box and confidence as well as the full text;
  auto-redact and Measure's scale-bar reading use the same words.
- `Esc` in Select and `q` cancel. A cancel is the user's decision and is
  reported apart from a failure to capture or deliver (exit 1, not 3 or
  4).

## Projects and history

Each shot keeps an editable **project**, so a recent shot can be reopened
with its layers still editable (`screenie edit last`). A project is:

```text
the source capture
the layout and coordinate metadata (monitors, windows, scales)
the selection
the ordered layers, and their ordered operations
the Measure calibration, if any
```

A project reproduces the same image from its capture and operations,
whatever happened during the run. Runtime state is not part of it: the
mode stack, undo history, render caches, key history, warnings and
transient cursors.

Recent projects live in the runtime directory; `--save-project` saves one
beside the image, and `--no-history` keeps none. Where they are and how
long they're kept is under FILES in [cli.md](cli.md).

## Coordinates and scaling

Picker interaction uses logical desktop coordinates. Image operations use
device (image) pixels. There is one conversion from logical to pixels:

```text
rnd(v) = floor(v + 0.5)
```

applied to each edge on its own. Crop boundaries, edge pixels and the
cursor's pixel all agree under this rule, so mixed and fractional monitor
scales never give two readings of the same edge. Output scale for a box
across monitors is in [cli.md](cli.md) (COORDINATES, `--scale`).
