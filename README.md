# screenie

Keyboard-driven screenshots built around one interaction grammar.

screenie freezes the desktop, then lets you select, inspect, edit and extract from that frame using controls modeled after vim visual-block mode. The keyboard is the primary interface, but the mouse is a complete screenshot workflow using the same underlying operations.

The core idea is simple: learn how **Select** works, and the rest of the picker should feel familiar.

## Status

screenie is a Rust program, used daily on Hyprland. It began as a Python prototype for settling the UX, which is now frozen and kept only as a historical reference (no longer a fallback, since 2026-10-08).

This README is the authoritative specification for picker behavior. When implementation and README disagree, the README wins.

Supporting design documents:

- `docs/cli.md` — Rust command-line interface
- `docs/auto-redact.md` — automatic redaction
- `docs/measure.md` — measurements and calibration
- `docs/annotate.md` — annotation extensions
- `docs/architecture-rust.md` — architecture of the Rust rewrite
- `TODO.md` — port notes and deferred work
- `docs/python-prototype.md` — running, testing and comparing against the Python prototype

## What screenie does

Press Print and screenie captures the visible desktop once. Every monitor is then covered by an overlay showing that frozen frame.

From there you can:

- select a region;
- inspect individual pixels;
- magnify or enhance an area without changing the output;
- censor content;
- draw and annotate;
- apply image filters;
- manage editable layers;
- OCR text;
- copy a colour;
- decode QR codes and barcodes;
- pin a floating copy;
- save or copy the resulting image.

The capture itself is never modified. Editing is represented as ordered operations in layers over the original frame.

What you see in the frozen frame is what the output is based on. Live desktop changes after capture do not enter the screenshot.

## Platform

### Current prototype

The current Python implementation targets:

- Wayland
- Hyprland
- `gtk-layer-shell`
- `grim`

### Rust implementation

The Rust port targets Wayland first, with Hyprland as the initial supported compositor environment.

The architecture is intended to allow X11 support later without changing picker behavior.

Recording, scrolling captures, uploads, beautification and live/unfrozen picking are non-goals.

---

# The picker

## The common grammar

The picker is modal.

**Select** is always the base mode. Other modes temporarily take control of input while preserving the same movement and adjustment vocabulary wherever that vocabulary makes sense.

### Directions

`h j k l` and the arrow keys mean directions everywhere.

Held directions repeat with acceleration and combine, so holding `k` and `l` moves diagonally.

| Input             | Meaning                                                     |
| ----------------- | ----------------------------------------------------------- |
| `hjkl` / arrows   | Steer the active thing                                      |
| `Shift+hjkl`      | Move the whole box, or take a larger step in point modes    |
| `Ctrl+hjkl`       | Jump to the next meaningful target                          |
| `Ctrl+Shift+hjkl` | Slide or jump the whole box to the next meaningful boundary |
| `Alt+hjkl`        | Halve or scale toward that side                             |
| `Alt+Shift+hjkl`  | Double or scale outward toward that side                    |
| `Ctrl+Alt+hjkl`   | Jump to the next edge in the picture (boxes)                |

The target of a jump depends on the mode.

Examples:

- Select jumps between windows and layout boundaries.
- Measure can jump to detected picture edges.
- A mode where a modifier has no meaning ignores that modified key completely.

A meaningless modifier never falls through to the unmodified action.

### Adjustment

`+` and `-` adjust **the current amount**.

The mouse wheel is the same operation.

Depending on the mode, that may mean:

- selection scope;
- cursor step;
- line width;
- censor strength;
- filter amount;
- loupe size;
- zoom;
- selected layer.

At any moment there should be at most one obvious adjustable amount.

### Universal controls

| Input   | Meaning                                                     |
| ------- | ----------------------------------------------------------- |
| `z`     | Loupe                                                       |
| `e`     | Enhance                                                     |
| `u`     | Undo the last completed action in the current editing scope |
| `Enter` | Done / confirm                                              |
| `Esc`   | Back                                                        |
| `q`     | Quit                                                        |
| `?`     | Show or hide the current mode's key legend                  |

Keys held across a mode transition do not count as fresh presses in the new mode.

Other command keys fire once per physical press unless their action explicitly supports repeat.

---

# Mouse

The mouse exposes the same screenshot workflow without requiring keyboard commands.

