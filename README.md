# screenie

Keyboard-driven screenshots for Hyprland: a region picker you drive like vim
visual-block mode, with a mouse fallback, plus tools for looking closer
(loupe, enhance, filters), censoring, OCR, colour picking, QR decoding and
pinning.

**Status:** Python prototype, for iterating on the controls. This README is
the spec; the real implementation will be a Rust port (see `TODO.md`).

## Running

```
bin/screenie [--save-dir DIR] [--window] [--lang LANG]
```

- no flags: the picker. Images go to the clipboard, and a copy to
  `$XDG_RUNTIME_DIR/screenie/`.
- `--save-dir DIR`: also save images to `DIR/<timestamp>.png`.
- `--window`: capture the active window straight away (its own pixels, even
  if covered), no picker.
- `--lang`: tesseract language for OCR (default `eng`, or `$SCREENIE_LANG`).

`$XDG_RUNTIME_DIR/screenie/last` always holds the last image's path (for
annotating it afterwards). Results are announced with `notify-send`.

Dependencies (Arch): `python-gobject python-cairo gtk3 gtk-layer-shell grim
wl-clipboard libnotify`; `tesseract tesseract-data-eng` for OCR, `zbar` for
QR, `swayimg` for pins.

## The picker

The whole layout (every monitor) is captured once and frozen; you work on
that frame, so what you see is exactly what you get. The picker is modal:
**Select** is the base, and keys descend into other modes.

### The rules (the same in every mode)

**Keys.** `h j k l` (or the arrows) are directions everywhere; holding one
repeats, speeding up, and held directions combine (`k`+`l` is diagonal).
Modifiers on them mean one thing each:

| | |
|---|---|
| `hjkl` | Move whatever you're steering: the box's cursor corner, Insert's cursor, the loupe, the lens |
| `Shift+hjkl` | Move the whole box (in the point modes: a bigger step) |
| `Ctrl+hjkl` | Jump: select the neighbouring window that way |
| `Ctrl+Shift+hjkl` | Jump: slide the box to the next window / monitor edge or quarter line |
| `Alt+hjkl` / `Alt+Shift+hjkl` | Scale: halve / double the box (or the lens) toward that side |
| `+` / `-` | Adjust the current amount: the box's scope, the insert step, line width, filter / censor strength, loupe size, zoom, the layer picked (hold to repeat) |
| `z` / `e` | The lenses (loupe / enhance), from any mode |
| `u` | Undo: in Select the last box or layer change; in a build tool the last action on the open layer (wherever in it); in Insert the cursor's last move |
| `Enter` | Done: capture in Select, commit in a build tool, back in the views |
| `Esc` | Back one level; in Select, cancel |
| `q` | Quit the picker now, from any mode (with locked work in an open layer, press it twice: the tag warns) |
| `?` | Show / hide the current mode's keys, on top of the box (the full reference will be the man page) |

