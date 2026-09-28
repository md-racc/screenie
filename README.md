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

The whole layout (every monitor) is captured once and frozen; you select from
that frame, so what you see is exactly what you get. The picker is modal:
**Select** is the base mode, and some keys descend into another mode. Esc
always climbs back out one level (from Select it cancels).

`h j k l` are vim directions everywhere (arrows work too). Holding a direction
repeats, speeding up; held directions combine (`k`+`l` is diagonal). Other
keys fire once per press (holding a toggle doesn't flicker it).

### Select (base mode)

The box starts on the active window. It's an anchor corner plus a cursor
corner (the white square), like visual-block mode.

| Key | Action |
|---|---|
| `h j k l` | Move the cursor corner 5px; keep going past the anchor and it crosses over |
| `o` / `O` | Cursor to the opposite / sideways corner |
| `w` `b` `}` `{` | Cursor to the next window or monitor edge right / left / down / up (skips window gaps) |
| `0` `$` `g` `G` | Cursor to the far left / right / top / bottom |
| `Shift+hjkl`, or hold `Space` + `hjkl` | Move the box |
| `Ctrl+hjkl` | Select the neighbouring window that way (like `SUPER+hjkl` focus) |
| `Ctrl+Shift+hjkl` | Slide the box until an edge hits a window or monitor edge |
| `Alt+hjkl` | Halve the box toward that side |
| `c` | Centre mode: the cursor sits at the centre; `h`/`k` grow, `l`/`j` shrink, symmetrically |
| `Tab` / `Shift+Tab` | Next / previous visible window |
| `a` | Nearest window → its monitor → everything → back |
| `u` | Undo (box changes and censors) |
| drag | Draw a box (anchor where you pressed, cursor where you let go); never captures |
| click | Snap to the window under the pointer |
| `f` / `F` | Filter: off → grey → invert → contrast (shown live in the box; applies to the output) |
| `+` / `-` | Contrast filter stronger / weaker; below the weakest it's back to raw |
| `s` | Sharp / smooth pixels (for enhance) |
| `i` | → **Insert** (a free cursor to open the tools at) |
| `z` | → **Loupe** |
| `e` | → **Enhance** (a magnifying lens inside the box) |
| `x` | → **Censor** |
| `Enter` | Capture the image |
| `y` | Yank the text (OCR) |
| `p` | Yank the hex colour under the cursor corner |
| `Q` | Decode a QR code / barcode in the box and copy it (stays open if none) |
| `P` | Pin: float the box, filtered, on top of every workspace |
| `Esc` / `q` / right-click | Cancel |

### Insert (`i`)

Like vim's insert mode: the box stays put and a free cursor appears at its
centre: a solid 3×3 block on the cursor pixel inside a hollow 15×15 square,
both drawn inverted so they show on any colour. Move it
anywhere inside the box, then open the tools right there. The label shows
the cursor's offset inside the box.

| Key | Action |
|---|---|
| `h j k l` | Move 5 pixels: ballpark (the loupe is for precision). Hold to speed up from 40 px/s; the top speed scales with the box, about 1s to cross it |
| `Shift+hjkl` | Move 20 pixels |
| `Ctrl+hjkl`, `w` `b` `}` `{` | To the next window / monitor edge that way, or the box edge |
| `Alt+hjkl` | Halfway to the box edge that way |
| `0` `$` `g` `G` | The box's left / right / top / bottom edge |
| `m` | The box's centre |
| click | Put the cursor there |
| `z` | → **Loupe** at the cursor (when it's turned off, the cursor follows where the loupe went) |
| `e` | → **Enhance**, lens centred on the cursor |
| `x` | → **Censor**, with the cursor as the box's top-left (the censor's own cursor starts bottom-right); Enter / Esc come back here |
| `p` | Yank the cursor pixel's hex colour |
| `f` `F` `+` `-` `s` | Filters, as in Select |
| `Esc` / right-click | Back to the box |

### Loupe (`z` toggles it)

A pixel cursor of its own appears dead centre in the box, with a magnified
view (pixel grid, the cursor pixel outlined in red, its colour and
coordinates).

| Key | Action |
|---|---|
| `h j k l` | Move the pixel cursor one pixel (hold to repeat); it can't leave the box |
| `Shift+hjkl` | Move it one loupe-width (n pixels for an n×n loupe) |
| click / drag | Put the pixel cursor under the pointer / drag it |
| `+` / `-` | Bigger / smaller loupe window |
| `p` | Yank that pixel's hex colour |
| `x` | → **Censor**, starting at the loupe's pixel (Esc / Enter comes back to the loupe) |
| `z` / `Esc` / right-click | Loupe off |

### Censor (`x`)

The main box freezes and a second, orange box appears inside it: a 20px
square (about a line of text) at the centre of the box, or at the loupe's
pixel when started from the loupe. It shows a live preview of the censor,
is edited with all of Select's box keys and the mouse, and can't leave the
main box.

| Key | Action |
|---|---|
| box keys, drag | Shape the censor box (`z` for the loupe in here too) |
| `t` | Swap this censor's type: pixelate / black bar |
| `x` | Multi-censor: burn this one in and start the next right here, same type |
| `Enter` | Burn the censor into the frame and go back (`Enter` again in Select captures) |
| `Esc` / right-click | Drop it and go back |

Each censor is one undo step in Select.

### Enhance (`e`)

A lens (a sub-window, half the box's size) appears inside the box and shows
what's under it magnified, with the filters and sharp / smooth pixels
applied. Everything is limited to the box: the lens can't leave it and never
shows anything from outside it.

| Key | Action |
|---|---|
| `Shift+hjkl` | Move the lens (hold to keep going) |
| `j` / `k` | Zoom in / out: a tap moves to the next 0.5 (2 → 2.5 → 3); hold to zoom continuously |
| drag | Draw a new lens |
| `f` `F` `+` `-` | Filters, as in Select |
| `s` | Sharp / smooth pixels |
| `Esc` / right-click | Back |

## Layout of the code

| Module | What |
|---|---|
| `cli.py` | Arguments, and carrying out how the picker finished (every output) |
| `hypr.py` | Hyprland state (`hyprctl`) and capture (`grim`) |
| `frame.py` | The frozen frame: filters, crops, pixel reads, censoring |
| `geometry.py` | The box: visual-block cursor, snapping, bounds, undo. No GTK |
| `app.py` | Overlay windows (gtk-layer-shell), key repeat, the mode stack |
| `modes/` | `select`, `loupe`, `censor`, `enhance`: keys, repeat rates, drawing |
| `output.py` | Clipboard, saving, notifications, OCR, QR, pins |
| `draw.py` | Shared drawing helpers |

## Tests

```
python -m unittest discover -s tests
```

`test_geometry.py` is pure logic. `test_modes.py` drives the real App with
synthetic events on a synthetic frame (needs a Wayland session; windows are
created but never shown).