| Input        | Meaning                                |
| ------------ | -------------------------------------- |
| left drag    | Draw or manipulate the active geometry |
| left click   | Place, select or snap the active thing |
| wheel        | `+` / `-`                              |
| right click  | `Enter`                                |
| middle click | `Esc`                                  |
| click a palette item | What its key does                |
| drag the box's edge or corner | Move that edge or corner (Select, Censor, Filter) |
| `Alt`+drag inside the box | Move the box             |

The basic mouse workflow is therefore:

1. click a window or drag a region;
2. scroll if a wider scope is wanted;
3. click a mode in the palette (censor, annotate…) and work in it;
4. right click (or click a finish in Select's palette: text, colour, code, pin).

Every palette item is clickable. Clicks on any chip are the chip's: they
never reach the picture or the box, and a drag that starts on one does
nothing. The item under the pointer is drawn inverted.

A press within 6px of the box's edge, inside or out, takes hold of it,
and near a corner, of the corner; the pointer shows a resize arrow there
first. Dragging an edge moves only it, a corner moves while the opposite
stays, and an edge can cross the opposite one. Centre mode (`c`) makes it
resize about the centre. One drag is one undo step. Anywhere else a drag
still draws a new box, and a click still does what the mode's click does.
Annotate's shapes keep a plain drag for drawing anew, since their cursor
corner is always under the pointer.

Keyboard and mouse do not need identical physical event sequences. They must reach the same operations and editable model.

---

# Tags

screenie uses small on-screen text chips around the active box.

Each kind has a preferred side:

- status: bottom;
- palette and key legend: top;
- lists such as Filters and Layers: sides.

Placement rules:

1. outside the box on the preferred side;
2. inside the box on that same side if necessary;
3. another side as fallback.

Tags must:

- stay entirely on one monitor;
- never cross a monitor seam;
- avoid each other;
- avoid mode-owned UI such as the loupe;
- avoid obscuring the active box whenever possible.

Borders are white. Chips are near-opaque black.

A status tag is intentionally dense and vim-like, for example:

```text
CENSOR  560×90  (659,259)  layer +1  pixelate 10px  20×20
```

Palettes are always visible when a mode has directly selectable kinds, and in Select, whose palette holds the modes it opens and its finishes. The full key legend is behind `?`. Palette items are clickable (see Mouse).

---

# Mode families

| Kind                | Modes                           | Purpose                               | Changes output?   |
| ------------------- | ------------------------------- | ------------------------------------- | ----------------- |
| **Target**          | Select, Insert                  | Choose a region or point              | No                |
| **Lens**            | Loupe, Enhance                  | Change how the frozen image is viewed | Never             |
| **Build**           | Censor, Annotate, Shape, Filter | Add operations to the open layer      | When committed    |
| **Manage**          | Layers                          | Inspect and modify committed layers   | Yes               |
| **Measure**         | Measure                         | Measure the frozen picture            | Never             |
| **Temporary input** | Text entry and similar children | Collect a value or temporary geometry | Only if confirmed |
| **Finish**          | image, OCR, colour, QR, pin     | Produce an output                     | —                 |

Only the top mode receives input.

Modes draw from bottom to top, so temporary children appear over the mode that opened them.

Lenses may return a position to their parent. They never return modified pixels.

---

# Layers

The captured frame is immutable.

The first build tool opened creates an implicit **open layer**, clipped to the current selection.

Every build operation created before the layer is committed is added to that same layer in order.

Order matters.

For example, a filter added after an annotation filters the annotation too.

A layer contains editable operations such as:

- censors;
- pen strokes;
- shapes;
- lines and arrows;
- text;
- filters;
- highlights;
- callouts.

## Locking and committing

Build tools distinguish between the item currently being edited and operations already locked into the open layer.

Common build behavior:

| Input                  | Meaning                                                  |
| ---------------------- | -------------------------------------------------------- |
| current tool key       | Lock the current item and begin another                  |
| another build-tool key | Lock the current item and switch tools in the same layer |
| `u`                    | Undo the last completed action in the open layer         |
| `Enter`                | Lock the current item and commit the layer               |
| `Esc`                  | Discard the open layer                                   |

If the open layer contains locked work, the first `Esc` warns instead of discarding it. A second `Esc` discards it. Any unrelated key cancels the warning.

`q` behaves similarly when quitting with locked work.

Undo never automatically crosses an editing-scope boundary.

If the open layer has nothing left to undo, `u` does not begin undoing committed project history.

---

# Select

Select chooses the output region and is the base of the picker.

The box is represented as:

- an anchor corner;
- a cursor corner.

This behaves like vim visual-block mode.

The cursor may cross the anchor naturally.

| Input               | Action                                               |
| ------------------- | ---------------------------------------------------- |
| `h j k l`           | Move the cursor corner                               |
| `o` / `Space`       | Move the cursor to the opposite corner               |
| `O` / `Shift+Space` | Move the cursor to the sideways corner               |
| `w b } {`           | Next nearby window or monitor edge                   |
| `Ctrl+Alt+hjkl`     | Next edge in the picture: the cursor's edge moves to the next boundary that way (also in Censor and Filter) |
| `0 $ g G`           | Far left / right / top / bottom                      |
| `c`                 | Centre mode: resize symmetrically around the centre  |
| `+` / `-`           | Change selection scope                               |
| `.`                 | Recall previously accepted regions                   |
| `u`                 | Undo the last Select/project action                  |
| click               | Snap to the window under the pointer, or its monitor |
| drag                | Draw a region                                        |
| `i`                 | Insert                                               |
| `m`                 | Measure                                              |
| `z`                 | Loupe                                                |
| `e`                 | Enhance                                              |
| `x`                 | Censor                                               |
| `a`                 | Annotate                                             |
| `f`                 | Filter                                               |
| `1`–`9`             | Layers                                               |
| `Enter`             | Capture image                                        |
| `y`                 | OCR                                                  |
| `p`                 | Copy colour                                          |
| `Q`                 | Decode QR/barcode                                    |
| `P`                 | Pin                                                  |
| `Esc`               | Cancel                                               |

The box starts on the active window.

## Scope

Select distinguishes between geometry bounds and selection scope.

The box remains ordinary anchor/cursor geometry. Scope changes which spatial objects are meaningful to snapping and widening.

The scopes are:

```text
window → monitor → everything
```

The mouse wheel changes scope in Select.

On a manually drawn box, widening may first expand it to the containing window before moving outward through broader scopes.

## Recall

`.` recalls accepted boxes.

The history contains the last ten accepted regions and keeps them associated with monitor identity so they can be restored in the same place when possible.

Repeated `.` walks backward through the ring.


The picture-edge jump treats the box's edge as a line between pixels: it
looks from the pixel just ahead of it, along the cursor corner's row or
column, and stops just before the first one that differs. So each press
stops at a boundary (up to a thing, then past it), the same stops come
back the other way, and it stays on the corner's monitor. With none that
way, the box stays and the status says so.
---

# Insert

Insert selects a single pixel position inside the current box.

The box remains fixed. A free cursor appears inside it.

The cursor is drawn so it remains visible against arbitrary colours.

| Input           | Action                            |
| --------------- | --------------------------------- |
| `h j k l`       | Move by the current step          |
| `Shift+hjkl`    | Move by a larger step             |
| `+` / `-`       | Adjust step size                  |
| `u`             | Undo the cursor's previous move   |
| click           | Place the cursor                  |
| `Alt+hjkl`      | Move halfway toward that edge     |
| `w b } {`       | Next meaningful edge              |
| `0 $ g G`       | Box left / right / top / bottom   |
| `m`             | Box centre                        |
| `z`             | Loupe at the cursor               |
| `e`             | Enhance centred on the cursor     |
| `x a f`         | Open a build tool from the cursor |
| `p`             | Copy the cursor pixel's colour    |
| `Enter` / `Esc` | Return to Select                  |

The normal step is adjustable from 1–20 pixels.

---

# Loupe

Loupe is a precision pixel lens.

It shows:

- a magnified crop;
- a pixel grid;
- the active pixel;
- its colour;
- its coordinates.

The magnifier is positioned like a tag and stays on the cursor's monitor.

| Input                 | Action                                  |
| --------------------- | --------------------------------------- |
| `h j k l`             | Move one pixel                          |
| `Shift+hjkl`          | Move by approximately one loupe width   |
| `+` / `-`             | Change loupe size                       |
| `v` / `V`             | Change view filter                      |
| click / drag          | Place or move the cursor                |
| `Space` / `Shift+Space` | Over Measure: drop a point (a path's) at the loupe's pixel, exactly, and stay |
| `p`                   | Copy pixel colour                       |
| `e`                   | Enhance at the loupe position           |
| `x a f`               | Open a build tool at the loupe position |
| `z` / `Enter` / `Esc` | Close Loupe                             |

When Loupe closes, its parent may adopt the loupe position.

The loupe remembers its last pixel for the rest of the run (it isn't saved). From Select, `z` reopens it there, clamped into the box if the box has changed, and Annotate's shapes start there.

Loupe never modifies image pixels.

---

# Enhance

Enhance is a movable magnified view inside the selection.

It never shows pixels outside the current box.

| Input            | Action                           |
| ---------------- | -------------------------------- |
| `h j k l`        | Move the lens                    |
| `Shift+hjkl`     | Move by larger steps             |
| `Alt+hjkl`       | Halve the lens toward that side  |
| `Alt+Shift+hjkl` | Double the lens toward that side |
| `+` / `-`        | Change zoom in 0.5 steps         |
| `v` / `V`        | Cycle view filters               |
| `s`              | Sharp / smooth scaling           |
| drag             | Draw a new lens                  |
| click            | Centre lens                      |
| `z`              | Loupe at the lens centre         |
| `Enter` / `Esc`  | Close Enhance                    |

View filters include:

- off;
- greyscale;
- invert;
- contrast/readability enhancement.

View filters only change what the user sees. They never reach image output.

Use the Filter build tool to modify the image itself.

---

# Censor

Censor creates obscuring rectangles inside the current selection.

A censor uses the same anchor/cursor box grammar as Select.

Types:

- pixelate;
- solid bar.

| Input                 | Action                            |
| --------------------- | --------------------------------- |
| `t`                   | Switch censor type                |
| `+` / `-`             | Adjust the current censor amount  |
| `x`                   | Lock the censor and begin another |
| `r`                   | Find secrets (auto-redact)        |
| common box controls   | Resize / move / scale             |
| common layer controls | Undo / commit / discard           |

Pixelate uses adjustable block size.

The default remains 10px pixelation.

Pixelation is visual obscuring, not cryptographically secure redaction. screenie intentionally keeps it as the default manual censor style.

Auto-redact may use different defaults for detected sensitive content.

---

# Filter

Filter manages an ordered stack of image filters inside the open layer.

Each filter has:

- a kind;
- its own region;
- an amount where applicable;
- mute state;
- stable ordering.

A newly inserted filter becomes selected.

The selected filter's region uses the normal anchor/cursor box grammar.

The selected filter's amount is the current `+` / `-` adjustment.

Current filters:

| Key | Filter     | Adjustment    |
| --- | ---------- | ------------- |
| `1` | Greyscale  | 25%–100%      |
| `2` | Invert     | none          |
| `3` | Brightness | −5…+5         |
| `4` | Contrast   | −4…+4         |
| `5` | Blur       | 2–20px radius |

Other controls:

| Input          | Action                        |
| -------------- | ----------------------------- |
| `f`            | Duplicate selected filter     |
| list controls  | Select, reorder, delete, mute |
| box controls   | Edit selected filter region   |
| layer controls | Undo / commit / discard       |

---

# Annotate

Annotate adds visible markup.

Annotation properties include a colour and line width.

Current palette:

```text
1 white
2 black
3 red
4 green
5 blue
```

The default is red.

| Input          | Action                                       |
| -------------- | -------------------------------------------- |
| drag           | Draw a freehand pen stroke                   |
| `Shift`+drag   | Hold the line straight                       |
| `Ctrl`+drag    | Hold the line as an arc                      |
| `Ctrl`+`+`/`-`, `Ctrl`+wheel | Arc bulge, 2px a step (below 0: the other side) |
| `h j k l`      | Move the pen point (`Shift`: a larger step)  |
| `Space`        | Pen down / up (up locks the stroke)          |
| `1`–`5`        | Choose colour                                |
| `p`            | Eyedropper                                   |
| `+` / `-`      | Adjust line width                            |
| `m`            | Highlighter on / off                         |
| `r`            | Rectangle                                    |
| `c`            | Circle                                       |
| `v`            | Arrow                                        |
| `i`            | Text                                         |
| `n`            | Numbered callout at the pen                  |
| `N`            | Set the next callout number                  |
| `z` / `e`      | Open a lens                                  |
| layer controls | Undo / commit / discard                      |

The pen works with the mouse or the keyboard: the keyboard moves a pen point, drawn as the pixel cursor, and `Space` puts the pen down and lifts it.

While drawing with the mouse, holding `Shift` holds the piece from where it was pressed straight to the pointer, and holding `Ctrl` holds it as an arc over that chord. The arc's bulge is its height off the chord: it starts at 24px, `Ctrl`+`+`/`-` or `Ctrl`+wheel change it by 2px, below zero it bulges to the other side, and it's kept for the rest of the run. Letting go of the modifier carries the line on freehand from the end of the piece, and pressing it again starts a new piece from there. The status tag says `straight` or `arc 24px` while one is held. The highlighter bends the same way.

## Shapes

Shape is a temporary child of Annotate.

Its geometry uses the same anchor/cursor model as Select.

A shape starts as a point: at the loupe's last pixel if the loupe has been used this run, else at the pointer. That point is the anchor and stays put; the cursor corner grows the shape, with the box keys or by following the mouse. `o` makes the other corner the cursor. A drag draws a new shape instead.

Shapes:

- rectangle;
- circle;
- straight line;
- arrow.

| Input        | Action                                    |
| ------------ | ----------------------------------------- |
| `t`          | Rectangle → circle → line → arrow         |
| `V`          | Arrow: a head at both ends, or at one     |
| `1`–`5`      | Colour                                    |
| `+` / `-`    | Width                                     |
| `p`          | Eyedropper                                |
| `a`          | Lock shape and return to Annotate         |
| `Esc`        | Drop shape and return                     |
| box controls | Manipulate shape                          |

A circle fills its bounding box, so a square box produces a circle.

A line or arrow runs from the anchor to the cursor: the tail is pinned at the starting point, the tip follows the cursor, and `o` (the opposite corner) flips which end moves. Arrowheads scale with the width.

---

# Annotation extensions

Annotate is extended without new interaction grammars: each extension reuses the point, anchor/cursor, trace, colour and adjustment behavior above.

## Text

`i` opens a text entry at the pen point.

Text entry temporarily suspends normal letter commands: typed characters become literal text until confirmed or cancelled.

| Input                   | Action                                   |
| ----------------------- | ---------------------------------------- |
| characters              | Type                                     |
| `Shift+Enter`           | New line                                 |
| `Backspace`             | Delete the last character                |
| `Enter`                 | Lock the text                            |
| `Esc`                   | Drop it                                  |
| `Ctrl+hjkl`             | Move it 1px (`Ctrl+Shift`: 10px)         |
| `Ctrl+` `+` / `-`       | Size                                     |
| `Ctrl+1`–`5`            | Colour                                   |
| `Ctrl+b`                | Background chip on / off (on by default) |

The chip is black behind light ink and white behind dark ink.

The implementation stays intentionally small rather than becoming a rich-text editor.

## Lines and arrows

Straight lines and arrows use anchor-to-cursor geometry: see Shapes.

## Highlighter

Highlighter is a translucent stroke using the same trace interaction as the pen: the ink at 40%, multiplied, five times the line width, so text under it stays readable.

## Numbered callouts

`n` places a numbered disc at the pen point, in the ink with the number in black or white.

The next number is the open layer's highest plus one, so undoing a callout frees its number. `N` types a number to continue from instead.

---

# Lists

Filter and Layers share one list grammar.

| Input     | Action                        |
| --------- | ----------------------------- |
| `[` / `]` | Select the item below / above |
| `<` / `>` | Move selected item down / up  |
| `d`       | Delete                        |
| `Space`   | Mute / hide                   |

Both display the top of the stack first.

Selection follows the item when it is reordered.

Deleting an item selects the next row down the list as displayed (the item below it in the stack) where possible, otherwise the row above it.

The wheel remains `+` / `-`; it is not repurposed for list scrolling.

---

# Layers view

Layers manages committed layers.

Opening a layer shows the original capture with that layer applied in its own clip, while other committed layers are hidden for inspection.

| Input             | Action                                   |
| ----------------- | ---------------------------------------- |
| `1`–`9`           | Pick a layer                             |
| click             | Pick the topmost layer under the pointer |
| `[` / `]`         | Select layer                             |
| `+` / `-` / wheel | Select layer                             |
| `<` / `>`         | Reorder                                  |
| `d`               | Delete                                   |
| `Space`           | Hide / show                              |
| `u`               | Undo the last layer-management action    |
| `Enter` / `Esc`   | Return to Select                         |

A committed layer counts as one Select/project-history action.

---

# Undo

Undo records semantic actions, not raw input events.

Examples of undoable actions:

- lock an operation;
- add/delete/reorder a filter;
- change a filter amount;
- commit/delete/move a layer;
- alter selection history.

Repeated key movement and mouse drag motion are not individual undo steps.

Held adjustments should collapse into a single logical adjustment where practical.

There are two principal scopes:

## Open-layer history

Used while building.

Censor, Annotate, Shape and Filter share the same open-layer history.

Undo can therefore cross tool boundaries inside that layer.

Example:

```text
censor
rectangle
contrast
u → removes/reverts contrast
u → removes rectangle
u → removes censor
```

## Project history

Used from Select and project-management contexts.

It contains operations such as:

- selection changes;
- committed layer creation;
- layer deletion;
- layer movement;
- visibility changes.

Undo does not automatically fall through from an empty open-layer history into project history.

---

# Output

The picker can finish with:

- image;
- OCR text;
- colour;
- QR/barcode text;
- pin.

Image output is copied to the clipboard by default.

The Rust CLI additionally defines explicit output and scripting behavior in `docs/cli.md`.

Machine-readable output uses `--json`.

Cancellation is a user action and is distinct from capture or delivery failure.

---

# OCR

OCR operates on the selected image region.

Internally, OCR should preserve structured information including:

- full text;
- word text;
- word bounding boxes;
- confidence where available.

That representation is shared by:

- OCR output;
- auto-redact;
- future scale-bar detection;
- possible future word-level text selection.

The current Python prototype uses Tesseract.

---

# Auto-redact

Auto-redact finds likely secrets in the box and proposes censors over
them. You review and accept them; it never decides silently, except with
`--redact auto`.

Press `r` in Censor to scan the box. OCR runs in the background, so the
picker stays live; the status tag shows `scanning…`, then
`N found (M ticked)`.

Suggestions draw as dashed boxes, each with its category in a chip.
Ticked ones preview the censor they become: a black bar, or pixelation
with `[redact] kind = "pixelate"`. The side list shows them
top to bottom, labels elided (`adi@…`), never whole:

```text
> ● email      adi@…
  ● token      ghp_…
  · entropy    k9Xq…
```

| Input            | Action                                              |
| ---------------- | --------------------------------------------------- |
| `[` / `]`        | Select a suggestion (`[` down the list)             |
| `Space`          | Tick / untick it                                    |
| `a` / `Enter`    | Accept the ticked ones as censors, one undo step    |
| `d`              | Drop one                                            |
| `r`              | Rescan (after moving the box)                       |
| `Esc`            | Drop the suggestions, back to Censor                |

Accepted suggestions are ordinary censors from then on, and `u` in Censor
undoes the acceptance. They need no special project representation.

Detectors: email addresses; IPv4 and IPv6 addresses; MAC addresses;
known token prefixes (`sk-`, `ghp_`, `glpat-`, `AKIA`, `xox?-`, JWTs);
long random-looking strings; secret URL parameters (only the value is
censored); card numbers (Luhn-checked), their security codes (3-4 digits after
CVV, CVC, CID or "security code", or on their own near a card number)
and their expiry dates (`MM/YY` after "exp", "valid thru" and the like,
or near a card number);
IBANs (mod-97-checked); your home
path; your hostname; private-key blocks (proposed whole); and QR codes
and barcodes, whatever they hold (proposed whole, labelled by what their
contents look like, `code url-secret http…`). Off unless
`[redact] detectors` names them: phone numbers and UUIDs. From the
config: `literal` strings (your name, your employer), `domains` (with
their subdomains), and `[[redact.custom]]` regexes, each its own category.
Names and literals match through OCR's usual confusions (`rn` for `m`,
`0` for `o`, `1` for `l`, any case).
Low-confidence matches, such as less random strings, an email OCR misread
(`©` for `@`), or digit groups that may be a phone number, are proposed
unticked.

It is best effort. OCR misses small, stylised, low-contrast and rotated
text, and text in images.

See `docs/auto-redact.md`.

---

# Measure

Measure (`m` in Select) measures lengths, angles and areas over the frozen
picture: in capture pixels, or in real units once calibrated. It opens no
layer, and its drawings are overlay only: they're never in the image.

Points are placed with Insert's pixel cursor, starting at the box's cursor
corner and kept on that corner's monitor. The mouse moves it too, once it
moves.

| Input                | Action                                                 |
| -------------------- | ------------------------------------------------------ |
| `h j k l`            | Move the cursor a step (Shift: ×4; Alt: halfway to the edge) |
| `Ctrl+h j k l`       | Jump to the next edge in the picture that way          |
| `0 $ g G`            | The monitor's far edge                                 |
| `+` / `-`, wheel     | Step size; with a line open, bend it into an arc       |
| `s`                  | Line: ends at its second point                         |
| `c`                  | Circle: three points on it                             |
| `A`                  | Area: a point inside (click or `Space`) measures the region round it |
| `r`                  | Rectangle: two corners; Enter with none measures the box |
| `^`                  | Angle: arm, vertex, arm; first on a vertex of lines, every angle round it at once |
| `Space` / click      | Drop a point (snaps to points within 4px; `Alt+Space`: no snap) |
| Alt+click            | Select the measurement under it                        |
| Alt+drag             | On a measurement's point: move it                      |
| `o`                  | Edit the selected measurement's points: `o` the next, the cursor keys move it, `Space`/`Enter` keep, `Esc` put back |
| `Shift+Space` / Shift+click | Drop a point and keep the line going, as a path; a Shift+click within 12px of another measurement's point lands on it exactly, else within 12px of a line or path lands on the line (the loupe's points are never snapped) |
| hold `Ctrl`          | The mouse cursor locks to 0°, 45° or 90° from the last point (a click lands there) |
| `Enter`              | Finish, ending at the cursor; with nothing in progress, deliver |
| `u`                  | Take back the last point, else the last action (a measurement, delete, calibration, `x`, move, edit) |
| `C`                  | Calibrate against the selected line or arc (until then, pixels) |
| `t` / `T`            | Metric ↔ imperial / pixels                             |
| `[` / `]` `<` / `>` `d` `x` | Select, move, delete, leave out of the total (Space drops points here, so `x`) |
| `z` / `e`            | Loupe / Enhance at the cursor                          |
| `y`                  | Deliver the measurements as text                       |
| `Esc`                | Abandon the one in progress; else back (warns first)   |

A line ends at its second point. With its first point down, `+`/`-` or the wheel bend it, 2px at a time, into an arc over the chord to the cursor (below zero, the other way); the status shows `arc 10px`, and the live length runs along the arc. Bent, it's measured as an arc: length, radius and angle swept. A Shift'd point keeps it open as a path, which Enter (or a plain point) finishes.

With `A`, a point inside a region measures its area: it floods out from
that pixel over similar colours, stopped by the picture's edges (a room's
walls on a plan) and by anything you've drawn (a line across a doorway),
and measures the outline, things inside it included. It measures to drawn
lines, and to the last pixel inside a picture's edge. If the flood would
reach the monitor's edge, it isn't closed in, and says so: draw lines to
close it. A shape outlined with a closed path is measured the same way.