A modifier that means nothing in a mode is ignored there (the key doesn't
act as if unmodified). Other keys fire once per press (holding a toggle
doesn't flicker it).

**Mouse.** A plain mouse does everything a screenshot needs, the same way
in every mode:

| | |
|---|---|
| left drag | Draw: the box (or the tool's box, the lens), a pen line; it stops at an edge it can't cross |
| left click | Place: in Select, snap to the window under the pointer (or its monitor); in a tool's box, move its cursor corner there; put a cursor or the lens there; pick a layer |
| wheel | `+` / `-` (in Select: window → monitor → everything, and back; on a box you drew, up widens it to its window and down leaves it) |
| right click | `Enter` |
| middle click | `Esc` |

So: click a window (or drag a box), scroll to widen it, right click.

**Tags.** One status tag shows the current mode vim-style, the box size,
the active cursor's pixel, and what the mode is working on
(`CENSOR  560×90  (659,259)  layer +1  pixelate 10px  20×20`). Each kind
of tag has its own side of the box: the status tag on the **bottom**, the
palette (Filter's filters, Annotate's shapes: always) and key legend (`?`,
the current mode's only) on the **top**, lists (the layer
view, the filter stack) on the **sides**. Each goes just outside the box first, so the box
stays uncovered; then inside on the same side (while it's small next to the
box); then another side. Never off a monitor or across the gap between two,
never on top of each other. Every border is white; the tag
says which box is which.

### Kinds of mode

| Kind | Modes | What it does | Changes the picture? | `u` |
|---|---|---|---|---|
| **Target** | Select (a region), Insert (a point) | Says *where*; tools opened from it start there | No | Box changes, committed layers |
| **Lens** | Loupe, Enhance | Changes how you *see*: magnification, view filters | Never | – |
| **Build** | Censor, Annotate (+ Shape), Filter | Adds operations to the open layer | Only when the layer is committed | The layer's last operation |
| **Manage** | Layers (`1`–`9`) | Browse the committed layers; delete, reorder, hide | Yes | Delete and move are undo steps in Select |
| **Finish** | `Enter` `y` `p` `Q` `P` | Produces the output | – | – |

Lenses can be opened from anywhere and hand back at most a *position*
(turning the loupe off moves Insert's cursor to it), never pixels.

### Layers

The capture itself is never modified. The first build tool you open (`x`,
`a`, `f`) opens a **layer**, clipped to the box; every build tool you use
before committing adds its operations to that same layer, **in order**
(order matters: a filter after an annotation filters it). Operations are
*locked* in as you go and preview live, but nothing is committed until
`Enter`. The picture you see, and every output, is the capture with the
committed layers (and the open one) applied in order.

In every build tool:

| Key | Action |
|---|---|
| its own key (`x` / `f`; `a` in a shape) | Lock the item being edited, start another |
| another tool's key (`x` `a` `f`) | Lock it and switch tool, same layer |
| `u` | Undo the last action on the layer (a lock, an insert, a delete, a move, a mute, a run of `+`/`-`, a reshape), wherever in the layer it was |
| `Enter` | Lock it and commit the layer; back to where the layer was opened |
| `Esc` | Throw the layer away and go back. With locked work in it, the first `Esc` only warns (`Esc again to discard 3`, in the tag); any other key takes the warning back |

`u` in Select (and in the layer view) undoes the last thing done there; a
committed layer counts as one thing, as do the layer view's deletes and
moves.

**Lists** (the filter stack, the layer view) share one set of keys: `[` / `]`
select the one below / above, `<` / `>` move it down / up, `d` deletes it,
`Space` mutes / hides it. Both list the top of the stack first.

### Select (target: a region)

The box starts on the active window. It's an anchor corner plus a cursor
corner (the white square), like visual-block mode.

| Key | Action |
|---|---|
| `h j k l` | Move the cursor corner 5px; keep going past the anchor and it crosses over |
| `o` or `Space` / `O` or `Shift+Space` | Cursor to the opposite / sideways corner |
| `w` `b` `}` `{` | Cursor to the next window or monitor edge right / left / down / up (skips window gaps) |
| `0` `$` `g` `G` | Cursor to the far left / right / top / bottom |
| `c` | Centre mode: the cursor sits at the centre; `h`/`k` grow, `l`/`j` shrink, symmetrically |
| `+` / `-` | Scope: nearest window → its monitor → everything, and back (on a box you drew, `+` widens it to its window, `-` leaves it) |
| modifiers | See [the rules](#the-rules-the-same-in-every-mode) |
| `u` | Undo the last box or layer change |
| click | Snap to the window under the pointer (or its monitor, if there's no window there) |
| `i` / `z` / `e` | → **Insert** / **Loupe** / **Enhance** |
| `x` / `a` / `f` | → **Censor** / **Annotate** / **Filter** (opens a layer) |
| `1`–`9` | → **Layers**, at that layer (once there are any) |
| `Enter` | Capture the image (not an empty box: the tag says so) |
| `y` | Yank the text (OCR) |
| `p` | Yank the hex colour under the cursor |
| `Q` | Decode a QR code / barcode in the box and copy it (stays open if none) |
| `P` | Pin: float the box on top of every workspace |
| `Esc` | Cancel |

### Insert (target: a point)

The box stays put and a free cursor appears at its centre: a solid 3×3
block on the cursor pixel inside a hollow 15×15 square, both drawn inverted
so they show on any colour.

| Key | Action |
|---|---|
| `h j k l` | Move one step, 5 pixels: ballpark (the loupe is for precision). The top speed scales with the box |
| `+` / `-` | The step, 1–20 pixels |
| `Shift+hjkl` | Move four steps |
| `u` | Back to where the cursor was |
| click | Put the cursor there |
| `Alt+hjkl` | Halfway to the box edge that way |
| `w` `b` `}` `{` | The next window / monitor edge that way, or the box edge |
| `0` `$` `g` `G` / `m` | The box's left / right / top / bottom edge / its centre |
| `z` / `e` | → **Loupe** at the cursor (the cursor follows it back) / **Enhance** centred on it |
| `x` / `a` / `f` | → a build tool; a censor starts with the cursor as its top-left |
| `p` | Yank the cursor pixel's hex colour |
| `Enter` / `Esc` | Back to the box |

### Loupe (lens; `z` toggles it)

A pixel cursor of its own (the Insert cursor) with a magnified view: pixel
grid, the cursor pixel outlined, its colour (with a swatch) and
coordinates. The magnifier is placed like a tag: beside the cursor, off the
box if it can be, on the cursor's monitor.

| Key | Action |
|---|---|
| `h j k l` | Move one pixel; it can't leave the box |
| `Shift+hjkl` | Move one loupe-width |
| `+` / `-` | Bigger / smaller loupe window |
| `v` / `V` | View filter |
| click / drag | Put the pixel cursor there / drag it |
| `p` | Yank that pixel's hex colour |
| `e` | → **Enhance** at the loupe's pixel |
| `x` / `a` / `f` | → a build tool at the loupe's pixel |
| `z` / `Enter` / `Esc` | Loupe off |

### Enhance (lens; `e`)

A lens (a sub-window, half the box's size) inside the box shows what's
under it magnified. It can't leave the box and never shows anything outside
it.

| Key | Action |
|---|---|
| `h j k l` | Move the lens (`Shift`: bigger steps) |
| `Alt+hjkl` / `Alt+Shift+hjkl` | Halve / double the lens toward that side |
| `+` / `-` | Zoom in / out: a tap moves to the next 0.5 (2 → 2.5 → 3) |
| `v` / `V` | View filter: off → grey → invert → contrast (the contrast one flips dark backgrounds, for reading) |
| `s` | Sharp / smooth pixels |
| drag / click | Draw a new lens / centre the lens there |
| `z` | → **Loupe** at the lens centre |
| `Enter` / `Esc` | Back |

View filters only change what you see in the lenses; they never reach the
output. To bake a filter into the picture, use the Filter build tool.

### Censor (build; `x`)

A 20px censor box (about a line of text) inside the main box: at its
centre, at the loupe's pixel, or with its top-left on Insert's cursor,
depending on where you opened it. Edited with the box keys and the mouse
(a click moves its cursor corner there); it can't leave the main box.

| Key | Action |
|---|---|
| `t` | Swap this censor's type: pixelate / bar |
| `+` / `-` | Pixelate: block size, 4–40px (default 10). Bar: its shade, black → white (default black) |
| `x` | Lock it, start the next right here (same type) |
| box keys, layer keys | See above |

### Filter (build; `f`)

A **stack** of filters, folded into the layer on `Enter`. It opens raw; a
digit inserts a filter just above the selected one (on top if none is) and
selects it (the tag on top shows which digit is which). Every filter has its own box,
starting as the whole main box; the box shows and reshapes the selected
filter's. The list shows no numbers, since the digits pick kinds here.

| Key | Filter | `+` / `-` |
|---|---|---|
| `1` | Greyscale | Amount, 25%–100% |
| `2` | Invert | – |
| `3` | Brightness | Brighter / darker, −5…+5 in 10% steps |
| `4` | Contrast | More (an S-curve) / less (toward grey), −4…+4; keeps colour |
| `5` | Blur | Radius, 2–20px |

| Key | Action |
|---|---|
| `f` | Duplicate the selected filter, just above it |
| list keys | `[` `]` select, `<` `>` move, `d` delete, `Space` mute |
| box keys, layer keys | See above |

### Annotate (build; `a`)

Lines and shapes, clipped to the box. The tag shows the colour (with a
swatch) and line width. The pen is drawn with the mouse; `Enter` or a tool
key in the middle of a line keeps it.

| Key | Action |
|---|---|
| drag | Pen: a freehand line (locked when you let go) |
| `1` `2` `3` `4` `5` | White / black / red / green / blue (starts red) |
| `p` | Eyedropper: the colour under the pointer, until a number key |
| `+` / `-` | Thicker / thinner line |
| `r` / `c` | → **Shape**: a rectangle / circle |
| `z` / `e` | The loupe / enhance, at the pointer |
| layer keys | See above (`a` itself does nothing in the pen) |

#### Shape (`r` / `c` in Annotate)

A box of its own inside the main box, edited with the box keys and the
mouse (a click moves its cursor corner there; centre mode `c` is handy for
circles). The circle fills the box, so a
square box gives a circle.

| Key | Action |
|---|---|
| `t` | Swap rectangle / circle |
| `1`–`5`, `+` `-`, `p` | Colour, line width, eyedropper (at the shape's cursor) |
| `a` | Lock it, back to the pen |
| `Esc` | Drop just this shape, back to the pen |
| layer keys | See above |

### Layers (manage; `1`–`9` in Select)

The picked layer is shown on its own: the capture with only that layer
applied, inside the box it was made in (hiding the layers below and above
it). The list shows every layer, top first.

| Key | Action |
|---|---|
| `1`–`9` / click | Pick a layer / the topmost one under the pointer |
| list keys, `+` `-` | `[` `]` (or `+` `-`, the wheel) select, `<` `>` move (it applies earlier / later), `d` delete, `Space` hide |
| `u` | Undo the last delete / move |
| `Enter` / `Esc` | Back to Select |

## Layout of the code

| Module | What |
|---|---|
| `cli.py` | Arguments, and carrying out how the picker finished (every output) |
| `hypr.py` | Hyprland state (`hyprctl`) and capture (`grim`) |
| `frame.py` | The frozen frame: the capture, the committed picture, the view (with the open layer, rebuilt lazily from cached snapshots); view filters, crops, pixel reads |
| `layers.py` | Operations (censor, stroke, shape, filter; each with a `key()` and `bbox()`), layers, undo entries |
| `keys.py` | Key-name sets shared by the modes (the mouse arrives as keys too) |
| `geometry.py` | The box: visual-block cursor, snapping, bounds, undo. No GTK |
| `app.py` | Overlay windows (gtk-layer-shell), key repeat, the mode stack, the open layer |
| `modes/` | `box` (editing a box: the base of Select and the tools' boxes); `select`, `insert` (targets); `loupe`, `enhance` (lenses); `censor`, `filter`, `annotate` (build; shared plumbing in `layer`); `layers_view` |
| `output.py` | Clipboard, saving, notifications, OCR, QR, pins |
| `draw.py` | Shared drawing helpers |

## Tests

```
python -m unittest discover -s tests
```

`test_geometry.py`, `test_draw.py` (tag placement) and `test_hypr.py`
(which windows count as visible) are pure logic. `test_modes.py` drives the
real App with synthetic events on a synthetic frame, and `test_audit.py`
holds a regression test for each finding of the audit (both need a Wayland
session; windows are created but never shown).