Circles and angles finish at three points, rectangles at two. With `^`,
a first point on a vertex of straight lines (any line or path point, or
where two lines cross; a click reaches 12px for it) measures every angle
round it at once: each line ending there is one arm, each passing through
is two, and every gap between neighbouring arms under 180° is an angle,
so a T gives its two right angles but not the straight back, and a corner
not its outside. Anywhere else, the angle takes its three points as usual.
Angles draw as an arc between their arms, labelled just outside it. A
crossing is rounded to its pixel, so opposite angles there can differ by
a fraction of a degree.

Everything is in pixels until you calibrate: select a line or arc (the
one just drawn is selected; `[ ]` or Alt+click pick another) and press
`C`, which asks for its real length. If the line runs along a scale bar,
`C` reads it (OCR round the line, in the background): the numbers at the
line's ends, `0 … 100 m`, fill in `100 m`, with an unread 0 taken as 0,
and a lone label such as `5 km` reads as itself. Typing replaces the
reading. The real length can be typed (`= 5 m`; units `mm cm m
km in ft yd mi`, `12'6"`, `12ft 6in`, and arithmetic such as `2*2.5 m`).
The unit is free: any other name after the number is a unit of its own
(`= 4.2 ly`, `= 3 bricks`, `= 2 light years`), and values read as
multiples of it. A number with no unit (`= 4`) is in `u`, and Enter alone
makes the line 1 u.
Each value is shown in the unit of the reference's system that suits its
size: km, m, cm or mm; mi, ft or in. `t` switches system (metric ↔
imperial), so plain units have nothing to convert to.
Esc stays in pixels. That measurement becomes the reference, drawn dashed
and labelled `ref`. Everything reads in its unit from then on, with an
error estimate: `4.20 m ±0.08`. The status shows the scale
(`1 m = 84.2 px ±1.7%`) and warns when the reference is short. The scale is
kept for the rest of the session, and `--measure-scale 84.2px=1m` starts
calibrated.

The side list shows every measurement and the total of the lengths and of
the areas it counts. `y` (or Enter) delivers:

```text
ref   5.00 m  (411.0 px)
L2    4.20 m ±0.08     at 37.5°
A1    12.6 m² ±0.5     perimeter 14.3 m
∠1    37.5°
```

With `--json`, `kind` is `measure`, with `scale` and `measurements`
(points in capture pixels).

See `docs/measure.md`.

---

# Projects and history

The Rust implementation keeps editable project state so recent screenshots can be reopened.

The durable project model consists conceptually of:

```text
source capture
layout / coordinate metadata
selection
ordered layers
ordered operations
```

The following are runtime state and are not required to reproduce the edited image:

- mode stack;
- undo history;
- render caches;
- key history;
- temporary warnings;
- transient cursor state.

A project should reproduce the same image from its source capture and operations regardless of runtime history.

Recent projects may live in temporary storage for `edit last`.

Explicit project saving is controlled separately.

---

# Coordinates and scaling

Picker interaction uses logical desktop coordinates.

Image operations use device/image pixel coordinates.

There is one authoritative logical-to-pixel conversion rule:

```text
rnd(v) = floor(v + 0.5)
```

applied independently at every edge.

Crop boundaries, edge pixels and cursor-pixel lookup must agree under this rule.

Mixed and fractional monitor scaling must not create different interpretations of the same edge.

---

# Current Python prototype

The prototype currently implements:

- Select;
- Insert;
- Loupe;
- Enhance;
- Censor;
- Annotate pen/rectangle/circle;
- Filter;
- Layers;
- image output;
- OCR;
- colour picking;
- QR/barcode decoding;
- pins;
- region recall.

Its purpose is to settle interaction behavior, not to become the long-term implementation.

Current dependencies include GTK3, cairo, gtk-layer-shell, grim, wl-clipboard, notify-send, Tesseract, zbar and swayimg.

The Python CLI remains intentionally small and is not being expanded while the Rust port is underway.

How to run and test it, its tuning constants, and where it differs from this README: `docs/python-prototype.md`.

---

# Rust port

The Rust implementation reached behavioral parity before expanding the feature set. The order it was built in (milestones 1-11 are done; 12 is deferred):

1. headless Select core and behavioral tests;
2. Wayland capture + overlays + Select + image output;
3. Censor, open layers and undo;
4. current Annotate behavior;
5. Filter and rendering caches;
6. Layers;
7. Loupe and Enhance;
8. CLI/output parity, OCR, QR and pins;
9. switch the normal screenshot workflow to Rust;
10. text, lines, arrows, highlighter and callouts;
11. Measure and auto-redact;
12. additional platform support such as X11.

The Python prototype was the behavioral reference until parity; this README is now the only one.

---

# Testing

Behavioral tests are the executable form of this specification.

The Rust core should be testable without Wayland or a renderer.

Headless tests should drive the same semantic input model used by the interactive picker.

Important test areas include:

- anchor/cursor crossing;
- modifier semantics;
- selection scope;
- bounds and clipping;
- key repeat;
- held keys across mode transitions;
- mode-stack transitions;
- open-layer behavior;
- undo scope;
- list behavior;
- serialization round trips;
- logical/pixel rounding;
- rendered image equivalence.

The existing Python test suite remains useful as a behavioral reference during the port.

---

# Development rule

Behavior changes update all three together:

1. this README;
2. the relevant on-screen legend/palette;
3. tests.

The interaction grammar is a product constraint, not an implementation detail.
